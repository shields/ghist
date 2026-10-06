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

use crate::git::log::Record;

pub fn lines(record: &Record) -> Vec<Vec<u8>> {
    let format = if record.hash.len() == 40 {
        b"sha1 ".as_slice()
    } else {
        b"sha256 "
    };
    let mut first = [format, &record.hash].concat();
    if !record.decorations.is_empty() {
        first.extend_from_slice(b" (");
        first.extend_from_slice(&decorations(&record.decorations));
        first.push(b')');
    }
    let mut lines = vec![first];
    if record.parents.len() > 1 {
        let mut merge = b"Merge:".to_vec();
        for parent in &record.parents {
            merge.push(b' ');
            merge.extend_from_slice(
                parent
                    .hash
                    .get(..parent.unique.max(7))
                    .unwrap_or(&parent.hash),
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

fn decorations(bytes: &[u8]) -> Vec<u8> {
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
        let name = if let Some(target) = name.strip_prefix(b"HEAD -> ") {
            output.extend_from_slice(b"HEAD -> ");
            target
        } else if let Some(target) = name.strip_prefix(b"tag: ") {
            output.extend_from_slice(b"tag: ");
            target
        } else {
            name
        };
        let name = [b"refs/heads/".as_slice(), b"refs/remotes/", b"refs/tags/"]
            .into_iter()
            .find_map(|prefix| name.strip_prefix(prefix))
            .unwrap_or(name);
        output.extend_from_slice(name);
        if next.is_empty() {
            break;
        }
        output.extend_from_slice(b", ");
        rest = next;
    }
    output
}

#[cfg(test)]
pub mod tests {
    use super::*;

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
        assert_eq!(lines(&record), expected);
        record.committer_name = b"Ada".to_vec();
        record.committer_date = b"2026-10-05 13:12:00 +0000".to_vec();
        let rendered = lines(&record);
        assert_eq!(rendered[2], b"Commit:     Ada <grace@example.com>");
        assert_eq!(rendered[3], expected[2]);
        assert_eq!(rendered[4], b"CommitDate: 2026-10-05 13:12:00 +0000");
        record.committer_name.clone_from(&record.author_name);
        record.committer_email = b"another@example.com".to_vec();
        assert_eq!(
            lines(&record)[2],
            b"Commit:     Grace <another@example.com>"
        );
        record.hash = vec![b'a'; 64];
        record.decorations.clear();
        assert_eq!(
            lines(&record)[0],
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
        assert_eq!(lines(&record)[1], b"Merge: 0123456 01234567");
        assert_eq!(decorations(b"HEAD -> refs/heads/main, tag: refs/tags/v1, refs/remotes/origin/main, refs/heads/feature, refs/stash, grafted, replaced, refs/custom/a"), b"HEAD -> main, tag: v1, origin/main, feature, refs/stash, grafted, replaced, refs/custom/a");
        assert_eq!(
            decorations(b"HEAD, tag: refs/tags/\xff"),
            b"HEAD, tag: \xff"
        );
        assert_eq!(
            decorations(b"refs/heads/a,b, tag: refs/tags/c,d"),
            b"a,b, tag: c,d"
        );
    }
}
