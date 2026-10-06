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

use crate::color::{Palette, Sgr};
use crate::git::log::Record;

pub fn lines(record: &Record, colors: Option<&Palette>) -> Vec<Vec<u8>> {
    let format = if record.hash.len() == 40 {
        b"sha1 ".as_slice()
    } else {
        b"sha256 "
    };
    let mut first = Vec::new();
    hash(
        &mut first,
        format,
        &record.hash,
        record.unique,
        colors.map(|colors| &colors.commit),
    );
    if !record.decorations.is_empty() {
        span(&mut first, b" (", colors.map(|colors| &colors.commit));
        first.extend_from_slice(&decorations(&record.decorations, colors));
        span(&mut first, b")", colors.map(|colors| &colors.commit));
    }
    let mut lines = vec![first];
    if record.parents.len() > 1 {
        let mut merge = b"Merge:".to_vec();
        for parent in &record.parents {
            merge.push(b' ');
            let display = parent
                .hash
                .get(..parent.unique.max(7))
                .unwrap_or(&parent.hash);
            hash(
                &mut merge,
                b"",
                display,
                parent.unique,
                colors.map(|colors| &colors.commit),
            );
        }
        lines.push(merge);
    }
    lines.push(identity(
        b"Author:     ",
        &record.author_name,
        &record.author_email,
    ));
    if record.author_name != record.committer_name || record.author_email != record.committer_email
    {
        lines.push(identity(
            b"Commit:     ",
            &record.committer_name,
            &record.committer_email,
        ));
    }
    lines.push([b"AuthorDate: ", record.author_date.as_slice()].concat());
    if record.author_date != record.committer_date {
        lines.push([b"CommitDate: ", record.committer_date.as_slice()].concat());
    }
    lines
}

fn identity(label: &[u8], name: &[u8], email: &[u8]) -> Vec<u8> {
    [label, name, b" <", email, b">"].concat()
}

fn hash(out: &mut Vec<u8>, label: &[u8], hash: &[u8], unique: usize, color: Option<&Sgr>) {
    if let Some(color) = color {
        out.extend_from_slice(&color.0);
        out.extend_from_slice(label);
        let (prefix, tail) = hash.split_at(unique.min(hash.len()));
        out.extend_from_slice(prefix);
        if !tail.is_empty() {
            out.extend_from_slice(b"\x1b[22;2m");
            out.extend_from_slice(tail);
        }
        out.extend_from_slice(b"\x1b[m");
    } else {
        out.extend_from_slice(label);
        out.extend_from_slice(hash);
    }
}

fn span(out: &mut Vec<u8>, text: &[u8], color: Option<&Sgr>) {
    if let Some(color) = color {
        out.extend_from_slice(&color.0);
    }
    out.extend_from_slice(text);
    if color.is_some() {
        out.extend_from_slice(b"\x1b[m");
    }
}

fn decorations(bytes: &[u8], colors: Option<&Palette>) -> Vec<u8> {
    let mut output = Vec::new();
    let mut rest = bytes;
    loop {
        let (name, next) = rest.windows(2).position(|part| part == b", ").map_or(
            (rest, b"".as_slice()),
            |index| {
                let (name, tail) = rest.split_at(index);
                (name, tail.get(2..).unwrap_or_default())
            },
        );
        if let Some(target) = name.strip_prefix(b"HEAD -> ") {
            span(&mut output, b"HEAD", colors.map(|colors| &colors.head));
            span(&mut output, b" -> ", colors.map(|colors| &colors.commit));
            reference(&mut output, target, colors);
        } else if let Some(target) = name.strip_prefix(b"tag: ") {
            let target = target.strip_prefix(b"refs/tags/").unwrap_or(target);
            span(
                &mut output,
                &[b"tag: ", target].concat(),
                colors.map(|colors| &colors.tag),
            );
        } else {
            reference(&mut output, name, colors);
        }
        if next.is_empty() {
            break;
        }
        span(&mut output, b", ", colors.map(|colors| &colors.commit));
        rest = next;
    }
    output
}

