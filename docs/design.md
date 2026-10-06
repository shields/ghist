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

This document specifies the intended implementation. Verification claims below
refer to the design prototypes; implementation tests must establish them anew.

## Decisions

### User-settled

- **Rust, with a `git log` subprocess as the backend.** ghist does all the
  rendering. gix is not used.
  - Why: exact parity on rev syntax, path limiting with history simplification
    (and Bloom filters), mailmap, encodings, shallow clones, reftable and
    sha256.
  - Why: `git log -p` applies the user's porcelain diff config
    (`diff.algorithm=histogram`, `diff.colorMoved`).
- **Pager like git:** `$GIT_PAGER`, then `core.pager`, then `$PAGER`, then
  `less`. An empty value or `cat` means no pager. `LESS=FRX` and `LV=-c` are set
  only if unset.
- **Dates** are ISO 8601 in the ident's own timezone (git's `%ai`):
  `2026-10-05 20:49:00 -0700`.
- **Arguments:** `ghist [<rev>|<a>..<b>|<a>...<b>|^<rev>…] [--] [<path>…]`. git
  does the walking and the history simplification.
- **Flags:** `-p`, `--stat` (both may be combined), `-h`/`--help`, `--version`
  and `--`. Nothing else.
- **Order:** always topological.
- **Diffs:** merges show no diff or stat. Root commits are diffed against the
  empty tree, as in git.
- **Graph:** curved light box drawing (`│ ─ ╭ ╮ ╯ ╰ ├ ┤ ┬ ┴ ┼`), two cells per
  lane.
  - The node is `●`, uncolored.
  - The first-parent chain stays in column 0, and new parents go to the right.
  - Crossings are drawn with the vertical on top (`─│─`).
  - Lane colors follow git's per-lane assignment.
- **Header order** (user note: identities together, then dates together):

  ```
  ●    sha1 3f2a9c1e5b7d9f0a2c4e6b8d0f1a3c5e7b9d1f3a (HEAD -> main)
  ├─╮  Merge: a1b2c3d f4e5d6c
  │ │  Author:     Grace Hopper <grace@example.com>
  │ │  Commit:     Ada Lovelace <ada@example.com>        ← only if it differs after mailmap
  │ │  AuthorDate: 2026-10-04 09:12:44 -0400
  │ │  CommitDate: 2026-10-05 20:41:13 -0700            ← only if %ci != %ai (epoch or tz)
  │ │
  │ │      Add the frobnicator
  ```

### Made by the design phase (all verified)

- **Decorations are always shown**, using git's order and HEAD collapsing. They
  come from `%D` with `--decorate=full`; `%(decorate:…)` is not needed.
- **`NO_COLOR` turns off `auto` color only**, as no-color.org specifies. An
  explicit `color.ui=always` still wins.
- **Lines ghist generates have no trailing whitespace.** Patch content passes
  through byte for byte.
- **Always Unicode**, with no ASCII fallback. Box-drawing characters in
  ambiguous-width (CJK) terminals are unspecified.
- **Shortest unique prefix comes from `--abbrev=4` with `%h`.** It is unique
  among all objects; this was checked against `cat-file` on 2M objects.
  - **Visible side effect:** patch `index` lines also use shortest-unique
    abbreviations, such as `index 1b01..3db4`. No git flag separates the two.
  - The README documents this.
- **`Merge:` line:** each parent's unique prefix is bright, followed by a dim
  tail out to `max(7, u)` characters.
- **Mailmap:** honored as git does. Use `%aN`/`%aE`/`%cN`/`%cE` when
  `log.mailmap` is true (the default), else `%an`/`%ae`/`%cn`/`%ce`. `%aN`
  ignores `log.mailmap=false`, so the choice is needed. `Commit:` is shown when
  the name or email differs bytewise.
- **Graph compaction is on**, as its own commit (C16): at most one "jog" row per
  commit moves the rightmost lane into a hole (`╭─╯`). Without it, merge-heavy
  repos get wider than git: the median for ohmyzsh is 21 lanes without
  compaction, versus 13 with it and 12 in git. With compaction, brew, esphome
  and bun match git exactly.
- **Applying your fail-fast rule** (these deliberately differ from git):
  - Every invalid color value is fatal (exit 128), including bad
    `log.graphColors` entries, which git only warns about.
  - If the pager can't be spawned, that is an error.
  - A nonzero exit from the pager becomes ghist's exit status. If the pager
    command doesn't exist, `sh` reports it and ghist exits 127.
- **`EPIPE`** (for example `| head`, or quitting `less` early) is a quiet
  exit 0.
- **Signals:** SIGINT, QUIT, TERM and HUP set a flag. ghist then closes the
  pager pipe, waits for the pager, and re-raises the signal so the shell sees a
  death by signal, as with git.
