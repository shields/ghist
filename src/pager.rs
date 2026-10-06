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

use std::ffi::{OsStr, OsString};
use std::io;
use std::os::unix::ffi::OsStringExt;
use std::process::{ChildStdin, Command, Stdio};

use crate::{
    Context, Exit, env,
    error::Error,
    git::{ChildGuard, config::Config},
};

pub fn resolve(ctx: &Context, config: &Config) -> Result<Option<OsString>, Error> {
    if !ctx.stdout_is_terminal {
        return Ok(None);
    }
    let command = if let Some(command) = env::value(ctx, "GIT_PAGER") {
        command.to_owned()
    } else if let Some(value) = config.last(&[b"core.pager"]) {
        OsString::from_vec(
            value
                .bytes()
                .ok_or_else(|| Error::Config {
                    key: b"core.pager".to_vec(),
                    value: None,
                })?
                .to_vec(),
        )
    } else {
        env::value(ctx, "PAGER")
            .unwrap_or_else(|| OsStr::new("less"))
            .to_owned()
    };
    Ok((!command.is_empty() && command != "cat").then_some(command))
}

pub fn child_env(ctx: &Context) -> Vec<(OsString, OsString)> {
    let mut values: Vec<_> = ctx
        .env
        .iter()
        .filter(|(key, _)| key != "GIT_PAGER_IN_USE")
        .cloned()
        .collect();
    for (key, default) in [("LESS", "FRX"), ("LV", "-c")] {
        if env::value(ctx, key).is_none() {
            values.push((key.into(), default.into()));
        }
    }
    if env::value(ctx, "COLUMNS").is_none()
        && ctx.terminal_columns.is_some_and(|columns| columns > 0)
    {
        values.push(("COLUMNS".into(), env::columns(ctx).to_string().into()));
    }
    values
}

pub struct Pager {
    child: ChildGuard,
    pub input: Option<ChildStdin>,
}

impl Pager {
    pub fn spawn(ctx: &Context, command: &OsStr) -> io::Result<Self> {
        let mut child = ChildGuard(
            Command::new("sh")
                .arg("-c")
                .arg(command)
                .env_clear()
                .envs(child_env(ctx))
                .current_dir(&ctx.cwd)
                .stdin(Stdio::piped())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()?,
        );
        let input = child.0.stdin.take();
        Ok(Self { child, input })
    }

    pub fn finish(mut self) -> Result<(), Error> {
        drop(self.input.take());
        let exit = crate::git::exit(self.child.0.wait()?);
        if exit == Exit::Code(0) {
            Ok(())
        } else {
            Err(Error::Pager(exit))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precedence_disabling_and_nonterminal_output() {
        let mut ctx = Context {
            stdout_is_terminal: true,
            ..Context::default()
        };
        let config = Config::default();
        assert_eq!(resolve(&ctx, &config).unwrap(), Some("less".into()));
        ctx.env.push(("PAGER".into(), "fallback".into()));
        assert_eq!(resolve(&ctx, &config).unwrap(), Some("fallback".into()));
        let config = Config::parse(b"core.pager\nconfigured\0").unwrap();
        assert_eq!(resolve(&ctx, &config).unwrap(), Some("configured".into()));
        for command in ["chosen", "", "cat"] {
            ctx.env.retain(|(key, _)| key != "GIT_PAGER");
            ctx.env.push(("GIT_PAGER".into(), command.into()));
            assert_eq!(
                resolve(&ctx, &config).unwrap(),
                (command == "chosen").then(|| command.into())
            );
        }
        ctx.env.clear();
        assert_eq!(
            resolve(&ctx, &Config::parse(b"core.pager\0").unwrap())
                .unwrap_err()
                .exit(),
            Exit::Code(128)
        );
        ctx.stdout_is_terminal = false;
        assert_eq!(resolve(&ctx, &config).unwrap(), None);
    }

    #[test]
    fn child_environment_preserves_explicit_values_and_removes_in_use() {
        let mut ctx = Context {
            terminal_columns: Some(132),
            ..Context::default()
        };
        assert_eq!(
            child_env(&ctx),
            [
                ("LESS".into(), "FRX".into()),
                ("LV".into(), "-c".into()),
                ("COLUMNS".into(), "132".into())
            ]
        );
        ctx.env = [
            ("LESS", ""),
            ("LV", "custom"),
            ("COLUMNS", "bad"),
            ("GIT_PAGER_IN_USE", "true"),
        ]
        .map(|(key, value)| (key.into(), value.into()))
        .to_vec();
        assert_eq!(child_env(&ctx), ctx.env[..3]);
        ctx.env.clear();
        ctx.terminal_columns = Some(0);
        assert_eq!(child_env(&ctx).len(), 2);
        ctx.terminal_columns = None;
        assert_eq!(child_env(&ctx).len(), 2);
    }
}
