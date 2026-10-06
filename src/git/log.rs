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

use std::io::{self, BufRead};
use std::process::Command;

use crate::Context;
use crate::args::LogArgs;
use crate::error::Error;
use crate::oid::Oid;

use super::{Process, command};

const MARKER: &[u8] = b"\x1e\x1fghist\n";

pub fn log_command(ctx: &Context, args: &LogArgs, mailmap: bool, color: bool) -> Command {
    let mut command = command(ctx);
    command.args([
        "log",
        "--no-diff-merges",
        "--topo-order",
        "--parents",
        "--abbrev=4",
        "--decorate=full",
        "--no-follow",
        "--no-show-signature",
        "--no-ext-diff",
        if color {
            "--color=always"
        } else {
            "--color=never"
        },
    ]);
    let identities = if mailmap {
        "%aN%x00%aE%x00%ai%x00%cN%x00%cE%x00%ci"
    } else {
        "%an%x00%ae%x00%ai%x00%cn%x00%ce%x00%ci"
    };
    command.arg(format!(
        "--format=tformat:%x1e%x1fghist%n%H%x00%h%x00%P%x00%p%x00{identities}%x00%D%x00%B%x00"
    ));
    if args.patch {
        command.arg("-p");
    }
    if args.stat {
        command.args(["--raw", "--numstat"]);
    }
    command.arg("--end-of-options").args(&args.before);
    if let Some(paths) = &args.after {
        command.arg("--").args(paths);
    }
    command
}

type Visitor<'a> = dyn FnMut(Record, &mut LogReader<'_>) -> Result<(), Error> + 'a;

pub fn walk(
    ctx: &Context,
    args: &LogArgs,
    mailmap: bool,
    color: bool,
    flush: &mut dyn FnMut() -> io::Result<()>,
    visit: &mut Visitor<'_>,
) -> Result<Vec<u8>, Error> {
    let mut process = Process::spawn(&mut log_command(ctx, args, mailmap, color))?;
    let mut reader = super::buffer::FlushReader::new(&mut process.stdout, flush);
    let parsed = read_records(&mut reader, visit);
    let stderr = process.finish()?;
    match parsed {
        Ok(()) => Ok(stderr),
        Err(error) => Err(error.with_stderr(stderr)),
    }
}

fn read_records(reader: &mut dyn BufRead, visit: &mut Visitor<'_>) -> Result<(), Error> {
    let mut reader = LogReader::new(reader);
    while let Some(record) = reader.next_record()? {
        visit(record, &mut reader)?;
    }
    Ok(())
}

#[derive(Debug, Eq, PartialEq)]
pub struct Parent {
    pub oid: Oid,
    pub hash: Vec<u8>,
    pub unique: usize,
}

#[derive(Debug, Eq, PartialEq)]
pub struct Record {
    pub oid: Oid,
    pub hash: Vec<u8>,
    pub unique: usize,
    pub parents: Vec<Parent>,
    pub author_name: Vec<u8>,
    pub author_email: Vec<u8>,
    pub author_date: Vec<u8>,
    pub committer_name: Vec<u8>,
    pub committer_email: Vec<u8>,
    pub committer_date: Vec<u8>,
    pub decorations: Vec<u8>,
    pub message: Vec<u8>,
}

impl Record {
    pub fn parse(fields: [Vec<u8>; 12]) -> Result<Self, Error> {
        let [
            hash,
            short,
            parents,
            short_parents,
            author_name,
            author_email,
            author_date,
            committer_name,
            committer_email,
            committer_date,
            decorations,
            message,
        ] = fields;
        let oid = Oid::parse(&hash)?;
        let unique = abbreviation(&hash, &short)?;
        let full: Vec<_> = parents
            .split(|&byte| byte == b' ')
            .filter(|part| !part.is_empty())
            .collect();
        let short: Vec<_> = short_parents
            .split(|&byte| byte == b' ')
            .filter(|part| !part.is_empty())
            .collect();
        if full.len() != short.len() {
            return Err(Error::Protocol("parent counts differ"));
        }
        let parents = full
            .into_iter()
            .zip(short)
            .map(|(full, short)| {
                if full.len() != hash.len() {
                    return Err(Error::Protocol("parent object format differs"));
                }
                Ok(Parent {
                    oid: Oid::parse(full)?,
                    hash: full.to_vec(),
                    unique: abbreviation(full, short)?,
                })
            })
            .collect::<Result<_, Error>>()?;
        Ok(Self {
            oid,
            hash,
            unique,
            parents,
            author_name,
            author_email,
            author_date,
            committer_name,
            committer_email,
            committer_date,
            decorations,
            message,
        })
    }
}

