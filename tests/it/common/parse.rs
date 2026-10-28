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

#[derive(Debug, Default, Eq, PartialEq)]
pub struct CommitText {
    pub hash: Vec<u8>,
    pub merge: Vec<u8>,
    pub author: Vec<u8>,
    pub committer: Option<Vec<u8>>,
    pub author_date: Vec<u8>,
    pub committer_date: Option<Vec<u8>>,
    pub message: Vec<Vec<u8>>,
}

pub fn message_line_matches(expected: &[u8], actual: &[u8]) -> bool {
    use unicode_width::UnicodeWidthChar;

    let mut remaining = actual;
    for part in expected.split_inclusive(|&byte| byte == b'\t') {
        let text = part.strip_suffix(b"\t").unwrap_or(part);
        let Some(rest) = remaining.strip_prefix(text) else {
            return false;
        };
        remaining = rest;
        if part.ends_with(b"\t") {
            let prefix = actual
                .get(..actual.len() - remaining.len())
                .unwrap_or_default();
            let columns = std::str::from_utf8(prefix).ok().and_then(|text| {
                text.chars()
                    .map(UnicodeWidthChar::width)
                    .sum::<Option<usize>>()
            });
            let padding = columns.map(|columns| vec![b' '; 8 - columns.saturating_sub(4) % 8]);
            let Some(rest) = remaining.strip_prefix(padding.as_deref().unwrap_or(b"\t")) else {
                return false;
            };
            remaining = rest;
        }
    }
    remaining.is_empty()
}

pub fn commits(bytes: &[u8]) -> Result<Vec<CommitText>, String> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let bytes = super::graph_text::strip(bytes)?;
    let mut result = Vec::<CommitText>::new();
    let mut body = false;
    for line in bytes.split(|&byte| byte == b'\n') {
        let hash = [b"commit ".as_slice(), b"sha1 ", b"sha256 "]
            .into_iter()
            .find_map(|prefix| line.strip_prefix(prefix));
        if let Some(hash) = hash {
            result.push(CommitText {
                hash: hash
                    .split(|&byte| byte == b' ')
                    .next()
                    .unwrap_or_default()
                    .to_vec(),
                ..CommitText::default()
            });
            body = false;
            continue;
        }
        let current = result
            .last_mut()
            .ok_or_else(|| "content before the first commit".to_owned())?;
        if body {
            current.message.push(if line == b"    " {
                Vec::new()
            } else {
                line.to_vec()
            });
        } else if line.is_empty() {
            body = true;
        } else if let Some(value) = line.strip_prefix(b"Merge:") {
            current.merge = value.trim_ascii().to_vec();
        } else if let Some(value) = line.strip_prefix(b"Author:") {
            current.author = value.trim_ascii().to_vec();
        } else if let Some(value) = line.strip_prefix(b"Commit:") {
            current.committer = Some(value.trim_ascii().to_vec());
        } else if let Some(value) = line.strip_prefix(b"AuthorDate:") {
            current.author_date = value.trim_ascii().to_vec();
        } else if let Some(value) = line.strip_prefix(b"CommitDate:") {
            current.committer_date = Some(value.trim_ascii().to_vec());
        } else {
            return Err(format!("unexpected header: {line:?}"));
        }
    }
    for commit in &mut result {
        commit
            .committer
            .get_or_insert_with(|| commit.author.clone());
        commit
            .committer_date
            .get_or_insert_with(|| commit.author_date.clone());
        while commit.message.last().is_some_and(Vec::is_empty) {
            commit.message.pop();
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_comparison_preserves_text_and_checks_tab_cells() {
        for (expected, actual) in [
            (b"".as_slice(), b"".as_slice()),
            (b"    text", b"    text"),
            (b"    a\tb\tc", b"    a       b       c"),
            (b"    a\t  b", b"    a         b"),
            (
                "    a\u{ad}\tx".as_bytes(),
                "    a\u{ad}       x".as_bytes(),
            ),
            (
                "    \u{1faf9}\tx".as_bytes(),
                "    \u{1faf9}      x".as_bytes(),
            ),
            (b"    a\xff\tx", b"    a\xff\tx"),
            (b"    a\x1b\tx", b"    a\x1b\tx"),
        ] {
            assert!(message_line_matches(expected, actual));
            assert!(!message_line_matches(expected, &[actual, b"x"].concat()));
        }
        for actual in [
            "    a\u{ad}      x",
            "    a\u{ad}        x",
            "    a\u{ad}       y",
        ] {
            assert!(!message_line_matches(
                "    a\u{ad}\tx".as_bytes(),
                actual.as_bytes()
            ));
        }
    }

    #[test]
    fn fuller_and_conditional_headers_normalize_only_blank_indentation() {
        let reference = b"commit abc\nAuthor:     A <a>\nAuthorDate: date\nCommit:     A <a>\nCommitDate: date\n\n    body\x0c\n    \n    next\n\ncommit def\nMerge: abc ghi\nAuthor:     A <a>\nAuthorDate: date\nCommit:     B <b>\nCommitDate: later\n\n";
        let actual = b"sha1 abc (HEAD -> main)\nAuthor:     A <a>\nAuthorDate: date\n\n    body\x0c\n\n    next\n\nsha256 def\nMerge: abc ghi\nAuthor:     A <a>\nCommit:     B <b>\nAuthorDate: date\nCommitDate: later\n";
        let expected = commits(reference).unwrap();
        assert_eq!(commits(actual).unwrap(), expected);
        assert_eq!(expected.len(), 2);
        assert_eq!(
            expected[0].message,
            [b"    body\x0c".to_vec(), vec![], b"    next".to_vec()]
        );
        assert_eq!(expected[1].committer, Some(b"B <b>".to_vec()));
        assert_eq!(expected[1].merge, b"abc ghi");
        assert_eq!(commits(b"").unwrap(), vec![]);
        assert_eq!(
            commits(b"bad\n").unwrap_err(),
            "content before the first commit"
        );
        assert_eq!(
            commits(b"sha1 abc\nunknown\n").unwrap_err(),
            "unexpected header: [117, 110, 107, 110, 111, 119, 110]"
        );
    }
}
