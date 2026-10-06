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

use std::io::{self, Read, Write};
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::fs::{FileType, OFlags, fcntl_getfl, fcntl_setfl, fstat};
use rustix::io::Errno;

pub fn check(signal: &AtomicUsize) -> io::Result<()> {
    if signal.load(Ordering::Relaxed) == 0 {
        Ok(())
    } else {
        // Read helpers retry Interrupted errors, which would swallow cancellation.
        Err(io::Error::other("interrupted by signal"))
    }
}

fn retry(result: Result<usize, Errno>) -> io::Result<Option<usize>> {
    match result {
        Err(Errno::AGAIN | Errno::INTR) => Ok(None),
        result => Ok(Some(result?)),
    }
}

fn ready(fd: BorrowedFd<'_>, signal: &AtomicUsize, events: PollFlags) -> io::Result<()> {
    loop {
        check(signal)?;
        let mut fds = [PollFd::new(&fd, events)];
        if retry(poll(
            &mut fds,
            Some(&Timespec {
                tv_sec: 0,
                tv_nsec: 50_000_000,
            }),
        ))?
        .is_some_and(|count| count > 0)
        {
            return check(signal);
        }
    }
}

fn operation(
    fd: BorrowedFd<'_>,
    signal: &AtomicUsize,
    events: PollFlags,
    action: &mut dyn FnMut() -> Result<usize, Errno>,
) -> io::Result<usize> {
    loop {
        ready(fd, signal, events)?;
        if let Some(count) = retry(action())? {
            return Ok(count);
        }
    }
}

pub struct Pipe {
    fd: OwnedFd,
    signal: Arc<AtomicUsize>,
}

impl Pipe {
    pub fn new(fd: OwnedFd, signal: &Arc<AtomicUsize>) -> io::Result<Self> {
        fcntl_setfl(&fd, fcntl_getfl(&fd)? | OFlags::NONBLOCK)?;
        Ok(Self {
            fd,
            signal: Arc::clone(signal),
        })
    }
}

impl Read for Pipe {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        operation(self.fd.as_fd(), &self.signal, PollFlags::IN, &mut || {
            rustix::io::read(&self.fd, &mut *bytes)
        })
    }
}

impl Write for Pipe {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        operation(self.fd.as_fd(), &self.signal, PollFlags::OUT, &mut || {
            rustix::io::write(&self.fd, bytes)
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        check(&self.signal)
    }
}

pub struct Writer<'a> {
    fd: BorrowedFd<'a>,
    signal: &'a AtomicUsize,
    limit: Option<usize>,
}

impl<'a> Writer<'a> {
    pub const fn new(fd: BorrowedFd<'a>, signal: &'a AtomicUsize) -> Self {
        Self {
            fd,
            signal,
            limit: None,
        }
    }
}

impl Write for Writer<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        let limit = if let Some(limit) = self.limit {
            limit
        } else {
            let kind = FileType::from_raw_mode(fstat(self.fd)?.st_mode);
            // A blocking pipe write larger than PIPE_BUF can block after poll reports writable.
            let limit = if matches!(kind, FileType::Fifo | FileType::Socket)
                || rustix::termios::isatty(self.fd)
            {
                512
            } else {
                usize::MAX
            };
            self.limit = Some(limit);
            limit
        };
        let bytes = bytes.get(..limit).unwrap_or(bytes);
        operation(self.fd, self.signal, PollFlags::OUT, &mut || {
            rustix::io::write(self.fd, bytes)
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        check(self.signal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Seek;
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    #[test]
    fn retries_transient_errors_and_preserves_failures() {
        for errno in [Errno::INTR, Errno::AGAIN] {
            assert_eq!(retry(Err(errno)).unwrap(), None);
        }
        assert_eq!(retry(Ok(3)).unwrap(), Some(3));
        assert_eq!(
            retry(Err(Errno::BADF)).unwrap_err().raw_os_error(),
            Some(Errno::BADF.raw_os_error())
        );
        let (socket, _peer) = UnixStream::pair().unwrap();
        let signal = AtomicUsize::new(0);
        let mut calls = 0;
        assert_eq!(
            operation(socket.as_fd(), &signal, PollFlags::OUT, &mut || {
                calls += 1;
                if calls == 1 { Err(Errno::AGAIN) } else { Ok(7) }
            })
            .unwrap(),
            7
        );
    }

    #[test]
    fn pipe_and_inherited_writer_preserve_bytes_and_blocking_modes() {
        let signal = Arc::new(AtomicUsize::new(0));
        let (socket, peer) = UnixStream::pair().unwrap();
        let mut input = Pipe::new(socket.into(), &signal).unwrap();
        let mut output = Pipe::new(peer.into(), &signal).unwrap();
        assert_eq!(input.read(&mut []).unwrap(), 0);
        assert_eq!(output.write(&[]).unwrap(), 0);
        output.write_all(b"bytes").unwrap();
        output.flush().unwrap();
        let mut bytes = [0; 5];
        input.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"bytes");
        let ctx = crate::Context::default();
        let (socket, mut peer) = UnixStream::pair().unwrap();
        let before = fcntl_getfl(&socket).unwrap();
        let mut output = ctx.writer(socket.as_fd());
        assert_eq!(output.write(&[]).unwrap(), 0);
        assert_eq!(output.write(&[b'x'; 1024]).unwrap(), 512);
        output.write_all(b"!").unwrap();
        output.flush().unwrap();
        let mut bytes = [0; 513];
        peer.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes[..512], &[b'x'; 512]);
        assert_eq!(bytes[512], b'!');
        assert_eq!(
            fcntl_getfl(&socket).unwrap().contains(OFlags::NONBLOCK),
            before.contains(OFlags::NONBLOCK)
        );
        let mut file = tempfile::tempfile().unwrap();
        ctx.writer(file.as_fd()).write_all(&[b'z'; 4096]).unwrap();
        file.rewind().unwrap();
        let mut bytes = [0; 4096];
        file.read_exact(&mut bytes).unwrap();
        assert_eq!(bytes, [b'z'; 4096]);
    }

    #[test]
    fn cancellation_interrupts_empty_reads_and_full_writes() {
        for writing in [false, true] {
            let signal = Arc::new(AtomicUsize::new(0));
            let (socket, _peer) = UnixStream::pair().unwrap();
            let mut pipe = Pipe::new(socket.into(), &signal).unwrap();
            let started = std::time::Instant::now();
            std::thread::scope(|scope| {
                scope.spawn(|| {
                    std::thread::sleep(Duration::from_millis(120));
                    signal.store(15, Ordering::Relaxed);
                });
                let error = if writing {
                    pipe.write_all(&vec![b'x'; 4 * 1024 * 1024]).unwrap_err()
                } else {
                    pipe.read(&mut [0]).unwrap_err()
                };
                assert_eq!(error.kind(), io::ErrorKind::Other);
            });
            assert!(started.elapsed() < Duration::from_secs(5));
            assert_eq!(pipe.flush().unwrap_err().kind(), io::ErrorKind::Other);
        }
    }
}
