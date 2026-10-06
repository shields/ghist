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

use std::io;

use crate::{
    Context,
    color::{Palette, Sgr},
    env,
    error::Error,
    git::{
        config::Config,
        stat::{Change, File},
    },
};

#[derive(Clone, Copy)]
pub struct Options {
    pub columns: usize,
    name: usize,
    graph: i32,
}

impl Options {
    pub fn read(ctx: &Context, config: &Config) -> Result<Self, Error> {
        Ok(Self {
            columns: env::columns(ctx),
            // Negative name widths are uncapped in Git, so use the zero-width sentinel.
            name: usize::try_from(config.integer(b"diff.statnamewidth", 0)?).unwrap_or_default(),
            graph: config.integer(b"diff.statgraphwidth", 0)?,
        })
    }
}

struct Layout {
    name: usize,
    graph: usize,
    digits: usize,
    maximum: usize,
}

impl Layout {
    fn new(files: &[File], width: usize, options: Options) -> Self {
        let mut name = files
            .iter()
            .map(|file| display_width(&file.name))
            .max()
            .unwrap_or_default();
        let maximum = files
            .iter()
            .map(|file| match file.change {
                Change::Text { added, deleted } => added + deleted,
                Change::Binary { .. } => 0,
            })
            .max()
            .unwrap_or_default();
        let binary = files
            .iter()
            .any(|file| matches!(file.change, Change::Binary { .. }));
        let digits = maximum.to_string().len().max(if binary { 3 } else { 1 });
        let width = width.max(digits + 22);
        let available = width - digits - 6;
        if options.name > 0 {
            name = name.min(options.name);
        }
        if options.graph < 0 {
            let extra =
                usize::try_from(options.graph.unsigned_abs()).expect("32-bit widths fit usize");
            return Self {
                name: name.min(available.saturating_add(extra)),
                graph: usize::MAX,
                digits,
                maximum,
            };
        }
        let binary_width = files
            .iter()
            .map(|file| match file.change {
                Change::Binary {
                    before,
                    after,
                    same: false,
                } => format!("{before} -> {after} bytes").len(),
                _ => 0,
            })
            .max()
            .unwrap_or_default();
        let mut graph = maximum.max(binary_width);
        if options.graph > 0 {
            graph = graph
                .min(usize::try_from(options.graph).expect("positive 32-bit widths fit usize"));
        }
        if name.saturating_add(graph) > available {
            let preferred = name.min(width - (width / 8 * 3 + width % 8 * 3 / 8));
            if graph > available.saturating_sub(preferred) {
                graph = available.saturating_sub(preferred).max(6);
            }
            name = name.min(available - graph);
        }
        Self {
            name,
            graph,
            digits,
            maximum,
        }
    }
}

fn display_width(bytes: &[u8]) -> usize {
    std::str::from_utf8(bytes)
        .ok()
        .and_then(|text| {
            text.chars().try_fold(0, |total, character| {
                Some(total + super::width::character(character)?)
            })
        })
        .unwrap_or(bytes.len())
}

fn filename(bytes: &[u8], limit: usize) -> Vec<u8> {
    let width = display_width(bytes);
    if width <= limit {
        return bytes.to_vec();
    }
    let remaining = limit.saturating_sub(3);
    let mut offset = bytes.len().saturating_sub(remaining);
    if let Ok(text) = std::str::from_utf8(bytes)
        && text
            .chars()
            .all(|character| super::width::character(character).is_some())
    {
        let mut width = width;
        for (index, character) in text.char_indices() {
            if width <= remaining {
                break;
            }
            let size =
                super::width::character(character).expect("filename character widths are valid");
            width -= size;
            offset = index + character.len_utf8();
        }
    }
    let tail = bytes.get(offset..).unwrap_or_default();
    let tail = tail
        .iter()
        .position(|&byte| byte == b'/')
        .and_then(|index| tail.get(index..))
        .unwrap_or(tail);
    [b"...".as_slice(), tail].concat()
}

