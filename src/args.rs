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

use std::ffi::OsString;

use crate::error::Error;

pub const HELP: &str = "\
ghist — an opinionated git log replacement

Usage: ghist [-p] [--stat] [<revision>…] [--] [<path>…]

  -p           Show patches
  --stat       Show diff statistics
  -h, --help   Show this help
  --version    Show the version
  --           Treat all remaining arguments as paths

Revisions include refs, a..b, a...b, and ^rev. Git resolves revisions and paths.
History is always shown in topological order with decorations.
";

#[derive(Debug, Eq, PartialEq)]
pub enum Action {
    Help,
    Version,
    Log(LogArgs),
}

#[derive(Debug, Default, Eq, PartialEq)]
pub struct LogArgs {
    pub patch: bool,
    pub stat: bool,
    pub before: Vec<OsString>,
    pub after: Option<Vec<OsString>>,
}

pub fn parse(args: &[OsString]) -> Result<Action, Error> {
    let mut log = LogArgs::default();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_encoded_bytes() {
            b"--" => {
                log.after = Some(args.cloned().collect());
                break;
            }
            b"--help" => return Ok(Action::Help),
            b"--version" => return Ok(Action::Version),
            b"--stat" => log.stat = true,
            [b'-', tail @ ..] => {
                if tail.is_empty() {
                    return Err(Error::Usage(arg.clone()));
                }
                for flag in tail {
                    match flag {
                        b'p' => log.patch = true,
                        b'h' => return Ok(Action::Help),
                        _ => return Err(Error::Usage(arg.clone())),
                    }
                }
            }
            _ => log.before.push(arg.clone()),
        }
    }
    Ok(Action::Log(log))
}

#[cfg(test)]
mod tests {
    use std::os::unix::ffi::OsStringExt;

    use super::*;

    #[test]
    fn default_log() {
        assert_eq!(parse(&[]).unwrap(), Action::Log(LogArgs::default()));
    }

    #[test]
    fn flags_and_git_operands() {
        let args = ["-pp", "main", "--stat", "a..b", "a...b", "^old", "path"];
        assert_eq!(
            parse(&args.map(OsString::from)).unwrap(),
            Action::Log(LogArgs {
                patch: true,
                stat: true,
                before: ["main", "a..b", "a...b", "^old", "path"]
                    .map(OsString::from)
                    .to_vec(),
                after: None,
            })
        );
    }

    #[test]
    fn informational_flags_stop_parsing() {
        for help in ["-h", "--help", "-ph", "-hp"] {
            assert_eq!(parse(&[help.into(), "--bad".into()]).unwrap(), Action::Help);
        }
        assert_eq!(parse(&["--version".into()]).unwrap(), Action::Version);
    }

    #[test]
    fn separator_preserves_paths_and_empty_separator() {
        for paths in [vec![], vec!["--help", "-p", "--", "--version"]] {
            let mut args = vec!["HEAD".into(), "--".into()];
            args.extend(paths.iter().map(OsString::from));
            assert_eq!(
                parse(&args).unwrap(),
                Action::Log(LogArgs {
                    before: vec!["HEAD".into()],
                    after: Some(paths.iter().map(OsString::from).collect()),
                    ..LogArgs::default()
                })
            );
        }
    }

    #[test]
    fn rejects_all_other_options() {
        for flag in [
            "-",
            "-q",
            "-pq",
            "--all",
            "--stat=80",
            "--pretty=raw",
            "-n1",
            "--color=always",
        ] {
            let error = parse(&[flag.into()]).unwrap_err();
            assert_eq!(
                error.to_string(),
                format!("unknown option: {flag}; see ghist --help")
            );
        }
    }

    #[test]
    fn preserves_non_utf8_operands() {
        let path = OsString::from_vec(vec![b'f', 0xff]);
        let args = vec![path.clone(), "--".into(), path.clone()];
        assert_eq!(
            parse(&args).unwrap(),
            Action::Log(LogArgs {
                before: vec![path.clone()],
                after: Some(vec![path]),
                ..LogArgs::default()
            })
        );
        let invalid_flag = OsString::from_vec(vec![b'-', 0xff]);
        assert_eq!(
            parse(&[invalid_flag]).unwrap_err().exit(),
            crate::Exit::Code(2)
        );
    }
}
