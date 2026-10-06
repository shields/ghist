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

mod cell;
mod layout;
mod paint;

pub use layout::Graph;
pub use paint::Prefixes;

use cell::Cell;

pub type Row = Vec<Cell>;

#[derive(Debug, Default, Eq, PartialEq)]
pub struct Shape {
    pub rows: Vec<Row>,
    pub pad: Row,
    pub width: usize,
}

impl Shape {
    pub const fn text_column(&self) -> usize {
        2 * self.width + 1
    }

    pub const fn fixed_rows(&self) -> usize {
        self.rows.len()
    }
}
