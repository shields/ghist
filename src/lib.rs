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

mod args;
mod color;
mod env;
mod error;
mod git;
mod graph;
mod oid;
mod out;
mod pager;
mod render;

use std::cell::RefCell;
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
pub fn run(ctx: &Context, out: &mut dyn Write, err: &mut dyn Write) -> Exit {
    match execute(ctx, out, err) {
        Ok(()) => Exit::Code(0),
        Err(error) if error.is_broken_pipe() => Exit::Code(0),
        Err(error) => match error.report(err) {
            Ok(()) => error.exit(),
            Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Exit::Code(0),
            Err(_) => Exit::Code(1),
        },
    }
}

fn execute(ctx: &Context, out: &mut dyn Write, err: &mut dyn Write) -> Result<(), error::Error> {
    match args::parse(&ctx.args)? {
        args::Action::Help => out.write_all(args::HELP.as_bytes())?,
        args::Action::Version => {
            out.write_all(concat!("ghist ", env!("CARGO_PKG_VERSION"), "\n").as_bytes())?;
        }
        args::Action::Log(args) => return log(ctx, &args, out, err),
    }
    out.flush()?;
    Ok(())
}

fn log(
    ctx: &Context,
    args: &args::LogArgs,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), error::Error> {
    let (config, mut stderr) = git::config::Config::read(ctx)?;
    let result = (|| {
        let mailmap = config.boolean(b"log.mailmap", true)?;
        color::validate(&config)?;
        let pager = pager::resolve(ctx, &config)?;
        let color = color::want_color(ctx, &config, pager.is_some())?;
        let palette = color.then(|| color::Palette::read(&config)).transpose()?;
        let (hidden, warnings) = git::revs::hidden(ctx, args)?;
        stderr.extend(warnings);
        let output = RefCell::new(out::Output::new(ctx, out, pager));
        let mut writer = out::Shared(&output);
        let mut renderer = render::Renderer::new(&mut writer, palette.as_ref(), hidden);
        let mut flush = || output.borrow_mut().flush();
        let walked = git::log::walk(ctx, args, mailmap, color, &mut flush, &mut |record, _| {
            Ok(renderer.commit(&record)?)
        });
        let finished = output.borrow_mut().finish();
        match walked {
            Ok(warnings) => stderr.extend(warnings),
            Err(error) => {
                if error.is_broken_pipe() {
                    finished?;
                }
                return Err(error);
            }
        }
        finished?;
        Ok(())
    })();
    match result {
        Ok(()) => {
            err.write_all(&stderr)?;
            err.flush()?;
            Ok(())
        }
        Err(error) => Err(error::Error::with_stderr(error, stderr)),
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;

    struct FailAfter {
        left: usize,
        fail_flush: bool,
    }

    impl Write for FailAfter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.left == 0 {
                return Err(io::Error::other("write failed"));
            }
            let count = self.left.min(bytes.len());
            self.left -= count;
            Ok(count)
        }

        fn flush(&mut self) -> io::Result<()> {
            if self.fail_flush {
                Err(io::Error::other("flush failed"))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn broken_stdout_and_stderr_are_quiet_successes() {
        struct BrokenPipe;
        impl Write for BrokenPipe {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
        }
        let ctx = Context {
            args: vec!["--help".into()],
            ..Context::default()
        };
        let mut err = Vec::new();
        assert_eq!(run(&ctx, &mut BrokenPipe, &mut err), Exit::Code(0));
        assert_eq!(err, b"");
        let ctx = Context {
            args: vec!["--bad".into()],
            ..Context::default()
        };
        assert_eq!(run(&ctx, &mut Vec::new(), &mut BrokenPipe), Exit::Code(0));
        assert_eq!(
            BrokenPipe.flush().unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
    }

    #[test]
    fn informational_output() {
        for (arg, expected) in [
            ("--help", args::HELP),
            (
                "--version",
                concat!("ghist ", env!("CARGO_PKG_VERSION"), "\n"),
            ),
        ] {
            let ctx = Context {
                args: vec![arg.into()],
                ..Context::default()
            };
            let mut out = Vec::new();
            let mut err = Vec::new();
            assert_eq!(run(&ctx, &mut out, &mut err), Exit::Code(0));
            assert_eq!(out, expected.as_bytes());
            assert_eq!(err, b"");
        }
    }

    #[test]
    fn usage_error_goes_to_stderr() {
        let ctx = Context {
            args: vec!["--bad".into()],
            ..Context::default()
        };
        let mut out = Vec::new();
        let mut err = Vec::new();
        assert_eq!(run(&ctx, &mut out, &mut err), Exit::Code(2));
        assert_eq!(out, b"");
        assert_eq!(err, b"ghist: unknown option: --bad; see ghist --help\n");
    }

    #[test]
    fn handles_each_partial_stdout_write() {
        for (arg, size) in [
            ("--help", args::HELP.len()),
            (
                "--version",
                concat!("ghist ", env!("CARGO_PKG_VERSION"), "\n").len(),
            ),
        ] {
            let ctx = Context {
                args: vec![arg.into()],
                ..Context::default()
            };
            for left in 0..size {
                let mut err = Vec::new();
                assert_eq!(
                    run(
                        &ctx,
                        &mut FailAfter {
                            left,
                            fail_flush: false
                        },
                        &mut err
                    ),
                    Exit::Code(1)
                );
                assert_eq!(err, b"ghist: write failed\n");
            }
        }
    }

    #[test]
    fn handles_stderr_write_and_flush_failure() {
        let ctx = Context {
            args: vec!["--bad".into()],
            ..Context::default()
        };
        let size = b"ghist: unknown option: --bad; see ghist --help\n".len();
        for left in 0..size {
            assert_eq!(
                run(
                    &ctx,
                    &mut Vec::new(),
                    &mut FailAfter {
                        left,
                        fail_flush: false
                    }
                ),
                Exit::Code(1)
            );
        }
        assert_eq!(
            run(
                &ctx,
                &mut Vec::new(),
                &mut FailAfter {
                    left: size,
                    fail_flush: true
                }
            ),
            Exit::Code(1)
        );
        assert_eq!(
            run(
                &ctx,
                &mut Vec::new(),
                &mut FailAfter {
                    left: size,
                    fail_flush: false
                }
            ),
            Exit::Code(2)
        );
    }

    #[test]
    fn handles_stdout_flush_failure() {
        let mut err = Vec::new();
        assert_eq!(
            run(
                &Context {
                    args: vec!["--help".into()],
                    ..Context::default()
                },
                &mut FailAfter {
                    left: usize::MAX,
                    fail_flush: true
                },
                &mut err
            ),
            Exit::Code(1)
        );
        assert_eq!(err, b"ghist: flush failed\n");
    }
}