fn reference(out: &mut Vec<u8>, name: &[u8], colors: Option<&Palette>) {
    for (prefix, color) in [
        (
            b"refs/heads/".as_slice(),
            colors.map(|colors| &colors.branch),
        ),
        (
            b"refs/remotes/".as_slice(),
            colors.map(|colors| &colors.remote),
        ),
        (b"refs/tags/".as_slice(), colors.map(|colors| &colors.tag)),
    ] {
        if let Some(short) = name.strip_prefix(prefix) {
            span(out, short, color);
            return;
        }
    }
    let color = colors.map(|colors| match name {
        b"HEAD" => &colors.head,
        b"refs/stash" => &colors.stash,
        b"grafted" | b"replaced" => &colors.grafted,
        _ => &colors.commit,
    });
    span(out, name, color);
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::git::config::Config;

    #[test]
    fn colored_hash_tails_and_decorations() {
        let mut record = record();
        let palette = Palette::read(&Config::default()).unwrap();
        assert_eq!(lines(&record, Some(&palette))[0], b"\x1b[33msha1 0123\x1b[22;2m456789abcdef0123456789abcdef01234567\x1b[m\x1b[33m (\x1b[m\x1b[1;36mHEAD\x1b[m\x1b[33m -> \x1b[m\x1b[1;32mmain\x1b[m\x1b[33m)\x1b[m");
        record.unique = record.hash.len();
        record.decorations.clear();
        assert_eq!(
            lines(&record, Some(&palette))[0],
            b"\x1b[33msha1 0123456789abcdef0123456789abcdef01234567\x1b[m"
        );
        for unique in [4, 8] {
            record.parents.push(crate::git::log::Parent {
                oid: record.oid,
                hash: record.hash.clone(),
                unique,
            });
        }
        assert_eq!(
            lines(&record, Some(&palette))[1],
            b"Merge: \x1b[33m0123\x1b[22;2m456\x1b[m \x1b[33m01234567\x1b[m"
        );
        assert_eq!(decorations(b"HEAD, tag: refs/tags/v1, refs/remotes/origin/main, refs/stash, grafted, replaced, refs/custom/x, refs/tags/v2", Some(&palette)), b"\x1b[1;36mHEAD\x1b[m\x1b[33m, \x1b[m\x1b[1;33mtag: v1\x1b[m\x1b[33m, \x1b[m\x1b[1;31morigin/main\x1b[m\x1b[33m, \x1b[m\x1b[1;35mrefs/stash\x1b[m\x1b[33m, \x1b[m\x1b[1;34mgrafted\x1b[m\x1b[33m, \x1b[m\x1b[1;34mreplaced\x1b[m\x1b[33m, \x1b[m\x1b[33mrefs/custom/x\x1b[m\x1b[33m, \x1b[m\x1b[1;33mv2\x1b[m");
        let palette = Palette::read(
            &Config::parse(b"color.diff.commit\nbold blue\0color.decorate.branch\n\0").unwrap(),
        )
        .unwrap();
        record.unique = 4;
        record.decorations = b"refs/heads/main".to_vec();
        assert_eq!(lines(&record, Some(&palette))[0], b"\x1b[1;34msha1 0123\x1b[22;2m456789abcdef0123456789abcdef01234567\x1b[m\x1b[1;34m (\x1b[mmain\x1b[m\x1b[1;34m)\x1b[m");
        for config in [
            b"color.diff.commit\0".as_slice(),
            b"color.decorate.tag\nbad\0",
        ] {
            assert_eq!(
                Palette::read(&Config::parse(config).unwrap())
                    .unwrap_err()
                    .exit(),
                crate::Exit::Code(128)
            );
        }
    }

    pub fn record() -> Record {
        Record::parse(
            [
                b"0123456789abcdef0123456789abcdef01234567".as_slice(),
                b"0123",
                b"",
                b"",
                b"Grace",
                b"grace@example.com",
                b"2026-10-05 09:12:00 -0400",
                b"Grace",
                b"grace@example.com",
                b"2026-10-05 09:12:00 -0400",
                b"HEAD -> refs/heads/main",
                b"subject\n\nbody\n",
            ]
            .map(<[u8]>::to_vec),
        )
        .unwrap()
    }

    #[test]
    fn identities_dates_and_header_order() {
        let mut record = record();
        let expected = [
            b"sha1 0123456789abcdef0123456789abcdef01234567 (HEAD -> main)".as_slice(),
            b"Author:     Grace <grace@example.com>",
            b"AuthorDate: 2026-10-05 09:12:00 -0400",
        ]
        .map(<[u8]>::to_vec);
        assert_eq!(lines(&record, None), expected);
        record.committer_name = b"Ada".to_vec();
        record.committer_date = b"2026-10-05 13:12:00 +0000".to_vec();
        let rendered = lines(&record, None);
        assert_eq!(rendered[2], b"Commit:     Ada <grace@example.com>");
        assert_eq!(rendered[3], expected[2]);
        assert_eq!(rendered[4], b"CommitDate: 2026-10-05 13:12:00 +0000");
        record.committer_name.clone_from(&record.author_name);
        record.committer_email = b"another@example.com".to_vec();
        assert_eq!(
            lines(&record, None)[2],
            b"Commit:     Grace <another@example.com>"
        );
        record.hash = vec![b'a'; 64];
        record.decorations.clear();
        assert_eq!(
            lines(&record, None)[0],
            [b"sha256 ".as_slice(), &[b'a'; 64]].concat()
        );
    }

    #[test]
    fn merge_abbreviations_and_decoration_kinds() {
        let mut record = record();
        for unique in [4, 8] {
            record.parents.push(crate::git::log::Parent {
                oid: record.oid,
                hash: record.hash.clone(),
                unique,
            });
        }
        assert_eq!(lines(&record, None)[1], b"Merge: 0123456 01234567");
        assert_eq!(decorations(b"HEAD -> refs/heads/main, tag: refs/tags/v1, refs/remotes/origin/main, refs/heads/feature, refs/stash, grafted, replaced, refs/custom/a", None), b"HEAD -> main, tag: v1, origin/main, feature, refs/stash, grafted, replaced, refs/custom/a");
        assert_eq!(
            decorations(b"HEAD, tag: refs/tags/\xff", None),
            b"HEAD, tag: \xff"
        );
        assert_eq!(
            decorations(b"refs/heads/a,b, tag: refs/tags/c,d", None),
            b"a,b, tag: c,d"
        );
    }
}
