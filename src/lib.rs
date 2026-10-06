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
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Default)]
pub struct Context {
    pub args: Vec<OsString>,
    pub env: Vec<(OsString, OsString)>,
    pub cwd: PathBuf,
    pub stdout_is_terminal: bool,
    pub terminal_columns: Option<usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Exit {
    Code(u8),
    Signal(i32),
}

#[must_use]
pub fn run(_ctx: &Context, _out: &mut dyn Write, _err: &mut dyn Write) -> Exit {
    Exit::Code(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaffold_runs() {
        assert_eq!(
            run(&Context::default(), &mut Vec::new(), &mut Vec::new()),
            Exit::Code(0)
        );
    }
}
