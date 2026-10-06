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

use std::collections::{BTreeMap, BTreeSet};

use super::{ansi, graph_text};

type Origins = BTreeSet<Vec<u8>>;
pub type Edges = BTreeMap<Vec<u8>, Origins>;

const U: u8 = 1;
const D: u8 = 2;
const L: u8 = 4;
const R: u8 = 8;

pub fn parse(bytes: &[u8]) -> Result<Edges, String> {
    let bytes = ansi::strip(bytes);
    let mut column = usize::MAX;
    let mut node_column = 0;
    let mut active = BTreeMap::<usize, Origins>::new();
    let mut edges = Edges::new();
    for line in bytes.split_inclusive(|&byte| byte == b'\n') {
        let mut rest = line;
        let mut prefix = Vec::new();
        let mut node = None;
        while let Some((glyph, next)) = graph_text::cell(rest) {
            if glyph == '●' && prefix.len() < column {
                node = Some(prefix.len());
            }
            prefix.push(glyph);
            rest = next;
        }
        let hash = rest
            .strip_prefix(b"sha1 ")
            .or_else(|| rest.strip_prefix(b"sha256 "));
        let commit = if let (Some(node), Some(hash)) = (node, hash) {
            column = prefix.len();
            node_column = node;
            Some((
                node,
                hash.split(|&byte| byte == b' ' || byte == b'\n')
                    .next()
                    .unwrap_or_default()
                    .to_vec(),
            ))
        } else {
            prefix.truncate(column);
            None
        };
        if column == usize::MAX {
            return Err("graph content before the first node".into());
        }
        active = row(&prefix, node_column, commit, &active, &mut edges)?;
    }
    Ok(edges)
}

const fn arms(glyph: char) -> u8 {
    match glyph {
        '│' => U | D,
        '─' => L | R,
        '╭' => D | R,
        '╮' => D | L,
        '╯' => U | L,
        '╰' => U | R,
        '├' => U | D | R,
        '┤' => U | D | L,
        '┬' => D | L | R,
        '┴' => U | L | R,
        '┼' => U | D | L | R,
        _ => 0,
    }
}

fn row(
    glyphs: &[char],
    node_column: usize,
    commit: Option<(usize, Vec<u8>)>,
    active: &BTreeMap<usize, Origins>,
    edges: &mut Edges,
) -> Result<BTreeMap<usize, Origins>, String> {
    let mut ports: Vec<_> = glyphs.iter().copied().map(arms).collect();
    for (index, glyph) in glyphs.iter().enumerate() {
        if *glyph == '│'
            && index
                .checked_sub(1)
                .and_then(|index| glyphs.get(index))
                .is_some_and(|&glyph| arms(glyph) & R != 0)
            && glyphs
                .get(index + 1)
                .is_some_and(|&glyph| arms(glyph) & L != 0)
        {
            *ports
                .get_mut(index)
                .expect("ports and glyphs have equal lengths") |= L | R;
        }
    }
    let mut next = BTreeMap::new();
    for (index, glyph) in glyphs.iter().enumerate() {
        if arms(*glyph) & (U | D) == U | D {
            next.insert(index, active.get(&index).cloned().unwrap_or_default());
        }
    }
    if let Some((index, hash)) = commit {
        for child in active.get(&index).into_iter().flatten() {
            edges.entry(child.clone()).or_default().insert(hash.clone());
        }
        edges.entry(hash.clone()).or_default();
        next.insert(index, BTreeSet::from([hash]));
    }
    let mut start = 0;
    while start < ports.len() {
        if ports.get(start).copied().unwrap_or_default() & R == 0 {
            if ports.get(start).copied().unwrap_or_default() & L != 0 {
                return Err(format!("unconnected left arm: {glyphs:?}"));
            }
            start += 1;
            continue;
        }
        let mut end = start;
        while ports.get(end).copied().unwrap_or_default() & R != 0 {
            end += 1;
            if ports.get(end).copied().unwrap_or_default() & L == 0 {
                return Err(format!("unconnected right arm: {glyphs:?}"));
            }
        }
        let mut origins = Origins::new();
        for index in start..=end {
            if glyphs.get(index) != Some(&'│')
                && ports.get(index).copied().unwrap_or_default() & U != 0
                && (index == node_column || ports.get(index).copied().unwrap_or_default() & D == 0)
            {
                origins.extend(active.get(&index).into_iter().flatten().cloned());
            }
        }
        for index in start..=end {
            if glyphs.get(index) != Some(&'│')
                && ports.get(index).copied().unwrap_or_default() & D != 0
            {
                next.entry(index)
                    .or_default()
                    .extend(origins.iter().cloned());
            }
        }
        start = end + 1;
    }
    Ok(next)
}

pub fn compare(repo: &super::repo::TestRepo, bytes: &[u8], args: &[&str]) -> std::io::Result<()> {
    let mut command = vec![
        "log",
        "--topo-order",
        "--parents",
        "--format=%H %P",
        "--no-patch",
        "--no-show-signature",
        "--no-follow",
        "--color=never",
        "--end-of-options",
    ];
    command.extend_from_slice(args);
    let reference = repo.git(command)?;
    let mut expected = Edges::new();
    for line in reference
        .split(|&byte| byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let mut words = line
            .split(|&byte| byte == b' ')
            .filter(|word| !word.is_empty());
        let hash = words
            .next()
            .ok_or_else(|| std::io::Error::other("missing commit hash"))?;
        expected.insert(hash.to_vec(), words.map(<[u8]>::to_vec).collect());
    }
    let shown: BTreeSet<_> = expected.keys().cloned().collect();
    for parents in expected.values_mut() {
        parents.retain(|parent| shown.contains(parent));
    }
    let observed = parse(bytes).map_err(std::io::Error::other)?;
    if observed != expected {
        return Err(std::io::Error::other(format!(
            "graph edges differ for {args:?}\nactual: {observed:?}\nexpected: {expected:?}\n{}",
            String::from_utf8_lossy(bytes)
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separates_crossings_from_junctions() {
        let output = "●      sha1 a\n├─┬─╮  Merge: b c d\n│ │ │\n● │ │    sha1 b\n├─│─│─╮  Merge: d e\n├─│─╯ │\n│ │   │\n│ ●   │  sha1 c\n├─┼───┤  Merge: d e f\n│ │   │\n│ │   ●  sha1 e\n├─│───╯\n│ │\n│ ●  sha1 f\n├─╯\n●  sha1 d\n\n";
        let expected: Edges = [
            ("a", vec!["b", "c", "d"]),
            ("b", vec!["d", "e"]),
            ("c", vec!["d", "e", "f"]),
            ("d", vec![]),
            ("e", vec!["d"]),
            ("f", vec!["d"]),
        ]
        .into_iter()
        .map(|(node, parents)| {
            (
                node.as_bytes().to_vec(),
                parents
                    .into_iter()
                    .map(|parent| parent.as_bytes().to_vec())
                    .collect(),
            )
        })
        .collect();
        assert_eq!(parse(output.as_bytes()).unwrap(), expected);
        assert!(
            parse("●    sha1 a\n├─   Author: A\n".as_bytes())
                .unwrap_err()
                .contains("unconnected right")
        );
    }
}
