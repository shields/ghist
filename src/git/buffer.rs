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

use std::io::{self, BufRead, BufReader, Read};

pub trait Buffered: BufRead {
    fn has_buffer(&self) -> bool;
}

impl<R: Read> Buffered for BufReader<R> {
    fn has_buffer(&self) -> bool {
        !self.buffer().is_empty()
    }
}

pub struct FlushReader<'a> {
    reader: &'a mut dyn Buffered,
    flush: &'a mut dyn FnMut() -> io::Result<()>,
}

impl<'a> FlushReader<'a> {
    pub fn new(reader: &'a mut dyn Buffered, flush: &'a mut dyn FnMut() -> io::Result<()>) -> Self {
        Self { reader, flush }
    }
}

impl Read for FlushReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        let buffer = self.fill_buf()?;
        let count = buffer.len().min(output.len());
        output
            .iter_mut()
            .zip(buffer)
            .take(count)
            .for_each(|(out, &byte)| *out = byte);
        self.consume(count);
        Ok(count)
    }
}

impl BufRead for FlushReader<'_> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if !self.reader.has_buffer() {
            (self.flush)()?;
        }
        self.reader.fill_buf()
    }

    fn consume(&mut self, count: usize) {
        self.reader.consume(count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn flushes_only_when_the_input_buffer_is_empty() {
        let calls = Cell::new(0);
        let mut flush = || {
            calls.set(calls.get() + 1);
            Ok(())
        };
        let mut input = BufReader::with_capacity(2, b"abcd".as_slice());
        let mut reader = FlushReader::new(&mut input, &mut flush);
        assert_eq!(reader.read(&mut []).unwrap(), 0);
        assert_eq!(calls.get(), 0);
        let mut byte = [0];
        for (expected, flushes) in [(b'a', 1), (b'b', 1), (b'c', 2), (b'd', 2)] {
            assert_eq!(reader.read(&mut byte).unwrap(), 1);
            assert_eq!(byte, [expected]);
            assert_eq!(calls.get(), flushes);
        }
        assert_eq!(reader.read(&mut byte).unwrap(), 0);
        assert_eq!(calls.get(), 3);
    }

    #[test]
    fn flush_and_input_errors_propagate() {
        struct BadRead;
        impl Read for BadRead {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("read failed"))
            }
        }
        let mut input = BufReader::new(b"data".as_slice());
        let mut flush = || Err(io::Error::other("flush failed"));
        let mut reader = FlushReader::new(&mut input, &mut flush);
        assert_eq!(
            reader.read(&mut [0]).unwrap_err().to_string(),
            "flush failed"
        );
        let mut input = BufReader::new(BadRead);
        let mut flush = || Ok(());
        assert_eq!(
            FlushReader::new(&mut input, &mut flush)
                .read(&mut [0])
                .unwrap_err()
                .to_string(),
            "read failed"
        );
    }
}
