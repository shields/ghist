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
use crate::graph::{Graph, Prefixes};
use crate::oid::Oid;
use std::num::NonZeroUsize;

pub struct Renderer<'a> {
    out: &'a mut dyn Write,
    first: bool,
    colors: Option<&'a Palette>,
    graph: Graph<Oid>,
    prefixes: Prefixes,
}

impl<'a> Renderer<'a> {
    pub fn new(out: &'a mut dyn Write, colors: Option<&'a Palette>) -> Self {
        Self {
            out,
            first: true,
            colors,
            graph: Graph::new(
                colors
                    .and_then(|palette| NonZeroUsize::new(palette.graph.len()))
                    .unwrap_or(NonZeroUsize::MIN),
            ),
            prefixes: Prefixes::default(),
        }
    }

    pub fn commit(&mut self, record: &Record) -> io::Result<()> {
        if !self.first {
            write_line(self.out, self.prefixes.separator(), b"")?;
        }
        self.first = false;
        let parents: Vec<_> = record.parents.iter().map(|parent| parent.oid).collect();
        self.prefixes.paint(
            self.graph.next(&record.oid, &parents),
            self.colors.map(|palette| palette.graph.as_slice()),
        );
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
        self.finish()
    }

    fn finish(&mut self) -> io::Result<()> {
        for prefix in self.prefixes.leftover() {
            write_line(self.out, prefix, b"")?;
        }
        Ok(())
    }

    fn line(&mut self, text: &[u8]) -> io::Result<()> {
        write_line(self.out, self.prefixes.next_line(text.is_empty()), text)
    }
}

fn write_line(out: &mut dyn Write, prefix: &[u8], text: &[u8]) -> io::Result<()> {
    out.write_all(prefix)?;
    out.write_all(text)?;
    out.write_all(b"\n")
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
    fn merge_mockup_aligns_headers_and_message() {
        let mut record = header::tests::record();
        record.committer_name = b"Ada".to_vec();
        record.committer_email = b"ada@example.com".to_vec();
        record.committer_date = b"2026-10-05 20:41:13 -0700".to_vec();
        record.message = b"Add the frobnicator\n".to_vec();
        record.parents = (*b"12")
            .map(|digit| {
                let hash = vec![digit; 40];
                crate::git::log::Parent {
                    oid: Oid::parse(&hash).unwrap(),
                    hash,
                    unique: 4,
                }
            })
            .into_iter()
            .collect();
        let mut out = Vec::new();
        Renderer::new(&mut out, None).commit(&record).unwrap();
        assert_eq!(
            out,
            concat!(
                "●    sha1 0123456789abcdef0123456789abcdef01234567 (HEAD -> main)\n",
                "├─╮  Merge: 1111111 2222222\n",
                "│ │  Author:     Grace <grace@example.com>\n",
                "│ │  Commit:     Ada <ada@example.com>\n",
                "│ │  AuthorDate: 2026-10-05 09:12:00 -0400\n",
                "│ │  CommitDate: 2026-10-05 20:41:13 -0700\n",
                "│ │\n",
                "│ │      Add the frobnicator\n",
            )
            .as_bytes()
        );
    }

    #[test]
    fn leftover_rows_are_written_and_propagate_errors() {
        let record = header::tests::record();
        let mut shape = crate::graph::Shape::default();
        let mut graph = Graph::new(NonZeroUsize::MIN);
        let generated = graph.next(&record.oid, &[]);
        shape.rows.clone_from(&generated.rows);
        shape.width = generated.width;
        let mut out = Vec::new();
        let mut renderer = Renderer::new(&mut out, None);
        renderer.prefixes.paint(&shape, None);
        renderer.finish().unwrap();
        assert_eq!(out, "●\n".as_bytes());
        for count in 0..out.len() {
            let mut writer = FailAfter(count);
            let mut renderer = Renderer::new(&mut writer, None);
            renderer.prefixes.paint(&shape, None);
            assert_eq!(renderer.finish().unwrap_err().to_string(), "output failed");
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
        assert!(
            out.windows(14)
                .any(|part| part == "    body\n\n● ".as_bytes())
        );
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
