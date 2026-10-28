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

use unicode_width::UnicodeWidthChar;

pub fn character(character: char) -> Option<usize> {
    character.width()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn character_categories() {
        for (input, expected) in [
            ('A', Some(1)),
            ('界', Some(2)),
            ('\u{ad}', Some(0)),
            ('\u{5c8}', Some(0)),
            ('\u{200d}', Some(0)),
            ('\u{18e00}', Some(2)),
            ('\u{1faf9}', Some(2)),
            ('\x1b', None),
        ] {
            assert_eq!(character(input), expected, "{input:?}");
        }
    }
}
