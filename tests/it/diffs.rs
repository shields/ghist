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
        history::{Change, Commit, History},
        patches,
        repo::TestRepo,
    };

    fn write(path: &[u8], data: &[u8]) -> Change {
        Change::Write {
            mode: 0o100_644,
            path: path.to_vec(),
            data: data.to_vec(),
        }
    }

    #[test]
    fn patches_match_git_with_colors_algorithms_renames_and_root_settings() {
        for format in ["sha1", "sha256"] {
            let repo = TestRepo::new(format).unwrap();
            let mut history = History::default();
            let root = history.push(Commit {
                changes: vec![
                    write(
                        b"text",
                        b"start\nkeep\nold \t\n\xff\n\x1b[31mliteral\x1b[m\nlast",
                    ),
                    write(b"binary", b"a\0b"),
                    write(b"rename me", b"rename\nrename\nrename\n"),
                    Change::Write {
                        mode: 0o120_000,
                        path: b"link".to_vec(),
                        data: b"text".to_vec(),
                    },
                ],
                ..Commit::default()
            });
            let left = history.push(Commit {
                parents: vec![root],
                changes: vec![
                    write(
                        b"text",
                        b"start\nnew \t\nkeep\n\xff\n\x1b[31mliteral\x1b[m\nlast\n",
                    ),
                    write(b"binary", b"changed\0data"),
                    Change::Rename {
                        from: b"rename me".to_vec(),
                        to: "renamed/雪\tfile".as_bytes().to_vec(),
                    },
                ],
                ..Commit::default()
            });
            let right = history.push(Commit {
                branch: "refs/heads/side".into(),
                parents: vec![root],
                changes: vec![write(b"side", b"branch\n")],
                ..Commit::default()
            });
            let merge = history.push(Commit {
                parents: vec![left, right],
                changes: vec![write(b"merge-only", b"not shown\n")],
                ..Commit::default()
            });
            history.push(Commit {
                parents: vec![merge],
                changes: vec![
                    Change::Delete(b"binary".to_vec()),
                    Change::Write {
                        mode: 0o100_755,
                        path: b"text".to_vec(),
                        data: b"executable\n".to_vec(),
                    },
                ],
                ..Commit::default()
            });
            repo.import(&history).unwrap();
            for color in ["never", "always"] {
                repo.git(["config", "color.diff", color]).unwrap();
                for algorithm in ["myers", "minimal", "patience", "histogram"] {
                    repo.git(["config", "diff.algorithm", algorithm]).unwrap();
                    repo.git(["config", "diff.colorMoved", "zebra"]).unwrap();
                    for args in [vec![], vec!["HEAD~2..HEAD"], vec!["HEAD", "--", "text"]] {
                        patches::compare(&repo, &args).unwrap();
                    }
                }
                repo.git(["config", "log.showRoot", "false"]).unwrap();
                patches::compare(&repo, &[]).unwrap();
                repo.git(["config", "log.showRoot", "true"]).unwrap();
            }
        }
    }
}