fn abbreviation(full: &[u8], short: &[u8]) -> Result<usize, Error> {
    if short.is_empty() || !full.starts_with(short) {
        Err(Error::Protocol("invalid object ID abbreviation"))
    } else {
        Ok(short.len())
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum State {
    Marker,
    Fields,
    Separator,
    Diff,
    Done,
}

pub struct LogReader<'a> {
    reader: &'a mut dyn BufRead,
    state: State,
    line: Vec<u8>,
}

impl<'a> LogReader<'a> {
    pub fn new(reader: &'a mut dyn BufRead) -> Self {
        Self {
            reader,
            state: State::Marker,
            line: Vec::new(),
        }
    }

    pub fn next_record(&mut self) -> Result<Option<Record>, Error> {
        while self.diff_line()?.is_some() {}
        if self.state == State::Done {
            return Ok(None);
        }
        if self.state != State::Fields {
            self.read_line()?;
            if self.line.is_empty() {
                self.state = State::Done;
                return Ok(None);
            }
            if self.line != MARKER {
                return Err(Error::Protocol("invalid record marker"));
            }
        }
        let mut fields = std::array::from_fn(|_| Vec::new());
        for field in &mut fields {
            self.reader.read_until(0, field)?;
            if field.pop() != Some(0) {
                return Err(Error::Protocol("unterminated record field"));
            }
        }
        let mut newline = [0];
        match self.reader.read_exact(&mut newline) {
            Ok(()) if newline == *b"\n" => {}
            Ok(()) => return Err(Error::Protocol("invalid record terminator")),
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
                return Err(Error::Protocol("missing record terminator"));
            }
            Err(error) => return Err(error.into()),
        }
        self.state = State::Separator;
        Ok(Some(Record::parse(fields)?))
    }

    pub fn diff_line(&mut self) -> Result<Option<&[u8]>, Error> {
        if !matches!(self.state, State::Separator | State::Diff) {
            return Ok(None);
        }
        self.read_line()?;
        if self.line.is_empty() {
            self.state = State::Done;
            return Ok(None);
        }
        if self.line == MARKER {
            self.state = State::Fields;
            return Ok(None);
        }
        if self.state == State::Separator {
            if self.line != b"\n" {
                return Err(Error::Protocol("missing diff separator"));
            }
            self.state = State::Diff;
            self.read_line()?;
            if self.line.is_empty() {
                return Err(Error::Protocol("empty diff section"));
            }
        }
        if self.line.starts_with(b"\x1e") {
            return Err(Error::Protocol("invalid diff framing"));
        }
        Ok(Some(&self.line))
    }

    fn read_line(&mut self) -> Result<(), Error> {
        self.line.clear();
        self.reader.read_until(b'\n', &mut self.line)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufReader;

    fn fields() -> [Vec<u8>; 12] {
        [
            b"0123456789abcdef0123456789abcdef01234567".as_slice(),
            b"0123",
            b"",
            b"",
            b"Author",
            b"a@example.com",
            b"2026-10-05 01:02:03 +0000",
            b"Author",
            b"a@example.com",
            b"2026-10-05 01:02:03 +0000",
            b"HEAD -> refs/heads/main",
            b"message\n",
        ]
        .map(<[u8]>::to_vec)
    }

    fn encode(fields: &[Vec<u8>; 12]) -> Vec<u8> {
        let mut bytes = MARKER.to_vec();
        for field in fields {
            bytes.extend_from_slice(field);
            bytes.push(0);
        }
        bytes.push(b'\n');
        bytes
    }

    #[test]
    fn message_marker_and_chunk_boundaries() {
        let mut fields = fields();
        fields[11] = b"\nsubject\n\x1e\x1fghist\n\xff\t\n".to_vec();
        let bytes = encode(&fields);
        for capacity in [1, 2, 7, 64, 8192] {
            let mut input = BufReader::with_capacity(capacity, bytes.as_slice());
            let mut reader = LogReader::new(&mut input);
            assert_eq!(reader.next_record().unwrap().unwrap().message, fields[11]);
            assert_eq!(reader.next_record().unwrap(), None);
            assert_eq!(reader.next_record().unwrap(), None);
        }
    }

    #[test]
    fn diffs_and_adjacent_records() {
        let bytes = encode(&fields());
        let patch = b"diff --git a/a b/a\n+\x1e\x1fghist\n-old \t\n";
        let stream = [bytes.as_slice(), b"\n", patch, &bytes, &bytes].concat();
        let mut input = stream.as_slice();
        let mut reader = LogReader::new(&mut input);
        reader.next_record().unwrap().unwrap();
        let mut actual = Vec::new();
        while let Some(line) = reader.diff_line().unwrap() {
            actual.extend_from_slice(line);
        }
        assert_eq!(actual, patch);
        assert_eq!(
            reader.next_record().unwrap(),
            Some(Record::parse(fields()).unwrap())
        );
        assert_eq!(
            reader.next_record().unwrap(),
            Some(Record::parse(fields()).unwrap())
        );
        assert_eq!(reader.next_record().unwrap(), None);
        let mut input = stream.as_slice();
        let mut reader = LogReader::new(&mut input);
        for _ in 0..3 {
            reader.next_record().unwrap().unwrap();
        }
        assert_eq!(reader.next_record().unwrap(), None);
    }

    #[test]
    fn empty_and_truncated_streams() {
        assert_eq!(
            LogReader::new(&mut b"".as_slice()).next_record().unwrap(),
            None
        );
        let bytes = encode(&fields());
        for end in 1..bytes.len() {
            let mut input = &bytes[..end];
            assert_eq!(
                LogReader::new(&mut input).next_record().unwrap_err().exit(),
                crate::Exit::Code(1)
            );
        }
        let mut bytes = bytes;
        *bytes.last_mut().unwrap() = b'x';
        assert_eq!(
            LogReader::new(&mut bytes.as_slice())
                .next_record()
                .unwrap_err()
                .to_string(),
            "invalid git output: invalid record terminator"
        );
    }

    #[test]
    fn invalid_diff_framing() {
        let record = encode(&fields());
        for tail in [
            b"unexpected\n".as_slice(),
            b"\n",
            b"\n\x1ewrong\n",
            b"\nline\n\x1ewrong\n",
        ] {
            let bytes = [record.as_slice(), tail].concat();
            let mut input = bytes.as_slice();
            let mut reader = LogReader::new(&mut input);
            reader.next_record().unwrap().unwrap();
            assert_eq!(
                reader.next_record().unwrap_err().exit(),
                crate::Exit::Code(1)
            );
        }
    }

    #[test]
    fn parent_validation_and_sha256() {
        let mut fields = fields();
        fields[2] = b"fedcba9876543210fedcba9876543210fedcba98".to_vec();
        fields[3] = b"fedc".to_vec();
        let record = Record::parse(fields.clone()).unwrap();
        assert_eq!(record.parents[0].unique, 4);
        for index in [0, 1, 2, 3] {
            let mut bad = fields.clone();
            bad[index] = b"bad".to_vec();
            assert_eq!(Record::parse(bad).unwrap_err().exit(), crate::Exit::Code(1));
        }
        fields[3].clear();
        assert_eq!(
            Record::parse(fields.clone()).unwrap_err().to_string(),
            "invalid git output: parent counts differ"
        );
        fields[1].clear();
        assert_eq!(
            Record::parse(fields.clone()).unwrap_err().to_string(),
            "invalid git output: invalid object ID abbreviation"
        );
        fields[0] = vec![b'a'; 64];
        fields[1] = b"aaaa".to_vec();
        fields[2] = vec![b'f'; 64];
        fields[3] = b"ffff".to_vec();
        assert_eq!(Record::parse(fields).unwrap().hash.len(), 64);
    }
    struct FailRead<'a> {
        bytes: &'a [u8],
        left: usize,
    }

    impl io::Read for FailRead<'_> {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            let count = self.fill_buf()?.read(out)?;
            self.consume(count);
            Ok(count)
        }
    }

    impl BufRead for FailRead<'_> {
        fn fill_buf(&mut self) -> io::Result<&[u8]> {
            if self.left == 0 {
                return Err(io::Error::other("read failed"));
            }
            Ok(self.bytes.get(..self.left).unwrap_or(self.bytes))
        }
        fn consume(&mut self, count: usize) {
            self.left = self.left.saturating_sub(count);
            self.bytes = self.bytes.get(count..).unwrap_or_default();
        }
    }

    #[test]
    fn io_errors_at_each_byte_boundary() {
        let record = encode(&fields());
        let bytes = [record.as_slice(), b"\npatch\n"].concat();
        for left in 0..=bytes.len() {
            let mut input = FailRead {
                bytes: &bytes,
                left,
            };
            let error = read_records(&mut input, &mut |_, _| Ok(())).unwrap_err();
            assert_eq!(error.to_string(), "read failed");
        }
        let mut input = FailRead {
            bytes: &[],
            left: 1,
        };
        assert_eq!(LogReader::new(&mut input).next_record().unwrap(), None);
    }

    #[test]
    fn visitor_failure_and_diff_eof() {
        let record = encode(&fields());
        let error = read_records(&mut record.as_slice(), &mut |_, _| {
            Err(Error::Io(io::Error::other("output failed")))
        })
        .unwrap_err();
        assert_eq!(error.to_string(), "output failed");
        let bytes = [record.as_slice(), b"\npatch without newline"].concat();
        let mut seen = Vec::new();
        read_records(&mut bytes.as_slice(), &mut |_, reader| {
            while let Some(line) = reader.diff_line()? {
                seen.extend_from_slice(line);
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(seen, b"patch without newline");
    }

    #[test]
    fn command_neutralizes_config_and_preserves_git_operands() {
        let ctx = Context::default();
        for mailmap in [true, false] {
            let args = LogArgs {
                patch: true,
                stat: true,
                before: vec!["a..b".into(), "path".into()],
                after: Some(vec!["-p".into()]),
            };
            let command = log_command(&ctx, &args, mailmap, false);
            let invocation: Vec<_> = command
                .get_args()
                .map(|arg| arg.to_str().unwrap())
                .collect();
            assert_eq!(
                invocation[..11],
                [
                    "--no-pager",
                    "log",
                    "--no-diff-merges",
                    "--topo-order",
                    "--parents",
                    "--abbrev=4",
                    "--decorate=full",
                    "--no-follow",
                    "--no-show-signature",
                    "--no-ext-diff",
                    "--color=never"
                ]
            );
            assert!(invocation[11].contains(if mailmap { "%aN%x00%aE" } else { "%an%x00%ae" }));
            assert_eq!(
                invocation[12..],
                [
                    "-p",
                    "--raw",
                    "--numstat",
                    "--end-of-options",
                    "a..b",
                    "path",
                    "--",
                    "-p"
                ]
            );
        }
        let command = log_command(&ctx, &LogArgs::default(), true, true);
        assert_eq!(command.get_args().last().unwrap(), "--end-of-options");
        assert!(command.get_args().any(|arg| arg == "--color=always"));
    }
}
