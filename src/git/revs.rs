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
use std::ffi::{OsStr, OsString};
use std::io::{BufRead, Read};
use std::os::unix::ffi::OsStrExt;
use std::process::Command;
use std::slice;

use super::{Process, command};
use crate::{Context, Exit, args::LogArgs, error::Error, oid::Oid};

pub fn hidden(ctx: &Context, args: &LogArgs) -> Result<HashSet<Oid>, Error> {
    if !args.before.iter().any(|arg| {
        let bytes = arg.as_bytes();
        bytes.contains(&b'^') || bytes.windows(2).any(|part| part == b"..")
    }) {
        return Ok(HashSet::new());
    }
    if excludes(ctx, args)? {
        boundaries(ctx, args)
    } else {
        Ok(HashSet::new())
    }
}

fn excludes(ctx: &Context, args: &LogArgs) -> Result<bool, Error> {
    if let Some(paths) = &args.after {
        return Ok(expand(ctx, &args.before, paths)?.unwrap_or(true));
    }
    for operand in &args.before {
        match expand(ctx, slice::from_ref(operand), &[])? {
            Some(false) => {}
            Some(true) => return Ok(true),
            None => return revision(ctx, operand),
        }
    }
    Ok(false)
}

fn expand(
    ctx: &Context,
    revisions: &[OsString],
    paths: &[OsString],
) -> Result<Option<bool>, Error> {
    let mut process = Process::spawn(ctx, &mut expansion(ctx, revisions, paths))?;
    let mut bytes = Vec::new();
    let read = process.stdout.read_to_end(&mut bytes);
    let stderr = match process.finish() {
        Err(Error::Git {
            exit: Exit::Code(_),
            ..
        }) => return Ok(None),
        finished => finished?,
    };
    read.map_err(Error::from)
        .and_then(|_| has_negative(&bytes))
        .map(Some)
        .map_err(|error| error.with_stderr(stderr))
}

fn revision(ctx: &Context, operand: &OsStr) -> Result<bool, Error> {
    match Process::spawn(ctx, &mut probe(ctx, operand))?.finish() {
        Err(Error::Git {
            exit: Exit::Code(_),
            ..
        }) => Ok(false),
        probed => probed.map(|_| true),
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

fn prepass(ctx: &Context) -> Command {
    let mut command = command(ctx);
    // git log resolves an ambiguous short object ID to the commit; these must agree.
    command.args(["-c", "core.disambiguate=committish"]);
    command
}

fn expansion(ctx: &Context, revisions: &[OsString], paths: &[OsString]) -> Command {
    let mut command = prepass(ctx);
    command
        .args(["rev-parse", "--revs-only"])
        .args(revisions)
        .arg("--")
        .args(paths);
    command
}

fn probe(ctx: &Context, operand: &OsStr) -> Command {
    let mut command = prepass(ctx);
    command
        .args(["rev-list", "--no-walk", "--max-count=0", "--end-of-options"])
        .arg(operand);
    command
}

fn walk(ctx: &Context, args: &LogArgs) -> Command {
    let mut command = prepass(ctx);
    command
        .args(["rev-list", "--boundary", "--parents", "--end-of-options"])
        .args(&args.before);
    if let Some(paths) = &args.after {
        command.arg("--").args(paths);
    }
    command
}

fn boundaries(ctx: &Context, args: &LogArgs) -> Result<HashSet<Oid>, Error> {
    let mut process = Process::spawn(ctx, &mut walk(ctx, args))?;
    let result = parse(&mut process.stdout);
    let stderr = process.finish()?;
    result.map_err(|error| error.with_stderr(stderr))
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
        fn words(command: &Command) -> Vec<&str> {
            command
                .get_args()
                .map(|arg| arg.to_str().unwrap())
                .collect()
        }
        let ctx = Context::default();
        let revisions = ["a..b".into(), "^c".into()];
        let paths = ["file".into(), "-p".into()];
        assert_eq!(
            words(&expansion(&ctx, &revisions, &paths)),
            [
                "--no-pager",
                "-c",
                "core.disambiguate=committish",
                "rev-parse",
                "--revs-only",
                "a..b",
                "^c",
                "--",
                "file",
                "-p"
            ]
        );
        assert_eq!(
            words(&expansion(&ctx, &revisions, &[])),
            [
                "--no-pager",
                "-c",
                "core.disambiguate=committish",
                "rev-parse",
                "--revs-only",
                "a..b",
                "^c",
                "--"
            ]
        );
        assert_eq!(
            words(&probe(&ctx, OsStr::new("-p"))),
            [
                "--no-pager",
                "-c",
                "core.disambiguate=committish",
                "rev-list",
                "--no-walk",
                "--max-count=0",
                "--end-of-options",
                "-p"
            ]
        );
        for after in [None, Some(paths.to_vec())] {
            let args = LogArgs {
                before: revisions.to_vec(),
                after: after.clone(),
                ..LogArgs::default()
            };
            let mut expected = vec![
                "--no-pager",
                "-c",
                "core.disambiguate=committish",
                "rev-list",
                "--boundary",
                "--parents",
                "--end-of-options",
                "a..b",
                "^c",
            ];
            if after.is_some() {
                expected.extend(["--", "file", "-p"]);
            }
            assert_eq!(words(&walk(&ctx, &args)), expected);
        }
    }
}
