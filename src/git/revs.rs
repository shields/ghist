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

use std::collections::HashSet;
use std::io::{BufRead, Read};
use std::os::unix::ffi::OsStrExt;
use std::process::Command;

use super::{Process, command};
use crate::{Context, args::LogArgs, error::Error, oid::Oid};

pub fn hidden(ctx: &Context, args: &LogArgs) -> Result<(HashSet<Oid>, Vec<u8>), Error> {
    if !args.before.iter().any(|arg| {
        let bytes = arg.as_bytes();
        bytes.contains(&b'^') || bytes.windows(2).any(|part| part == b"..")
    }) {
        return Ok((HashSet::new(), Vec::new()));
    }
    let mut process = Process::spawn(ctx, &mut invocation(ctx, args, false))?;
    let mut bytes = Vec::new();
    let read = process.stdout.read_to_end(&mut bytes);
    let stderr = process.finish()?;
    let negative = match read.map_err(Error::from).and_then(|_| has_negative(&bytes)) {
        Ok(negative) => negative,
        Err(error) => return Err(error.with_stderr(stderr)),
    };
    if !negative {
        return Ok((HashSet::new(), stderr));
    }
    match boundaries(ctx, args) {
        Ok((hidden, warnings)) => Ok((hidden, [stderr, warnings].concat())),
        Err(error) => Err(error.with_stderr(stderr)),
    }
}

fn has_negative(bytes: &[u8]) -> Result<bool, Error> {
    let mut negative = false;
    for line in bytes
        .split(|&byte| byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let hash = line.strip_prefix(b"^").map_or(line, |hash| {
            negative = true;
            hash
        });
        Oid::parse(hash)?;
    }
    Ok(negative)
}

fn invocation(ctx: &Context, args: &LogArgs, boundary: bool) -> Command {
    let mut command = command(ctx);
    command.args(if boundary {
        &["rev-list", "--boundary", "--parents", "--end-of-options"][..]
    } else {
        &["rev-parse", "--revs-only"][..]
    });
    command.args(&args.before);
    if let Some(paths) = &args.after {
        command.arg("--").args(paths);
    }
    command
}

fn boundaries(ctx: &Context, args: &LogArgs) -> Result<(HashSet<Oid>, Vec<u8>), Error> {
    let mut process = Process::spawn(ctx, &mut invocation(ctx, args, true))?;
    let result = parse(&mut process.stdout);
    let stderr = process.finish()?;
    match result {
        Ok(hidden) => Ok((hidden, stderr)),
        Err(error) => Err(error.with_stderr(stderr)),
    }
}

fn parse(reader: &mut dyn BufRead) -> Result<HashSet<Oid>, Error> {
    let mut result = HashSet::new();
    let mut line = Vec::new();
    while reader.read_until(b'\n', &mut line)? != 0 {
        if let Some(boundary) = line.strip_prefix(b"-") {
            let hash = boundary
                .split(|&byte| byte == b' ' || byte == b'\n')
                .next()
                .unwrap_or_default();
            result.insert(Oid::parse(hash)?);
        }
        line.clear();
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, BufReader};

    #[test]
    fn boundary_records_in_both_formats() {
        struct BadRead;
        impl Read for BadRead {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("read failed"))
            }
        }
        for size in [40, 64] {
            let hash = vec![b'a'; size];
            let input = [
                hash.as_slice(),
                b"\n-",
                &hash,
                b" ",
                &vec![b'b'; size],
                b"\n-",
                &hash,
            ]
            .concat();
            let mut reader = BufReader::with_capacity(1, input.as_slice());
            assert_eq!(
                parse(&mut reader).unwrap(),
                HashSet::from([Oid::parse(&hash).unwrap()])
            );
        }
        assert!(parse(&mut b"".as_slice()).unwrap().is_empty());
        assert!(
            parse(&mut b"-bad\n".as_slice())
                .unwrap_err()
                .to_string()
                .contains("object ID")
        );
        assert_eq!(
            parse(&mut BufReader::new(BadRead)).unwrap_err().to_string(),
            "read failed"
        );
    }

    #[test]
    fn preserves_revisions_paths_and_parent_rewriting() {
        let ctx = Context::default();
        for boundary in [false, true] {
            let args = LogArgs {
                before: vec!["a..b".into(), "^c".into()],
                after: Some(vec!["file".into(), "-p".into()]),
                ..LogArgs::default()
            };
            let command = invocation(&ctx, &args, boundary);
            let actual: Vec<_> = command
                .get_args()
                .map(|arg| arg.to_str().unwrap())
                .collect();
            let mut expected = vec!["--no-pager"];
            if boundary {
                expected.extend(["rev-list", "--boundary", "--parents", "--end-of-options"]);
            } else {
                expected.extend(["rev-parse", "--revs-only"]);
            }
            expected.extend(["a..b", "^c", "--", "file", "-p"]);
            assert_eq!(actual, expected);
        }
    }
}
