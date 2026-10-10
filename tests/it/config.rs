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
    use crate::common::history::{Commit, History};
    use crate::common::repo::TestRepo;
    use ghist::{Context, Exit};
    use std::io::{self, Write};

    fn capture(ctx: &Context) -> (Exit, Vec<u8>, Vec<u8>) {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let exit = ghist::run(ctx, &mut out, &mut err);
        (exit, out, err)
    }

    #[test]
    fn informational_options_do_not_start_git() {
        let repo = TestRepo::new("sha1").unwrap();
        let mut ctx = repo.fake_git(b"exit 99\n").unwrap();
        for arg in ["--help", "--version"] {
            ctx.args = vec![arg.into()];
            let (exit, out, err) = capture(&ctx);
            assert_eq!(exit, Exit::Code(0));
            assert!(out.starts_with(b"ghist"));
            assert_eq!(err, b"");
        }
        for shell in ["bash", "zsh"] {
            ctx.args = vec!["--completions".into(), shell.into()];
            let (exit, out, err) = capture(&ctx);
            assert_eq!(exit, Exit::Code(0));
            assert_ne!(out, []);
            assert_eq!(err, b"");
        }
    }

    #[test]
    fn reads_real_config_and_rejects_invalid_boolean() {
        let repo = TestRepo::new("sha1").unwrap();
        let mut history = History::default();
        history.push(Commit::default());
        repo.import(&history).unwrap();
        for value in [
            "true",
            "false",
            "",
            "1k",
            "-1",
            "0x0",
            "010",
            "2147483647",
            "-2147483648",
            "\u{b}1",
        ] {
            repo.git(["config", "log.mailmap", value]).unwrap();
            assert_eq!(
                capture(&repo.context(&["HEAD..HEAD"])),
                (Exit::Code(0), vec![], vec![])
            );
            repo.git(["config", "--type=bool", "log.mailmap"]).unwrap();
        }
        for value in ["invalid", "08", "2147483648", "1 ", "0x+1"] {
            repo.git(["config", "log.mailmap", value]).unwrap();
            let (exit, out, err) = capture(&repo.context(&["HEAD..HEAD"]));
            assert_eq!(exit, Exit::Code(128), "{value}");
            assert_eq!(out, b"");
            assert!(err.starts_with(b"ghist: invalid configuration log.mailmap:"));
            assert_eq!(
                repo.git(["config", "--type=bool", "log.mailmap"])
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::Other
            );
        }
    }

    #[test]
    fn config_subprocess_has_explicit_arguments_and_environment() {
        let repo = TestRepo::new("sha1").unwrap();
        let ctx = repo
            .fake_git(
                br#"
if test "$2" = log; then exit 0; fi
test "$*" = '--no-pager config --list -z' || exit 91
test "$LC_ALL" = C || exit 92
test "$GIT_CONFIG_NOSYSTEM" = 1 || exit 93
if read value; then exit 94; fi
printf 'log.mailmap\ntrue\000'
printf 'warning\377\n' >&2
"#,
            )
            .unwrap();
        assert_eq!(
            capture(&ctx),
            (Exit::Code(0), vec![], b"warning\xff\n".to_vec())
        );
    }

    #[test]
    fn prioritizes_git_failures_over_malformed_stdout() {
        let repo = TestRepo::new("sha1").unwrap();
        let ctx = repo
            .fake_git(b"printf malformed; printf 'fatal\\377\\n' >&2; exit 42\n")
            .unwrap();
        assert_eq!(
            capture(&ctx),
            (Exit::Code(42), vec![], b"fatal\xff\n".to_vec())
        );
        let ctx = repo.fake_git(b"printf malformed\n").unwrap();
        let (exit, out, err) = capture(&ctx);
        assert_eq!(exit, Exit::Code(1));
        assert_eq!(out, b"");
        assert_eq!(
            err,
            b"ghist: invalid git output: unterminated config entry\n"
        );
    }

    #[test]
    fn drains_large_stderr_without_deadlocking() {
        let repo = TestRepo::new("sha1").unwrap();
        let ctx = repo.fake_git(b"if test \"$2\" = log; then exit 0; fi\ni=0; while test $i -lt 20000; do printf warning >&2; i=$((i + 1)); done\nprintf 'log.mailmap\\ntrue\\000'\n").unwrap();
        let (exit, out, err) = capture(&ctx);
        assert_eq!(exit, Exit::Code(0));
        assert_eq!(out, b"");
        assert_eq!(err, b"warning".repeat(20000));
    }

    #[test]
    fn handles_child_signals_and_spawn_errors() {
        let repo = TestRepo::new("sha1").unwrap();
        let mut ctx = repo.fake_git(b"kill -TERM $$\n").unwrap();
        assert_eq!(capture(&ctx), (Exit::Signal(15), vec![], vec![]));
        ctx.cwd = repo.cwd.join("missing");
        let (exit, out, err) = capture(&ctx);
        assert_eq!(exit, Exit::Code(1));
        assert_eq!(out, b"");
        assert!(err.starts_with(b"ghist:"));
    }

    struct FailedWriter {
        fail_write: bool,
    }

    impl Write for FailedWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.fail_write {
                Err(io::Error::other("write failed"))
            } else {
                Ok(bytes.len())
            }
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("flush failed"))
        }
    }

    #[test]
    fn surfaces_stderr_write_failures() {
        let repo = TestRepo::new("sha1").unwrap();
        let ctx = repo.fake_git(b"printf warning >&2\n").unwrap();
        for fail_write in [true, false] {
            assert_eq!(
                ghist::run(&ctx, &mut Vec::new(), &mut FailedWriter { fail_write }),
                Exit::Code(1)
            );
        }
    }
}
