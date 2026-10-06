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
    use crate::common::ansi;
    use crate::common::history::Change;
    use crate::common::{
        history::{Commit, History},
        repo::TestRepo,
    };
    use ghist::Exit;

    fn capture(repo: &TestRepo) -> Vec<u8> {
        let mut out = Vec::new();
        let mut err = Vec::new();
        assert_eq!(
            ghist::run(&repo.context(&[]), &mut out, &mut err),
            Exit::Code(0),
            "{err:?}"
        );
        assert_eq!(err, b"");
        out
    }

    #[test]
    fn configured_escape_sequences_match_git_and_strip_to_plain_output() {
        let repo = TestRepo::new("sha1").unwrap();
        let mut history = History::default();
        history.push(Commit::default());
        repo.import(&history).unwrap();
        let plain = capture(&repo);
        repo.git(["config", "color.ui", "always"]).unwrap();
        for value in [
            "",
            "normal",
            "reset",
            "RESET bold blue",
            "red blue",
            "bold nobold dim nodim",
            "#abc #AB12EF",
            "8 15",
            "16 255",
            "+1 01",
            "-01",
            "default",
            "strike reverse blink ul italic",
        ] {
            repo.git(["config", "color.diff.commit", value]).unwrap();
            let git_color = repo
                .git(["config", "--get-color", "color.diff.commit"])
                .unwrap();
            let actual = capture(&repo);
            assert!(
                actual.starts_with(&[git_color.as_slice(), b"sha1 "].concat()),
                "{value}: {actual:?}"
            );
            assert_eq!(ansi::strip(&actual), plain, "{value}");
        }
    }

    #[test]
    fn dim_boundary_matches_all_object_neighbors_in_both_formats() {
        for format in ["sha1", "sha256"] {
            let repo = TestRepo::new(format).unwrap();
            let mut history = History::default();
            let mut parent = history.push(Commit {
                changes: (0..3000)
                    .map(|index| Change::Write {
                        mode: 0o100_644,
                        path: format!("file-{index}").into_bytes(),
                        data: format!("blob {index}\n").into_bytes(),
                    })
                    .collect(),
                ..Commit::default()
            });
            for index in 0..500 {
                parent = history.push(Commit {
                    parents: vec![parent],
                    message: format!("commit {index}\n").into_bytes(),
                    ..Commit::default()
                });
            }
            repo.import(&history).unwrap();
            let plain = capture(&repo);
            repo.git(["config", "color.ui", "always"]).unwrap();
            let colored = capture(&repo);
            assert_eq!(ansi::strip(&colored), plain);
            let objects = repo
                .git([
                    "cat-file",
                    "--batch-all-objects",
                    "--batch-check=%(objectname)",
                ])
                .unwrap();
            let mut objects: Vec<_> = objects
                .split(|&byte| byte == b'\n')
                .filter(|oid| !oid.is_empty())
                .collect();
            objects.sort_unstable();
            objects.dedup();
            let label = format!("{format} ");
            let mut collisions = 0;
            let mut headers = 0;
            for line in colored.split(|&byte| byte == b'\n') {
                let text = ansi::strip(line);
                let Some(after_label) = text.strip_prefix(label.as_bytes()) else {
                    continue;
                };
                let oid = after_label.split(|&byte| byte == b' ').next().unwrap();
                let index = objects.binary_search(&oid).unwrap();
                let previous = index.checked_sub(1).and_then(|index| objects.get(index));
                let next = objects.get(index + 1);
                let unique = previous
                    .into_iter()
                    .chain(next)
                    .map(|neighbor| {
                        oid.iter()
                            .zip(neighbor.iter())
                            .take_while(|(left, right)| left == right)
                            .count()
                            + 1
                    })
                    .max()
                    .unwrap_or(1)
                    .max(4);
                collisions += usize::from(unique > 4);
                let start = line
                    .windows(label.len())
                    .position(|part| part == label.as_bytes())
                    .unwrap()
                    + label.len();
                let dim = line
                    .windows(b"\x1b[22;2m".len())
                    .position(|part| part == b"\x1b[22;2m")
                    .unwrap();
                assert_eq!(dim - start, unique, "{format}: {oid:?}");
                headers += 1;
            }
            assert_eq!(headers, 501);
            assert!(collisions > 0, "{format}");
        }
    }

    #[test]
    fn invalid_colors_fail_even_with_color_disabled() {
        for key in [
            "color.decorate.branch",
            "color.decorate.head",
            "log.graphColors",
            "color.diff.commit",
            "color.diff.new",
            "color.diff.old",
        ] {
            let repo = TestRepo::new("sha1").unwrap();
            let mut history = History::default();
            history.push(Commit::default());
            repo.import(&history).unwrap();
            repo.git(["config", "color.ui", "never"]).unwrap();
            repo.git(["config", key, "not-a-color"]).unwrap();
            let mut out = Vec::new();
            let mut err = Vec::new();
            assert_eq!(
                ghist::run(&repo.context(&[]), &mut out, &mut err),
                Exit::Code(128),
                "{key}"
            );
            assert_eq!(out, b"");
            assert!(err.starts_with(b"ghist: invalid configuration"));
        }
    }

    #[test]
    fn one_color_decision_controls_the_git_invocation() {
        let repo = TestRepo::new("sha1").unwrap();
        let mut ctx = repo
            .fake_git(
                br#"
if test "$2" = config; then printf 'color.ui\n%s\000' "$TEST_COLOR"; exit 0; fi
for arg in "$@"; do
    case "$arg" in
        --color=*) test "$arg" = "--color=$TEST_EXPECT" || exit 91; exit 0;;
    esac
done
exit 92
"#,
            )
            .unwrap();
        for (setting, terminal, term, no_color, expected) in [
            ("always", false, "dumb", "1", "always"),
            ("never", true, "xterm", "", "never"),
            ("auto", true, "xterm", "", "always"),
            ("auto", true, "xterm", "1", "never"),
            ("auto", false, "xterm", "", "never"),
            ("auto", true, "dumb", "", "never"),
        ] {
            ctx.stdout_is_terminal = terminal;
            ctx.env.retain(|(key, _)| {
                !["TEST_COLOR", "TEST_EXPECT", "TERM", "NO_COLOR"]
                    .iter()
                    .any(|name| key == name)
            });
            ctx.env.extend(
                [
                    ("TEST_COLOR", setting),
                    ("TEST_EXPECT", expected),
                    ("TERM", term),
                    ("NO_COLOR", no_color),
                ]
                .map(|(key, value)| (key.into(), value.into())),
            );
            let mut err = Vec::new();
            assert_eq!(
                ghist::run(&ctx, &mut Vec::new(), &mut err),
                Exit::Code(0),
                "{setting}: {err:?}"
            );
            assert_eq!(err, b"");
        }
    }
}
