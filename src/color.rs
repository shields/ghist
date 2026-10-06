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

use crate::{Context, env, error::Error, git::config::Config};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Sgr(pub Vec<u8>);

#[derive(Clone, Copy)]
enum Color {
    Normal,
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

impl Color {
    fn parse(token: &str) -> Option<Self> {
        let lower = token.to_ascii_lowercase();
        match lower.as_str() {
            "normal" => return Some(Self::Normal),
            "default" => return Some(Self::Default),
            _ => {}
        }
        if let Some(hex) = lower.strip_prefix('#') {
            if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return None;
            }
            let value = u32::from_str_radix(hex, 16).ok()?;
            let (red, green, blue) = match hex.len() {
                3 => (
                    ((value >> 8) & 15) * 17,
                    ((value >> 4) & 15) * 17,
                    (value & 15) * 17,
                ),
                6 => ((value >> 16) & 255, (value >> 8) & 255, value & 255),
                _ => return None,
            };
            return Some(Self::Rgb(
                u8::try_from(red).ok()?,
                u8::try_from(green).ok()?,
                u8::try_from(blue).ok()?,
            ));
        }
        let (name, offset) = lower
            .strip_prefix("bright")
            .map_or((lower.as_str(), 0), |name| (name, 8));
        let names = [
            "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
        ];
        if let Some(index) = names.iter().position(|&color| color == name) {
            return Some(Self::Indexed(u8::try_from(index).ok()? + offset));
        }
        match lower.parse::<i16>().ok()? {
            -1 => Some(Self::Normal),
            value => Some(Self::Indexed(u8::try_from(value).ok()?)),
        }
    }

    fn parameter(self, background: bool) -> Option<String> {
        let base = if background { 40 } else { 30 };
        match self {
            Self::Normal => None,
            Self::Default => Some((base + 9).to_string()),
            Self::Indexed(index @ 0..=7) => Some((base + index).to_string()),
            Self::Indexed(index @ 8..=15) => Some((base + 60 + index - 8).to_string()),
            Self::Indexed(index) => Some(format!("{};5;{index}", base + 8)),
            Self::Rgb(red, green, blue) => Some(format!("{};2;{red};{green};{blue}", base + 8)),
        }
    }
}

pub fn parse(bytes: &[u8]) -> Option<Sgr> {
    let input = std::str::from_utf8(bytes).ok()?;
    let mut reset = false;
    let mut attributes = 0_u32;
    let mut colors = Vec::new();
    for token in input.split_ascii_whitespace() {
        if token.eq_ignore_ascii_case("reset") {
            reset = true;
        } else if let Some(attribute) = attribute(token) {
            attributes |= 1 << attribute;
        } else {
            if colors.len() == 2 {
                return None;
            }
            colors.push(Color::parse(token)?);
        }
    }
    let mut parameters = Vec::new();
    if reset {
        parameters.push(String::new());
    }
    for code in 1..30 {
        if attributes & (1 << code) != 0 {
            parameters.push(code.to_string());
        }
    }
    for (index, color) in colors.into_iter().enumerate() {
        if let Some(parameter) = color.parameter(index == 1) {
            parameters.push(parameter);
        }
    }
    if parameters.is_empty() {
        return Some(Sgr::default());
    }
    Some(Sgr(format!("\x1b[{}m", parameters.join(";")).into_bytes()))
}

