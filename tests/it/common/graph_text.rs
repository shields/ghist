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

pub fn strip(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut column = usize::MAX;
    let mut output = Vec::new();
    for line in bytes.split_inclusive(|&byte| byte == b'\n') {
        let plain = super::ansi::strip(line);
        let mut rest = plain.as_slice();
        let mut cells = 0;
        let mut node = false;
        while let Some((glyph, next)) = cell(rest) {
            node |= glyph == '●' && cells < column;
            cells += 1;
            rest = next;
        }
        if node && (rest.starts_with(b"sha1 ") || rest.starts_with(b"sha256 ")) {
            column = cells;
        }
        if column == usize::MAX {
            output.extend_from_slice(line);
            continue;
        }
        rest = line;
        for _ in 0..column {
            while let Some(next) = super::ansi::prefix(rest) {
                rest = next;
            }
            if rest == b"\n" || rest.is_empty() {
                break;
            }
            rest = cell(rest)
                .ok_or_else(|| format!("invalid graph prefix: {line:?}"))?
                .1;
        }
        output.extend_from_slice(rest);
    }
    Ok(output)
}

pub fn cell(bytes: &[u8]) -> Option<(char, &[u8])> {
    if let Some(tail) = bytes.strip_prefix(b" ") {
        return Some((' ', tail));
    }
    let glyph = std::str::from_utf8(bytes.get(..3)?).ok()?.chars().next()?;
    "●│─╭╮╯╰├┤┬┴┼"
        .contains(glyph)
        .then(|| (glyph, bytes.get(3..).unwrap_or_default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_only_the_graph_column() {
        let input = "●      sha1 abc\n├─┬─╮  Merge: a b c\n│ │ │  Author: A\n│ │ │\n│ │ │      ● sha1 message\n│ │ │      ".as_bytes();
        let input = [
            input,
            b"\xff\n",
            "● │ │  sha1 def\n│ │ │  Author: B\n│ │ │\n".as_bytes(),
        ]
        .concat();
        assert_eq!(
            strip(&input).unwrap(),
            [
                b"sha1 abc\nMerge: a b c\nAuthor: A\n\n    ".as_slice(),
                "● sha1 message\n    ".as_bytes(),
                b"\xff\nsha1 def\nAuthor: B\n\n"
            ]
            .concat()
        );
        assert_eq!(
            strip(b"commit abc\n    message\n").unwrap(),
            b"commit abc\n    message\n"
        );
        assert_eq!(
            strip("●  sha1 abc\nbad\n".as_bytes()).unwrap_err(),
            "invalid graph prefix: [98, 97, 100, 10]"
        );
    }
}
