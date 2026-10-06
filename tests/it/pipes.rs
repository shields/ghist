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
    use ghist::Exit;
    use std::io::{self, BufRead, BufReader, Write};
    use std::sync::mpsc;
    use std::time::Duration;

    fn large_repo() -> TestRepo {
        let repo = TestRepo::new("sha1").unwrap();
        let mut history = History::default();
        let mut message = vec![b'x'; 256 * 1024];
        message.push(b'\n');
        let first = history.push(Commit {
            message: message.clone(),
            ..Commit::default()
        });
        history.push(Commit {
            parents: vec![first],
            message,
            ..Commit::default()
        });
        repo.import(&history).unwrap();
        repo
    }

    #[test]
    fn closing_a_real_stdout_pipe_exits_zero_without_diagnostics() {
        let repo = large_repo();
        let mut child = repo.ghist_command(&[]).spawn().unwrap();
        let mut reader = BufReader::new(child.stdout.take().unwrap());
        let mut line = Vec::new();
        reader.read_until(b'\n', &mut line).unwrap();
        drop(reader);
        let output = child.wait_with_output().unwrap();
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert_eq!(output.stderr, b"");
        assert!(line.windows(5).any(|part| part == b"sha1 "));
    }

    #[test]
    fn early_pager_exit_is_quiet_but_preserves_nonzero_status() {
        let repo = large_repo();
        for status in [0, 7] {
            for _ in 0..4 {
                let mut ctx = repo.context(&[]);
                ctx.stdout_is_terminal = true;
                ctx.env
                    .push(("GIT_PAGER".into(), format!("exit {status}").into()));
                let mut err = Vec::new();
                assert_eq!(
                    ghist::run(&ctx, &mut Vec::new(), &mut err),
                    Exit::Code(status),
                    "{err:?}"
                );
                assert_eq!(err, b"");
            }
        }
    }

    #[test]
    fn a_closed_output_stops_an_unbounded_git_stream() {
        struct Closed;
        impl Write for Closed {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let repo = TestRepo::new("sha1").unwrap();
        let mut ctx = repo.fake_git(br#"if test "$2" = config; then exit 0; fi
while test ! -e "$STOP"; do
printf '\036\037ghist\naaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\000aaaa\000\000\000A\000a\000date\000A\000a\000date\000\000message\000\n'
done
"#).unwrap();
        let stop = repo.cwd.join("stop");
        ctx.env.push(("STOP".into(), stop.clone().into_os_string()));
        let (send, receive) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let mut err = Vec::new();
            let exit = ghist::run(&ctx, &mut Closed, &mut err);
            send.send((exit, err)).unwrap();
        });
        let completed = receive.recv_timeout(Duration::from_secs(30));
        if completed.is_err() {
            std::fs::write(stop, b"stop").unwrap();
        }
        worker.join().unwrap();
        assert_eq!(completed.unwrap(), (Exit::Code(0), vec![]));
        Closed.flush().unwrap();
    }
}
