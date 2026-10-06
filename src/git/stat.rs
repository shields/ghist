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

use super::log::LogReader;
use crate::error::Error;

#[derive(Debug)]
pub struct File {
    pub name: Vec<u8>,
    pub change: Change,
}

#[derive(Debug)]
pub enum Change {
    Text {
        added: usize,
        deleted: usize,
    },
    Binary {
        before: usize,
        after: usize,
        same: bool,
    },
}

#[derive(Debug)]
struct Raw {
    old: Vec<u8>,
    new: Vec<u8>,
    absent_old: bool,
    absent_new: bool,
}

impl Raw {
    fn parse(line: &[u8]) -> Result<Self, Error> {
        let header = line
            .strip_prefix(b":")
            .and_then(|line| line.split(|&byte| byte == b'\t').next())
            .ok_or(Error::Protocol("invalid raw diff"))?;
        let parts: Vec<_> = header.split(|&byte| byte == b' ').collect();
        let [old_mode, new_mode, old, new, status] = parts.as_slice() else {
            return Err(Error::Protocol("invalid raw diff fields"));
        };
        if [old_mode, new_mode]
            .iter()
            .any(|mode| mode.len() != 6 || !mode.iter().all(|byte| (b'0'..=b'7').contains(byte)))
            || [old, new].iter().any(|hash| {
                !(4..=64).contains(&hash.len()) || !hash.iter().all(u8::is_ascii_hexdigit)
            })
            || status.is_empty()
            || !line.contains(&b'\t')
        {
            return Err(Error::Protocol("invalid raw diff fields"));
        }
        Ok(Self {
            old: old.to_vec(),
            new: new.to_vec(),
            absent_old: *old_mode == b"000000",
            absent_new: *new_mode == b"000000",
        })
    }

    fn file(
        &self,
        line: &[u8],
        size: &mut dyn FnMut(&[u8]) -> Result<usize, Error>,
    ) -> Result<File, Error> {
        let line = line
            .strip_suffix(b"\n")
            .ok_or(Error::Protocol("unterminated numstat line"))?;
        let mut parts = line.splitn(3, |&byte| byte == b'\t');
        let added = parts.next().unwrap_or_default();
        let deleted = parts
            .next()
            .ok_or(Error::Protocol("missing numstat count"))?;
        let name = parts
            .next()
            .filter(|name| !name.is_empty())
            .ok_or(Error::Protocol("missing numstat path"))?
            .to_vec();
        let change = if added == b"-" && deleted == b"-" {
            let same = self.old == self.new;
            Change::Binary {
                before: if same || self.absent_old {
                    0
                } else {
                    size(&self.old)?
                },
                after: if same || self.absent_new {
                    0
                } else {
                    size(&self.new)?
                },
                same,
            }
        } else {
            let added = number(added)?;
            let deleted = number(deleted)?;
            added
                .checked_add(deleted)
                .ok_or(Error::Protocol("numstat count overflow"))?;
            Change::Text { added, deleted }
        };
        Ok(File { name, change })
    }
}

fn number(bytes: &[u8]) -> Result<usize, Error> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return Err(Error::Protocol("invalid numstat count"));
    }
    std::str::from_utf8(bytes)
        .ok()
        .and_then(|value| value.parse().ok())
        .ok_or(Error::Protocol("numstat count overflow"))
}

