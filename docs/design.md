<!--
Copyright © 2026 Michael Shields

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
-->

# Design

[README.md](../README.md) describes the interface. The
[edge-case matrix](edge-cases.md) records its verification.

## Git owns history semantics

The renderer delegates walking, revision syntax, path simplification, mailmap,
encodings, shallow boundaries, replacement objects, and diffs to Git. Every Git
child receives the working directory and complete environment from `Context`;
tests supply an allowlisted environment. Child guards kill and reap unfinished
processes.

The process sequence is:

1. `git --no-pager config --list -z` reads configuration. Last entries win,
   including aliases whose relative order matters.
2. If an operand before `--` contains `^` or `..`, `rev-parse --revs-only`
   expands it. A negative result triggers
   `rev-list --boundary --parents --end-of-options`, with the same paths. Only
   the hidden boundary IDs are retained. `--parents` is necessary for
   path-limited parent rewriting.
3. The main `git log` streams records and requested diffs.
4. Binary stat entries lazily start one
   `git cat-file --batch-check=%(objectsize)` child with piped stdin.

Repository changes during a run are unspecified: different subprocesses can
observe different states.

## Record protocol

The main invocation is:

```text
git --no-pager log --no-diff-merges --topo-order --parents --abbrev=4 --decorate=full
  --no-follow --no-show-signature --no-ext-diff --color=always|never
  --format=tformat:%x1e%x1fghist%n%H%x00%h%x00%P%x00%p%x00%aN%x00%aE%x00%ai%x00%cN%x00%cE%x00%ci%x00%D%x00%B%x00
  [-p] [--raw --numstat] --end-of-options <operands> [-- <paths>]
```

When `log.mailmap=false`, lowercase `%an/%ae/%cn/%ce` replace the mailmapped
identity placeholders. No `--root` is added, so `log.showRoot` applies.
`--no-follow`, `--no-show-signature`, and `--no-ext-diff` prevent configuration
from changing the record stream or its rewritten parents.

```text
record := "\x1e\x1fghist\n" field×12 "\n" [ "\n" diff-section ]
field  := bytes-without-NUL "\x00"
```

Fields are read to NUL, never searched for markers. Messages may contain the
marker bytes. Git truncates messages at NUL; ref names and identities cannot
contain NUL. In a diff section, a line beginning with record separator cannot be
a patch, raw record, or numstat record.

Validation checks full hash lengths (40 or 64), abbreviation prefixes, paired
parent counts, framing, raw fields, and numeric counts. Git’s nonzero status
wins over protocol errors, preserving its stderr. `-z` is absent because it
would introduce NUL-delimited filenames into the diff section.

## Rendering

The renderer emits headers, normalized message lines, and then the optional diff
section. Patches are streamed one line at a time; no complete patch or history
is retained. Raw and numstat records are buffered only for the current commit’s
stat layout.

