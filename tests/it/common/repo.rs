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
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::{fs, thread};

use tempfile::TempDir;

use super::{env, history::History};

pub struct TestRepo {
    _dir: TempDir,
    pub cwd: PathBuf,
    pub env: Vec<(OsString, OsString)>,
}

impl TestRepo {
    pub fn new(format: &str) -> io::Result<Self> {
        let dir = TempDir::new()?;
        let home = dir.path().join("home");
        let cwd = dir.path().join("repo");
        fs::create_dir(&home)?;
        fs::create_dir(&cwd)?;
        let repo = Self {
            env: env::isolated(&home),
            cwd,
            _dir: dir,
        };
        repo.git([
            "init",
            "--quiet",
            "--initial-branch=main",
            &format!("--object-format={format}"),
        ])?;
        repo.git(["config", "user.name", "A U Thor"])?;
        repo.git(["config", "user.email", "author@example.com"])?;
        Ok(repo)
    }

    pub fn context(&self, args: &[&str]) -> ghist::Context {
        ghist::Context {
            cwd: self.cwd.clone(),
            env: self.env.clone(),
            args: args.iter().map(OsString::from).collect(),
            ..ghist::Context::default()
        }
    }

    pub fn git(&self, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> io::Result<Vec<u8>> {
        checked(self.command(args).output()?)
    }

    pub fn import(&self, history: &History) -> io::Result<()> {
        let mut stream = Vec::new();
        history.encode(&mut stream)?;
        let mut child = self
            .command(["fast-import", "--quiet", "--force", "--done"])
            .stdin(Stdio::piped())
            .spawn()?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("missing import stdin"))?;
        thread::scope(|scope| {
            let writer = scope.spawn(move || stdin.write_all(&stream));
            let output = child.wait_with_output()?;
            match writer.join() {
                Ok(result) => result?,
                Err(payload) => std::panic::resume_unwind(payload),
            }
            checked(output)
        })?;
        Ok(())
    }

    pub fn shallow(&self, depth: usize) -> io::Result<Self> {
        let dir = TempDir::new()?;
        let home = dir.path().join("home");
        fs::create_dir(&home)?;
        let destination = dir.path().join("repo");
        self.git([
            OsStr::new("clone"),
            OsStr::new("--quiet"),
            OsStr::new("--no-local"),
            OsStr::new("--depth"),
            OsStr::new(&depth.to_string()),
            self.cwd.as_os_str(),
            destination.as_os_str(),
        ])?;
        Ok(Self {
            env: env::isolated(&home),
            cwd: destination,
            _dir: dir,
        })
    }

    pub fn write(&self, path: impl AsRef<Path>, bytes: &[u8]) -> io::Result<()> {
        fs::write(self.cwd.join(path), bytes)
    }

    fn command(&self, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Command {
        let mut command = Command::new("git");
        command
            .arg("--no-pager")
            .args(args)
            .env_clear()
            .envs(self.env.iter().cloned())
            .current_dir(&self.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }
}

fn checked(output: Output) -> io::Result<Vec<u8>> {
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(io::Error::other(format!(
            "git exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )))
    }
}