pub fn read(
    reader: &mut LogReader<'_>,
    size: &mut dyn FnMut(&[u8]) -> Result<usize, Error>,
) -> Result<Vec<File>, Error> {
    let mut raw = Vec::new();
    while let Some(line) = reader.diff_line()? {
        if line.starts_with(b":") {
            raw.push(Raw::parse(line)?);
        } else {
            let first = raw
                .first()
                .ok_or(Error::Protocol("numstat without raw diff"))?;
            let mut files = vec![first.file(line, size)?];
            for entry in raw.iter().skip(1) {
                let line = reader
                    .diff_line()?
                    .ok_or(Error::Protocol("missing numstat line"))?;
                files.push(entry.file(line, size)?);
            }
            return Ok(files);
        }
    }
    if raw.is_empty() {
        Ok(Vec::new())
    } else {
        Err(Error::Protocol("missing numstat lines"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_diff(diff: &[u8]) -> Result<Vec<File>, Error> {
        let mut bytes = b"\x1e\x1fghist\n".to_vec();
        for field in [
            b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".as_slice(),
            b"aaaa",
            b"",
            b"",
            b"A",
            b"a",
            b"date",
            b"A",
            b"a",
            b"date",
            b"",
            b"",
        ] {
            bytes.extend_from_slice(field);
            bytes.push(0);
        }
        bytes.push(b'\n');
        if !diff.is_empty() {
            bytes.push(b'\n');
            bytes.extend_from_slice(diff);
        }
        let mut input = bytes.as_slice();
        let mut reader = LogReader::new(&mut input);
        reader.next_record()?;
        read(&mut reader, &mut |_| Ok(12))
    }

    #[test]
    fn paired_records_binary_sizes_and_missing_sides() {
        let raw = b":000000 100644 0000 aaaa A\tnew\n:100644 000000 bbbb 0000 D\told\n:100644 100755 cccc cccc M\tmode\n";
        let bytes = [raw.as_slice(), b"1\t0\tnew\n-\t-\told\n0\t0\tmode\n"].concat();
        let files = parse_diff(&bytes).unwrap();
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].name, b"new");
        assert!(matches!(
            files[0].change,
            Change::Text {
                added: 1,
                deleted: 0
            }
        ));
        assert!(matches!(
            files[1].change,
            Change::Binary {
                before: 12,
                after: 0,
                same: false
            }
        ));
        let same = Raw::parse(b":100644 100644 aaaa aaaa R100\told\tnew\n").unwrap();
        let mut lookup = |_: &[u8]| Err(Error::Protocol("size failed"));
        let file = same.file(b"-\t-\told => new\n", &mut lookup).unwrap();
        assert!(matches!(file.change, Change::Binary { same: true, .. }));
        let added = Raw::parse(b":000000 100644 0000 aaaa A\tnew\n").unwrap();
        let file = added
            .file(b"-\t-\tnew\n", &mut |hash| {
                assert_eq!(hash, b"aaaa");
                Ok(9)
            })
            .unwrap();
        assert!(matches!(
            file.change,
            Change::Binary {
                before: 0,
                after: 9,
                same: false
            }
        ));
        let changed = Raw::parse(b":100644 100644 aaaa bbbb M\tnew\n").unwrap();
        assert_eq!(
            changed
                .file(b"-\t-\tnew\n", &mut lookup)
                .unwrap_err()
                .to_string(),
            "invalid git output: size failed"
        );
        for failing in [b"aaaa", b"bbbb"] {
            assert_eq!(
                changed
                    .file(b"-\t-\tnew\n", &mut |hash| {
                        if hash == failing {
                            Err(Error::Protocol("size failed"))
                        } else {
                            Ok(7)
                        }
                    })
                    .unwrap_err()
                    .to_string(),
                "invalid git output: size failed"
            );
        }
        assert!(parse_diff(b"").unwrap().is_empty());
    }

    #[test]
    fn malformed_raw_records_and_numstat_counts() {
        for line in [
            b"".as_slice(),
            b":one\tpath\n",
            b":100648 100644 aaaa bbbb M\tpath\n",
            b":100644 100644 bad bbbb M\tpath\n",
            b":100644 100644 aaaa zzzz M\tpath\n",
            b":100644 100644 aaaa bbbb \tpath\n",
            b":100644 100644 aaaa bbbb M",
        ] {
            assert!(Raw::parse(line).is_err(), "{line:?}");
        }
        let raw = Raw::parse(b":100644 100644 aaaa bbbb M\tpath\n").unwrap();
        let calls = std::cell::Cell::new(0);
        let mut lookup = |_: &[u8]| {
            calls.set(calls.get() + 1);
            Ok(0)
        };
        for line in [
            b"1\t2\tpath".as_slice(),
            b"\n",
            b"1\t0\n",
            b"1\t0\t\n",
            b"-\t1\tpath\n",
            b"\t0\tpath\n",
            b"9999999999999999999999999999999\t0\tpath\n",
        ] {
            assert!(raw.file(line, &mut lookup).is_err(), "{line:?}");
        }
        let overflow = format!("{}\t1\tpath\n", usize::MAX);
        assert_eq!(
            raw.file(overflow.as_bytes(), &mut lookup)
                .unwrap_err()
                .to_string(),
            "invalid git output: numstat count overflow"
        );
        assert_eq!(calls.get(), 0);
        raw.file(b"-\t-\tpath\n", &mut lookup).unwrap();
        assert_eq!(calls.get(), 2);
        for diff in [
            b":100644 100644 aaaa bbbb M\tpath\n".as_slice(),
            b"1\t0\tpath\n",
            b":100644 100644 aaaa bbbb M\tpath\n:100644 100644 cccc dddd M\tsecond\n1\t0\tpath\n",
        ] {
            assert!(parse_diff(diff).is_err(), "{diff:?}");
        }
    }
}
