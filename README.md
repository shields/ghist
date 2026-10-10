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

# ghist

An opinionated `git log` replacement, with a Unicode graph and Git’s own colors.

## Behavior

```text
ghist [-p|--patch] [--stat] [<rev>|<a>..<b>|<a>...<b>|^<rev>…] [--] [<path>…]
```

Git handles revision syntax, topological order, path filtering, mailmap,
encodings, and diffs. SHA-1 and SHA-256 repositories, shallow clones, and
replacement objects work as they do in `git log`.

Use `-p` or `--patch` for patches, `--stat` for a summary, or both. Merges show
neither. Root commits show changes against the empty tree unless `log.showRoot`
is false. Git’s diff algorithm, rename detection, and diff colors apply.

The other flags are `-h`/`--help` and `--version`. There are no display flags.

## Display

```text
●    sha1 3f2a9c1e5b7d9f0a2c4e6b8d0f1a3c5e7b9d1f3a (HEAD -> main)
├─╮  Merge: a1b2c3d f4e5d6c
│ │  Author:     Grace Hopper <grace@example.com>
│ │  Commit:     Ada Lovelace <ada@example.com>
│ │  AuthorDate: 2026-10-04 09:12:44 -0400
│ │  CommitDate: 2026-10-05 20:41:13 -0700
│ │
│ │      Add the frobnicator
```

- Full hashes are labeled `sha1` or `sha256`. The part after the shortest unique
  prefix is dimmed. Merge-parent hashes extend to at least seven digits, with
  the same dimming.
- `Commit:` appears only when the committer’s identity differs from the author’s
  after mailmap. `CommitDate:` appears only when the date or time zone differs.
  Dates retain each identity’s own time zone.
- Decorations are always shown. Graph lanes use `log.graphColors`, or Git’s
  default palette. The graph always uses Unicode.
- Stat widths account for the graph column. Terminal width comes from `COLUMNS`,
  the terminal, or 80 columns, in that order.
- Generated lines have no trailing whitespace. Patch bytes are preserved,
  including trailing whitespace and Git’s colors. Patch `index` lines use
  shortest-unique abbreviations with a minimum of four digits because Git shares
  the abbreviation setting with commit headers.

Color follows Git’s configuration. `NO_COLOR` disables automatic color; explicit
`color.ui=always` or `color.diff=always` still wins. Invalid color values are
errors, including invalid `log.graphColors` entries.

When stdout is a terminal, the pager is `$GIT_PAGER`, then `core.pager`, then
`$PAGER`, then `less`. An empty value or `cat` disables it. `LESS=FRX` and
`LV=-c` are defaults only when those variables are unset. Git diagnostics appear
after the pager exits. A failed pager returns an error; quitting a successful
pager early, or piping into `head`, exits quietly.

## Recommended usage

```sh
if command -v ghist >/dev/null 2>&1; then
    alias gl='ghist'
else
    alias gl='git log --graph --pretty=fuller --topo-order'
fi
```

Requires Git 2.29 or newer and a Unix system. Build and install from this
checkout with `make install`. On large repositories, a commit graph can improve
first-page latency:

```sh
git commit-graph write --reachable
```

## Development

Install [Bun](https://bun.sh/), [Lefthook](https://lefthook.dev/), and
[cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov), then run:

```sh
bun install --frozen-lockfile
rustup toolchain install "$(make print-nightly)" --profile minimal --component llvm-tools-preview
lefthook install
make lint test coverage
```

`rust-toolchain.toml` pins stable Rust. Coverage uses the dated nightly in the
Makefile and requires 100% line coverage. The pre-commit hook runs lint and
coverage. `make fmt` formats Rust, documentation, and configuration files.

The generated differential tests compare output with Git. Run
`PROPTEST_CASES=1024 make test` for the full milestone check, or set
`GHIST_DIFF_REPOS` to a colon-separated list of local repositories for full
history comparisons. The [edge-case matrix](docs/edge-cases.md) records
behavioral coverage; [the design](docs/design.md) describes the protocol.

Run `make bench BENCH_REPO=/path/to/repo` with
[hyperfine](https://github.com/sharkdp/hyperfine) installed to compare first
page, full history, patches, and stats with Git.

`BENCH_REV` selects the patch/stat range and defaults to `HEAD~100..HEAD`. See
[the recorded benchmarks](docs/benchmarks.md) for timings and the release
profile decision.

## Publishing

`make publish-dry-run` packages a temporary copy of the current commit using
[gitcalver](https://github.com/gitcalver/rust). It requires a clean checkout on
the default branch. Install gitcalver with `cargo install gitcalver --locked`.
`make publish` publishes that version.

The first publish is manual: crates.io Trusted Publishing cannot create a crate.
After it exists, configure its trusted publisher for `shields/ghist`, workflow
`publish.yml`, and environment `release`. See the
[crates.io setup instructions](https://crates.io/docs/trusted-publishing).
Subsequent pushes to `main` run the full CI workflow before publishing with a
short-lived OIDC token.
