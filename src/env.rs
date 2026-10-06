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

pub fn git_bool(value: Option<&[u8]>) -> Option<bool> {
    let Some(value) = value else {
        return Some(true);
    };
    if [b"true".as_slice(), b"yes", b"on"]
        .iter()
        .any(|word| value.eq_ignore_ascii_case(word))
    {
        Some(true)
    } else if [b"false".as_slice(), b"no", b"off", b""]
        .iter()
        .any(|word| value.eq_ignore_ascii_case(word))
    {
        Some(false)
    } else {
        git_int(value).map(|value| value != 0)
    }
}

fn git_int(value: &[u8]) -> Option<i32> {
    let value = std::str::from_utf8(value)
        .ok()?
        .trim_start_matches(|c: char| c.is_ascii_whitespace() || c == '\u{b}');
    let (value, scale) = match value.as_bytes().last() {
        Some(b'k' | b'K') => (value.get(..value.len() - 1)?, 1024),
        Some(b'm' | b'M') => (value.get(..value.len() - 1)?, 1024 * 1024),
        Some(b'g' | b'G') => (value.get(..value.len() - 1)?, 1024 * 1024 * 1024),
        _ => (value, 1),
    };
    let (digits, sign) = value.strip_prefix('-').map_or_else(
        || (value.strip_prefix('+').unwrap_or(value), 1),
        |value| (value, -1),
    );
    let (digits, radix) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
        .map_or_else(
            || (digits, if digits.starts_with('0') { 8 } else { 10 }),
            |hex| (hex, 16),
        );
    if digits.is_empty() || !digits.chars().all(|digit| digit.is_digit(radix)) {
        return None;
    }
    let value = i64::from_str_radix(digits, radix)
        .ok()?
        .checked_mul(sign)?
        .checked_mul(scale)?;
    i32::try_from(value).ok()
}

pub fn value<'a>(ctx: &'a crate::Context, key: &str) -> Option<&'a std::ffi::OsStr> {
    ctx.env
        .iter()
        .rev()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_os_str())
}

pub fn columns(ctx: &crate::Context) -> usize {
    value(ctx, "COLUMNS")
        .and_then(|value| value.to_str())
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|&columns| columns > 0)
        .or_else(|| ctx.terminal_columns.filter(|&columns| columns > 0))
        .unwrap_or(80)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_columns_prefer_positive_environment_values() {
        use std::os::unix::ffi::OsStringExt;
        let mut ctx = crate::Context {
            terminal_columns: Some(132),
            ..crate::Context::default()
        };
        assert_eq!(columns(&ctx), 132);
        ctx.env.push(("COLUMNS".into(), "95".into()));
        assert_eq!(columns(&ctx), 95);
        for value in [
            b"0".as_slice(),
            b"-1",
            b"invalid",
            b"",
            b"999999999999999999999999",
            b"\xff",
        ] {
            ctx.env = vec![(
                "COLUMNS".into(),
                std::ffi::OsString::from_vec(value.to_vec()),
            )];
            assert_eq!(columns(&ctx), 132);
        }
        ctx.env.clear();
        ctx.terminal_columns = None;
        assert_eq!(columns(&ctx), 80);
        ctx.terminal_columns = Some(0);
        assert_eq!(columns(&ctx), 80);
    }

    #[test]
    fn boolean_keywords() {
        assert_eq!(git_bool(None), Some(true));
        for word in [b"true".as_slice(), b"yes", b"ON", b"True"] {
            assert_eq!(git_bool(Some(word)), Some(true));
        }
        for word in [b"false".as_slice(), b"no", b"OFF", b""] {
            assert_eq!(git_bool(Some(word)), Some(false));
        }
    }

    #[test]
    fn numeric_booleans() {
        for word in [
            "1",
            "-1",
            "+1",
            "010",
            "0x10",
            "0X10",
            "1k",
            "1K",
            "1m",
            "1M",
            "1g",
            "1G",
            " 1",
            "\u{b}1",
            "-2147483648",
            "2147483647",
        ] {
            assert_eq!(git_bool(Some(word.as_bytes())), Some(true), "{word}");
        }
        for word in ["0", "-0", "+0", "000", "0x0", "0K"] {
            assert_eq!(git_bool(Some(word.as_bytes())), Some(false), "{word}");
        }
        for word in [
            "08",
            "2147483648",
            "-2147483649",
            "2g",
            "1 ",
            " true",
            "auto",
            "k",
            "-",
            "0x",
            "1kb",
            "0x+1",
            "1.0",
            "9999999999999999999999999999",
        ] {
            assert_eq!(git_bool(Some(word.as_bytes())), None, "{word}");
        }
        assert_eq!(git_bool(Some(&[0xff])), None);
    }
}
