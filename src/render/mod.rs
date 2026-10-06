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
pub mod stat;
mod width;

use std::collections::HashSet;
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
    hidden: HashSet<Oid>,
    column: usize,
}

impl<'a> Renderer<'a> {
    pub fn new(out: &'a mut dyn Write, colors: Option<&'a Palette>, hidden: HashSet<Oid>) -> Self {
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
            hidden,
            column: 0,
        }
    }

    pub fn commit(&mut self, record: &Record) -> io::Result<()> {
        if !self.first {
            write_line(self.out, self.prefixes.separator(), b"")?;
        }
        self.first = false;
        let parents: Vec<_> = record
            .parents
            .iter()
            .map(|parent| parent.oid)
            .filter(|oid| !self.hidden.contains(oid))
            .collect();
        let shape = self.graph.next(&record.oid, &parents);
        self.column = shape.text_column();
        self.prefixes
            .paint(shape, self.colors.map(|palette| palette.graph.as_slice()));
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

    pub fn stat(
        &mut self,
        files: &[crate::git::stat::File],
        options: stat::Options,
        patch: bool,
    ) -> Result<(), crate::error::Error> {
        if !files.is_empty() {
            self.line(if patch { b"---" } else { b"" })?;
        }
        let colors = self.colors;
        stat::render(
            files,
            options.columns.saturating_sub(self.column),
            options,
            colors,
            &mut |line| self.line(line),
        )
    }

    pub fn patch_line(&mut self, line: &[u8], first: bool) -> io::Result<()> {
        if first {
            self.line(b"")?;
        }
        self.out.write_all(self.prefixes.next_line(line == b"\n"))?;
        self.out.write_all(line)
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

    struct FailAfter(usize, io::ErrorKind);

    impl Write for FailAfter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.0 == 0 {
                return Err(io::Error::new(self.1, "output failed"));
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
    fn patch_bytes_and_every_partial_write_are_preserved() {
        let record = header::tests::record();
        let patch = b"diff --git a/x b/x\n+\xff \t\n\n\x1b[32m+text\x1b[m\nlast";
        let mut out = Vec::new();
        let mut renderer = Renderer::new(&mut out, None, HashSet::new());
        renderer.commit(&record).unwrap();
        let mut first = true;
        for line in patch.split_inclusive(|&byte| byte == b'\n') {
            renderer.patch_line(line, first).unwrap();
            first = false;
        }
        assert!(
            out.ends_with(
                b"\n   diff --git a/x b/x\n   +\xff \t\n\n   \x1b[32m+text\x1b[m\n   last"
            )
        );
        for kind in [io::ErrorKind::Other, io::ErrorKind::BrokenPipe] {
            for count in 0..=out.len() {
                let mut writer = FailAfter(count, kind);
                let mut renderer = Renderer::new(&mut writer, None, HashSet::new());
                let result = (|| {
                    renderer.commit(&record)?;
                    let mut first = true;
                    for line in patch.split_inclusive(|&byte| byte == b'\n') {
                        renderer.patch_line(line, first)?;
                        first = false;
                    }
                    Ok::<_, io::Error>(())
                })();
                if count == out.len() {
                    result.unwrap();
                } else {
                    assert_eq!(result.unwrap_err().kind(), kind);
                }
            }
        }
    }

    #[test]
    fn stat_prefixes_and_every_partial_write_are_preserved() {
        let record = header::tests::record();
        let files = [crate::git::stat::File {
            name: b"file".to_vec(),
            change: crate::git::stat::Change::Text {
                added: 2,
                deleted: 1,
            },
        }];
        let options = stat::Options::read(
            &crate::Context::default(),
            &crate::git::config::Config::parse(b"").unwrap(),
        )
        .unwrap();
        let mut out = Vec::new();
        let mut renderer = Renderer::new(&mut out, None, HashSet::new());
        renderer.commit(&record).unwrap();
        renderer.stat(&files, options, false).unwrap();
        assert!(out.ends_with(
            b"\n    file | 3 ++-\n    1 file changed, 2 insertions(+), 1 deletion(-)\n"
        ));
        for kind in [io::ErrorKind::Other, io::ErrorKind::BrokenPipe] {
            for count in 0..=out.len() {
                let mut writer = FailAfter(count, kind);
                let mut renderer = Renderer::new(&mut writer, None, HashSet::new());
                let result = (|| {
                    renderer.commit(&record)?;
                    renderer.stat(&files, options, false)
                })();
                if count == out.len() {
                    result.unwrap();
                } else {
                    let error = result.unwrap_err();
                    assert_eq!(error.to_string(), "output failed");
                    assert_eq!(error.is_broken_pipe(), kind == io::ErrorKind::BrokenPipe);
                }
            }
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
        Renderer::new(&mut out, None, HashSet::new())
            .commit(&record)
            .unwrap();
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
        let mut renderer = Renderer::new(&mut out, None, HashSet::new());
        renderer.prefixes.paint(&shape, None);
        renderer.finish().unwrap();
        assert_eq!(out, "●\n".as_bytes());
        for count in 0..out.len() {
            let mut writer = FailAfter(count, io::ErrorKind::Other);
            let mut renderer = Renderer::new(&mut writer, None, HashSet::new());
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
        let mut renderer = Renderer::new(&mut out, None, HashSet::new());
        renderer.commit(&record).unwrap();
        renderer.commit(&empty).unwrap();
        assert!(
            out.windows(14)
                .any(|part| part == "    body\n\n● ".as_bytes())
        );
        assert!(!out.ends_with(b"\n\n"));
        for kind in [io::ErrorKind::Other, io::ErrorKind::BrokenPipe] {
            for count in 0..out.len() {
                let mut writer = FailAfter(count, kind);
                let mut renderer = Renderer::new(&mut writer, None, HashSet::new());
                let error = renderer
                    .commit(&record)
                    .and_then(|()| renderer.commit(&empty))
                    .unwrap_err();
                assert_eq!(error.kind(), kind);
                assert_eq!(error.to_string(), "output failed");
            }
        }
        FailAfter(1, io::ErrorKind::Other).flush().unwrap();
    }
}
