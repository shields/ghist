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

#[cfg(test)]
mod tests {
    use crate::common::history::{Commit, History, Identity};
    use crate::common::repo::TestRepo;
    use ghist::Exit;

    fn render(repo: &TestRepo) -> Vec<u8> {
        let mut out = Vec::new();
        let mut err = Vec::new();
        assert_eq!(
            ghist::run(&repo.context(&[]), &mut out, &mut err),
            Exit::Code(0),
            "{}",
            String::from_utf8_lossy(&err)
        );
        assert_eq!(err, b"");
        crate::common::graph_text::strip(&out).unwrap()
    }

    fn contains(bytes: &[u8], part: &[u8]) -> bool {
        bytes.windows(part.len()).any(|window| window == part)
    }

    #[test]
    fn unicode_tab_stops_follow_display_widths_across_scalar_values() {
        use unicode_width::UnicodeWidthChar;

        let characters: Vec<_> = (1..=0x10_ffff)
            .filter_map(char::from_u32)
            .filter(|&character| !matches!(character, '\n' | '\r' | '\t'))
            .collect();
        let mut message = String::new();
        for character in &characters {
            message.push('a');
            message.push(*character);
            message.push_str("\tx\n");
        }
        let repo = TestRepo::new("sha1").unwrap();
        let mut history = History::default();
        history.push(Commit {
            message: message.into_bytes(),
            ..Commit::default()
        });
        repo.import(&history).unwrap();
        let actual = render(&repo);
        let body = &actual[actual.windows(2).position(|part| part == b"\n\n").unwrap() + 2..];
        let lines: Vec<_> = body.split(|&byte| byte == b'\n').collect();
        assert_eq!(lines.len(), characters.len() + 1);
        assert_eq!(lines.last().unwrap(), b"");
        for (character, line) in characters.into_iter().zip(lines) {
            let prefix = format!("    a{character}");
            let tail = line.strip_prefix(prefix.as_bytes()).unwrap();
            if let Some(width) = character.width() {
                assert_eq!(tail.last(), Some(&b'x'), "U+{:04X}", u32::from(character));
                let padding = &tail[..tail.len() - 1];
                assert!((1..=8).contains(&padding.len()));
                assert!(padding.iter().all(|&byte| byte == b' '));
                assert_eq!(
                    (1 + width + padding.len()) % 8,
                    0,
                    "U+{:04X}",
                    u32::from(character)
                );
            } else {
                assert_eq!(tail, b"\tx", "U+{:04X}", u32::from(character));
            }
        }
    }

    #[test]
    #[cfg(not(coverage_nightly))]
    fn unicode_spacing_is_independent_of_git_history_comparisons() {
        for (message, encoding) in [
            (
                "a\u{ad}\tx\na\u{5c8}\tx\n\u{18e00}\tx\n\u{1faf9}\tx\n".as_bytes(),
                None,
            ),
            (b"\xad\t\x0b", Some("ISO-8859-1")),
        ] {
            let repo = TestRepo::new("sha1").unwrap();
            let mut history = History::default();
            history.push(Commit {
                message: message.to_vec(),
                encoding: encoding.map(str::to_owned),
                ..Commit::default()
            });
            repo.import(&history).unwrap();
            crate::common::oracle::fuller(&repo, &[]).unwrap();
        }
    }

    #[test]
    fn full_hashes_conditional_headers_and_dates_in_both_formats() {
        for format in ["sha1", "sha256"] {
            let repo = TestRepo::new(format).unwrap();
            let mut history = History::default();
            history.push(Commit {
                message: b"\nsubject \t\n\nbody\tend\n\n".to_vec(),
                ..Commit::default()
            });
            repo.import(&history).unwrap();
            let oid = repo.git(["rev-parse", "HEAD"]).unwrap();
            let oid = oid.trim_ascii_end();
            let expected = [format.as_bytes(), b" ", oid, b" (HEAD -> main)\nAuthor:     A U Thor <author@example.com>\nAuthorDate: 2023-11-14 22:13:20 +0000\n\n    subject\n\n    body    end\n"].concat();
            assert_eq!(render(&repo), expected);
            repo.git(["checkout", "--detach", "HEAD"]).unwrap();
            assert!(contains(&render(&repo), b"(HEAD, main)"));
        }
        let repo = TestRepo::new("sha1").unwrap();
        let mut history = History::default();
        history.push(Commit {
            committer: Identity {
                name: b"Other".to_vec(),
                date: "1700000000 +0530".into(),
                ..Identity::default()
            },
            ..Commit::default()
        });
        repo.import(&history).unwrap();
        assert!(contains(&render(&repo), b"Author:     A U Thor <author@example.com>\nCommit:     Other <author@example.com>\nAuthorDate: 2023-11-14 22:13:20 +0000\nCommitDate: 2023-11-15 03:43:20 +0530\n"));
    }

