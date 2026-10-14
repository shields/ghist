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
    #[cfg(not(coverage_nightly))]
    use crate::common::oracle;
    use crate::common::{
        history::{Change, Commit, History},
        repo::TestRepo,
    };
    use ghist::Exit;

    fn run(repo: &TestRepo, script: &[u8], args: &[&str]) -> (Exit, Vec<u8>) {
        let mut ctx = repo.fake_git(script).unwrap();
        ctx.args = args.iter().map(|arg| (*arg).into()).collect();
        let mut out = Vec::new();
        let mut err = Vec::new();
        let exit = ghist::run(&ctx, &mut out, &mut err);
        assert_eq!(out, b"");
        (exit, err)
    }

    #[test]
    fn only_negative_revision_expansions_run_the_boundary_walk() {
        let repo = TestRepo::new("sha1").unwrap();
        for args in [vec![], vec!["HEAD"], vec!["--", "file^name..txt"]] {
            assert_eq!(
                run(
                    &repo,
                    b"case \"$2\" in config|log) exit 0;; *) exit 91;; esac\n",
                    &args
                ),
                (Exit::Code(0), vec![])
            );
        }
        for args in [vec!["HEAD^"], vec!["file..name"]] {
            assert_eq!(
                run(
                    &repo,
                    br#"case "$2" in
config|log) exit 0;;
rev-parse) printf 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n'; printf 'rev warning\n' >&2;;
*) exit 92;;
esac
"#,
                    &args
                ),
                (Exit::Code(0), vec![])
            );
        }
        assert_eq!(
            run(
                &repo,
                br#"case "$2" in
config|log) exit 0;;
rev-parse) test "$4" != file || { printf 'rejected\n' >&2; exit 128; }; printf 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n';;
rev-list) test "$3" = --no-walk && test "$6" = file || kill -TERM $$; printf 'not a revision\n' >&2; exit 128;;
esac
"#,
                &["HEAD^", "file"]
            ),
            (Exit::Code(0), vec![])
        );
        for args in [vec!["HEAD~2.."], vec!["HEAD", "^HEAD~2"]] {
            assert_eq!(run(&repo, br#"case "$2" in
config) : > walked;;
rev-parse) printf '^aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n'; printf 'rev warning\n' >&2;;
rev-list) printf '%s\n' '-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'; printf 'boundary warning\n' >&2; printf 1 > walked;;
log) test -s walked || exit 94; printf 'log warning\n' >&2;;
esac
"#, &args), (Exit::Code(0), b"log warning\n".to_vec()));
        }
    }

    #[test]
    fn prepass_errors_keep_stderr_and_prioritize_git_failures() {
        let repo = TestRepo::new("sha1").unwrap();
        for (script, exit, expected) in [
            (br#"case "$2" in config) exit 0;; rev-parse) printf 'bad\n'; printf 'rev warning\n' >&2;; esac
"#.as_slice(), Exit::Code(1), b"rev warning\nghist: invalid git output: invalid object ID length\n".as_slice()),
            (br#"case "$2" in config) exit 0;; rev-parse) printf 'rejected\n' >&2; exit 70;; rev-list) test "$3" = --no-walk && exit 0; printf 'revision failure\377\n' >&2; exit 71;; esac
"#.as_slice(), Exit::Code(71), b"revision failure\xff\n".as_slice()),
            (br#"case "$2" in config) exit 0;; rev-parse) kill -TERM $$;; esac
"#.as_slice(), Exit::Signal(15), b"".as_slice()),
            (br#"case "$2" in config) exit 0;; rev-parse) exit 128;; rev-list) kill -TERM $$;; esac
"#.as_slice(), Exit::Signal(15), b"".as_slice()),
            (br#"case "$2" in config) exit 0;; rev-parse) printf '^aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n'; printf 'rev warning\n' >&2;; rev-list) printf '%s\n' '-bad'; printf 'boundary failure\n' >&2; exit 72;; esac
"#.as_slice(), Exit::Code(72), b"boundary failure\n".as_slice()),
            (br#"case "$2" in config) exit 0;; rev-parse) printf '^aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n'; printf 'rev warning\n' >&2;; rev-list) printf '%s\n' '-bad'; printf 'boundary warning\n' >&2;; esac
"#.as_slice(), Exit::Code(1), b"boundary warning\nghist: invalid git output: invalid object ID length\n".as_slice()),
        ] {
            assert_eq!(run(&repo, script, &["HEAD~2.."]), (exit, expected.to_vec()));
        }
    }

    #[test]
    fn prepass_warnings_appear_once_as_in_git() {
        let repo = TestRepo::new("sha1").unwrap();
        let mut history = History::default();
        let mut parent = history.push(Commit::default());
        for _ in 1..4 {
            parent = history.push(Commit {
                parents: vec![parent],
                ..Commit::default()
            });
        }
        history.refs = vec![
            ("refs/tags/dup".into(), parent - 1),
            ("refs/heads/dup".into(), parent),
        ];
        repo.import(&history).unwrap();
        for args in [vec!["dup^"], vec!["dup", "^dup~2"], vec!["dup~2..dup"]] {
            let expected = repo
                .git_stderr(["log", "--end-of-options"].iter().chain(&args))
                .unwrap();
            assert_ne!(expected, b"");
            let mut err = Vec::new();
            assert_eq!(
                ghist::run(&repo.context(&args), &mut Vec::new(), &mut err),
                Exit::Code(0)
            );
            assert_eq!(
                String::from_utf8_lossy(&err),
                String::from_utf8_lossy(&expected),
                "{args:?}"
            );
        }
    }

    #[cfg(not(coverage_nightly))]
    #[test]
    fn parent_shorthand_exclusions_match_git() {
        for format in ["sha1", "sha256"] {
            let repo = TestRepo::new(format).unwrap();
            let mut history = History::default();
            let mut push = |parents: Vec<usize>, branch: &str| {
                let data = format!("{}\n", history.commits.len()).into_bytes();
                history.push(Commit {
                    parents,
                    branch: branch.into(),
                    changes: vec![Change::Write {
                        mode: 0o100_644,
                        path: b"file".to_vec(),
                        data,
                    }],
                    ..Commit::default()
                })
            };
            let root = push(vec![], "refs/heads/main");
            let left = push(vec![root], "refs/heads/main");
            let right = push(vec![root], "refs/heads/main");
            let merge = push(vec![left, right], "refs/heads/main");
            push(vec![merge], "refs/heads/b1");
            let other = push(vec![left, right], "refs/heads/main");
            push(vec![other], "refs/heads/b0");
            repo.import(&history).unwrap();
            repo.write("file", b"worktree\n").unwrap();
            for args in [
                vec!["b1", "^b1^@", "b0"],
                vec!["^b1^@", "b1"],
                vec!["b0", "^b0~1^!"],
                vec!["b0", "^b0~1^-"],
                vec!["b0", "^b0~1^-2"],
                vec!["b1", "^b1^@", "--"],
                vec!["b1", "^b1^@", "--", "file"],
                vec!["b1", "^b1^@", "file"],
                vec!["b0~1..", "file"],
                vec!["b1^", "file"],
                vec!["b1^@"],
                vec!["b0~1^!"],
            ] {
                oracle::fuller(&repo, &args).unwrap();
            }
        }
    }

    #[test]
    fn path_limited_ranges_hide_rewritten_parents() {
        for format in ["sha1", "sha256"] {
            let repo = TestRepo::new(format).unwrap();
            let mut history = History::default();
            for (parents, path, content) in [
                (vec![], "file", "root"),
                (vec![1], "other", "ignored"),
                (vec![2], "file", "left"),
                (vec![1], "file", "right"),
                (vec![3, 4], "file", "merge"),
            ] {
                history.push(Commit {
                    parents,
                    changes: vec![Change::Write {
                        mode: 0o100_644,
                        path: path.as_bytes().to_vec(),
                        data: content.as_bytes().to_vec(),
                    }],
                    ..Commit::default()
                });
            }
            repo.import(&history).unwrap();
            let mut out = Vec::new();
            let mut err = Vec::new();
            assert_eq!(
                ghist::run(
                    &repo.context(&["HEAD~2..HEAD", "--", "file"]),
                    &mut out,
                    &mut err
                ),
                Exit::Code(0),
                "{err:?}"
            );
            assert_eq!(err, b"");
            assert_eq!(
                out.windows("●".len())
                    .filter(|part| *part == "●".as_bytes())
                    .count(),
                3
            );
            let last = out
                .split(|&byte| byte == b'\n')
                .rev()
                .find(|line| !line.is_empty())
                .unwrap();
            assert!(
                !last.windows("│".len()).any(|part| part == "│".as_bytes()),
                "{}",
                String::from_utf8_lossy(&out)
            );
        }
    }

    #[test]
    fn excluded_parents_leave_no_dangling_graph_lanes() {
        for format in ["sha1", "sha256"] {
            let repo = TestRepo::new(format).unwrap();
            let mut history = History::default();
            let mut parent = 0;
            for index in 0..5 {
                parent = history.push(Commit {
                    parents: if parent == 0 { vec![] } else { vec![parent] },
                    changes: vec![Change::Write {
                        mode: 0o100_644,
                        path: b"file".to_vec(),
                        data: format!("{index}\n").into_bytes(),
                    }],
                    ..Commit::default()
                });
            }
            repo.import(&history).unwrap();
            repo.write("file", b"worktree\n").unwrap();
            for args in [
                vec!["HEAD~1.."],
                vec!["HEAD", "^HEAD~1"],
                vec!["HEAD~1...HEAD"],
                vec!["HEAD~1..", "--", "file"],
                vec!["HEAD", "^HEAD^@"],
                vec!["HEAD", "^HEAD~1^!", "--"],
                vec!["HEAD", "^HEAD~1^-", "file"],
                vec!["HEAD~1..", "file"],
            ] {
                let mut out = Vec::new();
                let mut err = Vec::new();
                assert_eq!(
                    ghist::run(&repo.context(&args), &mut out, &mut err),
                    Exit::Code(0),
                    "{err:?}"
                );
                assert_eq!(err, b"");
                assert!(
                    !out.windows("│".len()).any(|part| part == "│".as_bytes()),
                    "{}",
                    String::from_utf8_lossy(&out)
                );
                assert_eq!(
                    out.windows("●".len())
                        .filter(|part| *part == "●".as_bytes())
                        .count(),
                    1
                );
            }
        }
    }

    #[test]
    fn a_bare_parent_directory_is_a_path_as_in_git() {
        let repo = TestRepo::new("sha1").unwrap();
        let mut history = History::default();
        let mut parents = vec![];
        for index in 0..3 {
            parents = vec![history.push(Commit {
                parents,
                changes: vec![Change::Write {
                    mode: 0o100_644,
                    path: b"file".to_vec(),
                    data: format!("{index}\n").into_bytes(),
                }],
                ..Commit::default()
            })];
        }
        repo.import(&history).unwrap();
        std::fs::create_dir(repo.cwd.join("sub")).unwrap();
        for args in [vec![".."], vec!["HEAD", ".."]] {
            let ctx = ghist::Context {
                cwd: repo.cwd.join("sub"),
                ..repo.context(&args)
            };
            let mut out = Vec::new();
            let mut err = Vec::new();
            assert_eq!(
                ghist::run(&ctx, &mut out, &mut err),
                Exit::Code(0),
                "{args:?}: {}",
                String::from_utf8_lossy(&err)
            );
            assert_eq!(err, b"");
            assert_eq!(
                out.windows("●".len())
                    .filter(|part| *part == "●".as_bytes())
                    .count(),
                3
            );
        }
    }
}
