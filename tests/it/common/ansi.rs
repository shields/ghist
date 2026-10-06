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

pub fn strip(mut bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some((&byte, rest)) = bytes.split_first() {
        if let Some(rest) = prefix(bytes) {
            bytes = rest;
            continue;
        }
        out.push(byte);
        bytes = rest;
    }
    out
}

pub fn prefix(bytes: &[u8]) -> Option<&[u8]> {
    let parameters = bytes.strip_prefix(b"\x1b[")?;
    let end = parameters
        .iter()
        .take_while(|byte| byte.is_ascii_digit() || **byte == b';')
        .count();
    (parameters.get(end) == Some(&b'm')).then(|| parameters.get(end + 1..).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sgr_only_and_byte_preservation() {
        assert_eq!(strip(b"\x1b[;1;38;2;1;2;3mtext\x1b[m\xff"), b"text\xff");
        assert_eq!(
            strip(b"\x1b[2J\x1b]0;title\x07\x1b[31"),
            b"\x1b[2J\x1b]0;title\x07\x1b[31"
        );
        assert_eq!(strip(b""), b"");
    }
}
