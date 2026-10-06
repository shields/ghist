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
    use crate::common::{
        history::{Change, Commit, History},
        repo::TestRepo,
        stats,
    };

    fn file(path: &[u8], data: &[u8]) -> Change {
        Change::Write {
            mode: 0o100_644,
            path: path.to_vec(),
            data: data.to_vec(),
        }
    }

    fn fixture(format: &str) -> TestRepo {
        let repo = TestRepo::new(format).unwrap();
        let mut history = History::default();
        let root = history.push(Commit {
            changes: vec![
                file(
                    b"foo/bar/baz/quux/file-name",
                    b"old\n".repeat(100).as_slice(),
                ),
                file(b"binary", b"binary\0data"),
                file(b"renamed", b"same\0bytes"),
                file(b"text-mode", b"same text\n"),
                file("雪/😀é".as_bytes(), b"one\n"),
            ],
            ..Commit::default()
        });
        let left = history.push(Commit {
            parents: vec![root],
            changes: vec![
                file(
                    b"foo/bar/baz/quux/file-name",
                    b"new\n".repeat(50).as_slice(),
                ),
                file(b"binary", b"changed\0content"),
                Change::Rename {
                    from: b"renamed".to_vec(),
                    to: b"rename-to".to_vec(),
                },
            ],
            ..Commit::default()
        });
        let right = history.push(Commit {
            branch: "refs/heads/side".into(),
            parents: vec![root],
            changes: vec![file(b"other", b"a\nb\n")],
            ..Commit::default()
        });
        let merge = history.push(Commit {
            parents: vec![left, right],
            changes: vec![file(b"merge-only", b"hidden\n")],
            ..Commit::default()
        });
        let mode = history.push(Commit {
            parents: vec![merge],
            changes: vec![
                Change::Delete(b"binary".to_vec()),
                Change::Write {
                    mode: 0o100_755,
                    path: b"text-mode".to_vec(),
                    data: b"same text\n".to_vec(),
                },
                Change::Write {
                    mode: 0o100_755,
                    path: b"rename-to".to_vec(),
                    data: b"same\0bytes".to_vec(),
                },
            ],
            ..Commit::default()
        });
        history.push(Commit {
            parents: vec![mode],
            changes: vec![Change::Delete(b"text-mode".to_vec())],
            ..Commit::default()
        });
        repo.import(&history).unwrap();
        repo
    }

    #[test]
    fn stats_match_git_at_each_graph_width_in_both_object_formats() {
        for format in ["sha1", "sha256"] {
            let repo = fixture(format);
            for color in ["never", "always"] {
                repo.git(["config", "color.diff", color]).unwrap();
                for quote in ["true", "false"] {
                    repo.git(["config", "core.quotePath", quote]).unwrap();
                    for columns in [20, 40, 80, 200] {
                        stats::compare(&repo, &[], columns).unwrap();
                    }
                }
                for (name, graph) in [
                    ("1", "1"),
                    ("10", "6"),
                    ("0x20", "20"),
                    ("0", "-1"),
                    ("-1", "0"),
                    ("-7", "6"),
                ] {
                    repo.git(["config", "diff.statNameWidth", name]).unwrap();
                    repo.git(["config", "diff.statGraphWidth", graph]).unwrap();
                    stats::compare(&repo, &["HEAD~2..HEAD"], 60).unwrap();
                    stats::compare(&repo, &["HEAD", "--", "binary"], 60).unwrap();
                }
                repo.git(["config", "diff.statNameWidth", "0"]).unwrap();
                repo.git(["config", "diff.statGraphWidth", "0"]).unwrap();
                repo.git(["config", "log.showRoot", "false"]).unwrap();
                stats::compare(&repo, &[], 80).unwrap();
                repo.git(["config", "log.showRoot", "true"]).unwrap();
            }
        }
    }
    #[test]
    fn combined_stats_and_patches_match_git() {
        for format in ["sha1", "sha256"] {
            let repo = fixture(format);
            for color in ["never", "always"] {
                repo.git(["config", "color.diff", color]).unwrap();
                for columns in [20, 80, 200] {
                    stats::combined(&repo, &[], columns).unwrap();
                    stats::combined(&repo, &["HEAD~2..HEAD"], columns).unwrap();
                    stats::combined(&repo, &["HEAD", "--", "binary"], columns).unwrap();
                }
                repo.git(["config", "log.showRoot", "false"]).unwrap();
                stats::combined(&repo, &[], 80).unwrap();
                repo.git(["config", "log.showRoot", "true"]).unwrap();
            }
        }
    }

    fn fake_sizes(diff: &[u8], mode: &str) -> (TestRepo, ghist::Context) {
        let repo = TestRepo::new("sha1").unwrap();
        let mut bytes = b"\x1e\x1fghist\n".to_vec();
        for field in [
            b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".as_slice(),
            b"aaaa",
            b"",
            b"",
            b"A",
            b"a",
            b"date",
            b"A",
            b"a",
            b"date",
            b"",
            b"message",
        ] {
            bytes.extend_from_slice(field);
            bytes.push(0);
        }
        bytes.extend_from_slice(b"\n\n");
        bytes.extend_from_slice(diff);
        repo.write("stream", &bytes).unwrap();
        let mut ctx = repo
            .fake_git(
                br#"case "$2" in
config) exit 0 ;;
log)
    /bin/cat "$STREAM"
    if test "${LOG_STATUS-0}" != 0; then printf 'walk failed\n' >&2; fi
    exit "${LOG_STATUS-0}"
    ;;
