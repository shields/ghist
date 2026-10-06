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
        history::{Commit, History},
        repo::TestRepo,
    };
    use ghist::{Context, Exit};
    use std::fs;
    use std::io::{self, Write};
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::time::Duration;

    fn repo() -> TestRepo {
        let repo = TestRepo::new("sha1").unwrap();
        let mut history = History::default();
        history.push(Commit::default());
        repo.import(&history).unwrap();
        repo
    }

    fn paging_context(repo: &TestRepo, command: &str) -> Context {
        let mut ctx = repo.context(&[]);
        ctx.stdout_is_terminal = true;
        ctx.terminal_columns = Some(117);
        ctx.env.extend([
            ("TERM".into(), "xterm".into()),
            ("GIT_PAGER".into(), command.into()),
            ("OUT".into(), repo.cwd.join("pager.out").into_os_string()),
            ("DUMP".into(), repo.cwd.join("pager.env").into_os_string()),
            ("DONE".into(), repo.cwd.join("pager.done").into_os_string()),
        ]);
        ctx
    }

    #[test]
    fn an_invalid_pager_configuration_fails_before_spawning_a_pager() {
        let repo = TestRepo::new("sha1").unwrap();
        let mut ctx = repo
            .fake_git(
                br"printf 'core.pager\000'
",
            )
            .unwrap();
        ctx.stdout_is_terminal = true;
        let mut out = Vec::new();
        let mut err = Vec::new();
        assert_eq!(ghist::run(&ctx, &mut out, &mut err), Exit::Code(128));
        assert_eq!(out, b"");
        assert!(err.windows(10).any(|part| part == b"core.pager"));
    }

    #[test]
    fn captures_output_and_sets_only_missing_pager_environment() {
        let repo = repo();
        let mut ctx = paging_context(&repo, "/usr/bin/env > \"$DUMP\"; /bin/cat > \"$OUT\"");
        ctx.env.push(("GIT_PAGER_IN_USE".into(), "true".into()));
        for explicit in [false, true] {
            if explicit {
                ctx.env.extend(
                    [("LESS", ""), ("LV", "custom"), ("COLUMNS", "95")]
                        .map(|(key, value)| (key.into(), value.into())),
                );
            }
            let mut out = Vec::new();
            let mut err = Vec::new();
            assert_eq!(
                ghist::run(&ctx, &mut out, &mut err),
                Exit::Code(0),
                "{err:?}"
            );
            assert_eq!(out, b"");
            assert_eq!(err, b"");
            let rendered = fs::read(repo.cwd.join("pager.out")).unwrap();
            assert!(rendered.windows(5).any(|part| part == b"sha1 "));
            assert!(rendered.windows(2).any(|part| part == b"\x1b["));
            let values = fs::read_to_string(repo.cwd.join("pager.env")).unwrap();
            let lines: Vec<_> = values.lines().collect();
            for expected in if explicit {
                ["LESS=", "LV=custom", "COLUMNS=95"]
            } else {
                ["LESS=FRX", "LV=-c", "COLUMNS=117"]
            } {
                assert!(lines.contains(&expected), "{values}");
            }
            assert!(!values.contains("GIT_PAGER_IN_USE="));
        }
    }

    #[test]
    fn starts_lazily_and_propagates_nonzero_pager_exit() {
        let repo = repo();
        let mut ctx = paging_context(&repo, "/usr/bin/touch \"$DONE\"");
        for args in [vec!["HEAD..HEAD"], vec!["--help"], vec!["--version"]] {
            ctx.args = args.into_iter().map(Into::into).collect();
            assert_eq!(
                ghist::run(&ctx, &mut Vec::new(), &mut Vec::new()),
                Exit::Code(0)
            );
            assert!(!repo.cwd.join("pager.done").exists());
        }
        ctx.args = vec!["not-a-revision".into()];
        assert_eq!(
            ghist::run(&ctx, &mut Vec::new(), &mut Vec::new()),
            Exit::Code(128)
        );
        assert!(!repo.cwd.join("pager.done").exists());
        let ctx = paging_context(&repo, "/bin/cat > \"$OUT\"; exit 7");
        let mut err = Vec::new();
        assert_eq!(ghist::run(&ctx, &mut Vec::new(), &mut err), Exit::Code(7));
        assert_eq!(err, b"");
        let ctx = paging_context(&repo, "exec ghist-pager-that-does-not-exist");
        assert_eq!(
            ghist::run(&ctx, &mut Vec::new(), &mut Vec::new()),
            Exit::Code(127)
        );
    }

    #[test]
    fn disabled_pagers_write_directly_to_stdout() {
        let repo = repo();
        for pager in ["", "cat"] {
            let ctx = paging_context(&repo, pager);
            let mut out = Vec::new();
            assert_eq!(ghist::run(&ctx, &mut out, &mut Vec::new()), Exit::Code(0));
            assert_ne!(out, b"");
        }
    }

    #[test]
    fn early_pager_failure_preserves_its_status_during_streaming() {
        let repo = TestRepo::new("sha1").unwrap();
        let mut history = History::default();
        let first = history.push(Commit {
            message: vec![b'x'; 256 * 1024],
            ..Commit::default()
        });
        history.push(Commit {
            parents: vec![first],
            message: vec![b'y'; 256 * 1024],
            ..Commit::default()
        });
        repo.import(&history).unwrap();
        let ctx = paging_context(&repo, "exit 7");
        for _ in 0..4 {
            let mut err = Vec::new();
            assert_eq!(ghist::run(&ctx, &mut Vec::new(), &mut err), Exit::Code(7));
            assert_eq!(err, b"");
        }
    }

    fn scripted_context(repo: &TestRepo, command: &str, wait: bool) -> Context {
        let mut script = br#"if test "$2" = config; then printf 'config warning\n' >&2; exit 0; fi
printf '\036\037ghist\naaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\000aaaa\000\000\000A\000a\000date\000A\000a\000date\000\000message\000\n'
printf 'log warning\n' >&2
"#.to_vec();
        if wait {
            script.extend_from_slice(
                br#"while test ! -e "$SEEN"; do /bin/sleep 0.01; done
"#,
            );
        }
        let fake = repo.fake_git(&script).unwrap();
        let path = fake
            .env
            .iter()
            .find(|(key, _)| key == "PATH")
            .unwrap()
            .1
            .clone();
        let mut paths = path.into_string().unwrap();
        paths.push_str(":/bin:/usr/bin");
        let mut ctx = paging_context(repo, command);
        ctx.env.retain(|(key, _)| key != "PATH");
        ctx.env.push(("PATH".into(), paths.into()));
        ctx.env
            .push(("SEEN".into(), repo.cwd.join("pager.seen").into_os_string()));
        ctx
    }

    struct AfterPager {
        done: PathBuf,
        bytes: Vec<u8>,
    }

    impl Write for AfterPager {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if !self.done.exists() {
                return Err(io::Error::other("stderr arrived before pager exit"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn defers_git_stderr_until_the_pager_finishes() {
        let repo = repo();
        let ctx = scripted_context(
            &repo,
            "/bin/cat > \"$OUT\"; /usr/bin/touch \"$DONE\"",
            false,
        );
        let mut err = AfterPager {
            done: repo.cwd.join("pager.done"),
            bytes: vec![],
        };
        assert_eq!(ghist::run(&ctx, &mut Vec::new(), &mut err), Exit::Code(0));
        assert_eq!(err.bytes, b"config warning\nlog warning\n");
    }

    #[test]
    fn flushes_output_before_waiting_for_more_git_input() {
        let repo = repo();
        let ctx = scripted_context(
            &repo,
            "IFS= read -r line; printf '%s\\n' \"$line\" > \"$SEEN\"; /bin/cat > \"$OUT\"",
            true,
        );
        let (send, receive) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let mut err = Vec::new();
            let exit = ghist::run(&ctx, &mut Vec::new(), &mut err);
            send.send((exit, err)).unwrap();
        });
        let completed = receive.recv_timeout(Duration::from_secs(30));
        if completed.is_err() {
            fs::write(repo.cwd.join("pager.seen"), b"release blocked git").unwrap();
        }
        worker.join().unwrap();
        assert_eq!(
            completed.unwrap(),
            (Exit::Code(0), b"config warning\nlog warning\n".to_vec())
        );
        assert!(
            fs::read(repo.cwd.join("pager.seen"))
                .unwrap()
                .windows(5)
                .any(|part| part == b"sha1 ")
        );
    }

    #[test]
    fn early_configuration_errors_keep_config_stderr() {
        let repo = repo();
        let ctx = repo
            .fake_git(b"printf 'color.ui\\ninvalid\\000'; printf 'config warning\\n' >&2\n")
            .unwrap();
        let mut err = Vec::new();
        assert_eq!(ghist::run(&ctx, &mut Vec::new(), &mut err), Exit::Code(128));
        assert_eq!(
            err,
            b"config warning\nghist: invalid configuration color.ui: invalid\n"
        );
    }
}
