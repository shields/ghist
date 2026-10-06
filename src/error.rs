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
use std::{fmt, io};

use crate::Exit;

#[derive(Debug)]
pub enum Error {
    Usage(OsString),
    Io(io::Error),
}

impl Error {
    pub const fn exit(&self) -> Exit {
        match self {
            Self::Usage(_) => Exit::Code(2),
            Self::Io(_) => Exit::Code(1),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(arg) => write!(
                f,
                "unknown option: {}; see ghist --help",
                arg.to_string_lossy()
            ),
            Self::Io(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Usage(_) => None,
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
    fn usage_error() {
        let error = Error::Usage("--bad".into());
        assert_eq!(error.exit(), Exit::Code(2));
        assert_eq!(error.to_string(), "unknown option: --bad; see ghist --help");
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
