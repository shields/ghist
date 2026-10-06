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

pub mod buffer;
pub mod config;
pub mod log;
pub mod revs;
#[cfg(all(test, not(coverage_nightly)))]
mod stream_fuzz;

use std::io::{self, BufReader, Read};
use std::os::unix::process::ExitStatusExt;
use std::process::{Child, ChildStdout, Command, ExitStatus, Stdio};
use std::thread::{self, JoinHandle};

use crate::error::Error;
use crate::{Context, Exit};

pub struct ChildGuard(pub Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub struct Process {
    child: ChildGuard,
    pub stdout: BufReader<ChildStdout>,
    stderr: JoinHandle<io::Result<Vec<u8>>>,
}

pub fn command(ctx: &Context) -> Command {
    let mut command = Command::new("git");
    command
        .arg("--no-pager")
        .env_clear()
        .envs(ctx.env.iter().cloned())
        .current_dir(&ctx.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

impl Process {
    pub fn spawn(command: &mut Command) -> Result<Self, Error> {
        let mut child = ChildGuard(command.spawn()?);
        let stdout = BufReader::new(pipe(child.0.stdout.take())?);
        let mut stderr = pipe(child.0.stderr.take())?;
        let read = move || read_stderr(&mut stderr);
        let stderr = thread::Builder::new()
            .name("ghist-stderr".into())
            .spawn(read)?;
        Ok(Self {
            child,
            stdout,
            stderr,
        })
    }

    pub fn finish(mut self) -> Result<Vec<u8>, Error> {
        let drain = io::copy(&mut self.stdout, &mut io::sink());
        let status = self.child.0.wait()?;
        let stderr = joined(self.stderr.join())?;
        if !status.success() {
            return Err(Error::Git {
                exit: exit(status),
                stderr,
            });
        }
        drain?;
        Ok(stderr)
    }
}

fn pipe<T>(pipe: Option<T>) -> Result<T, Error> {
    pipe.ok_or(Error::Protocol("missing git pipe"))
}

fn read_stderr(reader: &mut dyn Read) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn joined(result: thread::Result<io::Result<Vec<u8>>>) -> Result<Vec<u8>, Error> {
    match result {
        Ok(result) => Ok(result?),
        Err(payload) => Err(Error::Io(io::Error::other(format!(
            "git stderr reader panicked: {payload:?}"
        )))),
    }
}

pub fn exit(status: ExitStatus) -> Exit {
    status.signal().map_or_else(
        || {
            Exit::Code(
                status
                    .code()
                    .and_then(|code| u8::try_from(code).ok())
                    .unwrap_or(1),
            )
        },
        Exit::Signal,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_pipes() {
        assert_eq!(pipe(Some(7)).unwrap(), 7);
        assert_eq!(
            pipe::<u8>(None).unwrap_err().to_string(),
            "invalid git output: missing git pipe"
        );
    }

    #[test]
    fn stderr_bytes_and_read_failure() {
        struct BadRead;
        impl Read for BadRead {
            fn read(&mut self, _out: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("read failed"))
            }
        }
        assert_eq!(
            read_stderr(&mut b"warning\xff\n".as_slice()).unwrap(),
            b"warning\xff\n"
        );
        assert_eq!(
            read_stderr(&mut BadRead).unwrap_err().to_string(),
            "read failed"
        );
    }

    #[test]
    fn stderr_reader_errors() {
        assert_eq!(joined(Ok(Ok(b"warning\n".to_vec()))).unwrap(), b"warning\n");
        assert_eq!(
            joined(Ok(Err(io::Error::other("read failed"))))
                .unwrap_err()
                .to_string(),
            "read failed"
        );
        assert!(
            joined(Err(Box::new("failure")))
                .unwrap_err()
                .to_string()
                .starts_with("git stderr reader panicked:")
        );
    }

    #[test]
    fn child_exit_status() {
        assert_eq!(exit(ExitStatus::from_raw(0)), Exit::Code(0));
        assert_eq!(exit(ExitStatus::from_raw(128 << 8)), Exit::Code(128));
        assert_eq!(exit(ExitStatus::from_raw(15)), Exit::Signal(15));
    }
}