Git reencodes messages before ghist reads them. Fuller-style normalization
removes leading and trailing blank lines, trims line endings, expands tabs to
eight-column stops, and indents four spaces. Invalid UTF-8 or control bytes stop
tab expansion. `render/width.rs` corrects differences between `unicode-width`
and Git’s character-width behavior, verified against Git’s
[tab expansion](https://git-scm.com/docs/pretty-options).

Painted graph prefixes are cached for each commit. Fixed rows are consumed by
header lines, then a continuation row repeats. Blank rows end after the last
graph glyph; text rows align at `2 × widest_lane_count + 1`. A patch’s bytes
follow the prefix unchanged. Combined stats and patches use `---` after the
message and preserve Git’s blank line between the summary and patch.

## Graph layout

`Graph<Id>` holds optional lanes, each with a target ID and palette index. A
missing lane is reusable space. The palette counter starts at `N − 1`.

For each commit:

1. Use its existing target lane, or the first hole for a new tip.
2. Deduplicate shown parents. For a merge or new tip, advance the counter once
   per parent, including parents already assigned a lane. Existing parents
   retain their colors; new targets take the current counter.
3. Continue the first parent in the node column. Tap an existing first parent to
   the left. Pull one from the right, but tap it instead if the node is not in
   column zero and another parent is new. Place other new parents in the nearest
   available lane at or to the right of the node.
4. Emit node, fan-out, pull, and optional compaction rows, in that order.
   Fan-out before pull distinguishes simultaneous attachments.
5. Move at most one lane: the rightmost occupied lane into the first earlier
   hole, excluding the node column. Trim trailing holes.

Cells carry directional arms and a color. A `│` between horizontal segments is a
crossing with the vertical on top; `┼` is a junction. Horizontal spans use the
far attachment’s color; pulls and compaction use the moving lane’s color.
Adjacent equal colors share an SGR span, reset before the text.

The graph sees Git’s rewritten `%P` minus the hidden boundary set. Header
`Merge:` lines retain all parents. Palette indices use `usize`, so configured
palettes are not limited to 65,535 entries.

## Stat layout

Raw entry _i_ pairs with numstat entry _i_. Text entries give insertion and
deletion counts. Binary entries use cat-file sizes, except missing sides (mode
`000000`) have size zero and identical IDs give plain `Bin`. Null abbreviations
are never queried: `0000` can also abbreviate a real object.

Layout is derived from black-box comparisons with Git, without translating its
source. Width is terminal columns minus the commit’s text column. Git’s minimum
width, numeric field width, filename truncation, and configured
`diff.statNameWidth` and `diff.statGraphWidth` determine the remaining space.
Long names retain a suffix, preferring a slash boundary within that suffix.

When bars need scaling, a nonzero count gets
`1 + floor(count × (graph_width − 1) / maximum_change)` cells. For a file with
both additions and deletions, scale the smaller side and give the remainder of
the scaled total to the larger side; ties scale deletions. Both nonzero sides
retain at least one cell. This rounding is significant for matching Git’s
output.

## Output, pager, and signals

One 64 KiB output buffer writes to stdout or a lazily spawned pager. The buffer
flushes before a Git read whose input buffer is empty, so small records reach
the terminal promptly. Git stderr is drained separately and reported after pager
cleanup.

Owned child pipes use nonblocking descriptors and poll in bounded intervals.
Inherited stdout/stderr keep their original descriptor flags. Writes to pipes,
sockets, and terminals are bounded so a successful poll does not lead into an
unbounded blocking write. Regular files use ordinary full writes.

SIGINT, SIGQUIT, SIGTERM, and SIGHUP handlers only set an atomic flag. Reads,
writes, and Git waits observe it, stop Git, close pager stdin, and wait for the
pager. `main` then restores the signal’s default disposition and re-raises it.
The pager is allowed to complete its cleanup.

Output failures stop both the history walker and any pending binary-size lookup
before waiting for their exit. Broken stdout or pager pipes are quiet successes,
unless the pager exits nonzero. A broken cat-file input pipe remains an error:
it is a failed backend request. Other I/O and protocol errors exit 1, usage
errors exit 2, invalid configuration exits 128, and Git and pager failures
retain their status.

## Release verification

The publishing workflow calls the complete CI workflow from the publishing
commit using `uses: $/.github/workflows/ci.yml`. This self-repository reference
satisfies GitHub's fully pinned Actions policy and zizmor's `self-repository`
audit. Only the publishing job receives OIDC permission, after CI succeeds.

GitHub introduced `$/` references on July 30, 2026. Its announcement explicitly
includes reusable workflows:

> It works everywhere the workspace-relative `./` syntax works, including
> workflow steps, composite action steps, nested composition, and reusable
> workflow calls.

Source:
[GitHub's release announcement](https://github.blog/changelog/2026-07-30-reference-same-repository-actions-with-self-repository-syntax/).
The
[reusable workflow reference](https://docs.github.com/en/actions/how-tos/reuse-automations/reuse-workflows#calling-a-reusable-workflow)
also recommends this form, and
[zizmor documents the corresponding audit](https://docs.zizmor.sh/audits/#self-repository).
These sources were checked on October 6, 2026.
