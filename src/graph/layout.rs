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

const HOLE_AGE: usize = 20;
const BURST: usize = 5;

#[derive(Clone)]
struct Lane<Id> {
    target: Id,
    color: usize,
    family: usize,
    origin: Option<usize>,
    opened: [Option<usize>; 2],
}

pub struct Graph<Id> {
    lanes: Vec<Result<Lane<Id>, usize>>,
    tick: usize,
    families: usize,
    ncolors: NonZeroUsize,
    counter: usize,
    shape: Shape,
}

impl<Id: Clone + Eq> Graph<Id> {
    pub fn new(ncolors: NonZeroUsize) -> Self {
        Self {
            lanes: Vec::new(),
            tick: 0,
            families: 0,
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

    fn place(&mut self, start: usize, target: Id, color: usize, origin: Option<usize>) -> usize {
        self.families += 1;
        let lane = Ok(Lane {
            target,
            color,
            family: self.families,
            origin,
            opened: [None; 2],
        });
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
        let node = existing.unwrap_or_else(|| self.place(0, id.clone(), self.counter, None));
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
        let mut assignments = Vec::new();
        for parent in shown {
            if existing.is_none() || shown.len() > 1 {
                self.counter = (self.counter + 1) % self.ncolors.get();
            }
            if assignments
                .iter()
                .any(|(assigned, _, _)| assigned == parent)
            {
                continue;
            }
            let position = self.find(parent);
            let color = position.map_or(self.counter, |index| occupied(&self.lanes, index).color);
            assignments.push((parent.clone(), position, color));
        }
        let merge = assignments.len() > 1;
        let lane = self.vacate(node);
        let first = assignments
            .first()
            .and_then(|&(_, position, _)| position)
            .filter(|&position| position != node);
        let fresh = assignments
            .iter()
            .skip(1)
            .any(|(_, position, _)| position.is_none());
        let (survivor, family) = first.map_or((node, lane.family), |position| {
            self.join(&lane, node, position, merge, fresh)
        });
        let mut targets = Vec::new();
        let mut pull = None;
        for (index, (parent, position, color)) in assignments.into_iter().enumerate() {
            let target = if index == 0 && survivor == node {
                let continuation = if let Some(position) = first {
                    pull = Some((position, color));
                    occupied(&self.lanes, position).clone()
                } else {
                    Lane {
                        target: parent,
                        color,
                        ..lane
                    }
                };
                *self
                    .lanes
                    .get_mut(node)
                    .expect("the node lane is allocated") = Ok(continuation);
                node
            } else {
                position.unwrap_or_else(|| self.place(node, parent, color, Some(family)))
            };
            targets.push((target, color, position.is_some()));
        }
        if fresh {
            let opener = occupied_mut(&mut self.lanes, survivor);
            opener.opened = [Some(self.tick), opener.opened[0]];
        }
        if targets.iter().any(|&(target, _, _)| target != node) {
            self.fanout(node, &incoming, &targets);
        }
        if let Some((source, color)) = pull {
            let mut row = vertical(&self.lanes);
            set(&mut row, source, UP, color);
            connect(&mut row, node, source, color);
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

    fn join(
        &mut self,
        lane: &Lane<Id>,
        node: usize,
        position: usize,
        merge: bool,
        fresh: bool,
    ) -> (usize, usize) {
        let survivor = if node == 0 {
            node
        } else if fresh {
            position
        } else if !merge && self.keeps_outer(lane, node, position) {
            node.max(position)
        } else {
            node.min(position)
        };
        let other = occupied_mut(&mut self.lanes, position);
        if node == 0 || other.origin == Some(lane.family) {
            other.family = lane.family;
            other.origin = lane.origin;
            other.opened = lane.opened;
        }
        (survivor, other.family)
    }

    fn keeps_outer(&self, lane: &Lane<Id>, node: usize, position: usize) -> bool {
        let other = occupied(&self.lanes, position);
        let Some(origin) = lane.origin.filter(|&origin| other.origin == Some(origin)) else {
            return false;
        };
        let inner = node.min(position);
        let outer = node.max(position);
        let bursting = self
            .lanes
            .iter()
            .take(inner)
            .filter_map(|slot| slot.as_ref().ok())
            .find(|opener| opener.family == origin)
            .and_then(|opener| opener.opened[1])
            .is_some_and(|opened| self.tick - opened <= BURST);
        lane.family > other.family
            && bursting
            && self
                .lanes
                .iter()
                .take(outer)
                .enumerate()
                .all(|(column, slot)| column == node || slot.is_ok())
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
        let &(_, tapped, _) = targets.first().expect("a fan-out has a first parent");
        let (down, color) = continuation.map_or((0, tapped), |lane| (DOWN, lane.color));
        set(&mut row, node, UP | horizontal | down, color);
        row.get_mut(2 * node)
            .expect("the node lane has a cell")
            .node = true;
        self.shape.width = self.shape.width.max(self.lanes.len());
        *self
            .shape
            .rows
            .first_mut()
            .expect("the node row is present") = row;
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

#[track_caller]
fn occupied<Id>(lanes: &[Result<Lane<Id>, usize>], column: usize) -> &Lane<Id> {
    lanes
        .get(column)
        .and_then(|slot| slot.as_ref().ok())
        .expect("the column is occupied")
}

#[track_caller]
fn occupied_mut<Id>(lanes: &mut [Result<Lane<Id>, usize>], column: usize) -> &mut Lane<Id> {
    lanes
        .get_mut(column)
        .and_then(|slot| slot.as_mut().ok())
        .expect("the column is occupied")
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
    use super::{HOLE_AGE, NonZeroUsize, Shape};
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
        assert_eq!(rows(graph.next(&5, &[4, 2])), ["●─╮", "│ │"]);
        assert_eq!(rows(graph.next(&4, &[2])), ["● │", "├─╯", "│"]);
        graph.next(&2, &[]);
        assert_eq!(rows(graph.next(&1, &[1])), ["●", "│"]);
    }

    #[test]
    fn fanout_pull_crossings_and_existing_parents() {
        let mut graph = Graph::new(NonZeroUsize::new(12).unwrap());
        assert_eq!(rows(graph.next(&9, &[8, 7, 6])), ["●─┬─╮", "│ │ │"]);
        assert_eq!(
            rows(graph.next(&8, &[6, 5])),
            ["●─│─│─╮", "├─│─╯ │", "│ │   │"]
        );
        assert_eq!(rows(graph.next(&7, &[6, 5, 4])), ["├─●───┤", "│ │   │"]);
        let shape = graph.next(&5, &[6]);
        assert_eq!(rows(shape), ["├─│───●", "│ │"]);
        assert_eq!(shape.rows[0][6].color, 2);
        assert_eq!(rows(graph.next(&4, &[6, 3])), ["├─●", "│ │"]);
        assert_eq!(rows(graph.next(&6, &[])), ["● │", "  │"]);
        assert_eq!(rows(graph.next(&3, &[])), ["  ●", ""]);
    }

    #[test]
    fn merges_that_open_a_lane_tap_their_first_parent() {
        let mut graph = Graph::new(NonZeroUsize::new(12).unwrap());
        graph.next(&6, &[1, 5, 4]);
        graph.next(&4, &[3]);
        let shape = graph.next(&5, &[3, 2]);
        assert_eq!(rows(shape), ["│ ●─┤", "│ │ │"]);
        assert_eq!(shape.pad[2].color, 4);
        assert_eq!(shape.pad[4].color, 2);
        assert_eq!(rows(graph.next(&2, &[1])), ["├─● │", "│   │"]);
        assert_eq!(rows(graph.next(&3, &[1])), ["├───●", "│"]);
    }

    #[test]
    fn merges_tap_first_parents_on_the_right_only_when_a_lane_opens() {
        let mut graph = Graph::new(NonZeroUsize::new(12).unwrap());
        graph.next(&10, &[1, 9, 8, 7]);
        assert_eq!(rows(graph.next(&9, &[7, 8, 6])), ["│ ●─┼─┤", "│ │ │ │"]);
        assert_eq!(rows(graph.next(&6, &[7])), ["│ ● │ │", "│ ├─│─╯", "│ │ │"]);
        assert_eq!(rows(graph.next(&7, &[8, 1])), ["├─● │", "│ ├─╯", "│ │"]);
    }

    #[test]
    fn tap_corners_take_the_first_parent_color() {
        let mut graph = Graph::new(NonZeroUsize::new(12).unwrap());
        graph.next(&10, &[5, 6, 7, 8, 9]);
        let shape = graph.next(&8, &[6, 9]);
        assert_eq!(rows(shape), ["│ ├─│─●─┤", "│ │ │   │"]);
        assert_eq!(shape.rows[0][6].color, 1);
        let shape = graph.next(&9, &[7, 5]);
        assert_eq!(rows(shape), ["├─│─┼───●", "│ │ │"]);
        assert_eq!(shape.rows[0][8].color, 2);
    }

    #[test]
    fn compaction_waits_twenty_commits_and_preserves_the_moving_color() {
        let mut graph = Graph::new(NonZeroUsize::new(12).unwrap());
        graph.next(&10, &[20, 8, 7, 6]);
        let shape = graph.next(&8, &[]);
        assert_eq!(rows(shape), ["│ ● │ │", "│   │ │"]);
        let shape = graph.next(&7, &[5, 20]);
        assert_eq!(rows(shape), ["├───● │", "│   │ │"]);
        for id in 20..38 {
            assert_eq!(rows(graph.next(&id, &[id + 1])), ["●   │ │", "│   │ │"]);
        }
        let shape = graph.next(&38, &[39]);
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
        for id in 20..19 + HOLE_AGE {
            graph.next(&id, &[id + 1]);
        }
        assert_eq!(
            rows(graph.next(&70, &[80, 19 + HOLE_AGE])),
            ["├───● │", "│   ├─╯", "│ ╭─╯", "│ │"]
        );
    }

    #[test]
    fn compaction_skips_newer_holes() {
        let mut graph = Graph::new(NonZeroUsize::new(12).unwrap());
        graph.next(&100, &[10, 2, 3, 4, 5]);
        graph.next(&4, &[]);
        for id in 10..9 + HOLE_AGE {
            graph.next(&id, &[id + 1]);
        }
        assert_eq!(
            rows(graph.next(&(9 + HOLE_AGE), &[2])),
            ["● │ │   │", "├─╯ │   │", "│   │ ╭─╯", "│   │ │"]
        );
    }

    #[test]
    fn sibling_joins_keep_the_outer_column() {
        let mut graph = Graph::new(NonZeroUsize::new(12).unwrap());
        assert_eq!(rows(graph.next(&9, &[7, 8])), ["●─╮", "│ │"]);
        assert_eq!(rows(graph.next(&8, &[1])), ["│ ●", "│ │"]);
        assert_eq!(rows(graph.next(&7, &[5, 6])), ["●─│─╮", "│ │ │"]);
        let shape = graph.next(&6, &[1]);
        assert_eq!(rows(shape), ["│ │ ●", "│ ╰─┤", "│   │"]);
        assert_eq!(shape.rows[1][2].color, 1);
        assert_eq!(shape.rows[1][4].color, 1);
        assert_eq!(rows(graph.next(&5, &[3, 4])), ["●─╮ │", "│ │ │"]);
        let shape = graph.next(&4, &[1]);
        assert_eq!(rows(shape), ["│ ●─┤", "│   │"]);
        assert_eq!(shape.rows[0][2].color, 1);
        assert_eq!(rows(graph.next(&3, &[1, 2])), ["●─╮ │", "├─│─╯", "│ │"]);
        let shape = graph.next(&2, &[1]);
        assert_eq!(rows(shape), ["├─●", "│"]);
        assert_eq!(shape.rows[0][2].color, 1);
        assert_eq!(rows(graph.next(&1, &[])), ["●", ""]);
    }

    #[test]
    fn sibling_merges_keep_the_inner_column() {
        let mut graph = Graph::new(NonZeroUsize::new(12).unwrap());
        graph.next(&9, &[7, 8]);
        graph.next(&8, &[1]);
        graph.next(&7, &[5, 6]);
        assert_eq!(rows(graph.next(&6, &[1, 5])), ["├─┼─●", "│ │"]);
    }

    #[test]
    fn column_zero_joins_take_the_mainline_family() {
        let mut graph = Graph::new(NonZeroUsize::new(12).unwrap());
        graph.next(&100, &[90, 10, 20]);
        graph.next(&20, &[21, 30]);
        assert_eq!(
            rows(graph.next(&90, &[30])),
            ["● │ │ │", "├─│─│─╯", "│ │ │"]
        );
        graph.next(&30, &[31, 40]);
        graph.next(&10, &[11]);
        assert_eq!(
            rows(graph.next(&40, &[11])),
            ["│ │ │ ●", "│ ╰─│─┤", "│   │ │"]
        );
    }

    #[test]
    fn lanes_rejoining_their_opener_take_its_family() {
        let mut graph = Graph::new(NonZeroUsize::new(12).unwrap());
        graph.next(&100, &[99, 10, 20]);
        graph.next(&10, &[11, 30]);
        graph.next(&30, &[12]);
        assert_eq!(
            rows(graph.next(&11, &[12])),
            ["│ ● │ │", "│ ├─│─╯", "│ │ │"]
        );
        graph.next(&99, &[98, 50]);
        assert_eq!(
            rows(graph.next(&20, &[12])),
            ["│ │ ● │", "│ ╰─┤ │", "│   │ │"]
        );
    }

    #[test]
    fn tapped_lanes_rejoining_their_opener_take_its_family() {
        let mut graph = Graph::new(NonZeroUsize::new(12).unwrap());
        graph.next(&100, &[99, 10]);
        graph.next(&10, &[11, 20, 30]);
        assert_eq!(rows(graph.next(&11, &[20, 40])), ["│ ●─┤ │", "│ │ │ │"]);
        graph.next(&99, &[98, 50]);
        assert_eq!(
            rows(graph.next(&50, &[20])),
            ["│ │ │ │ ●", "│ │ ╰─│─┤", "│ │   │ │"]
        );
    }

    #[test]
    fn other_pulled_lanes_keep_their_family() {
        let mut graph = Graph::new(NonZeroUsize::new(12).unwrap());
        graph.next(&100, &[99, 10, 20]);
        graph.next(&20, &[21, 30]);
        graph.next(&99, &[98, 15]);
        assert_eq!(
            rows(graph.next(&10, &[30])),
            ["│ ● │ │ │", "│ ├─│─╯ │", "│ │ │   │"]
        );
        assert_eq!(rows(graph.next(&21, &[30])), ["│ ├─●   │", "│ │     │"]);
    }

    #[test]
    fn merge_taps_open_lanes_from_the_tapped_family() {
        let mut graph = Graph::new(NonZeroUsize::new(12).unwrap());
        graph.next(&100, &[90, 10, 20]);
        graph.next(&10, &[11, 50]);
        assert_eq!(rows(graph.next(&20, &[11, 60])), ["│ ├─● │", "│ │ │ │"]);
        graph.next(&50, &[70]);
        assert_eq!(rows(graph.next(&60, &[70])), ["│ │ ●─┤", "│ │   │"]);
    }

    #[test]
    fn older_siblings_keep_the_inner_column() {
        let mut graph = Graph::new(NonZeroUsize::new(12).unwrap());
        graph.next(&14, &[10, 13]);
        graph.next(&13, &[12]);
        graph.next(&12, &[11, 10]);
        graph.next(&10, &[7, 9]);
        graph.next(&7, &[4, 6]);
        graph.next(&4, &[1, 3]);
        assert_eq!(
            rows(graph.next(&11, &[9])),
            ["│ ● │ │ │", "│ ├─╯ │ │", "│ │   │ │"]
        );
        graph.next(&9, &[8]);
        assert_eq!(
            rows(graph.next(&8, &[6])),
            ["│ ●   │ │", "│ ├───╯ │", "│ │     │"]
        );
        graph.next(&6, &[5]);
        assert_eq!(
            rows(graph.next(&5, &[3])),
            ["│ ●     │", "│ ├─────╯", "│ │"]
        );
    }

    #[test]
    fn siblings_straddling_their_opener_keep_the_inner_column() {
        let mut graph = Graph::new(NonZeroUsize::new(12).unwrap());
        graph.next(&100, &[99, 10]);
        graph.next(&10, &[11, 30]);
        graph.next(&11, &[30, 40]);
        graph.next(&30, &[31, 50]);
        graph.next(&99, &[98, 70]);
        assert_eq!(rows(graph.next(&50, &[40])), ["│ ├─│─● │", "│ │ │   │"]);
        assert_eq!(rows(graph.next(&31, &[32, 60])), ["│ │ ●─╮ │", "│ │ │ │ │"]);
    }

    #[test]
    fn siblings_beside_another_hole_keep_the_inner_column() {
        let mut graph = Graph::new(NonZeroUsize::new(12).unwrap());
        graph.next(&100, &[99, 10, 20]);
        graph.next(&10, &[]);
        graph.next(&20, &[21, 30]);
        graph.next(&21, &[22, 40]);
        graph.next(&30, &[5]);
        assert_eq!(rows(graph.next(&40, &[5])), ["│   │ ├─●", "│   │ │"]);
        let mut graph = Graph::new(NonZeroUsize::new(12).unwrap());
        graph.next(&100, &[90, 10, 20, 30]);
        graph.next(&90, &[80, 40]);
        graph.next(&20, &[]);
        assert_eq!(rows(graph.next(&30, &[10])), ["│ ├───● │", "│ │     │"]);
    }

    #[test]
    fn siblings_keep_the_outer_column_only_during_a_burst_of_branches() {
        for (linear, joined) in [
            (2, vec!["│ │ ●", "│ ╰─┤", "│   │"]),
            (3, vec!["│ ├─●", "│ │"]),
        ] {
            let mut graph = Graph::new(NonZeroUsize::new(12).unwrap());
            graph.next(&9, &[7, 8]);
            graph.next(&8, &[1]);
            graph.next(&7, &[5, 6]);
            for id in (5 - linear..5).rev() {
                graph.next(&(id + 1), &[id]);
            }
            assert_eq!(rows(graph.next(&6, &[1])), joined);
        }
        let mut graph = Graph::new(NonZeroUsize::new(12).unwrap());
        graph.next(&9, &[7, 8]);
        graph.next(&8, &[1]);
        graph.next(&7, &[5, 6]);
        graph.next(&5, &[4]);
        graph.next(&4, &[3]);
        graph.next(&3, &[2, 10]);
        assert_eq!(
            rows(graph.next(&6, &[1])),
            ["│ │ ● │", "│ ╰─┤ │", "│   │ │"]
        );
        let mut graph = Graph::new(NonZeroUsize::new(12).unwrap());
        graph.next(&10, &[13, 12, 11]);
        graph.next(&12, &[1]);
        assert_eq!(rows(graph.next(&11, &[1])), ["│ ├─●", "│ │"]);
    }

    #[test]
    fn separate_tips_are_not_siblings() {
        let mut graph = Graph::new(NonZeroUsize::new(12).unwrap());
        graph.next(&9, &[8, 7]);
        graph.next(&8, &[1, 6]);
        graph.next(&4, &[2]);
        assert_eq!(rows(graph.next(&3, &[2])), ["│ │ │ ├─●", "│ │ │ │"]);
    }

    #[test]
    fn lanes_opened_by_different_families_are_not_siblings() {
        let mut graph = Graph::new(NonZeroUsize::new(12).unwrap());
        graph.next(&100, &[90, 10]);
        graph.next(&10, &[11, 20]);
        graph.next(&11, &[12, 21]);
        graph.next(&90, &[80, 30]);
        assert_eq!(rows(graph.next(&30, &[20])), ["│ │ ├─│─●", "│ │ │ │"]);
    }

    #[test]
    fn siblings_compacted_into_column_zero_are_tapped() {
        let mut graph = Graph::new(NonZeroUsize::new(12).unwrap());
        graph.next(&100, &[90, 10]);
        graph.next(&90, &[]);
        graph.next(&10, &[11, 500]);
        for id in 11..9 + HOLE_AGE {
            graph.next(&id, &[id + 1]);
        }
        assert_eq!(
            rows(graph.next(&(9 + HOLE_AGE), &[10 + HOLE_AGE])),
            ["  ● │", "╭─│─╯", "│ │"]
        );
        graph.next(&(10 + HOLE_AGE), &[11 + HOLE_AGE, 600]);
        graph.next(&(11 + HOLE_AGE), &[12 + HOLE_AGE, 700]);
        assert_eq!(rows(graph.next(&600, &[500])), ["├─│─● │", "│ │   │"]);
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
        let mut graph = Graph::new(NonZeroUsize::new(4).unwrap());
        let shape = graph.next(&5, &[4, 3, 3]);
        assert_eq!(rows(shape), ["●─╮", "│ │"]);
        assert_eq!(shape.pad[0].color, 0);
        assert_eq!(shape.pad[2].color, 1);
        let shape = graph.next(&4, &[2, 2]);
        assert_eq!(rows(shape), ["● │", "│ │"]);
        assert_eq!(shape.pad[0].color, 3);
        let shape = graph.next(&3, &[2, 1]);
        assert_eq!(rows(shape), ["├─●", "│ │"]);
        assert_eq!(shape.pad[0].color, 3);
        assert_eq!(shape.pad[2].color, 2);
        let shape = graph.next(&1, &[2]);
        assert_eq!(rows(shape), ["├─●", "│"]);
        assert_eq!(shape.pad[0].color, 3);
        let shape = graph.next(&2, &[]);
        assert_eq!(rows(shape), ["●", ""]);
        let shape = graph.next(&8, &[7, 7, 6]);
        assert_eq!(rows(shape), ["●─╮", "│ │"]);
        assert_eq!(shape.pad[0].color, 3);
        assert_eq!(shape.pad[2].color, 1);
        let mut graph = Graph::new(NonZeroUsize::new(12).unwrap());
        graph.next(&9, &[7, 8]);
        graph.next(&8, &[1]);
        graph.next(&7, &[5, 6]);
        assert_eq!(rows(graph.next(&6, &[1, 1])), ["│ │ ●", "│ ╰─┤", "│   │"]);
    }
}