fn attribute(token: &str) -> Option<u8> {
    let (name, negative) = token
        .strip_prefix("no-")
        .or_else(|| token.strip_prefix("no"))
        .map_or((token, false), |name| (name, true));
    Some(match (name, negative) {
        ("bold", false) => 1,
        ("dim", false) => 2,
        ("italic", false) => 3,
        ("ul", false) => 4,
        ("blink", false) => 5,
        ("reverse", false) => 7,
        ("strike", false) => 9,
        ("bold" | "dim", true) => 22,
        ("italic", true) => 23,
        ("ul", true) => 24,
        ("blink", true) => 25,
        ("reverse", true) => 27,
        ("strike", true) => 29,
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorBool {
    Never,
    Auto,
    Always,
}

impl ColorBool {
    pub fn parse(value: Option<&[u8]>) -> Option<Self> {
        if let Some(value) = value {
            for (word, result) in [
                (b"never".as_slice(), Self::Never),
                (b"auto", Self::Auto),
                (b"always", Self::Always),
            ] {
                if value.eq_ignore_ascii_case(word) {
                    return Some(result);
                }
            }
        }
        env::git_bool(value).map(|enabled| if enabled { Self::Auto } else { Self::Never })
    }
}

pub fn want_color(ctx: &Context, config: &Config, paging: bool) -> Result<bool, Error> {
    let setting = config
        .last_entry(&[b"color.diff", b"diff.color"])
        .or_else(|| config.last_entry(&[b"color.ui"]));
    let setting = setting.map_or(Ok(ColorBool::Auto), |(key, value)| {
        ColorBool::parse(value.bytes()).ok_or_else(|| Error::Config {
            key: key.to_vec(),
            value: value.bytes().map(<[u8]>::to_vec),
        })
    })?;
    match setting {
        ColorBool::Never => return Ok(false),
        ColorBool::Always => return Ok(true),
        ColorBool::Auto => {}
    }
    let value = |key: &str| {
        ctx.env
            .iter()
            .rev()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_os_str())
    };
    if value("NO_COLOR").is_some_and(|value| !value.is_empty())
        || value("TERM").is_none_or(|value| value == "dumb")
    {
        return Ok(false);
    }
    let pager_color = config
        .last_entry(&[b"color.pager", b"pager.color"])
        .map_or(Ok(true), |(key, value)| {
            env::git_bool(value.bytes()).ok_or_else(|| Error::Config {
                key: key.to_vec(),
                value: value.bytes().map(<[u8]>::to_vec),
            })
        })?;
    Ok(ctx.stdout_is_terminal || (paging || value("GIT_PAGER_IN_USE").is_some()) && pager_color)
}

pub fn validate(config: &Config) -> Result<(), Error> {
    for key in [
        b"color.diff.commit".as_slice(),
        b"color.decorate.head",
        b"color.decorate.branch",
        b"color.decorate.remotebranch",
        b"color.decorate.tag",
        b"color.decorate.stash",
        b"color.decorate.grafted",
        b"color.diff.old",
        b"color.diff.new",
        b"log.graphcolors",
    ] {
        if let Some(value) = config.last(&[key]) {
            let failure = || Error::Config {
                key: key.to_vec(),
                value: value.bytes().map(<[u8]>::to_vec),
            };
            let bytes = value.bytes().ok_or_else(failure)?;
            if key == b"log.graphcolors" {
                for entry in bytes.split(|&byte| byte == b',') {
                    parse(entry).ok_or_else(failure)?;
                }
            } else {
                parse(bytes).ok_or_else(failure)?;
            }
        }
    }
    Ok(())
}

fn graph_colors(config: &Config) -> Result<Vec<Sgr>, Error> {
    let Some(value) = config.last(&[b"log.graphcolors"]) else {
        return Ok((0..12)
            .map(|index| {
                Sgr(format!(
                    "\x1b[{}{}m",
                    if index >= 6 { "1;" } else { "" },
                    31 + index % 6
                )
                .into_bytes())
            })
            .collect());
    };
    let failure = || Error::Config {
        key: b"log.graphcolors".to_vec(),
        value: value.bytes().map(<[u8]>::to_vec),
    };
    let bytes = value.bytes().ok_or_else(failure)?;
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    bytes
        .split(|&byte| byte == b',')
        .map(|entry| parse(entry).ok_or_else(failure))
        .collect()
}

#[derive(Debug)]
pub struct Palette {
    pub commit: Sgr,
    pub head: Sgr,
    pub branch: Sgr,
    pub remote: Sgr,
    pub tag: Sgr,
    pub stash: Sgr,
    pub grafted: Sgr,
    pub graph: Vec<Sgr>,
    pub new: Sgr,
    pub old: Sgr,
}

impl Palette {
    pub fn read(config: &Config) -> Result<Self, Error> {
        let color = |key: &[u8], default: &[u8]| {
            config.last(&[key]).map_or_else(
                || Ok(Sgr(default.to_vec())),
                |value| {
                    value.bytes().and_then(parse).ok_or_else(|| Error::Config {
                        key: key.to_vec(),
                        value: value.bytes().map(<[u8]>::to_vec),
                    })
                },
            )
        };
        Ok(Self {
            commit: color(b"color.diff.commit", b"\x1b[33m")?,
            head: color(b"color.decorate.head", b"\x1b[1;36m")?,
            branch: color(b"color.decorate.branch", b"\x1b[1;32m")?,
            remote: color(b"color.decorate.remotebranch", b"\x1b[1;31m")?,
            tag: color(b"color.decorate.tag", b"\x1b[1;33m")?,
            stash: color(b"color.decorate.stash", b"\x1b[1;35m")?,
            grafted: color(b"color.decorate.grafted", b"\x1b[1;34m")?,
            graph: graph_colors(config)?,
            new: color(b"color.diff.new", b"\x1b[32m")?,
            old: color(b"color.diff.old", b"\x1b[31m")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_palettes_support_defaults_empty_entries_and_large_lists() {
        let colors = graph_colors(&Config::default()).unwrap();
        assert_eq!(colors.len(), 12);
        assert_eq!(colors[0].0, b"\x1b[31m");
        assert_eq!(colors[11].0, b"\x1b[1;36m");
        assert_eq!(
            graph_colors(&Config::parse(b"log.graphcolors\n\0").unwrap()).unwrap(),
            vec![]
        );
        let colors =
            graph_colors(&Config::parse(b"log.graphcolors\nred,,blue\0").unwrap()).unwrap();
        assert_eq!(
            colors,
            [
                Sgr(b"\x1b[31m".to_vec()),
                Sgr::default(),
                Sgr(b"\x1b[34m".to_vec())
            ]
        );
        let large = format!("log.graphcolors\n{}blue\0", "red,".repeat(69_999));
        assert_eq!(
            graph_colors(&Config::parse(large.as_bytes()).unwrap())
                .unwrap()
                .len(),
            70_000
        );
        for value in [
            b"log.graphcolors\0".as_slice(),
            b"log.graphcolors\nred,bad\0",
        ] {
            assert_eq!(
                graph_colors(&Config::parse(value).unwrap())
                    .unwrap_err()
                    .exit(),
                crate::Exit::Code(128)
            );
        }
    }

    #[test]
    fn invalid_color_configuration_is_fatal() {
        validate(&Config::default()).unwrap();
        for config in [
            b"color.diff.commit\nblue bold\0".as_slice(),
            b"color.decorate.head\n\0",
            b"log.graphcolors\n\0",
            b"log.graphcolors\nred, #abc, \0",
        ] {
            validate(&Config::parse(config).unwrap()).unwrap();
        }
        for config in [
            b"log.graphcolors\0".as_slice(),
            b"log.graphcolors\nred,bad\0",
            b"color.diff.commit\0",
            b"color.decorate.head\nbad\0",
        ] {
            assert_eq!(
                validate(&Config::parse(config).unwrap())
                    .unwrap_err()
                    .exit(),
                crate::Exit::Code(128)
            );
        }
    }

    #[test]
    fn color_grammar_and_emission_order() {
        for (input, output) in [
            ("", ""),
            ("normal", ""),
            ("-1", ""),
            ("-01", ""),
            ("reset", "\x1b[m"),
            ("RESET reset", "\x1b[m"),
            ("reset bold red blue", "\x1b[;1;31;44m"),
            ("normal blue", "\x1b[44m"),
            ("red normal bold", "\x1b[1;31m"),
            ("bold nobold dim nodim", "\x1b[1;2;22m"),
            (
                "no-bold no-dim no-italic no-ul no-blink no-reverse no-strike",
                "\x1b[22;23;24;25;27;29m",
            ),
            (
                "strike reverse blink ul italic dim bold",
                "\x1b[1;2;3;4;5;7;9m",
            ),
            ("default default", "\x1b[39;49m"),
            ("brightRed brightwhite", "\x1b[91;107m"),
            ("8 15", "\x1b[90;107m"),
            ("16 255", "\x1b[38;5;16;48;5;255m"),
            ("#abc #AB12EF", "\x1b[38;2;170;187;204;48;2;171;18;239m"),
            ("+1 01", "\x1b[31;41m"),
            ("-0", "\x1b[30m"),
            (" \tRED\n\rblue ", "\x1b[31;44m"),
        ] {
            assert_eq!(
                parse(input.as_bytes()).unwrap().0,
                output.as_bytes(),
                "{input}"
            );
        }
        for input in [
            "BOLD",
            "underline",
            "brightdefault",
            "256",
            "-2",
            "0x1",
            "red blue green",
            "normal normal red",
            "#12",
            "#1234",
            "#ggg",
            "#123456789",
            "#",
            "no-Bold",
            "9999999",
        ] {
            assert_eq!(parse(input.as_bytes()), None, "{input}");
        }
        assert_eq!(parse(b"\xff"), None);
    }

    #[test]
    fn color_boolean_forms() {
        assert_eq!(ColorBool::parse(None), Some(ColorBool::Auto));
        for input in [b"true".as_slice(), b"yes", b"1", b"auto", b"AUTO"] {
            assert_eq!(ColorBool::parse(Some(input)), Some(ColorBool::Auto));
        }
        for input in [b"false".as_slice(), b"no", b"0", b"never", b""] {
            assert_eq!(ColorBool::parse(Some(input)), Some(ColorBool::Never));
        }
        assert_eq!(ColorBool::parse(Some(b"always")), Some(ColorBool::Always));
        assert_eq!(ColorBool::parse(Some(b"invalid")), None);
    }

    #[test]
    fn auto_color_environment_matrix() {
        for terminal in [false, true] {
            for paging in [false, true] {
                for in_use in [false, true] {
                    for pager_color in [false, true] {
                        for term in [None, Some(""), Some("dumb"), Some("xterm")] {
                            for no_color in [None, Some(""), Some("1")] {
                                let mut ctx = Context {
                                    stdout_is_terminal: terminal,
                                    ..Context::default()
                                };
                                for (key, value) in [
                                    ("TERM", term),
                                    ("NO_COLOR", no_color),
                                    ("GIT_PAGER_IN_USE", in_use.then_some("")),
                                ] {
                                    if let Some(value) = value {
                                        ctx.env.push((key.into(), value.into()));
                                    }
                                }
                                let config = Config::parse(
                                    format!("color.pager\n{pager_color}\0").as_bytes(),
                                )
                                .unwrap();
                                let expected = !matches!(term, None | Some("dumb"))
                                    && no_color != Some("1")
                                    && (terminal || (paging || in_use) && pager_color);
                                assert_eq!(want_color(&ctx, &config, paging).unwrap(), expected);
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn invalid_color_booleans_name_the_selected_key() {
        let ctx = Context {
            env: vec![("TERM".into(), "xterm".into())],
            ..Context::default()
        };
        for key in [
            "color.ui",
            "color.diff",
            "diff.color",
            "color.pager",
            "pager.color",
        ] {
            let config = Config::parse(format!("{key}\ninvalid\0").as_bytes()).unwrap();
            let error = want_color(&ctx, &config, true).unwrap_err();
            assert_eq!(error.exit(), crate::Exit::Code(128));
            assert_eq!(
                error.to_string(),
                format!("invalid configuration {key}: invalid")
            );
        }
        for (settings, key) in [
            ("color.ui\ninvalid\0diff.color\nbad\0", "diff.color"),
            ("diff.color\nnever\0color.diff\nbad\0", "color.diff"),
            ("color.pager\ntrue\0pager.color\nbad\0", "pager.color"),
            ("pager.color\nfalse\0color.pager\nbad\0", "color.pager"),
        ] {
            let config = Config::parse(settings.as_bytes()).unwrap();
            assert_eq!(
                want_color(&ctx, &config, true).unwrap_err().to_string(),
                format!("invalid configuration {key}: bad")
            );
        }
    }

    #[test]
    fn color_setting_aliases_and_errors() {
        let ctx = Context::default();
        for config in [
            b"color.ui\nalways\0".as_slice(),
            b"color.diff\nnever\0diff.color\nalways\0",
            b"color.ui\nnever\0color.diff\nalways\0",
        ] {
            assert!(want_color(&ctx, &Config::parse(config).unwrap(), false).unwrap());
        }
        assert!(
            !want_color(
                &ctx,
                &Config::parse(b"color.ui\nalways\0color.diff\nnever\0").unwrap(),
                true
            )
            .unwrap()
        );
        assert_eq!(
            want_color(
                &ctx,
                &Config::parse(b"color.diff\ninvalid\0").unwrap(),
                false
            )
            .unwrap_err()
            .exit(),
            crate::Exit::Code(128)
        );
        let ctx = Context {
            env: vec![("TERM".into(), "xterm".into())],
            ..Context::default()
        };
        assert!(want_color(&ctx, &Config::default(), true).unwrap());
        assert!(
            !want_color(
                &ctx,
                &Config::parse(b"color.pager\ntrue\0pager.color\nfalse\0").unwrap(),
                true
            )
            .unwrap()
        );
        assert_eq!(
            want_color(
                &ctx,
                &Config::parse(b"color.pager\ninvalid\0").unwrap(),
                true
            )
            .unwrap_err()
            .exit(),
            crate::Exit::Code(128)
        );
    }
}