- **git's stderr** (such as rename-limit warnings) is captured and printed after
  the pager exits. If git fails before any output, it is printed immediately.
- **Minimum git version:** the README says 2.29+, which is the newest flag ghist
  passes (`--no-diff-merges`). There is no runtime version check and no CI job
  for the oldest git; CI uses the runner's git (2.55).

## Architecture

### Process pipeline

| Step | Command                                                                                 | When                                                    | Notes                                                                                                                                                                                                    |
| ---- | --------------------------------------------------------------------------------------- | ------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A    | `git --no-pager config --list -z`                                                       | always                                                  | Last entry wins. Keys are canonicalized (section and variable name lowercased; subsection keeps its case). Provides colors, `core.pager`, `log.mailmap`, `diff.statGraphWidth` and `diff.statNameWidth`. |
| B    | `git --no-pager rev-parse --revs-only <before--> [-- <after>]`                          | only if an argument before `--` contains `^` or `..`    | Detects negative revs.                                                                                                                                                                                   |
| P    | `git --no-pager rev-list --boundary --parents --end-of-options <before--> [-- <after>]` | only if B printed a line starting with `^`              | Runs to completion first. Its `-<oid>` lines form the hidden-parent `HashSet`. `--parents` is mandatory: without it the set is wrong under path limiting (verified).                                     |
| C    | the main `git log` (below)                                                              | always                                                  | Streamed; its stderr is drained by a thread.                                                                                                                                                             |
| K    | `git cat-file --batch-check=%(objectsize)`                                              | started lazily, on the first binary file under `--stat` | Supplies the sizes for `Bin X -> Y bytes`.                                                                                                                                                               |

Every child process gets:

- `env_clear().envs(ctx.env)`: the full environment in production, an allowlist
  in tests;
- `current_dir(ctx.cwd)`;
- `stdin(Stdio::null())`;
- a `ChildGuard` that kills and reaps it on drop.

### The main `git log` command

```
git --no-pager log --no-diff-merges --topo-order --parents --abbrev=4 --decorate=full
  --no-follow --no-show-signature --no-ext-diff --color=always|never
  --format=tformat:%x1e%x1fghist%n%H%x00%h%x00%P%x00%p%x00%aN%x00%aE%x00%ai%x00%cN%x00%cE%x00%ci%x00%D%x00%B%x00
  [-p] [--raw --numstat] --end-of-options <before…> [-- <after…>]
```

- **Hostile config neutralized:**
  - `--no-follow`: `log.follow=true` breaks parent rewriting, leaving lanes that
    never end.
  - `--no-show-signature`: `log.showSignature` writes `gpg:` lines into stdout
    ahead of each record.
  - `--no-ext-diff`: ignores `diff.external`.
- `log.showRoot` is honored (no `--root` is passed). User arguments before `--`
  are passed through, with no `--` added, so git still disambiguates revs from
  paths. ghist rejects any `-`-prefixed argument other than its own flags before
  `--`.
- **Framing**, verified with `od -c`:

  ```
  record := "\x1e\x1fghist\n" field×12 (each NUL-terminated) "\n" [ "\n" diffsec ]
  ```

  - Fields never contain NUL: git truncates messages at NUL, and refnames and
    idents can't contain it.
  - `%B` is read with `read_until(0)` and never scanned for the marker.
  - No diff-section line can start with 0x1E: raw lines start with `:`, numstat
    lines with a digit or `-`, and patch lines with a sign, a header word or
    ESC.
  - `-z` is never used, because `--numstat -z` puts NULs in the diff section.
  - The second `\n` (SEP) is present only when the commit has a diff.
  - In stat mode the diff section holds raw lines, then numstat lines, then
    (with `-p`) a blank line and the patch.

- **Validation:**
  - `len(%H)` must be 40 (`sha1`) or 64 (`sha256`). This is the object-format
    detection; no extra process is needed.
  - `%h` must be a prefix of `%H`.
  - `%P` and `%p` must have the same number of entries.
  - On any protocol error, first `wait()` for git and report git's own error if
    it failed.
- **Shown parents:** `%P` minus the hidden set. `%P` is already rewritten under
  path limiting, and empty for shallow (grafted) commits, which therefore render
  as roots.

### Rendering (`render/`)

**Per-commit line sequence:**

1. `<node> sha1 <hash> (decos)`
2. `Merge:` (merges only)
3. `Author:`, then `Commit:`?, then `AuthorDate:`, then `CommitDate:`?
4. A blank line and the message, unless the message is empty.
5. If there is a diff: a blank line with `-p` alone or `--stat` alone, or `---`
   with both.
