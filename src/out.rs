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

use std::cell::RefCell;
use std::ffi::OsString;
use std::io::{self, Write};

use crate::{Context, error::Error, pager::Pager};

const CAPACITY: usize = 64 * 1024;

pub struct Output<'a> {
    ctx: &'a Context,
    stdout: &'a mut dyn Write,
    command: Option<OsString>,
    pager: Option<Pager>,
    bytes: Vec<u8>,
    failed: bool,
}

impl<'a> Output<'a> {
    pub fn new(ctx: &'a Context, stdout: &'a mut dyn Write, command: Option<OsString>) -> Self {
        Self {
            ctx,
            stdout,
            command,
            pager: None,
            bytes: Vec::with_capacity(CAPACITY),
            failed: false,
        }
    }

    fn flush_buffer(&mut self) -> io::Result<()> {
        crate::signal::check(&self.ctx.signal)?;
        if self.failed {
            return Err(io::Error::other("output already failed"));
        }
        if !self.bytes.is_empty()
            && let Some(command) = self.command.take()
        {
            self.pager = Some(Pager::spawn(self.ctx, &command)?);
        }
        let target: &mut dyn Write = self.pager.as_mut().map_or(&mut *self.stdout, |pager| {
            pager
                .input
                .as_mut()
                .expect("a running pager has piped stdin")
        });
        target.write_all(&self.bytes)?;
        self.bytes.clear();
        target.flush()
    }

    pub fn finish(&mut self) -> Result<(), Error> {
        let flushed = if self.failed { Ok(()) } else { self.flush() };
        let waited = self.pager.take().map_or(Ok(()), Pager::finish);
        finish_result(flushed, waited)
    }
}

fn finish_result(flushed: io::Result<()>, waited: Result<(), Error>) -> Result<(), Error> {
    if let Err(error) = flushed {
        if error.kind() == io::ErrorKind::BrokenPipe {
            waited?;
        }
        return Err(error.into());
    }
    waited
}

impl Write for Output<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.failed {
            return Err(io::Error::other("output already failed"));
        }
        self.bytes.extend_from_slice(bytes);
        if self.bytes.len() >= CAPACITY {
            self.flush()?;
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let result = self.flush_buffer();
        self.failed |= result.is_err();
        result
    }
}

pub struct Shared<'a, 'b>(pub &'a RefCell<Output<'b>>);

impl Write for Shared<'_, '_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.borrow_mut().flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Failure {
        remaining: usize,
        flush: bool,
    }

    impl Write for Failure {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.remaining == 0 {
                return Err(io::Error::other("write failed"));
            }
            let count = self.remaining.min(bytes.len());
            self.remaining -= count;
            Ok(count)
        }
        fn flush(&mut self) -> io::Result<()> {
            if self.flush {
                Err(io::Error::other("flush failed"))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn buffers_until_capacity_and_flushes_shared_writers() {
        let ctx = Context::default();
        let mut bytes = Vec::new();
        let output = RefCell::new(Output::new(&ctx, &mut bytes, None));
        let mut writer = Shared(&output);
        writer.write_all(b"small").unwrap();
        assert_eq!(output.borrow().bytes, b"small");
        writer.write_all(&vec![b'x'; CAPACITY]).unwrap();
        assert_eq!(output.borrow().bytes, b"");
        writer.write_all(b"tail").unwrap();
        writer.flush().unwrap();
        output.borrow_mut().finish().unwrap();
        drop(output);
        assert_eq!(
            bytes,
            [b"small".as_slice(), &vec![b'x'; CAPACITY], b"tail"].concat()
        );
    }

    #[test]
    fn pager_failure_takes_precedence_over_a_closed_pipe() {
        let pipe = || Err(io::Error::from(io::ErrorKind::BrokenPipe));
        assert_eq!(
            finish_result(pipe(), Err(Error::Pager(crate::Exit::Code(7))))
                .unwrap_err()
                .exit(),
            crate::Exit::Code(7)
        );
        assert!(finish_result(pipe(), Ok(())).unwrap_err().is_broken_pipe());
        assert_eq!(
            finish_result(
                Err(io::Error::other("write failed")),
                Err(Error::Pager(crate::Exit::Code(7)))
            )
            .unwrap_err()
            .to_string(),
            "write failed"
        );
    }

    #[test]
    fn reports_a_pager_spawn_failure_without_retrying() {
        let ctx = Context {
            cwd: "/".into(),
            env: vec![("PATH".into(), "/nonexistent-ghist-test-path".into())],
            ..Context::default()
        };
        let mut bytes = Vec::new();
        let mut output = Output::new(&ctx, &mut bytes, Some("pager".into()));
        output.write_all(b"data").unwrap();
        assert_eq!(output.finish().unwrap_err().exit(), crate::Exit::Code(1));
        output.finish().unwrap();
        assert_eq!(bytes, b"");
    }

    #[test]
    fn every_partial_write_and_flush_error_is_reported_once() {
        let ctx = Context::default();
        for remaining in 0..4 {
            let mut failed = Failure {
                remaining,
                flush: false,
            };
            let mut output = Output::new(&ctx, &mut failed, None);
            output.write_all(b"data").unwrap();
            assert_eq!(output.flush().unwrap_err().to_string(), "write failed");
            assert_eq!(
                output.write(b"retry").unwrap_err().to_string(),
                "output already failed"
            );
            assert_eq!(
                output.flush().unwrap_err().to_string(),
                "output already failed"
            );
            output.finish().unwrap();
        }
        let mut failed = Failure {
            remaining: 1,
            flush: true,
        };
        let mut output = Output::new(&ctx, &mut failed, None);
        assert_eq!(output.finish().unwrap_err().to_string(), "flush failed");
        Failure {
            remaining: 1,
            flush: false,
        }
        .flush()
        .unwrap();
    }
}
