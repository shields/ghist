// Copyright © 2026 Michael Shields
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::num::NonZeroUsize;

use super::{
    Row, Shape,
    cell::{Cell, DOWN, LEFT, RIGHT, UP},
};

const HOLE_AGE: usize = 16;

#[derive(Clone)]
struct Lane<Id> {
    target: Id,
    color: usize,
}

pub struct Graph<Id> {
    lanes: Vec<Result<Lane<Id>, usize>>,
    tick: usize,
    ncolors: NonZeroUsize,
    counter: usize,
    shape: Shape,
}

impl<Id: Clone + Eq> Graph<Id> {
    pub fn new(ncolors: NonZeroUsize) -> Self {
        Self {
            lanes: Vec::new(),
            tick: 0,
            counter: ncolors.get() - 1,
            ncolors,
            shape: Shape::default(),
        }
    }

    fn find(&self, id: &Id) -> Option<usize> {
        self.lanes
            .iter()
            .position(|lane| lane.as_ref().is_ok_and(|lane| lane.target == *id))
    }

    fn place(&mut self, start: usize, target: Id, color: usize) -> usize {
        let lane = Ok(Lane { target, color });
        if let Some((index, slot)) = self
            .lanes
            .iter_mut()
            .enumerate()
            .skip(start)
            .find(|(_, slot)| slot.is_err())
        {
            *slot = lane;
            index
        } else {
            let index = self.lanes.len();
            self.lanes.push(lane);
            index
        }
    }

    #[track_caller]
    fn vacate(&mut self, column: usize) -> Lane<Id> {
        let slot = self
            .lanes
            .get_mut(column)
            .expect("the vacated column is allocated");
        std::mem::replace(slot, Err(self.tick)).expect("the vacated column is occupied")
    }

    pub fn next(&mut self, id: &Id, shown: &[Id]) -> &Shape {
        let existing = self.find(id);
        let node = existing.unwrap_or_else(|| self.place(0, id.clone(), self.counter));
        let incoming = vertical(&self.lanes);
        let mut node_row = incoming.clone();
        node_row
            .get_mut(2 * node)
            .expect("the node lane has a cell")
            .node = true;
        self.shape = Shape {
            rows: vec![node_row],
            pad: Vec::new(),
            width: self.lanes.len(),
        };
        let mut parents = Vec::new();
        for parent in shown {
            if !parents.contains(&parent) {
                parents.push(parent);
            }
        }
        let merge = parents.len() > 1;
        let mut assignments = Vec::new();
        for parent in parents {
            if existing.is_none() || merge {
                self.counter = (self.counter + 1) % self.ncolors.get();
            }
            let position = self.find(parent);
            let color = position
                .and_then(|index| self.lanes.get(index))
                .and_then(|lane| lane.as_ref().ok())
                .map_or(self.counter, |lane| lane.color);
            assignments.push((parent.clone(), position, color));
        }
        self.vacate(node);
        let tap_right = node > 0
            && assignments
                .iter()
                .skip(1)
                .any(|(_, position, _)| position.is_none());
        let mut targets = Vec::new();
        let mut pull = None;
        for (index, (parent, position, color)) in assignments.into_iter().enumerate() {
            let target = if index == 0
                && position.is_none_or(|position| position == node || position > node && !tap_right)
            {
                if let Some(position) = position.filter(|&position| position > node) {
                    pull = Some((position, color));
                }
                *self
                    .lanes
                    .get_mut(node)
                    .expect("the node lane is allocated") = Ok(Lane {
                    target: parent,
                    color,
                });
                node
            } else {
                position.unwrap_or_else(|| self.place(node, parent, color))
            };
            targets.push((target, color, position.is_some()));
        }
        if targets.iter().any(|&(target, _, _)| target != node) {
            self.fanout(node, &incoming, &targets);
        }
        if let Some((source, color)) = pull {
            let mut row = vertical(&self.lanes);
            connect(&mut row, node, source, color);
            set(&mut row, node, UP | DOWN | RIGHT, color);
            set(&mut row, source, UP | LEFT, color);
            self.shape.rows.push(row);
            self.vacate(source);
        }
        self.compact();
        while self.lanes.last().is_some_and(Result::is_err) {
            self.lanes.pop();
        }
        self.tick += 1;
        self.shape.width = self.shape.width.max(self.lanes.len());
        self.shape.pad = vertical(&self.lanes);
        &self.shape
    }

    fn compact(&mut self) {
        let Some(source) = self.lanes.iter().rposition(Result::is_ok) else {
            return;
        };
        let Some(target) = self.lanes.iter().take(source).position(|lane| {
            lane.as_ref()
                .is_err_and(|&freed| self.tick - freed >= HOLE_AGE)
        }) else {
            return;
        };
        let mut row = vertical(&self.lanes);
        let moving = self.vacate(source);
        connect(&mut row, target, source, moving.color);
        set(&mut row, target, DOWN | RIGHT, moving.color);
        set(&mut row, source, UP | LEFT, moving.color);
        *self.lanes.get_mut(target).expect("the hole is allocated") = Ok(moving);
        self.shape.rows.push(row);
    }

