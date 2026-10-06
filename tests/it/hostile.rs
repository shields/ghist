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
    use crate::common::history::{Change, Commit, History};
    use crate::common::repo::TestRepo;
    use ghist::Exit;
    use std::fs;
    use std::io;

    fn history(format: &str) -> io::Result<TestRepo> {
        let repo = TestRepo::new(format)?;
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
            changes: vec![Change::Write {
                mode: 0o100_644,
                path: b"file".to_vec(),
                data: b"two\n".to_vec(),
            }],
            ..Commit::default()
        });
        repo.import(&history)?;
        Ok(repo)
    }

    #[test]
    fn streams_both_formats_with_hostile_log_and_diff_config() {
        for format in ["sha1", "sha256"] {
            let repo = history(format).unwrap();
            repo.git(["config", "log.follow", "true"]).unwrap();
            repo.git(["config", "log.showSignature", "true"]).unwrap();
            repo.git(["config", "diff.external", "/does/not/exist"])
                .unwrap();
            repo.git(["config", "log.mailmap", "false"]).unwrap();
            repo.git(["config", "log.showRoot", "false"]).unwrap();
            for args in [
                vec![],
                vec!["-p"],
                vec!["--stat"],
                vec!["-p", "--stat"],
                vec!["HEAD~1.."],
                vec!["--", "file"],
            ] {
                let mut out = Vec::new();
                let mut err = Vec::new();
                let exit = ghist::run(&repo.context(&args), &mut out, &mut err);
                assert_eq!(
                    exit,
                    Exit::Code(0),
                    "{args:?}: {}",
                    String::from_utf8_lossy(&err)
                );
                assert_eq!(err, b"");
            }
        }
    }

    #[test]
    fn disables_signature_verification_for_signed_commits() {
        let mut repo = history("sha1").unwrap();
        let raw = repo.git(["cat-file", "commit", "HEAD"]).unwrap();
        let boundary = raw.windows(2).position(|part| part == b"\n\n").unwrap() + 1;
        let signed = [
            &raw[..boundary],
            b"gpgsig -----BEGIN PGP SIGNATURE-----\n dummy\n -----END PGP SIGNATURE-----\n",
            &raw[boundary..],
        ]
        .concat();
        let oid = repo
            .git_input(["hash-object", "-t", "commit", "-w", "--stdin"], &signed)
            .unwrap();
        repo.git([
            "update-ref",
            "refs/heads/main",
            std::str::from_utf8(&oid).unwrap().trim(),
        ])
        .unwrap();
        let called = repo.cwd.join("gpg-called");
        repo.env
            .push(("GHIST_GPG_CALLED".into(), called.clone().into_os_string()));
        let stub = repo
            .script(
                "gpg-stub",
                b"printf called > \"$GHIST_GPG_CALLED\"\nexit 1\n",
            )
            .unwrap();
        repo.git([
            std::ffi::OsStr::new("config"),
            std::ffi::OsStr::new("gpg.program"),
            stub.as_os_str(),
        ])
        .unwrap();
        repo.git(["config", "log.showSignature", "true"]).unwrap();
        repo.git(["log", "-1"]).unwrap();
        assert_eq!(fs::read(&called).unwrap(), b"called");
        fs::remove_file(&called).unwrap();
        let mut err = Vec::new();
        assert_eq!(
            ghist::run(&repo.context(&[]), &mut Vec::new(), &mut err),
            Exit::Code(0)
        );
        assert_eq!(err, b"");
        assert!(!called.exists());
    }

    #[test]
    fn invalid_colors_are_fatal_even_without_color_output() {
        let repo = history("sha1").unwrap();
        repo.git(["config", "color.diff.commit", "not-a-color"])
            .unwrap();
        let mut err = Vec::new();
        assert_eq!(
            ghist::run(&repo.context(&[]), &mut Vec::new(), &mut err),
            Exit::Code(128)
        );
        assert!(String::from_utf8_lossy(&err).contains("invalid color"));
    }
}
