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

#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

use std::io::{self, IsTerminal};
use std::process::ExitCode;

#[cfg_attr(coverage_nightly, coverage(off))]
fn main() -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(error) => {
            eprintln!("ghist: {error}");
            return ExitCode::FAILURE;
        }
    };
    let stdout = io::stdout();
    let ctx = ghist::Context {
        args: std::env::args_os().skip(1).collect(),
        env: std::env::vars_os().collect(),
        cwd,
        stdout_is_terminal: stdout.is_terminal(),
        terminal_columns: rustix::termios::tcgetwinsize(&stdout)
            .ok()
            .map(|size| usize::from(size.ws_col))
            .filter(|&columns| columns > 0),
    };
    match ghist::run(&ctx, &mut stdout.lock(), &mut io::stderr().lock()) {
        ghist::Exit::Code(code) => ExitCode::from(code),
        ghist::Exit::Signal(signal) => {
            if let Err(error) = signal_hook::low_level::emulate_default_handler(signal) {
                eprintln!("ghist: {error}");
            }
            ExitCode::FAILURE
        }
    }
}
