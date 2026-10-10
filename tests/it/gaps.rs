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
        oracle,
        repo::TestRepo,
    };

    #[test]
    fn shallow_replace_stash_and_large_octopus_histories() {
        for format in ["sha1", "sha256"] {
            let repo = TestRepo::new(format).unwrap();
            let mut history = History::default();
            let root = history.push(Commit {
                changes: vec![Change::Write {
                    mode: 0o100_644,
                    path: b"file".to_vec(),
                    data: b"root\n".to_vec(),
                }],
                ..Commit::default()
            });
            let mut parents = Vec::new();
            for index in 0..32 {
                parents.push(history.push(Commit {
                    parents: vec![root],
                    message: format!("parent {index}\n").into_bytes(),
                    ..Commit::default()
                }));
            }
            history.push(Commit {
                parents,
                message: b"octopus\n".to_vec(),
                ..Commit::default()
            });
            repo.import(&history).unwrap();
            oracle::fuller(&repo, &[]).unwrap();
            oracle::fuller(&repo.shallow(1).unwrap(), &[]).unwrap();
            let root_oid = repo.git(["rev-parse", "HEAD~2"]).unwrap();
            repo.git([
                "replace",
                "HEAD",
                std::str::from_utf8(root_oid.trim_ascii_end()).unwrap(),
            ])
            .unwrap();
            oracle::fuller(&repo, &[]).unwrap();
            repo.git(["replace", "--delete", "HEAD"]).unwrap();
            repo.git(["reset", "--hard", "HEAD"]).unwrap();
            repo.write("file", b"changed\n").unwrap();
            repo.git(["stash", "push", "--quiet"]).unwrap();
            oracle::fuller(&repo, &["refs/stash"]).unwrap();
        }
    }

    #[test]
    fn ranges_empty_walks_and_path_simplified_merges() {
        let repo = TestRepo::new("sha1").unwrap();
        let mut history = History::default();
        let root = history.push(Commit {
            message: b"root\n".to_vec(),
            changes: vec![Change::Write {
                mode: 0o100_644,
                path: b"chosen".to_vec(),
                data: b"one\n".to_vec(),
            }],
            ..Commit::default()
        });
        let side = history.push(Commit {
            branch: "refs/heads/side".into(),
            parents: vec![root],
            message: b"side\n".to_vec(),
            changes: vec![Change::Write {
                mode: 0o100_644,
                path: b"other".to_vec(),
                data: b"two\n".to_vec(),
            }],
            ..Commit::default()
        });
        let main = history.push(Commit {
            parents: vec![root],
            message: b"main\n".to_vec(),
            changes: vec![Change::Write {
                mode: 0o100_644,
                path: b"chosen".to_vec(),
                data: b"two\n".to_vec(),
            }],
            ..Commit::default()
        });
        history.push(Commit {
            parents: vec![main, side],
            message: b"merge\n".to_vec(),
            ..Commit::default()
        });
        repo.import(&history).unwrap();
        for args in [
            vec![],
            vec!["HEAD..HEAD"],
            vec!["HEAD~1.."],
            vec!["HEAD", "^side"],
            vec!["HEAD...side"],
            vec!["--", "chosen"],
            vec!["HEAD~2..", "--", "chosen"],
        ] {
            oracle::fuller(&repo, &args).unwrap();
        }
    }

    #[test]
    fn a_lane_moves_into_an_old_hole_without_changing_edges() {
        let repo = TestRepo::new("sha1").unwrap();
        let mut history = History::default();
        let mut chain = history.push(Commit {
            message: b"chain 0\n".to_vec(),
            ..Commit::default()
        });
        for index in 1..24 {
            chain = history.push(Commit {
                parents: vec![chain],
                message: format!("chain {index}\n").into_bytes(),
                ..Commit::default()
            });
        }
        let base = history.push(Commit {
            message: b"base\n".to_vec(),
            ..Commit::default()
        });
        let side = history.push(Commit {
            parents: vec![chain],
            message: b"side\n".to_vec(),
            ..Commit::default()
        });
        let merge = history.push(Commit {
            parents: vec![base, chain],
            message: b"merge\n".to_vec(),
            ..Commit::default()
        });
        history.push(Commit {
            parents: vec![base, merge, side],
            message: b"octopus\n".to_vec(),
            ..Commit::default()
        });
        repo.import(&history).unwrap();
        let out = String::from_utf8(oracle::fuller(&repo, &[]).unwrap()).unwrap();
        let twentieth = repo.git(["rev-parse", "HEAD^2^2~19"]).unwrap();
        let twentieth = std::str::from_utf8(twentieth.trim_ascii_end()).unwrap();
        assert!(
            out.contains(&format!("│   ●  sha1 {twentieth}\n│ ╭─╯  Author:")),
            "{out}"
        );
    }
}
