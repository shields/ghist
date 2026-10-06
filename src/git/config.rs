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

use std::io::Read;

use crate::Context;
use crate::error::Error;

use super::{Process, command};

#[derive(Debug, Default, Eq, PartialEq)]
pub struct Config {
    entries: Vec<(Vec<u8>, Value)>,
}

#[derive(Debug, Eq, PartialEq)]
pub enum Value {
    Bare,
    Bytes(Vec<u8>),
}

impl Value {
    pub fn bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Bare => None,
            Self::Bytes(bytes) => Some(bytes),
        }
    }
}

impl Config {
    pub fn read(ctx: &Context) -> Result<(Self, Vec<u8>), Error> {
        let mut process = Process::spawn(command(ctx).args(["config", "--list", "-z"]))?;
        let mut bytes = Vec::new();
        let read = process.stdout.read_to_end(&mut bytes);
        let stderr = process.finish()?;
        read?;
        Ok((Self::parse(&bytes)?, stderr))
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let mut entries = Vec::new();
        for entry in bytes.split_inclusive(|&byte| byte == 0) {
            let entry = entry
                .strip_suffix(b"\0")
                .ok_or(Error::Protocol("unterminated config entry"))?;
            let (key, value) = entry.iter().position(|&byte| byte == b'\n').map_or(
                (entry, Value::Bare),
                |index| {
                    let (key, value) = entry.split_at(index);
                    (key, Value::Bytes(value.iter().skip(1).copied().collect()))
                },
            );
            let first = key
                .iter()
                .position(|&byte| byte == b'.')
                .ok_or(Error::Protocol("invalid config key"))?;
            let last = key.iter().rposition(|&byte| byte == b'.').unwrap_or(first);
            if first == 0 || last + 1 == key.len() {
                return Err(Error::Protocol("invalid config key"));
            }
            let mut key = key.to_vec();
            for byte in key.iter_mut().take(first) {
                byte.make_ascii_lowercase();
            }
            for byte in key.iter_mut().skip(last + 1) {
                byte.make_ascii_lowercase();
            }
            entries.push((key, value));
        }
        Ok(Self { entries })
    }

    pub fn boolean(&self, key: &[u8], default: bool) -> Result<bool, Error> {
        self.last(&[key]).map_or(Ok(default), |value| {
            crate::env::git_bool(value.bytes()).ok_or_else(|| Error::Config {
                key: key.to_vec(),
                value: value.bytes().map(<[u8]>::to_vec),
            })
        })
    }

    pub fn last(&self, keys: &[&[u8]]) -> Option<&Value> {
        self.entries
            .iter()
            .rev()
            .find(|(key, _)| keys.contains(&key.as_slice()))
            .map(|(_, value)| value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config() {
        assert_eq!(Config::parse(b"").unwrap(), Config::default());
        assert_eq!(Config::default().last(&[b"log.mailmap"]), None);
    }

    #[test]
    fn values_and_alias_order() {
        let config = Config::parse(b"color.diff\nnever\0diff.color\nalways\0core.pager\n\0log.mailmap\0test.value\none\ntwo\xff\0").unwrap();
        assert_eq!(
            config.last(&[b"color.diff", b"diff.color"]),
            Some(&Value::Bytes(b"always".to_vec()))
        );
        assert_eq!(
            config.last(&[b"core.pager"]),
            Some(&Value::Bytes(b"".to_vec()))
        );
        assert_eq!(config.last(&[b"log.mailmap"]), Some(&Value::Bare));
        assert_eq!(
            config.last(&[b"test.value"]),
            Some(&Value::Bytes(b"one\ntwo\xff".to_vec()))
        );
    }

    #[test]
    fn canonicalizes_sections_and_variables_only() {
        let config = Config::parse(b"Remote.Origin.URL\nfirst\0REMOTE.origin.Url\nsecond\0remote.Origin.url\nthird\0CoRe.PaGeR\ncat\0").unwrap();
        assert_eq!(
            config.last(&[b"remote.Origin.url"]),
            Some(&Value::Bytes(b"third".to_vec()))
        );
        assert_eq!(
            config.last(&[b"remote.origin.url"]),
            Some(&Value::Bytes(b"second".to_vec()))
        );
        assert_eq!(
            config.last(&[b"core.pager"]),
            Some(&Value::Bytes(b"cat".to_vec()))
        );
    }

    #[test]
    fn boolean_values() {
        let config =
            Config::parse(b"test.bare\0test.empty\n\0test.true\non\0test.bad\nwat\0").unwrap();
        assert!(config.boolean(b"test.bare", false).unwrap());
        assert!(!config.boolean(b"test.empty", true).unwrap());
        assert!(config.boolean(b"test.true", false).unwrap());
        assert!(config.boolean(b"test.missing", true).unwrap());
        assert!(!config.boolean(b"test.missing", false).unwrap());
        assert_eq!(
            config.boolean(b"test.bad", false).unwrap_err().exit(),
            crate::Exit::Code(128)
        );
    }

    #[test]
    fn malformed_streams() {
        for bytes in [
            b"core.pager\ncat".as_slice(),
            b"\0",
            b"section\0",
            b".name\0",
            b"section.\0",
        ] {
            assert_eq!(
                Config::parse(bytes).unwrap_err().exit(),
                crate::Exit::Code(1)
            );
        }
    }
}
