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
    use std::ffi::OsStr;

    use crate::common::history::{Change, Commit, History, Identity, Tag};
    use crate::common::repo::TestRepo;

    #[test]
    fn imports_byte_exact_histories_in_both_object_formats() {
        for (format, hash_len) in [("sha1", 40), ("sha256", 64)] {
            let repo = TestRepo::new(format).unwrap();
            let mut history = History::default();
            let message = b"\nsubject\t\n\nbody\0tail\xff\n".to_vec();
            let root = history.push(Commit {
                message: message.clone(),
                changes: vec![
                    Change::Write {
                        mode: 0o100_644,
                        path: b"a file\t\xff".to_vec(),
                        data: b"hello\0world\n".to_vec(),
                    },
                    Change::Write {
                        mode: 0o120_000,
                        path: b"link".to_vec(),
                        data: b"a file\t\xff".to_vec(),
                    },
                    Change::Write {
                        mode: 0o100_755,
                        path: b"script".to_vec(),
                        data: b"#!/bin/sh\n".to_vec(),
                    },
                ],
                ..Commit::default()
            });
            let tip = history.push(Commit {
                parents: vec![root],
                encoding: Some("ISO-8859-1".into()),
                message: b"caf\xe9\n".to_vec(),
                committer: Identity {
                    name: b"C O Mmitter".to_vec(),
                    date: "1700000060 -0700".into(),
                    ..Identity::default()
                },
                changes: vec![
                    Change::Rename {
                        from: b"a file\t\xff".to_vec(),
                        to: b"renamed".to_vec(),
                    },
                    Change::Delete(b"script".to_vec()),
                ],
                ..Commit::default()
            });
            history.refs.push(("refs/remotes/origin/main".into(), tip));
            history.refs.push(("refs/tags/light".into(), root));
            history.tags.push(Tag {
                name: "v1".into(),
                target: tip,
                tagger: Identity::default(),
                message: b"release\n".to_vec(),
            });
            repo.import(&history).unwrap();
            assert_eq!(repo.git(["rev-parse", "HEAD"]).unwrap().len(), hash_len + 1);
            assert_eq!(
                repo.git(["show", "HEAD:renamed"]).unwrap(),
                b"hello\0world\n"
            );
            let commit = repo.git(["cat-file", "commit", "HEAD^"]).unwrap();
            assert!(commit.ends_with(&message));
            assert_eq!(
                repo.git(["log", "-1", "--format=%B"]).unwrap(),
                "café\n\n".as_bytes()
            );
            assert_eq!(
                repo.git(["rev-parse", "v1^{}"]).unwrap(),
                repo.git(["rev-parse", "HEAD"]).unwrap()
            );
            assert_eq!(
                repo.git(["rev-parse", "origin/main"]).unwrap(),
                repo.git(["rev-parse", "HEAD"]).unwrap()
            );
            let listing = repo.git(["ls-tree", "HEAD"]).unwrap();
            assert!(listing.starts_with(b"120000 blob "));
            let context = repo.context(&["-p", "HEAD"]);
            assert_eq!(context.cwd, repo.cwd);
            assert_eq!(context.env, repo.env);
            assert_eq!(context.args, ["-p", "HEAD"]);
        }
    }

    #[test]
    fn imports_roots_merges_and_octopus_parents() {
        let repo = TestRepo::new("sha1").unwrap();
        let mut history = History::default();
        let mut parents = Vec::new();
        for i in 0..12 {
            parents.push(history.push(Commit {
                branch: format!("refs/heads/side{i}"),
                message: format!("root{i}\n").into_bytes(),
                ..Commit::default()
            }));
        }
        history.push(Commit {
            parents,
            ..Commit::default()
        });
        repo.import(&history).unwrap();
        let parents = repo.git(["show", "-s", "--format=%P", "HEAD"]).unwrap();
        let mut expected = Vec::new();
        for i in 0..12 {
            if i != 0 {
                expected.push(b' ');
            }
            let oid = repo.git(["rev-parse", &format!("side{i}")]).unwrap();
            expected.extend_from_slice(oid.strip_suffix(b"\n").unwrap());
        }
        expected.push(b'\n');
        assert_eq!(parents, expected);
        assert_eq!(repo.git(["rev-list", "--count", "HEAD"]).unwrap(), b"13\n");
        assert_eq!(
            repo.git(["show", "-s", "--format=%P", "side0"]).unwrap(),
            b"\n"
        );
    }

    #[test]
    fn supports_shallow_stash_replace_and_mailmap_fixtures() {
        let repo = TestRepo::new("sha1").unwrap();
        let mut history = History::default();
        let root = history.push(Commit {
            changes: vec![Change::Write {
                mode: 0o100_644,
                path: b"file".to_vec(),
                data: b"one\n".to_vec(),
            }],
            ..Commit::default()
        });
        history.push(Commit {
            parents: vec![root],
            message: b"tip\n".to_vec(),
            ..Commit::default()
        });
        repo.import(&history).unwrap();
        let clone = repo.shallow(1).unwrap();
        assert_eq!(clone.git(["rev-list", "--count", "HEAD"]).unwrap(), b"1\n");
        assert_eq!(
            clone.git(["rev-parse", "--is-shallow-repository"]).unwrap(),
            b"true\n"
        );
        repo.git(["reset", "--hard"]).unwrap();
        repo.write("file", b"two\n").unwrap();
        repo.git(["stash", "push", "--quiet"]).unwrap();
        assert_eq!(
            repo.git(["rev-parse", "--verify", "refs/stash"])
                .unwrap()
                .len(),
            41
        );
        repo.write(
            ".mailmap",
            b"Mapped <mapped@example.com> A U Thor <author@example.com>\n",
        )
        .unwrap();
        assert_eq!(
            repo.git(["log", "-1", "--format=%aN <%aE>"]).unwrap(),
            b"Mapped <mapped@example.com>\n"
        );
        repo.git(["replace", "HEAD", "HEAD^"]).unwrap();
        assert_eq!(
            repo.git(["log", "-1", "--format=%s"]).unwrap(),
            b"message\n"
        );
    }

    #[test]
    fn isolates_environment_and_git_failures() {
        let repo = TestRepo::new("sha1").unwrap();
        for (key, _) in &repo.env {
            assert!(
                [
                    "PATH",
                    "TMPDIR",
                    "LLVM_PROFILE_FILE",
                    "HOME",
                    "GIT_CONFIG_GLOBAL",
                    "GIT_CONFIG_NOSYSTEM",
                    "GIT_CONFIG_COUNT",
                    "GIT_CONFIG_KEY_0",
                    "GIT_CONFIG_VALUE_0",
                    "LC_ALL",
                    "TZ"
                ]
                .iter()
                .any(|allowed| key == OsStr::new(allowed))
            );
        }
        assert_eq!(repo.git(["config", "--get", "gc.auto"]).unwrap(), b"0\n");
        let error = repo.git(["rev-parse", "--verify", "missing"]).unwrap_err();
        assert!(error.to_string().contains("git exited"));
    }

    #[test]
    fn rejects_forward_and_null_parent_marks() {
        for parent in [0, 1] {
            let mut history = History::default();
            history.push(Commit {
                parents: vec![parent],
                ..Commit::default()
            });
            assert_eq!(
                history.encode(&mut Vec::new()).unwrap_err().to_string(),
                "parent must refer to an earlier mark"
            );
        }
    }

    #[test]
    fn roots_do_not_inherit_an_existing_branch() {
        let repo = TestRepo::new("sha1").unwrap();
        let mut history = History::default();
        let first = history.push(Commit::default());
        history.refs.push(("refs/heads/first".into(), first));
        history.push(Commit {
            message: b"second root\n".to_vec(),
            ..Commit::default()
        });
        repo.import(&history).unwrap();
        assert_eq!(repo.git(["rev-list", "--count", "HEAD"]).unwrap(), b"1\n");
        assert_eq!(repo.git(["rev-list", "--all", "--count"]).unwrap(), b"2\n");
        assert_eq!(
            repo.git(["show", "-s", "--format=%P", "HEAD"]).unwrap(),
            b"\n"
        );
    }
}