6. The stat block, then a blank line, then the patch.

A blank separator row (the previous commit's pad row, trimmed) goes between
commits. Labels are 12 columns wide.

**Message normalization** (git's fuller rules):

- Drop leading blank lines.
- Right-trim each line of space, tab, CR and LF.
- Drop trailing blank lines.
- Expand tabs to 8-column stops measured from the start of the message. Stop
  expanding at invalid UTF-8 or a control character.
- Indent by 4 spaces.
- Use raw bytes throughout; git has already re-encoded to
  `i18n.logOutputEncoding`.

**Escape sequences.** Let C be `color.diff.commit`.

- Header: `C "sha1 " H[..u] \e[22;2m H[u..] \e[m`. SGR 22 clears bold, so a bold
  commit color dims predictably.
- Decorations: punctuation in C. `HEAD` uses the HEAD color. `tag: X` uses the
  tag color for both spans. `refs/heads/X` is shown as `X` in the branch color,
  `refs/remotes/X` as `X` in remoteBranch, `refs/stash` in stash, and
  `grafted`/`replaced` in the grafted color.
- Defaults are git's: commit 33, HEAD 1;36, branch 1;32, remote 1;31, tag 1;33,
  stash 1;35, grafted 1;34.

**Patch lines** are the cached pad prefix plus the line, verbatim. git has
already colored them.

**`--stat` is rendered by ghist** from `--raw` and `--numstat`, because the stat
width depends on each commit's graph width.

- Width: `W = term_cols − text_column`.
- The layout and scaling follow git's algorithm (`diff.c` show_stats). A Python
  reimplementation matched git byte for byte on 7,417 records at widths 20–200,
  including binaries, renames, `diff.statGraphWidth` and `diff.statNameWidth`.
- Raw line _i_ pairs with numstat line _i_. A side with mode `000000` has size
  0; never treat the abbreviated null oid as a lookup key, since `0000` is
  ambiguous. Otherwise the size comes from K. `id1 == id2` gives a plain `Bin`.
- Bars use `color.diff.new` and `color.diff.old`.

**Terminal width:** `COLUMNS > 0`, then `tcgetwinsize(stdout)` (measured in
`main` before the pager starts), then 80.

### Color decision (`color.rs`)

- **Setting:** the last of `color.diff`/`diff.color` wins; otherwise `color.ui`;
  otherwise auto.
- **Auto** means color is on when all of these hold:
  - `NO_COLOR` is unset or empty;
  - `TERM` is set and is not `dumb`;
  - and either we are paging and `color.pager` (or `pager.color`) is true, or
    stdout is a TTY, or `GIT_PAGER_IN_USE` is set with `color.pager` true.
- One decision drives ghist's own escapes and the `--color=` flag passed to git.
- **Color grammar** follows git exactly:
  - `[reset] [fg [bg]] [attrs…]`
  - colors: `normal`/`-1`, `default`, names, `bright*`, `0–255`,
    `#rgb`/`#rrggbb`
  - attributes, case-sensitive: `bold dim italic ul blink reverse strike`, with
    `no`/`no-` forms
  - emission order: an empty parameter for reset, then attributes ascending,
    then fg, then bg
- **`log.graphColors`:** a comma-separated list. If set, valid and empty, the
  graph is uncolored. If unset, git's 12 defaults apply: 31–36, then 1;31–1;36.

### Graph engine (`graph/`, pure, generic `Id`)

**State:**

- `lanes: Vec<Option<Lane{target, color}>>`, where `None` is a hole;
- `counter`, which starts at `ncolors − 1`.

**Per commit:**

1. The node goes in the lane that targets it. A new tip takes the leftmost hole,
   else appends.
2. For each shown parent (de-duplicated): if the commit is a merge or a new tip,
   `counter += 1` mod N, even when the parent already has a lane (this matches
   git, verified). The parent keeps its existing lane's color if it has one,
   otherwise takes `counter`.
3. Placement:
   - The first parent continues in the node's column.
   - If the first parent is already in a lane to the right, that lane is
     **pulled** in (`├─╯`).
   - Other parents tap existing lanes, or **drop** into the nearest free column
     at or to the right of the node (`├─╮`, `├─┬─╮`, `├─│─╮`).
4. Rows are emitted in this order: node row, fan-out row, pull row, jog row
   (compaction; never into the node's column), then the repeatable pad row.
   Fan-out before pull resolves the `├─┴─╮` ambiguity.
5. Trailing holes are trimmed.

**Glyphs.** Arms are an UP/DOWN/LEFT/RIGHT bitset, mapped through an exhaustive
`match`:

| Arms | Glyph | Arms | Glyph | Arms     | Glyph | Arms | Glyph |
| ---- | ----- | ---- | ----- | -------- | ----- | ---- | ----- |
| UD   | `│`   | LR   | `─`   | DR       | `╭`   | DL   | `╮`   |
| UR   | `╰`   | UL   | `╯`   | UDR      | `├`   | UDL  | `┤`   |
| DLR  | `┬`   | ULR  | `┴`   | all four | `┼`   |      |       |

- A crossing is `│` with `─` connectors on both sides. `┼` always means a real
  junction.

**Coloring:**

- Half-cells are colored lazygit-style: lane `│` in the lane's color, runs in
  the far-end attachment's color, pulls and jogs in the moving lane's color.
- Same-color runs are merged into one SGR span, with a reset before each color
  change and once before the text.

**Width and text column:**

- `w` is the commit's widest row. The text column is `2w + 1`, i.e. two spaces
  after the last glyph.
- Rows with no text are cut after the last glyph.
- A commit has at most 4 fixed rows, and fuller always has at least 3 or 4 text
  lines, so prefix-only lines can't happen. `leftover()` exists anyway and is
  tested.

**Contract with the renderer:**

```rust
impl Graph<Id> { fn new(ncolors: NonZeroU16) -> Self; fn next(&mut self, id: &Id, shown: &[Id]) -> &Shape; }
impl Shape { fn text_column(&self) -> usize; fn fixed_rows(&self) -> usize; }
impl Prefixes { fn paint(&mut self, &Shape, Option<&[Sgr]>); fn next_line(&mut self, text_empty: bool) -> &[u8];
                fn leftover(&mut self) -> impl Iterator<Item = &[u8]>; fn separator(&self) -> &[u8]; }
```

**Verified with a prototype of this exact algorithm:**

- Colors: 0 mismatches against git's colored `--graph` over 24,874 glyphs
  (ohmyzsh, brew, bun, freshl).
- Edges: parsing the rendered rows back gives `%P` exactly, with no ambiguous
  junctions, on 94k commits across six repos.

### Output and process management

- **`out.rs`:**
  - One output buffer, flushed when it reaches 64 KiB or just before a read from
    git that would block (`reader.buffer().is_empty()`).
  - It writes to stdout or to a pager started lazily on the first flush, so
    early errors never open the pager.
  - All write errors go through one function; `BrokenPipe` means stop, kill the
    children, exit 0.
- **Pager:** spawned as `sh -c <cmd>`. `COLUMNS` is exported if it was measured
  and is unset; `GIT_PAGER_IN_USE` is removed. At exit, close its stdin and wait
  for it.
- **Exit codes:**

  | Code         | Meaning                                  |
  | ------------ | ---------------------------------------- |
  | 0            | success, or EPIPE                        |
  | 1            | ghist I/O, spawn or protocol error       |
  | 2            | usage                                    |
  | 128          | invalid config                           |
  | git's code   | git failed (its stderr printed verbatim) |
  | pager's code | the pager exited nonzero                 |
  | re-raised    | a signal                                 |

### Module layout

```
src/main.rs          coverage(off): Context from args_os/vars_os/is_terminal/tcgetwinsize; signal-hook flags; Exit → re-raise
src/lib.rs           pub Context, pub enum Exit { Code(u8), Signal(i32) }, pub fn run(&Context, &mut dyn Write, &mut dyn Write) -> Exit
src/args.rs          parse → Action { Help, Version, Log(LogArgs) }; HELP const; bundling (-ph)
src/error.rs         hand-written Error enum + exit mapping (no thiserror/anyhow, like freshl)
src/env.rs           env lookups, git_bool, columns()
src/oid.rs           Oid { len, bytes: [u8; 32] }, hex decode
src/out.rs           buffered output, lazy pager target, write-error funnel
src/pager.rs         resolve(), child_env(), spawn
src/color.rs         Sgr, parse_color, ColorBool, want_color, Palette
src/git/{mod,config,revs,log,catfile}.rs
src/graph/{mod,cell,layout,paint}.rs
src/render/{mod,header,message,stat,width}.rs
```

The public API is minimal: `Context`, `Exit` and `run`. Internal loops take
`&mut dyn Write`/`BufRead`, so each compiles once and no instantiation is left
with dead arms, the freshl coverage trick. The process environment enters only
through `Context`, because edition 2024 makes `set_var` unsafe and unsafe code
is forbidden.

**Dependencies.** Check versions fresh when implementing:

- `rustix` (`termios`), `signal-hook` 0.4.x (`default-features = false`),
  `unicode-width` 0.2;
- dev: `proptest` and `tempfile`, pinned with `=`.

There is no anstream (its auto mode would strip color going into the pager
pipe), no anstyle, no jiff and no gix.