cat-file)
    : > "$STARTED"
    case "$MODE" in
    failed) printf 'size failed\n' >&2; exit 7 ;;
    unterminated) IFS= read -r hash; printf '12'; exit 0 ;;
    # exec closes saved stdin descriptors before the reply allows another write.
    closed) IFS= read -r hash; exec /usr/bin/printf '12\n' </dev/null ;;
    waiting) while test ! -e "$RELEASE"; do :; done; exit 0 ;;
    esac
    while IFS= read -r hash; do
        printf '%s\n' "$hash" >> "$LOOKUPS"
        if test "$MODE" = malformed; then printf 'bad\n'; else printf '12\n'; fi
    done
    if test -n "${WARNING-}"; then printf '%s\n' "$WARNING" >&2; fi
    ;;
esac
"#,
            )
            .unwrap();
        ctx.args = vec!["--stat".into()];
        for (key, name) in [
            ("STREAM", "stream"),
            ("STARTED", "started"),
            ("LOOKUPS", "lookups"),
        ] {
            ctx.env
                .push((key.into(), repo.cwd.join(name).into_os_string()));
        }
        ctx.env.push(("MODE".into(), mode.into()));
        (repo, ctx)
    }

    #[test]
    fn size_process_is_lazy_and_never_queries_missing_or_unchanged_sides() {
        for (diff, lookups) in [
            (
                b":000000 100644 0000 abcd A\tpath\n1\t0\tpath\n".as_slice(),
                b"".as_slice(),
            ),
            (
                b":100644 100644 abcd abcd R100\told\tnew\n-\t-\told => new\n",
                b"",
            ),
            (b":000000 100644 0000 abcd A\tpath\n-\t-\tpath\n", b"abcd\n"),
            (b":100644 000000 1234 0000 D\tpath\n-\t-\tpath\n", b"1234\n"),
            (
                b":100644 100644 1234 abcd M\tpath\n-\t-\tpath\n",
                b"1234\nabcd\n",
            ),
        ] {
            let (repo, ctx) = fake_sizes(diff, "normal");
            let mut out = Vec::new();
            let mut err = Vec::new();
            assert_eq!(
                ghist::run(&ctx, &mut out, &mut err),
                ghist::Exit::Code(0),
                "{err:?}"
            );
            assert_eq!(err, b"");
            assert_eq!(repo.cwd.join("started").exists(), !lookups.is_empty());
            if !lookups.is_empty() {
                assert_eq!(std::fs::read(repo.cwd.join("lookups")).unwrap(), lookups);
            }
        }
    }

    #[test]
    fn size_failures_preserve_exit_status_diagnostics_and_walk_error_precedence() {
        let diff = b":100644 100644 1234 abcd M\tpath\n-\t-\tpath\n";
        for (mode, status, diagnostic) in [
            ("failed", 7, "size failed"),
            ("malformed", 1, "invalid object size"),
            ("unterminated", 1, "unterminated object size"),
            ("closed", 1, "git input:"),
        ] {
            let (_repo, ctx) = fake_sizes(diff, mode);
            let mut err = Vec::new();
            assert_eq!(
                ghist::run(&ctx, &mut Vec::new(), &mut err),
                ghist::Exit::Code(status),
                "{mode}: {err:?}"
            );
            assert!(
                String::from_utf8_lossy(&err).contains(diagnostic),
                "{mode}: {err:?}"
            );
        }
        for (mode, extra) in [
            ("normal", "size warning\n"),
            ("failed", "size failed\n"),
            ("closed", ""),
        ] {
            let (_repo, mut ctx) = fake_sizes(diff, mode);
            ctx.env.push(("LOG_STATUS".into(), "6".into()));
            ctx.env.push(("WARNING".into(), "size warning".into()));
            let mut err = Vec::new();
            assert_eq!(
                ghist::run(&ctx, &mut Vec::new(), &mut err),
                ghist::Exit::Code(6),
                "{mode}: {err:?}"
            );
            assert_eq!(err, format!("{extra}walk failed\n").as_bytes());
        }
    }

    #[test]
    fn invalid_stat_widths_fail_before_starting_the_walk() {
        let repo = TestRepo::new("sha1").unwrap();
        for key in ["diff.statNameWidth", "diff.statGraphWidth"] {
            for value in ["", "bad", "08"] {
                repo.git(["config", key, value]).unwrap();
                let mut err = Vec::new();
                assert_eq!(
                    ghist::run(&repo.context(&["--stat"]), &mut Vec::new(), &mut err),
                    ghist::Exit::Code(128)
                );
                assert!(String::from_utf8_lossy(&err).contains(&key.to_ascii_lowercase()));
            }
            repo.git(["config", "--unset", key]).unwrap();
        }
    }

    #[test]
    fn output_failure_stops_a_pending_size_lookup() {
        use std::io::{self, Write};
        use std::path::PathBuf;
        use std::sync::mpsc;
        use std::time::{Duration, Instant};

        struct Failure {
            started: PathBuf,
            observed: mpsc::Sender<()>,
            kind: io::ErrorKind,
        }

        impl Write for Failure {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                let start = Instant::now();
                while !self.started.exists() {
                    if start.elapsed() > Duration::from_secs(30) {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "size lookup did not start",
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                self.observed.send(()).map_err(io::Error::other)?;
                Err(io::Error::new(self.kind, "output failed"))
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        for kind in [io::ErrorKind::BrokenPipe, io::ErrorKind::Other] {
            let (repo, mut ctx) =
                fake_sizes(b":000000 100644 0000 abcd A\tpath\n-\t-\tpath\n", "waiting");
            let release = repo.cwd.join("release");
            ctx.env
                .push(("RELEASE".into(), release.clone().into_os_string()));
            let (observed, failure) = mpsc::channel();
            let mut out = Failure {
                started: repo.cwd.join("started"),
                observed,
                kind,
            };
            let (send, receive) = mpsc::channel();
            let worker = std::thread::spawn(move || {
                let mut err = Vec::new();
                let exit = ghist::run(&ctx, &mut out, &mut err);
                send.send((exit, err)).unwrap();
            });
            let completed = failure
                .recv_timeout(Duration::from_secs(30))
                .and_then(|()| receive.recv_timeout(Duration::from_secs(10)));
            std::fs::write(release, b"release").unwrap();
            worker.join().unwrap();
            let expected = if kind == io::ErrorKind::BrokenPipe {
                (ghist::Exit::Code(0), Vec::new())
            } else {
                (ghist::Exit::Code(1), b"ghist: output failed\n".to_vec())
            };
            assert_eq!(completed.unwrap(), expected);
        }
    }
}