    fn fanout(&mut self, node: usize, incoming: &[Cell], targets: &[(usize, usize, bool)]) {
        let mut row = incoming.to_vec();
        row.resize(2 * self.lanes.len() - 1, Cell::default());
        let mut outward = targets.to_vec();
        outward.sort_by_key(|&(target, _, _)| std::cmp::Reverse(target.abs_diff(node)));
        for &(target, color, _) in &outward {
            if target != node {
                connect(&mut row, node, target, color);
            }
        }
        let left = targets
            .iter()
            .map(|&(target, _, _)| target)
            .min()
            .unwrap_or(node)
            .min(node);
        let right = targets
            .iter()
            .map(|&(target, _, _)| target)
            .max()
            .unwrap_or(node)
            .max(node);
        for &(target, color, existed) in targets {
            if target == node {
                continue;
            }
            let horizontal =
                if target > left { LEFT } else { 0 } | if target < right { RIGHT } else { 0 };
            set(
                &mut row,
                target,
                DOWN | if existed { UP } else { 0 } | horizontal,
                color,
            );
        }
        let continuation = self.lanes.get(node).and_then(|lane| lane.as_ref().ok());
        let horizontal = row
            .get(2 * node)
            .map_or(0, |cell| cell.arms & (LEFT | RIGHT));
        set(
            &mut row,
            node,
            UP | horizontal | if continuation.is_some() { DOWN } else { 0 },
            continuation.map_or(0, |lane| lane.color),
        );
        self.shape.width = self.shape.width.max(self.lanes.len());
        self.shape.rows.push(row);
    }
}

fn vertical<Id>(lanes: &[Result<Lane<Id>, usize>]) -> Row {
    let mut row = vec![Cell::default(); lanes.len().saturating_mul(2).saturating_sub(1)];
    for (index, lane) in lanes.iter().enumerate() {
        if let Ok(lane) = lane {
            set(&mut row, index, UP | DOWN, lane.color);
        }
    }
    row
}

fn set(row: &mut [Cell], lane: usize, arms: u8, color: usize) {
    *row.get_mut(2 * lane)
        .expect("rows span every occupied lane") = Cell::line(arms, color);
}

