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

# Benchmarks

Measurements on October 6, 2026 used an Apple M1 Max, 64 GiB RAM, macOS 27.0.1,
Rust 1.99.0, and Git 2.56.0. These measurements cover the implementation through
combined patches and stats.

| Repository             | HEAD                                       | Reachable commits |
| ---------------------- | ------------------------------------------ | ----------------: |
| Homebrew/brew          | `64efed206deeb9c2304d9e5b5910dcbf0a509c15` |            43,936 |
| Homebrew/homebrew-cask | `7966f681a7398fda77f65c81875ea8f704751bcd` |           483,740 |

Cask was a bare, blob-filtered clone. Objects for the measured patch range were
fetched before timing. Brew used an existing checkout. Neither repository was
rewritten or repacked for the measurements.

## Reproduction

`make bench BENCH_REPO=/path/to/repository` runs four comparisons with
hyperfine: the first 40 output lines, full history, patches, and stats.
`BENCH_REV` selects the patch/stat range and defaults to `HEAD~100..HEAD`. The
Git reference uses `--graph --pretty=fuller --topo-order`; diff comparisons also
use `--no-diff-merges`. Output is redirected to `/dev/null`.

To reproduce the color and width settings:

```sh
GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=color.diff GIT_CONFIG_VALUE_0=never \
    COLUMNS=80 make bench BENCH_REPO=/path/to/repository
```

The release-profile comparison below used three warmups and 15 measured runs per
command. `make bench` uses three warmups and 10 measured runs. The second binary
used `CARGO_PROFILE_RELEASE_LTO=thin`; all other build settings and source files
were identical.

## Release-profile comparison

Times are mean ± sample standard deviation, in milliseconds. Runs were
sequential on a workstation; the large full-history variance makes isolated
percentage differences unreliable.

| Repository | Workload       |    Default release |            ThinLTO |               Git |
| ---------- | -------------- | -----------------: | -----------------: | ----------------: |
| Brew       | First 40 lines |       288.4 ± 10.4 |       299.6 ± 22.4 |       271.5 ± 3.4 |
| Brew       | Full history   |      669.7 ± 127.0 |       602.0 ± 82.9 |      594.4 ± 24.4 |
| Brew       | Patches        |       206.7 ± 22.2 |       220.2 ± 21.4 |      164.9 ± 10.5 |
| Brew       | Stats          |       197.7 ± 20.6 |       197.6 ± 46.6 |      153.5 ± 10.0 |
| Cask       | First 40 lines |        40.0 ± 17.1 |         34.7 ± 7.0 |        11.8 ± 3.0 |
| Cask       | Full history   | 10,352.2 ± 4,407.1 | 12,068.8 ± 2,629.2 | 8,095.9 ± 1,623.4 |
| Cask       | Patches        |        92.8 ± 13.3 |        81.2 ± 15.6 |        25.9 ± 6.7 |
| Cask       | Stats          |        81.8 ± 17.4 |        75.5 ± 13.4 |        39.3 ± 6.9 |

ThinLTO does not show a consistent improvement of at least 3%, so the default
release profile is retained. The targets of matching Git's full-history speed
and adding only a few milliseconds to its first page are not met by these
measurements.

## Backend cost

A separate Cask run used three warmups and 10 measurements to compare the
backend commands with the complete program. The record-stream command is the
main Git invocation in [the design](design.md), with color disabled and no diff
flags.

Git's graph and fuller output took 7.417 ± 0.138 seconds. The record stream
alone took 8.844 ± 0.242 seconds; a separate configuration-read-plus-stream
measurement took 8.436 ± 0.187 seconds. These are independently timed commands,
so their differences must not be treated as additive component costs.

| Build                        | First 40 lines, mean ± deviation | Full history, mean ± deviation | Full-history median |
| ---------------------------- | -------------------------------: | -----------------------------: | ------------------: |
| Default release              |                    32.7 ± 5.4 ms |                9.051 ± 0.836 s |             8.738 s |
| ThinLTO                      |                    32.1 ± 6.3 ms |                8.629 ± 0.103 s |             8.633 s |
| Experimental message copying |                    30.3 ± 2.4 ms |                8.636 ± 0.145 s |             8.615 s |

The experiment preallocated message indentation and copied lines without tabs
directly. It was not retained. The default build's slower outliers inflate its
mean: the median improvements for ThinLTO and the experiment are only 1.2% and
1.4%, respectively. First-page medians were 30.4, 30.2, and 30.7 milliseconds.

Reading configuration by itself took 6.1 ± 0.9 milliseconds. This process is
part of every invocation. The backend comparison also shows that improving Rust
rendering alone cannot remove the entire observed full-history gap while
retaining this record protocol.
