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

use crate::error::Error;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Oid {
    len: u8,
    bytes: [u8; 32],
}

impl Oid {
    pub fn parse(hex: &[u8]) -> Result<Self, Error> {
        let len = match hex.len() {
            40 => 20,
            64 => 32,
            _ => return Err(Error::Protocol("invalid object ID length")),
        };
        let mut bytes = [0; 32];
        for (slot, [high, low]) in bytes.iter_mut().zip(hex.as_chunks::<2>().0) {
            *slot = (digit(*high)? << 4) | digit(*low)?;
        }
        Ok(Self { len, bytes })
    }
}

const fn digit(byte: u8) -> Result<u8, Error> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(Error::Protocol("non-hexadecimal object ID")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_formats_and_case() {
        let sha1 = Oid::parse(b"0123456789abcdef0123456789abcdef01234567").unwrap();
        let upper = Oid::parse(b"0123456789ABCDEF0123456789ABCDEF01234567").unwrap();
        assert_eq!(sha1, upper);
        assert_eq!(sha1.len, 20);
        assert_eq!(sha1.bytes[0], 1);
        assert_eq!(sha1.bytes[7], 0xef);
        let sha256 = Oid::parse(&[b'0'; 64]).unwrap();
        assert_eq!(sha256.len, 32);
        assert_ne!(sha1, sha256);
    }

    #[test]
    fn malformed_ids() {
        for len in [0, 4, 39, 41, 63, 65] {
            assert_eq!(
                Oid::parse(&vec![b'0'; len]).unwrap_err().to_string(),
                "invalid git output: invalid object ID length"
            );
        }
        for byte in [b'g', b'/', b' ', 0xff] {
            assert_eq!(
                Oid::parse(&[byte; 40]).unwrap_err().to_string(),
                "invalid git output: non-hexadecimal object ID"
            );
        }
    }
}
