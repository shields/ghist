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
use std::collections::BTreeMap;
use std::io;

pub fn compare(repo: &TestRepo, args: &[&str]) -> io::Result<()> {
    let mut requested = vec!["-p"];
    requested.extend_from_slice(args);
    let mut invocation = vec![
        "log",
        "--format=%x1e%H",
        "--topo-order",
        "--parents",
        "--abbrev=4",
        "--decorate=full",
        "--no-follow",
        "--no-show-signature",
        "--no-ext-diff",
        "--no-diff-merges",
        "-p",
        "--end-of-options",
    ];
    invocation.extend_from_slice(args);
    let expected = repo.git(invocation)?;
    let ctx = repo.context(&requested);
    let mut actual = Vec::new();
    let mut err = Vec::new();
    let exit = ghist::run(&ctx, &mut actual, &mut err);
    if exit != Exit::Code(0) || !err.is_empty() {
        return Err(io::Error::other(format!("{exit:?}: {err:?}")));
    }
    let actual = graph_text::strip(&actual).map_err(io::Error::other)?;
    let actual = parse(&actual);
    let expected = parse(&expected);
    if actual != expected {
        return Err(io::Error::other(format!(
            "patches differ: {args:?}\nactual: {actual:?}\nexpected: {expected:?}"
        )));
    }
    Ok(())
}

fn parse(bytes: &[u8]) -> BTreeMap<Vec<u8>, Vec<u8>> {
    let mut patches = BTreeMap::<Vec<u8>, Vec<u8>>::new();
    let mut hash = Vec::new();
    let mut in_patch = false;
    for line in bytes.split_inclusive(|&byte| byte == b'\n') {
        let plain = ansi::strip(line);
        let header = plain
            .strip_prefix(b"\x1e")
            .or_else(|| plain.strip_prefix(b"sha1 "))
            .or_else(|| plain.strip_prefix(b"sha256 "));
        if let Some(header) = header {
            hash = header
                .split(u8::is_ascii_whitespace)
                .next()
                .unwrap_or_default()
                .to_vec();
            patches.entry(hash.clone()).or_default();
            in_patch = false;
        } else {
            in_patch |= plain.starts_with(b"diff --git ");
            if in_patch {
                patches
                    .entry(hash.clone())
                    .or_default()
                    .extend_from_slice(line);
            }
        }
    }
    for bytes in patches.values_mut() {
        if bytes.ends_with(b"\n\n") {
            bytes.pop();
        }
    }
    patches
}
