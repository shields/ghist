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

#[cfg(test)]
mod tests {
    use crate::common::{
        history::{Commit, History},
        repo::TestRepo,
    };
    use ghist::Exit;
    use std::collections::BTreeMap;

    fn incoming(bytes: &[u8], git: bool) -> BTreeMap<Vec<u8>, Vec<u8>> {
        let mut result = BTreeMap::new();
        let mut previous = Vec::<(char, Vec<u8>)>::new();
        for line in bytes.split(|&byte| byte == b'\n') {
            let cells = colored_cells(line);
            let node = if git { '*' } else { '●' };
            if let Some(index) = cells.iter().position(|(glyph, _)| *glyph == node) {
                let text: String = cells
                    .iter()
                    .skip(index + 1)
                    .map(|(glyph, _)| glyph)
                    .collect();
                let mut words = text.split_whitespace();
                if !git {
                    assert!(words.any(|word| matches!(word, "sha1" | "sha256")));
                }
                let hash = words
                    .find(|word| {
                        matches!(word.len(), 40 | 64)
                            && word.bytes().all(|byte| byte.is_ascii_hexdigit())
                    })
                    .unwrap()
                    .as_bytes()
                    .to_vec();
                if let Some((glyph, color)) = previous.get(index)
                    && matches!(glyph, '|' | '│')
                {
                    result.insert(hash, color.clone());
                }
            }
            previous = cells;
        }
        result
    }

    fn colored_cells(mut bytes: &[u8]) -> Vec<(char, Vec<u8>)> {
        let mut output = Vec::new();
        let mut color = Vec::new();
        while !bytes.is_empty() {
            if bytes.starts_with(b"\x1b[") {
                let end = bytes.iter().position(|&byte| byte == b'm').unwrap() + 1;
                let sgr = &bytes[..end];
                if sgr == b"\x1b[m" || sgr == b"\x1b[0m" {
                    color.clear();
                } else {
                    color = sgr.to_vec();
                }
                bytes = &bytes[end..];
            } else {
                let glyph = std::str::from_utf8(bytes).unwrap().chars().next().unwrap();
                output.push((glyph, color.clone()));
                bytes = &bytes[glyph.len_utf8()..];
            }
        }
        output
    }

    #[test]
    fn lane_colors_match_git_across_merges_and_custom_palettes() {
        for format in ["sha1", "sha256"] {
            let repo = TestRepo::new(format).unwrap();
            let mut history = History::default();
            for index in 0..24 {
                let parents = if index == 0 {
                    vec![]
                } else if index % 4 == 0 {
                    vec![index, index / 2, 1]
                } else {
                    vec![index]
                };
                history.push(Commit {
                    parents,
                    ..Commit::default()
                });
            }
            repo.import(&history).unwrap();
            repo.git(["config", "color.ui", "always"]).unwrap();
            for palette in [
                None,
                Some(""),
                Some("normal"),
                Some("red,green,blue"),
                Some("bold blue, #a1b2c3, 200"),
                Some("red,,blue"),
                Some("red,"),
                Some("red,,"),
                Some("red,blue,"),
            ] {
                if let Some(palette) = palette {
                    repo.git(["config", "log.graphColors", palette]).unwrap();
                }
                // Git takes lane colors modulo the palette size, so an empty
                // palette divides by zero and dies with SIGFPE on x86. A single
                // uncolored entry differs only by reset sequences, which
                // colored_cells ignores, so Git runs with that while ghist
                // still reads the empty palette.
                let overrides: &[&str] = if palette == Some("") {
                    &["-c", "log.graphColors=normal"]
                } else {
                    &[]
                };
                let expected = repo
                    .git(overrides.iter().copied().chain([
                        "log",
                        "--graph",
                        "--color=always",
                        "--topo-order",
                        "--format=%H%n%n",
                        "--no-patch",
                        "--no-show-signature",
                    ]))
                    .unwrap();
                let mut actual = Vec::new();
                let mut err = Vec::new();
                assert_eq!(
                    ghist::run(&repo.context(&[]), &mut actual, &mut err),
                    Exit::Code(0),
                    "{err:?}"
                );
                assert_eq!(err, b"");
                let expected = incoming(&expected, true);
                let observed = incoming(&actual, false);
                assert_eq!(observed.len(), 23);
                let differences: Vec<_> = observed
                    .iter()
                    .filter_map(|(oid, color)| {
                        let expected = expected.get(oid);
                        (Some(color) != expected).then(|| {
                            (
                                String::from_utf8_lossy(oid).into_owned(),
                                color.clone(),
                                expected.cloned(),
                            )
                        })
                    })
                    .collect();
                assert_eq!(
                    observed.keys().collect::<Vec<_>>(),
                    expected.keys().collect::<Vec<_>>()
                );
                assert_eq!(differences, vec![], "{palette:?}");
            }
        }
    }
}