    #[test]
    fn mailmap_controls_identity_comparison_and_full_decorations() {
        let repo = TestRepo::new("sha1").unwrap();
        let mut history = History::default();
        let root = history.push(Commit {
            committer: Identity {
                name: b"Alias".to_vec(),
                email: b"alias@example.com".to_vec(),
                ..Identity::default()
            },
            ..Commit::default()
        });
        history.refs = vec![
            ("refs/tags/v1".into(), root),
            ("refs/remotes/origin/main".into(), root),
            ("refs/heads/a,b".into(), root),
        ];
        repo.import(&history).unwrap();
        repo.write(
            ".mailmap",
            b"A U Thor <author@example.com> Alias <alias@example.com>\n",
        )
        .unwrap();
        for setting in ["true", "false"] {
            repo.git(["config", "log.mailmap", setting]).unwrap();
            let out = render(&repo);
            assert_eq!(
                contains(&out, b"Commit:     Alias <alias@example.com>"),
                setting == "false"
            );
            for decoration in [b"tag: v1".as_slice(), b"origin/main", b"a,b"] {
                assert!(contains(&out, decoration));
            }
            assert!(!contains(&out, b"refs/heads/"));
        }
    }

    #[test]
    fn message_bytes_and_encoding_match_git_fuller() {
        for (message, encoding, output_encoding) in [
            (
                b"\n \t\nsubject \t\r\n\nbody\tend\n\n".as_slice(),
                None,
                "UTF-8",
            ),
            ("界\tx\na\u{301}\ty\n".as_bytes(), None, "UTF-8"),
            (b"invalid\xff\tbytes\n", Some("ISO-8859-1"), "ISO-8859-1"),
            (b"caf\xe9\ttext\n", Some("ISO-8859-1"), "UTF-8"),
            (b"escape\x1b[1m\ttext\ncontrol\x07\ttext\n", None, "UTF-8"),
            (b"message\0truncated\n", None, "UTF-8"),
            (b"\n \t\n", None, "UTF-8"),
            (b"no final newline", None, "UTF-8"),
            (b"form feed\x0c\nvertical tab\x0b\n", None, "UTF-8"),
            (b"\x0c\n\x0b\nbody\n\x0c\n\x0b\n", None, "UTF-8"),
        ] {
            let repo = TestRepo::new("sha1").unwrap();
            let mut history = History::default();
            history.push(Commit {
                message: message.to_vec(),
                encoding: encoding.map(str::to_owned),
                ..Commit::default()
            });
            repo.import(&history).unwrap();
            repo.git(["config", "i18n.logOutputEncoding", output_encoding])
                .unwrap();
            let reference = repo
                .git([
                    "log",
                    "--pretty=fuller",
                    "--date=iso",
                    "--no-show-signature",
                    "--color=never",
                ])
                .unwrap();
            let rendered = render(&repo);
            let body = |bytes: Vec<u8>| {
                bytes
                    .windows(2)
                    .position(|part| part == b"\n\n")
                    .map_or_else(Vec::new, |index| bytes[index + 2..].to_vec())
            };
            let expected: Vec<u8> = body(reference)
                .split(|&byte| byte == b'\n')
                .map(|line| {
                    if line == b"    " {
                        b"".as_slice()
                    } else {
                        line
                    }
                })
                .collect::<Vec<_>>()
                .join(&b'\n');
            assert_eq!(body(rendered), expected, "{message:?}");
        }
    }
}
