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
    // Git's character-width rules differ from unicode-width's grapheme rules.
    match character {
        '\u{600}'..='\u{604}'
        | '\u{6dd}'
        | '\u{2d7f}'
        | '\u{fff9}'..='\u{fffb}'
        | '\u{110bd}'
        | '\u{110cd}'
        | '\u{13430}'..='\u{1343f}' => Some(0),
        '\u{ad}'
        | '\u{9be}'
        | '\u{9d7}'
        | '\u{b3e}'
        | '\u{b57}'
        | '\u{bbe}'
        | '\u{bd7}'
        | '\u{cc0}'
        | '\u{cc2}'
        | '\u{cc7}'..='\u{cc8}'
        | '\u{cca}'..='\u{ccb}'
        | '\u{cd5}'..='\u{cd6}'
        | '\u{d3e}'
        | '\u{d4e}'
        | '\u{d57}'
        | '\u{dcf}'
        | '\u{ddf}'
        | '\u{1715}'
        | '\u{1734}'
        | '\u{17a4}'
        | '\u{17d8}'
        | '\u{1b35}'
        | '\u{1b3b}'
        | '\u{1b3d}'
        | '\u{1b43}'..='\u{1b44}'
        | '\u{1baa}'
        | '\u{1bf2}'..='\u{1bf3}'
        | '\u{2065}'
        | '\u{a8fa}'
        | '\u{a953}'
        | '\u{a9c0}'
        | '\u{d7b0}'..='\u{d7c6}'
        | '\u{d7cb}'..='\u{d7fb}'
        | '\u{ff9e}'..='\u{ffa0}'
        | '\u{fff0}'..='\u{fff8}'
        | '\u{111c0}'
        | '\u{111c2}'..='\u{111c3}'
        | '\u{11235}'
        | '\u{1133e}'
        | '\u{1134d}'
        | '\u{11357}'
        | '\u{113b8}'
        | '\u{113c2}'
        | '\u{113c5}'
        | '\u{113c7}'..='\u{113c9}'
        | '\u{113cf}'
        | '\u{113d1}'
        | '\u{114b0}'
        | '\u{114bd}'
        | '\u{115af}'
        | '\u{116b6}'
        | '\u{11930}'
        | '\u{1193d}'
        | '\u{1193f}'
        | '\u{11941}'
        | '\u{11a84}'..='\u{11a89}'
        | '\u{11d46}'
        | '\u{11f02}'
        | '\u{11f41}'
        | '\u{1d165}'..='\u{1d166}'
        | '\u{1d16d}'..='\u{1d172}'
        | '\u{e0000}'
        | '\u{e0002}'..='\u{e001f}'
        | '\u{e0080}'..='\u{e00ff}'
        | '\u{e01f0}'..='\u{e0fff}' => Some(1),
        '\u{302e}'..='\u{302f}' | '\u{3164}' | '\u{16ff0}'..='\u{16ff1}' => Some(2),
        '\u{fffe}'..='\u{ffff}' => None,
        _ => character.width(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn character_categories_and_git_compatibility() {
        for (input, expected) in [
            ('A', Some(1)),
            ('界', Some(2)),
            ('\u{ad}', Some(1)),
            ('\u{600}', Some(0)),
            ('\u{9be}', Some(1)),
            ('\u{17d8}', Some(1)),
            ('\u{200d}', Some(0)),
            ('\u{302e}', Some(2)),
            ('\u{fffe}', None),
            ('\x1b', None),
        ] {
            assert_eq!(character(input), expected, "{input:?}");
        }
    }
}
