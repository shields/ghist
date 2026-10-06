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

pub const UP: u8 = 1;
pub const DOWN: u8 = 2;
pub const LEFT: u8 = 4;
pub const RIGHT: u8 = 8;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Cell {
    pub arms: u8,
    pub color: usize,
    pub node: bool,
}

impl Cell {
    pub const fn line(arms: u8, color: usize) -> Self {
        Self {
            arms,
            color,
            node: false,
        }
    }

    pub const fn glyph(self) -> char {
        if self.node {
            return '●';
        }
        match self.arms {
            0 => ' ',
            1..=3 => '│',
            4 | 8 | 12 => '─',
            5 => '╯',
            6 => '╮',
            7 => '┤',
            9 => '╰',
            10 => '╭',
            11 => '├',
            13 => '┴',
            14 => '┬',
            _ => '┼',
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_arm_set_and_the_node() {
        let glyphs: String = (0..16).map(|arms| Cell::line(arms, 0).glyph()).collect();
        assert_eq!(glyphs, " │││─╯╮┤─╰╭├─┴┬┼");
        assert_eq!(
            Cell {
                node: true,
                ..Cell::default()
            }
            .glyph(),
            '●'
        );
    }
}
