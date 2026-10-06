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

use super::{ansi, graph_text, repo::TestRepo};
use ghist::Exit;
use std::io;

struct Block {
    hash: String,
    text: Vec<u8>,
    column: usize,
}

fn blocks(bytes: &[u8]) -> io::Result<Vec<Block>> {
    let mut columns = Vec::new();
    let mut column = usize::MAX;
    for line in bytes.split_inclusive(|&byte| byte == b'\n') {
        let plain = ansi::strip(line);
        let mut rest = plain.as_slice();
        let mut width = 0;
        let mut node = false;
        while let Some((glyph, next)) = graph_text::cell(rest) {
            node |= glyph == '●' && width < column;
            width += 1;
            rest = next;
        }
        if node && (rest.starts_with(b"sha1 ") || rest.starts_with(b"sha256 ")) {
            column = width;
            columns.push(column);
        }
    }
    let text = graph_text::strip(bytes).map_err(io::Error::other)?;
    let mut result = Vec::<Block>::new();
    let mut columns = columns.into_iter();
    for line in text.split_inclusive(|&byte| byte == b'\n') {
        let plain = ansi::strip(line);
        if let Some(hash) = plain
            .strip_prefix(b"sha1 ")
            .or_else(|| plain.strip_prefix(b"sha256 "))
        {
            let hash = hash
                .split(u8::is_ascii_whitespace)
                .next()
                .ok_or_else(|| io::Error::other("missing hash"))?;
            result.push(Block {
                hash: String::from_utf8(hash.to_vec()).map_err(io::Error::other)?,
                text: Vec::new(),
                column: columns
                    .next()
                    .ok_or_else(|| io::Error::other("missing graph width"))?,
            });
        }
        result
            .last_mut()
            .ok_or_else(|| io::Error::other("content before commit"))?
            .text
            .extend_from_slice(line);
    }
    for block in &mut result {
        if block.text.ends_with(b"\n\n") {
            block.text.pop();
        }
    }
    Ok(result)
}

fn capture(repo: &TestRepo, args: &[&str], columns: usize) -> io::Result<Vec<Block>> {
    let mut ctx = repo.context(args);
    ctx.env.push(("COLUMNS".into(), columns.to_string().into()));
    let mut out = Vec::new();
    let mut err = Vec::new();
    let exit = ghist::run(&ctx, &mut out, &mut err);
    if exit != Exit::Code(0) || !err.is_empty() {
        return Err(io::Error::other(format!("{exit:?}: {err:?}")));
    }
    blocks(&out)
}

pub fn compare(repo: &TestRepo, args: &[&str], columns: usize) -> io::Result<()> {
    compare_mode(repo, args, columns, false)
}

pub fn combined(repo: &TestRepo, args: &[&str], columns: usize) -> io::Result<()> {
    compare_mode(repo, args, columns, true)
}

fn compare_mode(repo: &TestRepo, args: &[&str], columns: usize, patch: bool) -> io::Result<()> {
    let plain = capture(repo, args, columns)?;
    let mut with_stat = vec!["--stat"];
    if patch {
        with_stat.push("-p");
    }
    with_stat.extend_from_slice(args);
    let observed = capture(repo, &with_stat, columns)?;
    if observed.len() != plain.len() {
        return Err(io::Error::other("stat output changed the commit count"));
    }
    for (actual, plain) in observed.into_iter().zip(plain) {
        let width = columns.saturating_sub(actual.column).max(1);
        let width_arg = format!("--stat={width}");
        let mut invocation = vec![
            "log",
            "--max-count=1",
            "--format=",
            "--no-diff-merges",
            "--abbrev=4",
            "--no-ext-diff",
            "--no-show-signature",
            "--no-follow",
            &width_arg,
        ];
        if patch {
            invocation.push("-p");
        }
        invocation.extend(["--end-of-options", &plain.hash]);
        if let Some(separator) = args.iter().position(|&arg| arg == "--") {
            invocation.extend_from_slice(args.get(separator..).unwrap_or_default());
        }
        let reference = repo.git(invocation)?;
        let mut expected = plain.text;
        if !reference.is_empty() {
            expected.extend_from_slice(if patch { b"---\n" } else { b"\n" });
            expected.extend_from_slice(&reference);
        }
        if actual.text != expected || actual.hash != plain.hash {
            return Err(io::Error::other(format!(
                "stat differs at {} with width {width}\nactual: {}\nexpected: {}",
                plain.hash,
                String::from_utf8_lossy(&actual.text),
                String::from_utf8_lossy(&expected)
            )));
        }
    }
    Ok(())
}