fn bars(added: usize, deleted: usize, width: usize, maximum: usize) -> (usize, usize) {
    if maximum <= width {
        return (added, deleted);
    }
    let scale = |count: usize| {
        if count == 0 {
            0
        } else {
            usize::try_from(
                1 + (count as u128) * (width.saturating_sub(1) as u128) / maximum as u128,
            )
            .expect("scaled counts fit the graph width")
        }
    };
    let total = scale(added + deleted);
    if added == 0 {
        return (0, total);
    }
    if deleted == 0 {
        return (total, 0);
    }
    let total = total.max(2);
    if added < deleted {
        let added = scale(added).min(total - 1);
        (added, total - added)
    } else {
        let deleted = scale(deleted).min(total - 1);
        (total - deleted, deleted)
    }
}

fn colored(bytes: &mut Vec<u8>, text: &[u8], color: Option<&Sgr>) {
    let color = color.filter(|color| !color.0.is_empty());
    if let Some(color) = color {
        bytes.extend_from_slice(&color.0);
    }
    bytes.extend_from_slice(text);
    if color.is_some() {
        bytes.extend_from_slice(b"\x1b[m");
    }
}

pub fn render(
    files: &[File],
    width: usize,
    options: Options,
    colors: Option<&Palette>,
    emit: &mut dyn FnMut(&[u8]) -> io::Result<()>,
) -> Result<(), Error> {
    if files.is_empty() {
        return Ok(());
    }
    let layout = Layout::new(files, width, options);
    let mut inserted = 0usize;
    let mut removed = 0usize;
    for file in files {
        let name = filename(&file.name, layout.name);
        let padding = layout.name.saturating_sub(display_width(&name));
        let mut line = b" ".to_vec();
        line.extend_from_slice(&name);
        line.extend(std::iter::repeat_n(b' ', padding));
        line.extend_from_slice(b" | ");
        match file.change {
            Change::Text { added, deleted } => {
                inserted = inserted
                    .checked_add(added)
                    .ok_or(Error::Protocol("stat insertion count overflow"))?;
                removed = removed
                    .checked_add(deleted)
                    .ok_or(Error::Protocol("stat deletion count overflow"))?;
                line.extend_from_slice(
                    format!("{:>width$}", added + deleted, width = layout.digits).as_bytes(),
                );
                let (added, deleted) = bars(added, deleted, layout.graph, layout.maximum);
                if added + deleted > 0 {
                    line.push(b' ');
                }
                if added > 0 {
                    colored(
                        &mut line,
                        &vec![b'+'; added],
                        colors.map(|palette| &palette.new),
                    );
                }
                if deleted > 0 {
                    colored(
                        &mut line,
                        &vec![b'-'; deleted],
                        colors.map(|palette| &palette.old),
                    );
                }
            }
            Change::Binary {
                before,
                after,
                same,
            } => {
                line.extend_from_slice(
                    format!("{:>width$}", "Bin", width = layout.digits).as_bytes(),
                );
                if !same {
                    line.push(b' ');
                    colored(
                        &mut line,
                        before.to_string().as_bytes(),
                        colors.map(|palette| &palette.old),
                    );
                    line.extend_from_slice(b" -> ");
                    colored(
                        &mut line,
                        after.to_string().as_bytes(),
                        colors.map(|palette| &palette.new),
                    );
                    line.extend_from_slice(b" bytes");
                }
            }
        }
        emit(&line)?;
    }
    let mut summary = format!(
        " {} file{} changed",
        files.len(),
        if files.len() == 1 { "" } else { "s" }
    )
    .into_bytes();
    if inserted > 0 || removed == 0 {
        summary.extend_from_slice(
            format!(
                ", {inserted} insertion{}(+)",
                if inserted == 1 { "" } else { "s" }
            )
            .as_bytes(),
        );
    }
    if removed > 0 || inserted == 0 {
        summary.extend_from_slice(
            format!(
                ", {removed} deletion{}(-)",
                if removed == 1 { "" } else { "s" }
            )
            .as_bytes(),
        );
    }
    emit(&summary)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> Options {
        Options {
            columns: 80,
            name: 0,
            graph: 0,
        }
    }

    #[test]
    fn width_limits_and_scaled_bar_rounding() {
        let files = [File {
            name: vec![b'x'; 100],
            change: Change::Text {
                added: 100,
                deleted: 0,
            },
        }];
        for (width, name, graph) in [
            (1, 10, 6),
            (30, 15, 6),
            (40, 25, 6),
            (50, 32, 9),
            (80, 50, 21),
            (120, 75, 36),
            (usize::MAX, 100, 100),
        ] {
            let layout = Layout::new(&files, width, options());
            assert_eq!((layout.name, layout.graph), (name, graph));
        }
        assert_eq!(bars(50, 100, 40, 150), (14, 26));
        assert_eq!(bars(100, 50, 40, 150), (26, 14));
        assert_eq!(bars(20, 20, 6, 100), (1, 2));
        assert_eq!(bars(0, 3, 6, 100), (0, 1));
        assert_eq!(bars(3, 0, 6, 100), (1, 0));
        assert_eq!(bars(3, 2, 6, 5), (3, 2));
        assert_eq!(bars(0, 0, 6, 100), (0, 0));
        let layout = Layout::new(
            &files,
            80,
            Options {
                name: 4,
                graph: 4,
                ..options()
            },
        );
        assert_eq!((layout.name, layout.graph), (4, 4));
        let layout = Layout::new(
            &files,
            80,
            Options {
                graph: 4,
                ..options()
            },
        );
        assert_eq!((layout.name, layout.graph), (67, 4));
        let layout = Layout::new(
            &files,
            80,
            Options {
                graph: -1,
                ..options()
            },
        );
        assert_eq!((layout.name, layout.graph), (72, usize::MAX));
    }

    #[test]
    fn truncation_preserves_utf8_and_prefers_path_boundaries() {
        assert_eq!(
            filename(b"foo/bar/baz/quux/file-name", 20),
            b".../quux/file-name"
        );
        assert_eq!(filename(b"abcdef", 1), b"...");
        assert_eq!(filename(b"abc", 3), b"abc");
        assert_eq!(
            filename("雪".repeat(15).as_bytes(), 10),
            "...雪雪雪".as_bytes()
        );
        assert_eq!(filename(b"a\xffxyz", 4), b"...z");
        assert_eq!(filename("a\u{ffff}xyz".as_bytes(), 4), b"...z");
        assert_eq!(display_width("雪😀é".as_bytes()), 5);
        assert_eq!(display_width(b"\xff"), 1);
    }

    #[test]
    fn output_errors_and_total_count_overflow_are_fatal() {
        let file = File {
            name: b"path".to_vec(),
            change: Change::Text {
                added: 1,
                deleted: 1,
            },
        };
        for remaining in [0, 1] {
            let mut calls = remaining;
            let mut emit = |_: &[u8]| {
                if calls == 0 {
                    Err(io::Error::other("write failed"))
                } else {
                    calls -= 1;
                    Ok(())
                }
            };
            render(&[], 80, options(), None, &mut emit).unwrap();
            let error =
                render(std::slice::from_ref(&file), 80, options(), None, &mut emit).unwrap_err();
            assert_eq!(error.to_string(), "write failed");
        }
        for deletion in [false, true] {
            let files = [usize::MAX, 1].map(|count| File {
                name: b"path".to_vec(),
                change: Change::Text {
                    added: if deletion { 0 } else { count },
                    deleted: if deletion { count } else { 0 },
                },
            });
            assert!(render(&files, 80, options(), None, &mut |_| Ok(())).is_err());
        }
    }
}
