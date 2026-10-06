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

use super::width;

pub fn lines(bytes: &[u8]) -> Vec<Vec<u8>> {
    let lines: Vec<_> = bytes
        .split(|&byte| byte == b'\n')
        .map(|line| {
            let end = line
                .iter()
                .rposition(|byte| !matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
                .map_or(0, |index| index + 1);
            line.get(..end).unwrap_or_default()
        })
        .collect();
    let Some(start) = lines.iter().position(|line| !line.is_empty()) else {
        return Vec::new();
    };
    let end = lines
        .iter()
        .rposition(|line| !line.is_empty())
        .map_or(start, |index| index + 1);
    lines
        .get(start..end)
        .unwrap_or_default()
        .iter()
        .map(|line| {
            if line.is_empty() {
                return Vec::new();
            }
            let mut out = b"    ".to_vec();
            expand(line, &mut out);
            out
        })
        .collect()
}

fn expand(line: &[u8], out: &mut Vec<u8>) {
    let mut column = 0;
    let mut offset = 0;
    for chunk in line.utf8_chunks() {
        for character in chunk.valid().chars() {
            if character == '\t' {
                let spaces = 8 - column % 8;
                out.resize(out.len() + spaces, b' ');
                column += spaces;
            } else if let Some(width) = width::character(character) {
                let mut buffer = [0; 4];
                out.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
                column += width;
            } else {
                out.extend_from_slice(line.get(offset..).unwrap_or_default());
                return;
            }
            offset += character.len_utf8();
        }
        if !chunk.invalid().is_empty() {
            out.extend_from_slice(line.get(offset..).unwrap_or_default());
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_lines_trimming_and_indentation() {
        assert_eq!(lines(b"\n \t\r\n"), Vec::<Vec<u8>>::new());
        assert_eq!(
            lines(b"\n \t\nsubject \t\r\n\nbody\n\n"),
            [b"    subject".to_vec(), vec![], b"    body".to_vec()]
        );
        assert_eq!(
            lines(b"no final newline"),
            [b"    no final newline".to_vec()]
        );
    }

    #[test]
    fn tabs_use_message_columns_and_stop_at_invalid_text() {
        for (input, expected) in [
            (b"\tx\ty".as_slice(), b"            x       y".as_slice()),
            ("界\tx".as_bytes(), "    界      x".as_bytes()),
            ("\u{ad}\tx".as_bytes(), "    \u{ad}       x".as_bytes()),
            (
                "a\u{0301}\tx".as_bytes(),
                "    a\u{0301}       x".as_bytes(),
            ),
            (b"a\xff\tx", b"    a\xff\tx"),
            (b"a\x1b[1m\tx", b"    a\x1b[1m\tx"),
            (b"a\x07\tx", b"    a\x07\tx"),
            (b"a\r\tx", b"    a\r\tx"),
            (b"12345678\tx", b"    12345678        x"),
        ] {
            assert_eq!(lines(input), [expected.to_vec()]);
        }
    }
}
