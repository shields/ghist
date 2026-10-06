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

use std::io::{self, Write};

#[derive(Clone, Debug)]
pub struct Identity {
    pub name: Vec<u8>,
    pub email: Vec<u8>,
    pub date: String,
}

impl Default for Identity {
    fn default() -> Self {
        Self {
            name: b"A U Thor".to_vec(),
            email: b"author@example.com".to_vec(),
            date: "1700000000 +0000".into(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Change {
    Write {
        mode: u32,
        path: Vec<u8>,
        data: Vec<u8>,
    },
    Delete(Vec<u8>),
    Rename {
        from: Vec<u8>,
        to: Vec<u8>,
    },
}

#[derive(Clone, Debug)]
pub struct Commit {
    pub branch: String,
    pub parents: Vec<usize>,
    pub author: Identity,
    pub committer: Identity,
    pub message: Vec<u8>,
    pub encoding: Option<String>,
    pub changes: Vec<Change>,
}

impl Default for Commit {
    fn default() -> Self {
        Self {
            branch: "refs/heads/main".into(),
            parents: Vec::new(),
            author: Identity::default(),
            committer: Identity::default(),
            message: b"message\n".to_vec(),
            encoding: None,
            changes: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct Tag {
    pub name: String,
    pub target: usize,
    pub tagger: Identity,
    pub message: Vec<u8>,
}

#[derive(Debug, Default)]
pub struct History {
    pub commits: Vec<Commit>,
    pub refs: Vec<(String, usize)>,
    pub tags: Vec<Tag>,
}

impl History {
    pub fn push(&mut self, commit: Commit) -> usize {
        self.commits.push(commit);
        self.commits.len()
    }

    pub fn encode(&self, out: &mut dyn Write) -> io::Result<()> {
        for (index, commit) in self.commits.iter().enumerate() {
            if commit.parents.is_empty() {
                writeln!(out, "reset {}\n", commit.branch)?;
            }
            writeln!(out, "commit {}\nmark :{}", commit.branch, index + 1)?;
            identity(out, "author", &commit.author)?;
            identity(out, "committer", &commit.committer)?;
            if let Some(encoding) = &commit.encoding {
                writeln!(out, "encoding {encoding}")?;
            }
            data(out, &commit.message)?;
            for (parent_index, parent) in commit.parents.iter().enumerate() {
                if *parent == 0 || *parent > index {
                    return Err(io::Error::other("parent must refer to an earlier mark"));
                }
                let command = if parent_index == 0 { "from" } else { "merge" };
                writeln!(out, "{command} :{parent}")?;
            }
            for change in &commit.changes {
                match change {
                    Change::Write {
                        mode,
                        path,
                        data: bytes,
                    } => {
                        write!(out, "M {mode:o} inline ")?;
                        quoted(out, path)?;
                        out.write_all(b"\n")?;
                        data(out, bytes)?;
                    }
                    Change::Delete(path) => {
                        out.write_all(b"D ")?;
                        quoted(out, path)?;
                        out.write_all(b"\n")?;
                    }
                    Change::Rename { from, to } => {
                        out.write_all(b"R ")?;
                        quoted(out, from)?;
                        out.write_all(b" ")?;
                        quoted(out, to)?;
                        out.write_all(b"\n")?;
                    }
                }
            }
            out.write_all(b"\n")?;
        }
        for (name, mark) in &self.refs {
            writeln!(out, "reset {name}\nfrom :{mark}\n")?;
        }
        for tag in &self.tags {
            writeln!(out, "tag {}\nfrom :{}", tag.name, tag.target)?;
            identity(out, "tagger", &tag.tagger)?;
            data(out, &tag.message)?;
        }
        out.write_all(b"done\n")
    }
}

fn identity(out: &mut dyn Write, kind: &str, who: &Identity) -> io::Result<()> {
    write!(out, "{kind} ")?;
    out.write_all(&who.name)?;
    out.write_all(b" <")?;
    out.write_all(&who.email)?;
    writeln!(out, "> {}", who.date)
}

fn data(out: &mut dyn Write, bytes: &[u8]) -> io::Result<()> {
    writeln!(out, "data {}", bytes.len())?;
    out.write_all(bytes)?;
    out.write_all(b"\n")
}

fn quoted(out: &mut dyn Write, bytes: &[u8]) -> io::Result<()> {
    out.write_all(b"\"")?;
    for byte in bytes {
        match byte {
            b'"' | b'\\' => out.write_all(&[b'\\', *byte])?,
            0x20..=0x7e => out.write_all(&[*byte])?,
            _ => write!(out, "\\{byte:03o}")?,
        }
    }
    out.write_all(b"\"")
}
