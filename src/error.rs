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

use std::ffi::OsString;
use std::io::Write;
use std::{fmt, io};

use crate::Exit;

#[derive(Debug)]
pub enum Error {
    Usage(OsString),
    CompletionShell,
    Io(io::Error),
    ChildIo(io::Error),
    Protocol(&'static str),
    Config {
        key: Vec<u8>,
        value: Option<Vec<u8>>,
    },
    Git {
        exit: Exit,
        stderr: Vec<u8>,
    },
    Pager(Exit),
    WithStderr {
        error: Box<Self>,
        stderr: Vec<u8>,
    },
}

impl Error {
    pub fn is_broken_pipe(&self) -> bool {
        match self {
            Self::Io(error) => error.kind() == io::ErrorKind::BrokenPipe,
            Self::WithStderr { error, .. } => error.is_broken_pipe(),
            _ => false,
        }
    }

    pub fn with_stderr(self, stderr: Vec<u8>) -> Self {
        if stderr.is_empty() {
            self
        } else {
            Self::WithStderr {
                error: Box::new(self),
                stderr,
            }
        }
    }

    pub fn report(&self, err: &mut dyn Write) -> io::Result<()> {
        match self {
            Self::Git { stderr, .. } => err.write_all(stderr)?,
            Self::Pager(_) => {}
            Self::WithStderr { error, stderr } => {
                err.write_all(stderr)?;
                return error.report(err);
            }
            _ => writeln!(err, "ghist: {self}")?,
        }
        err.flush()
    }

    pub const fn exit(&self) -> Exit {
        match self {
            Self::Usage(_) | Self::CompletionShell => Exit::Code(2),
            Self::Io(_) | Self::ChildIo(_) | Self::Protocol(_) => Exit::Code(1),
            Self::Config { .. } => Exit::Code(128),
            Self::Git { exit, .. } | Self::Pager(exit) => *exit,
            Self::WithStderr { error, .. } => error.exit(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CompletionShell => {
                write!(f, "--completions requires bash or zsh; see ghist --help")
            }
            Self::Usage(arg) => write!(
                f,
                "unknown option: {}; see ghist --help",
                arg.to_string_lossy()
            ),
            Self::Io(error) => error.fmt(f),
            Self::ChildIo(error) => write!(f, "git input: {error}"),
            Self::WithStderr { error, .. } => error.fmt(f),
            Self::Pager(exit) => write!(f, "pager failed: {exit:?}"),
            Self::Protocol(message) => write!(f, "invalid git output: {message}"),
            Self::Config { key, value } => write!(
                f,
                "invalid configuration {}: {}",
                String::from_utf8_lossy(key),
                String::from_utf8_lossy(value.as_deref().unwrap_or(b"(implicit true)"))
            ),
            Self::Git { stderr, .. } => {
                write!(f, "git failed: {}", String::from_utf8_lossy(stderr))
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) | Self::ChildIo(error) => Some(error),
            Self::WithStderr { error, .. } => Some(error),
            Self::Usage(_)
            | Self::CompletionShell
            | Self::Protocol(_)
            | Self::Config { .. }
            | Self::Git { .. }
            | Self::Pager(_) => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::*;

    #[test]
    fn child_pipe_errors_are_not_quiet_stdout_failures() {
        let error = Error::ChildIo(io::ErrorKind::BrokenPipe.into());
        assert!(!error.is_broken_pipe());
        assert_eq!(error.exit(), Exit::Code(1));
        assert!(error.to_string().starts_with("git input: "));
        assert!(error.source().is_some());
    }

    #[test]
    fn usage_error() {
        let error = Error::Usage("--bad".into());
        assert_eq!(error.exit(), Exit::Code(2));
        assert_eq!(error.to_string(), "unknown option: --bad; see ghist --help");
        assert!(error.source().is_none());
    }

    #[test]
    fn subprocess_and_configuration_errors() {
        for error in [
            Error::CompletionShell,
            Error::Pager(Exit::Code(42)),
            Error::Protocol("framing"),
            Error::Config {
                key: b"log.mailmap".to_vec(),
                value: Some(b"bad".to_vec()),
            },
            Error::Config {
                key: b"core.pager".to_vec(),
                value: None,
            },
            Error::Git {
                exit: Exit::Code(42),
                stderr: b"failure\xff\n".to_vec(),
            },
        ] {
            assert!(error.source().is_none());
            assert!(!error.is_broken_pipe());
            assert_ne!(error.to_string(), "");
            let mut err = Vec::new();
            error.report(&mut err).unwrap();
            if let Error::Git { .. } = error {
                assert_eq!(error.exit(), Exit::Code(42));
                assert_eq!(err, b"failure\xff\n");
            }
        }
    }

    #[test]
    fn broken_pipe_detection_preserves_diagnostic_wrappers() {
        let error = Error::Io(io::ErrorKind::BrokenPipe.into()).with_stderr(b"warning\n".to_vec());
        assert!(error.is_broken_pipe());
        assert!(!Error::Io(io::Error::other("failure")).is_broken_pipe());
    }

    #[test]
    fn diagnostic_context_preserves_error_and_raw_stderr() {
        let error = Error::Protocol("bad record").with_stderr(b"warning\xff\n".to_vec());
        assert_eq!(error.exit(), Exit::Code(1));
        assert_eq!(error.to_string(), "invalid git output: bad record");
        assert_eq!(error.source().unwrap().to_string(), error.to_string());
        let mut stderr = Vec::new();
        error.report(&mut stderr).unwrap();
        assert_eq!(
            stderr,
            b"warning\xff\nghist: invalid git output: bad record\n"
        );
        let error = Error::Protocol("bad record").with_stderr(Vec::new());
        assert!(error.source().is_none());
    }

    #[test]
    fn io_error() {
        let error = Error::from(io::Error::other("disk full"));
        assert_eq!(error.exit(), Exit::Code(1));
        assert_eq!(error.to_string(), "disk full");
        assert_eq!(error.source().unwrap().to_string(), "disk full");
    }
}
