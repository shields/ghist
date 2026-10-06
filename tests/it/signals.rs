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
    use crate::common::repo::TestRepo;
    use ghist::Exit;
    use rustix::process::{Pid, Signal, kill_process, test_kill_process};
    use std::io::{self, Read, Write};
    use std::os::unix::process::ExitStatusExt;
    use std::process::{Command, Stdio};
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    fn wait_until(mut ready: impl FnMut() -> bool) {
        let start = Instant::now();
        while !ready() {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "process did not become ready"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn signals_arriving_while_reporting_errors_are_preserved() {
        struct Interrupt<'a>(&'a std::sync::atomic::AtomicUsize);
        impl Write for Interrupt<'_> {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                self.0.store(2, Ordering::Relaxed);
                Err(io::Error::other("interrupted"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let ctx = ghist::Context {
            args: vec!["--bad".into()],
            ..ghist::Context::default()
        };
        let mut writer = Interrupt(&ctx.signal);
        assert_eq!(
            ghist::run(&ctx, &mut Vec::new(), &mut writer),
            Exit::Signal(2)
        );
        writer.flush().unwrap();
    }

    #[test]
    fn binary_reraises_each_signal_and_reaps_git_during_reads_and_waits() {
        for signal in [Signal::INT, Signal::QUIT, Signal::TERM, Signal::HUP] {
            for stage in ["config", "log", "wait"] {
                let repo = TestRepo::new("sha1").unwrap();
                let mut ctx = repo
                    .fake_git(
                        br#"if test "$STAGE" != config && test "$2" = config; then exit 0; fi
if test "$STAGE" = wait; then exec 1>&- 2>&-; fi
printf '%s' "$$" > "$PIDFILE"
while :; do :; done
"#,
                    )
                    .unwrap();
                let pidfile = repo.cwd.join("git.pid");
                ctx.env.push(("STAGE".into(), stage.into()));
                ctx.env
                    .push(("PIDFILE".into(), pidfile.clone().into_os_string()));
                let mut child = Command::new("/bin/sh")
                    .args(["-c", "ulimit -c 0; exec \"$GHIST\""])
                    .env_clear()
                    .envs(ctx.env)
                    .env("GHIST", env!("CARGO_BIN_EXE_ghist"))
                    .current_dir(&repo.cwd)
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap();
                wait_until(|| std::fs::read_to_string(&pidfile).is_ok_and(|s| !s.is_empty()));
                let git_pid =
                    Pid::from_raw(std::fs::read_to_string(pidfile).unwrap().parse().unwrap())
                        .unwrap();
                kill_process(
                    Pid::from_raw(i32::try_from(child.id()).unwrap()).unwrap(),
                    signal,
                )
                .unwrap();
                let start = Instant::now();
                let status = loop {
                    if let Some(status) = child.try_wait().unwrap() {
                        break status;
                    }
                    if start.elapsed() > Duration::from_secs(10) {
                        child.kill().unwrap();
                        child.wait().unwrap();
                        panic!("signal cleanup timed out");
                    }
                    std::thread::sleep(Duration::from_millis(5));
                };
                let mut stderr = Vec::new();
                child
                    .stderr
                    .take()
                    .unwrap()
                    .read_to_end(&mut stderr)
                    .unwrap();
                assert_eq!(
                    status.signal(),
                    Some(signal.as_raw()),
                    "{stage}: {stderr:?}"
                );
                assert_eq!(stderr, b"");
                assert!(test_kill_process(git_pid).is_err());
            }
        }
    }

    #[test]
    fn cancellation_stops_a_config_child_after_stdout_closes() {
        let repo = TestRepo::new("sha1").unwrap();
        let mut ctx = repo
            .fake_git(
                br#"exec 1>&- 2>&-
: > "$READY"
while :; do :; done
"#,
            )
            .unwrap();
        let ready = repo.cwd.join("git.ready");
        ctx.env
            .push(("READY".into(), ready.clone().into_os_string()));
        let signal = std::sync::Arc::clone(&ctx.signal);
        let worker = std::thread::spawn(move || {
            let mut err = Vec::new();
            (ghist::run(&ctx, &mut Vec::new(), &mut err), err)
        });
        wait_until(|| ready.exists());
        signal.store(15, Ordering::Relaxed);
        assert_eq!(worker.join().unwrap(), (Exit::Signal(15), vec![]));
    }

    #[test]
    fn signal_closes_pager_input_and_waits_for_cleanup() {
        let repo = TestRepo::new("sha1").unwrap();
        let mut ctx = repo.fake_git(br#"if test "$2" = config; then exit 0; fi
printf '\036\037ghist\naaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\000aaaa\000\000\000A\000a\000date\000A\000a\000date\000\000message\000\n'
while :; do :; done
"#).unwrap();
        ctx.stdout_is_terminal = true;
        let ready = repo.cwd.join("pager.ready");
        let closed = repo.cwd.join("pager.closed");
        let release = repo.cwd.join("pager.release");
        let done = repo.cwd.join("pager.done");
        let pager = repo
            .script(
                "pager",
                br#": > "$READY"
/bin/cat > /dev/null
: > "$CLOSED"
while test ! -e "$RELEASE"; do /bin/sleep 0.01; done
: > "$DONE"
"#,
            )
            .unwrap();
        ctx.env.push(("GIT_PAGER".into(), pager.into_os_string()));
        for (key, path) in [
            ("READY", &ready),
            ("CLOSED", &closed),
            ("RELEASE", &release),
            ("DONE", &done),
        ] {
            ctx.env.push((key.into(), path.clone().into_os_string()));
        }
        let bin = repo.cwd.join("bin");
        std::os::unix::fs::symlink("/bin/sh", bin.join("sh")).unwrap();
        let signal = std::sync::Arc::clone(&ctx.signal);
        let worker = std::thread::spawn(move || {
            let mut err = Vec::new();
            (ghist::run(&ctx, &mut Vec::new(), &mut err), err)
        });
        wait_until(|| ready.exists());
        signal.store(2, Ordering::Relaxed);
        wait_until(|| closed.exists());
        assert!(!worker.is_finished());
        assert!(!done.exists());
        std::fs::write(release, b"go").unwrap();
        assert_eq!(worker.join().unwrap(), (Exit::Signal(2), vec![]));
        assert!(done.exists());
    }
}