fn connect(row: &mut [Cell], from: usize, to: usize, color: usize) {
    let left = 2 * from.min(to);
    let right = 2 * from.max(to);
    for (index, cell) in row.iter_mut().enumerate().take(right + 1).skip(left) {
        if index == left {
            cell.arms |= RIGHT;
        } else if index == right {
            cell.arms |= LEFT;
        } else if cell.arms & (UP | DOWN) == 0 {
            *cell = Cell::line(LEFT | RIGHT, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{NonZeroUsize, Shape};
    use crate::graph::Graph;

    fn rows(shape: &Shape) -> Vec<String> {
        shape
            .rows
            .iter()
            .chain([&shape.pad])
            .map(|row| {
                row.iter()
                    .map(|cell| cell.glyph())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect()
    }

    #[test]
    fn linear_roots_and_reused_holes() {
        let mut graph = Graph::new(NonZeroUsize::new(12).unwrap());
        let shape = graph.next(&4, &[3]);
        assert_eq!(rows(shape), ["●", "│"]);
        assert_eq!(shape.text_column(), 3);
        assert_eq!(shape.fixed_rows(), 1);
        assert_eq!(rows(graph.next(&7, &[6])), ["│ ●", "│ │"]);
        assert_eq!(rows(graph.next(&3, &[])), ["● │", "  │"]);
        assert_eq!(rows(graph.next(&9, &[])), ["● │", "  │"]);
        assert_eq!(rows(graph.next(&6, &[])), ["  ●", ""]);
        assert!(graph.lanes.is_empty());
        assert_eq!(rows(graph.next(&5, &[4, 2])), ["●", "├─╮", "│ │"]);
        assert_eq!(rows(graph.next(&4, &[2])), ["● │", "├─╯", "│"]);
        graph.next(&2, &[]);
        assert_eq!(rows(graph.next(&1, &[1])), ["●", "│"]);
    }

    #[test]
    fn fanout_pull_crossings_and_existing_parents() {
        let mut graph = Graph::new(NonZeroUsize::new(12).unwrap());
        assert_eq!(rows(graph.next(&9, &[8, 7, 6])), ["●", "├─┬─╮", "│ │ │"]);
        assert_eq!(
            rows(graph.next(&8, &[6, 5])),
            ["● │ │", "├─│─│─╮", "├─│─╯ │", "│ │   │"]
        );
        assert_eq!(
            rows(graph.next(&7, &[6, 5, 4])),
            ["│ ●   │", "├─┼───┤", "│ │   │"]
        );
        assert_eq!(rows(graph.next(&5, &[6])), ["│ │   ●", "├─│───╯", "│ │"]);
        assert_eq!(rows(graph.next(&4, &[6, 3])), ["│ ●", "├─┤", "│ │"]);
        assert_eq!(rows(graph.next(&6, &[])), ["● │", "  │"]);
        assert_eq!(rows(graph.next(&3, &[])), ["  ●", ""]);
    }

    #[test]
    fn merges_that_open_a_lane_tap_their_first_parent() {
        let mut graph = Graph::new(NonZeroUsize::new(12).unwrap());
        graph.next(&6, &[1, 5, 4]);
        graph.next(&4, &[3]);
        let shape = graph.next(&5, &[3, 2]);
        assert_eq!(rows(shape), ["│ ● │", "│ ├─┤", "│ │ │"]);
        assert_eq!(shape.pad[2].color, 4);
        assert_eq!(shape.pad[4].color, 2);
        assert_eq!(rows(graph.next(&2, &[1])), ["│ ● │", "├─╯ │", "│   │"]);
        assert_eq!(rows(graph.next(&3, &[1])), ["│   ●", "├───╯", "│"]);
    }

    #[test]
    fn first_parents_on_the_right_are_tapped_only_when_a_lane_opens() {
        let mut graph = Graph::new(NonZeroUsize::new(12).unwrap());
        graph.next(&10, &[1, 9, 8, 7]);
        assert_eq!(
            rows(graph.next(&9, &[7, 8, 6])),
            ["│ ● │ │", "│ ├─┼─┤", "│ │ │ │"]
        );
        assert_eq!(rows(graph.next(&6, &[7])), ["│ ● │ │", "│ ├─│─╯", "│ │ │"]);
        assert_eq!(
            rows(graph.next(&7, &[8, 1])),
            ["│ ● │", "├─┤ │", "│ ├─╯", "│ │"]
        );
    }

    #[test]
    fn compaction_waits_sixteen_commits_and_preserves_the_moving_color() {
        let mut graph = Graph::new(NonZeroUsize::new(12).unwrap());
        graph.next(&10, &[20, 8, 7, 6]);
        let shape = graph.next(&8, &[]);
        assert_eq!(rows(shape), ["│ ● │ │", "│   │ │"]);
        let shape = graph.next(&7, &[5, 20]);
        assert_eq!(rows(shape), ["│   ● │", "├───┤ │", "│   │ │"]);
        for id in 20..34 {
            assert_eq!(rows(graph.next(&id, &[id + 1])), ["●   │ │", "│   │ │"]);
        }
        let shape = graph.next(&34, &[35]);
        assert_eq!(rows(shape), ["●   │ │", "│ ╭─│─╯", "│ │ │"]);
        assert_eq!(shape.fixed_rows(), 2);
        assert_eq!(shape.text_column(), 9);
        assert_eq!(shape.rows[1][2].color, 3);
        assert_eq!(shape.rows[1][4].color, 4);
        assert_eq!(shape.rows[1][6].color, 3);
        assert_eq!(shape.pad[2].color, 3);
    }

    #[test]
    fn compaction_follows_fanout_and_pull_rows() {
        let mut graph = Graph::new(NonZeroUsize::new(12).unwrap());
        graph.next(&100, &[20, 60, 70, 80]);
        graph.next(&60, &[]);
        for id in 20..35 {
            graph.next(&id, &[id + 1]);
        }
        assert_eq!(
            rows(graph.next(&70, &[80, 35])),
            ["│   ● │", "├───┤ │", "│   ├─╯", "│ ╭─╯", "│ │"]
        );
    }

    #[test]
    fn compaction_skips_newer_holes() {
        let mut graph = Graph::new(NonZeroUsize::new(12).unwrap());
        graph.next(&100, &[10, 2, 3, 4, 5]);
        graph.next(&4, &[]);
        for id in 10..25 {
            graph.next(&id, &[id + 1]);
        }
        assert_eq!(
            rows(graph.next(&25, &[2])),
            ["● │ │   │", "├─╯ │   │", "│   │ ╭─╯", "│   │ │"]
        );
    }

    #[test]
    fn color_indices_cover_large_configured_palettes() {
        let mut graph = Graph::new(NonZeroUsize::new(70_000).unwrap());
        graph.counter = 65_534;
        let shape = graph.next(&3, &[2, 1]);
        assert_eq!(shape.pad[0].color, 65_535);
        assert_eq!(shape.pad[2].color, 65_536);
    }

    #[test]
    fn duplicate_parents_and_counter_wrap() {
        let mut graph = Graph::new(NonZeroUsize::new(2).unwrap());
        let shape = graph.next(&5, &[4, 3, 3]);
        assert_eq!(rows(shape), ["●", "├─╮", "│ │"]);
        assert_eq!(shape.pad[0].color, 0);
        assert_eq!(shape.pad[2].color, 1);
        let shape = graph.next(&4, &[2]);
        assert_eq!(shape.pad[0].color, 1);
        let shape = graph.next(&3, &[2, 1]);
        assert_eq!(rows(shape), ["│ ●", "├─┤", "│ │"]);
        assert_eq!(shape.pad[0].color, 1);
        assert_eq!(shape.pad[2].color, 1);
        let shape = graph.next(&1, &[2]);
        assert_eq!(rows(shape), ["│ ●", "├─╯", "│"]);
        assert_eq!(shape.pad[0].color, 1);
        let shape = graph.next(&2, &[]);
        assert_eq!(rows(shape), ["●", ""]);
    }
}
