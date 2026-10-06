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

use std::io::{self, BufRead, Write};
use std::process::Stdio;

use super::{Process, buffer::FlushReader, command};
use crate::{Context, error::Error, signal::Pipe};

pub struct Sizes<'a> {
    ctx: &'a Context,
    child: Option<(Process, Pipe)>,
}

impl<'a> Sizes<'a> {
    pub const fn new(ctx: &'a Context) -> Self {
        Self { ctx, child: None }
    }

    pub fn get(
        &mut self,
        hash: &[u8],
        flush: &mut dyn FnMut() -> io::Result<()>,
    ) -> Result<usize, Error> {
        if self.child.is_none() {
            let mut process = Process::spawn(
                self.ctx,
                command(self.ctx)
                    .arg("cat-file")
                    .arg("--batch-check=%(objectsize)")
                    .stdin(Stdio::piped()),
            )?;
            let input = process.input()?;
            self.child = Some((process, input));
        }
        let (process, input) = self.child.as_mut().expect("the size process is started");
        input.write_all(hash).map_err(Error::ChildIo)?;
        input.write_all(b"\n").map_err(Error::ChildIo)?;
        let mut reader = FlushReader::new(&mut process.stdout, flush);
        let mut bytes = Vec::new();
        reader.read_until(b'\n', &mut bytes)?;
        parse(&bytes)
    }

    pub fn finish(self, stop: bool) -> Result<Vec<u8>, Error> {
        self.child.map_or_else(
            || Ok(Vec::new()),
            |(process, input)| {
                drop(input);
                if stop {
                    process.stop()
                } else {
                    process.finish()
                }
            },
        )
    }
}

fn parse(bytes: &[u8]) -> Result<usize, Error> {
    let bytes = bytes
        .strip_suffix(b"\n")
        .ok_or(Error::Protocol("unterminated object size"))?;
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return Err(Error::Protocol("invalid object size"));
    }
    std::str::from_utf8(bytes)
        .ok()
        .and_then(|value| value.parse().ok())
        .ok_or(Error::Protocol("object size overflow"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn object_sizes_and_invalid_responses() {
        assert_eq!(parse(b"0\n").unwrap(), 0);
        assert_eq!(parse(b"123456\n").unwrap(), 123_456);
        for value in [
            b"".as_slice(),
            b"1",
            b"\n",
            b"-1\n",
            b"abcd missing\n",
            b"999999999999999999999999999999\n",
        ] {
            assert!(parse(value).is_err(), "{value:?}");
        }
    }
}
