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

use std::num::NonZeroU16;

use super::{
    Row, Shape,
    cell::{Cell, DOWN, LEFT, RIGHT, UP},
};

#[derive(Clone)]
struct Lane<Id> {
    target: Id,
    color: u16,
}

pub struct Graph<Id> {
    lanes: Vec<Option<Lane<Id>>>,
    ncolors: NonZeroU16,
    counter: u16,
    shape: Shape,
}

impl<Id: Clone + Eq> Graph<Id> {
    pub fn new(ncolors: NonZeroU16) -> Self {
        Self {
            lanes: Vec::new(),
            counter: ncolors.get() - 1,
            ncolors,
            shape: Shape::default(),
        }
    }

    fn find(&self, id: &Id) -> Option<usize> {
        self.lanes
            .iter()
            .position(|lane| lane.as_ref().is_some_and(|lane| lane.target == *id))
    }

    fn place(&mut self, start: usize, target: Id, color: u16) -> usize {
        let lane = Some(Lane { target, color });
        if let Some((index, slot)) = self
            .lanes
            .iter_mut()
            .enumerate()
            .skip(start)
            .find(|(_, slot)| slot.is_none())
        {
            *slot = lane;
            index
        } else {
            let index = self.lanes.len();
            self.lanes.push(lane);
            index
        }
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
                .and_then(Option::as_ref)
                .map_or(self.counter, |lane| lane.color);
            assignments.push((parent.clone(), position, color));
        }
        *self
            .lanes
            .get_mut(node)
            .expect("the node lane is allocated") = None;
        let mut targets = Vec::new();
        let mut pull = None;
        for (index, (parent, position, color)) in assignments.into_iter().enumerate() {
            let target = if index == 0 && position.is_none_or(|position| position >= node) {
                if let Some(position) = position.filter(|&position| position > node) {
                    pull = Some((position, color));
                }
                *self
                    .lanes
                    .get_mut(node)
                    .expect("the node lane is allocated") = Some(Lane {
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
            *self
                .lanes
                .get_mut(source)
                .expect("a pull starts at an occupied lane") = None;
        }
        while self.lanes.last().is_some_and(Option::is_none) {
            self.lanes.pop();
        }
        self.shape.width = self.shape.width.max(self.lanes.len());
        self.shape.pad = vertical(&self.lanes);
        &self.shape
    }

    fn fanout(&mut self, node: usize, incoming: &[Cell], targets: &[(usize, u16, bool)]) {
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
        let continuation = self.lanes.get(node).and_then(Option::as_ref);
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

fn vertical<Id>(lanes: &[Option<Lane<Id>>]) -> Row {
    let mut row = vec![Cell::default(); lanes.len().saturating_mul(2).saturating_sub(1)];
    for (index, lane) in lanes.iter().enumerate() {
        if let Some(lane) = lane {
            set(&mut row, index, UP | DOWN, lane.color);
        }
    }
    row
}

fn set(row: &mut [Cell], lane: usize, arms: u8, color: u16) {
    *row.get_mut(2 * lane)
        .expect("rows span every occupied lane") = Cell::line(arms, color);
}

fn connect(row: &mut [Cell], from: usize, to: usize, color: u16) {
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
    use super::{NonZeroU16, Shape};
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
        let mut graph = Graph::new(NonZeroU16::new(12).unwrap());
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
        let mut graph = Graph::new(NonZeroU16::new(12).unwrap());
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
    fn duplicate_parents_and_counter_wrap() {
        let mut graph = Graph::new(NonZeroU16::new(2).unwrap());
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
