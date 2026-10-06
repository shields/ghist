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

mod header;
mod message;
mod width;

use std::io::{self, Write};

use crate::color::Palette;
use crate::git::log::Record;

pub struct Renderer<'a> {
    out: &'a mut dyn Write,
    first: bool,
    colors: Option<&'a Palette>,
}

impl<'a> Renderer<'a> {
    pub fn new(out: &'a mut dyn Write, colors: Option<&'a Palette>) -> Self {
        Self {
            out,
            first: true,
            colors,
        }
    }

    pub fn commit(&mut self, record: &Record) -> io::Result<()> {
        if !self.first {
            self.line(b"")?;
        }
        self.first = false;
        for line in header::lines(record, self.colors) {
            self.line(&line)?;
        }
        let message = message::lines(&record.message);
        if !message.is_empty() {
            self.line(b"")?;
            for line in message {
                self.line(&line)?;
            }
        }
        Ok(())
    }

    fn line(&mut self, text: &[u8]) -> io::Result<()> {
        self.out.write_all(text)?;
        self.out.write_all(b"\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FailAfter(usize);

    impl Write for FailAfter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.0 == 0 {
                return Err(io::Error::other("output failed"));
            }
            let size = self.0.min(bytes.len());
            self.0 -= size;
            Ok(size)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn separation_empty_messages_and_every_partial_write() {
        let record = header::tests::record();
        let mut empty = header::tests::record();
        empty.message.clear();
        let mut out = Vec::new();
        let mut renderer = Renderer::new(&mut out, None);
        renderer.commit(&record).unwrap();
        renderer.commit(&empty).unwrap();
        assert!(out.windows(14).any(|part| part == b"    body\n\nsha1"));
        assert!(!out.ends_with(b"\n\n"));
        for count in 0..out.len() {
            let mut writer = FailAfter(count);
            let mut renderer = Renderer::new(&mut writer, None);
            let error = renderer
                .commit(&record)
                .and_then(|()| renderer.commit(&empty))
                .unwrap_err();
            assert_eq!(error.to_string(), "output failed");
        }
        FailAfter(1).flush().unwrap();
    }
}
