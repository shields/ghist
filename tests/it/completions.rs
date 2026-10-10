#[cfg(test)]
mod tests {
    use crate::common::history::{Commit, History};
    use crate::common::repo::TestRepo;

    #[test]
    fn flags_and_shell_names() {
        let repo = TestRepo::new("sha1").unwrap();
        let cases: &[(&str, &[&str])] = &[
            ("ghist -p", &["ghist", "-p"]),
            ("ghist --pa", &["ghist", "--patch"]),
            ("ghist --st", &["ghist", "--stat"]),
            ("ghist -h", &["ghist", "-h"]),
            ("ghist --he", &["ghist", "--help"]),
            ("ghist --ve", &["ghist", "--version"]),
            ("ghist --co", &["ghist", "--completions"]),
            ("ghist --completions b", &["ghist", "--completions", "bash"]),
            ("ghist --completions z", &["ghist", "--completions", "zsh"]),
            ("ghist --al", &["ghist", "--al"]),
        ];
        for shell in ["bash", "bash-system", "zsh"] {
            for &(line, expected) in cases {
                assert_eq!(
                    repo.complete(shell, line).unwrap(),
                    expected,
                    "{shell}: {line}"
                );
            }
        }
    }

    #[test]
    fn refs_ranges_exclusions_and_paths() {
        let cases: &[(&str, &[&str])] = &[
            ("ghist ma", &["ghist", "main"]),
            ("ghist HE", &["ghist", "HEAD"]),
            (
                "ghist --stat -p fe",
                &["ghist", "--stat", "-p", "feature/topic"],
            ),
            ("ghist ori", &["ghist", "origin/main"]),
            ("ghist v", &["ghist", "v1"]),
            ("ghist refs/heads/ma", &["ghist", "refs/heads/main"]),
            ("ghist HEAD..refs/tags/v", &["ghist", "HEAD..refs/tags/v1"]),
            ("ghist HEAD..ma", &["ghist", "HEAD..main"]),
            ("ghist HEAD...ma", &["ghist", "HEAD...main"]),
            ("ghist ^ma", &["ghist", "^main"]),
            ("ghist sp", &["ghist", "space file"]),
            ("ghist HEAD -- sp", &["ghist", "HEAD", "--", "space file"]),
            ("ghist -- \"sp", &["ghist", "--", "space file"]),
            ("ghist -- 'sp", &["ghist", "--", "space file"]),
            ("ghist -- space\\ f", &["ghist", "--", "space file"]),
            ("ghist HEAD -- ma", &["ghist", "HEAD", "--", "ma"]),
            ("ghist -- --st", &["ghist", "--", "--stat-file"]),
            ("ghist -- range", &["ghist", "--", "range..file"]),
            ("ghist -- ^f", &["ghist", "--", "^file"]),
        ];
        for format in ["sha1", "sha256"] {
            let repo = TestRepo::new(format).unwrap();
            let mut history = History::default();
            history.push(Commit::default());
            history.refs = [
                "refs/heads/feature/topic",
                "refs/remotes/origin/main",
                "refs/tags/v1",
            ]
            .map(|name| (name.into(), 1))
            .to_vec();
            repo.import(&history).unwrap();
            for name in ["space file", "--stat-file", "range..file", "^file"] {
                repo.write(name, b"").unwrap();
            }
            std::fs::create_dir(repo.cwd.join("directory name")).unwrap();
            for shell in ["bash", "bash-system", "zsh"] {
                for &(line, expected) in cases {
                    assert_eq!(
                        repo.complete(shell, line).unwrap(),
                        expected,
                        "{format} {shell}: {line}"
                    );
                }
                let directory = if shell.starts_with("bash") {
                    "directory name/"
                } else {
                    "directory name"
                };
                assert_eq!(
                    repo.complete(shell, "ghist -- di").unwrap(),
                    ["ghist", "--", directory]
                );
            }
        }
    }

    #[test]
    fn ambiguous_refs_and_literal_shell_metacharacters() {
        let repo = TestRepo::new("sha1").unwrap();
        let mut history = History::default();
        history.push(Commit::default());
        let cases = [
            ("lit", "literal$(touch${IFS}owned)"),
            ("back", "backtick`touch${IFS}backtick-owned`"),
            ("par", "parameter${IFS}name"),
            ("sin", "single'quote"),
            ("dou", "double\"quote"),
        ];
        history.refs = cases
            .iter()
            .map(|(_, name)| (format!("refs/heads/{name}"), 1))
            .chain([("refs/tags/main".into(), 1)])
            .collect();
        repo.import(&history).unwrap();
        for shell in ["bash", "bash-system", "zsh"] {
            assert_eq!(
                repo.complete(shell, "ghist heads/m").unwrap(),
                ["ghist", "heads/main"]
            );
            assert_eq!(
                repo.complete(shell, "ghist tags/m").unwrap(),
                ["ghist", "tags/main"]
            );
            for (prefix, name) in cases {
                for revision_prefix in ["", "^", "HEAD..", "HEAD..."] {
                    let line = format!("ghist {revision_prefix}{prefix}");
                    assert_eq!(
                        repo.complete(shell, &line).unwrap(),
                        ["ghist", &format!("{revision_prefix}{name}")],
                        "{shell}: {line}"
                    );
                }
                for quote in ["'", "\""] {
                    let line = format!("ghist {quote}{prefix}");
                    assert_eq!(
                        repo.complete(shell, &line).unwrap(),
                        ["ghist", name],
                        "{shell}: {line}"
                    );
                }
            }
            assert!(!repo.cwd.join("owned").exists());
            assert!(!repo.cwd.join("backtick-owned").exists());
        }
    }

    #[test]
    fn newlines_in_paths_and_zsh_autoload() {
        let repo = TestRepo::new("sha1").unwrap();
        repo.write("newline\nfile", b"").unwrap();
        for shell in ["bash", "bash-system", "zsh", "zsh-autoload"] {
            assert_eq!(
                repo.complete(shell, "ghist -- new").unwrap(),
                ["ghist", "--", "newline\nfile"],
                "{shell}"
            );
        }
    }

    #[test]
    fn paths_outside_a_repository() {
        let mut repo = TestRepo::new("sha1").unwrap();
        repo.cwd = repo.cwd.parent().unwrap().to_owned();
        repo.write("space file", b"").unwrap();
        for shell in ["bash", "bash-system", "zsh"] {
            assert_eq!(
                repo.complete(shell, "ghist sp").unwrap(),
                ["ghist", "space file"]
            );
        }
    }
}
