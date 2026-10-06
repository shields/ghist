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
        ansi,
        history::{Commit, History},
        repo::TestRepo,
    };
    use ghist::Exit;
    use std::collections::BTreeSet;

    fn capture(repo: &TestRepo) -> Vec<u8> {
        let mut out = Vec::new();
        let mut err = Vec::new();
        assert_eq!(
            ghist::run(&repo.context(&[]), &mut out, &mut err),
            Exit::Code(0),
            "{err:?}"
        );
        assert_eq!(err, b"");
        out
    }

    #[test]
    fn deterministic_output_preserves_mainline_and_color_independent_text() {
        for format in ["sha1", "sha256"] {
            let repo = TestRepo::new(format).unwrap();
            let mut history = History::default();
            for index in 0..18 {
                let mut parents = if index == 0 { vec![] } else { vec![index] };
                if index > 2 {
                    parents.push(index / 2);
                }
                history.push(Commit {
                    parents,
                    message: "unicode 界\ttext\n\nbody \t\n".as_bytes().to_vec(),
                    ..Commit::default()
                });
            }
            repo.import(&history).unwrap();
            let plain = capture(&repo);
            assert_eq!(capture(&repo), plain);
            for line in plain.split(|&byte| byte == b'\n') {
                assert!(!line.ends_with(b" ") && !line.ends_with(b"\t"));
            }
            let mainline = repo.git(["rev-list", "--first-parent", "HEAD"]).unwrap();
            let mainline: BTreeSet<_> = mainline
                .split(|&byte| byte == b'\n')
                .filter(|line| !line.is_empty())
                .collect();
            let label = format!("{format} ");
            let mut checked = 0;
            for line in plain.split(|&byte| byte == b'\n') {
                if let Some(start) = line
                    .windows(label.len())
                    .position(|part| part == label.as_bytes())
                {
                    let hash = line[start + label.len()..]
                        .split(|&byte| byte == b' ')
                        .next()
                        .unwrap();
                    if mainline.contains(hash) {
                        assert!(
                            line.starts_with("●".as_bytes()),
                            "{}",
                            String::from_utf8_lossy(line)
                        );
                        checked += 1;
                    }
                }
            }
            assert_eq!(checked, mainline.len());
            repo.git(["config", "color.ui", "always"]).unwrap();
            assert_eq!(ansi::strip(&capture(&repo)), plain);
        }
    }
}
