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

use crate::color::Sgr;

use super::{Shape, cell::Cell};

#[derive(Default)]
struct Painted {
    full: Vec<u8>,
    trimmed: Vec<u8>,
}

#[derive(Default)]
pub struct Prefixes {
    rows: Vec<Painted>,
    pad: Painted,
    next: usize,
}

impl Prefixes {
    pub fn paint(&mut self, shape: &Shape, palette: Option<&[Sgr]>) {
        self.rows.clear();
        self.rows.reserve(shape.fixed_rows());
        self.rows.extend(
            shape
                .rows
                .iter()
                .map(|row| paint(row, shape.text_column(), palette)),
        );
        self.pad = paint(&shape.pad, shape.text_column(), palette);
        self.next = 0;
    }

    pub fn next_line(&mut self, text_empty: bool) -> &[u8] {
        let row = self.rows.get(self.next).unwrap_or(&self.pad);
        self.next = (self.next + 1).min(self.rows.len());
        if text_empty { &row.trimmed } else { &row.full }
    }

    pub fn leftover(&mut self) -> impl Iterator<Item = &[u8]> {
        let next = std::mem::replace(&mut self.next, self.rows.len());
        self.rows
            .iter()
            .skip(next)
            .map(|row| row.trimmed.as_slice())
    }

    pub fn separator(&self) -> &[u8] {
        &self.pad.trimmed
    }
}

fn paint(row: &[Cell], column: usize, palette: Option<&[Sgr]>) -> Painted {
    let length = row
        .iter()
        .rposition(|cell| cell.glyph() != ' ')
        .map_or(0, |index| index + 1);
    let mut bytes = Vec::new();
    let mut current = None;
    for cell in row.iter().take(length) {
        let color = if cell.node || cell.arms == 0 {
            None
        } else {
            palette
                .and_then(|palette| palette.get(usize::from(cell.color)))
                .filter(|sgr| !sgr.0.is_empty())
        };
        if color != current {
            if current.is_some() {
                bytes.extend_from_slice(b"\x1b[m");
            }
            if let Some(color) = color {
                bytes.extend_from_slice(&color.0);
            }
            current = color;
        }
        bytes.extend_from_slice(cell.glyph().encode_utf8(&mut [0; 4]).as_bytes());
    }
    if current.is_some() {
        bytes.extend_from_slice(b"\x1b[m");
    }
    let trimmed = bytes.clone();
    bytes.resize(bytes.len() + column.saturating_sub(length), b' ');
    Painted {
        full: bytes,
        trimmed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Graph;
    use std::num::NonZeroU16;

    #[test]
    fn fixed_rows_padding_blank_lines_and_leftovers() {
        let mut graph = Graph::new(NonZeroU16::MIN);
        let mut prefixes = Prefixes::default();
        let shape = graph.next(&9, &[8, 7, 6]);
        prefixes.paint(shape, None);
        assert_eq!(prefixes.next_line(false), "●      ".as_bytes());
        assert_eq!(prefixes.next_line(true), "├─┬─╮".as_bytes());
        for _ in 0..3 {
            assert_eq!(prefixes.next_line(false), "│ │ │  ".as_bytes());
        }
        assert_eq!(prefixes.next_line(true), "│ │ │".as_bytes());
        assert_eq!(prefixes.separator(), "│ │ │".as_bytes());
        assert_eq!(prefixes.leftover().count(), 0);
        prefixes.paint(shape, None);
        assert_eq!(
            prefixes.leftover().collect::<Vec<_>>(),
            ["●".as_bytes(), "├─┬─╮".as_bytes()]
        );
        assert_eq!(prefixes.leftover().count(), 0);
        prefixes.paint(
            &Shape {
                width: 1,
                ..Shape::default()
            },
            None,
        );
        assert_eq!(prefixes.next_line(false), b"   ");
        assert_eq!(prefixes.next_line(true), b"");
        assert_eq!(prefixes.separator(), b"");
    }

    #[test]
    fn colors_merge_runs_and_reset_before_text() {
        let colors = [
            Sgr(b"\x1b[31m".to_vec()),
            Sgr(b"\x1b[32m".to_vec()),
            Sgr::default(),
        ];
        let row = vec![
            Cell::line(3, 0),
            Cell::line(12, 1),
            Cell::line(6, 1),
            Cell::default(),
            Cell {
                node: true,
                ..Cell::default()
            },
            Cell::line(3, 2),
            Cell::line(3, 3),
            Cell::default(),
        ];
        let result = paint(&row, 9, Some(&colors));
        assert_eq!(
            result.full,
            "\x1b[31m│\x1b[m\x1b[32m─╮\x1b[m ●││  ".as_bytes()
        );
        assert_eq!(
            result.trimmed,
            "\x1b[31m│\x1b[m\x1b[32m─╮\x1b[m ●││".as_bytes()
        );
        assert_eq!(
            paint(&row[..1], 3, Some(&colors)).full,
            "\x1b[31m│\x1b[m  ".as_bytes()
        );
        assert_eq!(paint(&row[..1], 3, Some(&[])).full, "│  ".as_bytes());
    }
}
