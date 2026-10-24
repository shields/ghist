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

# Graph layout

This document explains why ghist’s lane graph draws what it draws: the model,
the results from graph drawing and online algorithms that apply to it, what
other tools do, and the measurements behind the rules in
[the design](design.md#graph-layout). The short version: computing a better
layout is cheap, but knowing enough to choose it is not. ghist decides each
commit’s rows from that commit and the ones before it, and three rules recover
much of what lookahead would buy.

## The model

Rows are fixed. Git’s `--topo-order` decides the order of commits, and every
commit’s rows are written before the next commit is read.

A lane is a column awaiting one commit. Its lifetime is an interval of rows,
from the commit that opens it to the row where it ends: at the awaited commit,
or where it joins another lane awaiting the same commit. Lanes alive in the same
row need different columns. The first-parent chain of the first commit stays in
column zero.

A connector is a horizontal span in one row: a fork from a commit to a new or
existing parent lane, a join, or a lane moving between columns. A connector
crosses every occupied column strictly between its ends, drawn as `│` between
`─` segments. Corners (`╭╮╯╰`) are bends.

Every parent connection leaves the commit’s dot on the commit row. When the
parent already has a lane, the connection reaches that lane’s current column.
Any move to a different column follows as a separate row of bends. A row without
a commit has no branching junctions.

The costs are crossings, bends, width, and rows. Width is paid on every text
line, because the text column is `2 × width + 1`, so the mean matters more than
the maximum. The lower bound on width at a row is the number of lanes alive
there, a property of Git’s order and not of the layout.

### Two lanes

Take two side lanes that fork from column zero and join it again, drawn in fixed
columns. Their crossings depend only on how their lifetimes relate:

| Lifetimes                           | Crossings                                    |
| ----------------------------------- | -------------------------------------------- |
| Disjoint                            | 0                                            |
| Interleaved: `s₁ < s₂ < e₁ < e₂`    | 1, whichever lane is outside                 |
| Nested: the second inside the first | 0 if the longer lane is outside, otherwise 2 |

So the minimum is the number of interleaved pairs, and a layout reaches it
exactly when column order respects nesting: the lane that ends later sits
further out. Brute force over 3,000 random instances of up to six lanes agreed
with this count every time.

Nesting can cost width. With `A = [0, 10] ⊃ B = [1, 2]` and
`C = [9, 20] ⊃ D = [11, 12]`, at most two lanes are alive at once, but
respecting both nests needs three columns: the best is three crossings at width
two or one crossing at width three.

First-fit, the rule “take the first free column,” gives the minimum width
because lanes arrive in order of their start. It is blind to nesting: the lane
that arrived first is innermost, even when it outlives everything that opens
inside it. On the large repositories measured below, 74–86% of first-fit’s
crossings were nested pairs drawn the wrong way round.

### What zero lookahead cannot do

For any rule that sees only a bounded number of commits ahead, two histories can
agree inside the window and differ after it:

- A merge opens lane `a`, and `a`’s commit is followed by linear commits.
- In one history, `a`’s parent comes next, so `a` belongs in column one.
- In the other, another merge opens a branch that ends inside `a`’s lifetime, so
  `a` belongs in column two with the newcomer inside it.

The rule sees the same rows in both. If it puts `a` in column one, the second
history pays a crossing, or two corners and a row to move `a` out. If it puts
`a` further out, the first history pays two characters of indentation on every
line until `a` ends. Repeating the gadget makes the loss grow without bound.
Each strategy pays in one currency:

| Strategy                                       | Pays         | Used by           |
| ---------------------------------------------- | ------------ | ----------------- |
| First free column                              | Crossings    | ghist, jj         |
| Insert new lanes at the commit, shift the rest | Bends, rows  | `git log --graph` |
| Reserve room inside every lane                 | Width        | None surveyed     |
| Look ahead or read the whole history           | Latency, CPU | IntelliJ, GitLab  |

## ghist’s rules

Beyond first-fit, three rules decide which column survives when lanes meet and
when holes close. None looks ahead, and none changes lane colors, which depend
only on the stream order and on the awaited lane’s color surviving a join.

**Merge-tap.** A merge outside column zero whose first parent is already awaited
on its right taps that lane instead of pulling it in, if another parent opens a
new lane. The new lane then starts in the node column, and the fan-out is a
single connection (`●─┤`) instead of a crossing, a pull, and usually a later
jog.

**Sibling joins in a merge train keep the outer column.** Each lane records a
family, shared along the lanes that continue one line of history, and the family
of the lane that opened it. When a single-parent commit joins two lanes opened
by the same family, keeping the outer column instead of the inner one is a bet:
the freed inner column sits next to their opener, so the opener’s next branch
can start there, inside the long-lived lane, without crossing it. The commit
pulls a lane on its left into its own column, or taps a lane on its right. A
pull connects at the dot (`├─●`) and then moves the lane (`╰─╮`); a tap connects
to the surviving lane (`●─┤`). A bet that no branch takes leaves a hole that
compaction later fills with a jog, so the outer column is kept only when the
past says the bet will pay:

- the node lane’s family started after the parent lane’s, so a long-lived branch
  that rejoins younger lanes, as in git-flow, does not walk right one column per
  merge;
- the opener’s lane lies left of both lanes;
- the inner column would be the only hole left of the outer lane, so the next
  branch lands in it rather than in an older hole; and
- the opener opened lanes at two commits within the last five, a merge train in
  progress.

Merges keep the plain rule, because a merge’s other connectors can cross the
moved lane. The window counts every shown commit, including the branches’ own,
so a train of branches three or more commits long falls outside it and keeps
first-fit’s columns. Without the guards, the rule made 119,096 such joins on the
24 repositories first measured, these 23 and go-git; their freed columns were
almost always reused, but the outer lanes starved other holes, which aged and
were compacted, so jogs rose 62%. The hole guard matters most; the burst guard
removes the crossings the unguarded rule added on zigbee-herdsman-converters and
zigbee2mqtt; the age guard fixes git-flow.

**Lazy compaction.** A hole is filled only after it has stayed empty for 20
commits. Most holes are reused by a new lane within a few commits, so moving a
lane into them immediately draws a jog that the next fork would have made
unnecessary. This is a rent-or-buy decision: an open hole costs up to two
characters of indentation on each commit’s lines, while a jog costs two corners,
a row, and a crossing with every lane it passes. The wait is a point on a smooth
trade-off between width and jogs, not a tuned optimum.

The example that prompted these rules is four one-commit branches forked from
one commit `X` and merged one after another. First-fit placed the long-lived
lane for `X` innermost and crossed it at every merge after the first:

```text
first-fit         ghist
●─╮               ●─╮
│ ●               │ ●
●─│─╮             ●─│─╮
│ ├─●             │ ├─●
│ │               │ ╰─╮
●─│─╮             ●─╮ │
│ ├─●             │ ●─┤
●─┼─╮             ●─┬─┤
╭─╯ │             ╭─│─╯
│ ╭─╯             ├─●
├─●               ●
●
```

The first crossing remains because nothing at the first merge says that `X`’s
lane will outlive the next branches. The last remains because a fan-out row
precedes a pull row: the last merge must open its branch before it can pull
`X`’s lane into column zero. Lookahead of a few commits would remove the first;
no layout removes the last.

## Measurements

These measurements predate separating parent connections from lane moves. The
lane placement policies remain the same, but a pull now draws its parent
connection on the commit row and two bends on the following row.

Measurements on October 9, 2026 used an Apple M1 Max, macOS 27.0.1, Rust 1.99.0,
and Git 2.56.0, on 23 local repositories chosen for their merges. Each figure
counts the graph prefixes of plain `ghist` output over the whole history:

- **Crossings:** `│` cells with a horizontal arm on both sides.
- **Corners:** `╭`, `╮`, `╯`, and `╰` cells.
- **Jogs:** compaction moves, which are the only rows that draw `╭`.
- **Graph rows:** rows with a horizontal connector.
- **Mean width:** each commit’s graph width in columns, holes included, averaged
  over commits.

First-fit with immediate compaction, then ghist’s rules:

| Repository                         | HEAD       | Commits |             Crossings |           Corners |           Jogs |        Graph rows |    Mean width |
| ---------------------------------- | ---------- | ------: | --------------------: | ----------------: | -------------: | ----------------: | ------------: |
| shields/homebrew-cask              | `c22c1bcc` | 475,047 | 2,089,483 → 1,440,195 | 331,152 → 293,543 | 19,897 → 1,094 | 311,567 → 292,763 |   9.47 → 9.59 |
| Homebrew/brew                      | `1b0b73c8` |  53,057 |      108,545 → 76,530 |   42,508 → 31,427 |    5,732 → 191 |   37,020 → 31,479 |   5.43 → 5.79 |
| esphome/esphome                    | `d59c006f` |  13,835 |         1,499 → 1,451 |     1,094 → 1,072 |        32 → 24 |     1,362 → 1,351 |   3.58 → 3.62 |
| shields/zigbee-herdsman-converters | `0a47792a` |  13,814 |             170 → 107 |         172 → 138 |         17 → 0 |         178 → 161 |   1.07 → 1.07 |
| open-webui/open-webui              | `b03fc97e` |   9,279 |        14,468 → 8,924 |     4,542 → 3,300 |      549 → 116 |     4,409 → 3,787 |   6.77 → 7.29 |
| jindrapetrik/jpexs-decompiler      | `3d91020a` |   7,864 |             110 → 102 |         594 → 566 |         13 → 0 |         614 → 600 |   1.39 → 1.40 |
| ohmyzsh/ohmyzsh                    | `3e7ef018` |   7,548 |       45,238 → 26,899 |     3,716 → 3,233 |      358 → 120 |     3,490 → 3,248 | 14.58 → 15.14 |
| Koenkk/zigbee2mqtt                 | `b0b02c30` |   6,626 |               81 → 55 |         254 → 228 |         13 → 1 |         329 → 316 |   1.61 → 1.62 |
| shields/WLED                       | `dc7eb8f4` |   5,344 |        10,962 → 7,733 |     2,286 → 1,691 |       352 → 85 |     2,163 → 1,865 |   9.06 → 9.71 |
| shields/awesome-mcp-servers        | `b3089197` |   4,822 |       54,126 → 37,678 |     4,382 → 3,780 |      466 → 166 |     4,228 → 3,927 | 20.72 → 21.76 |
| exelban/stats                      | `68416572` |   2,131 |               23 → 20 |          100 → 90 |          5 → 0 |          101 → 96 |   1.15 → 1.17 |
| Luligu/matterbridge                | `00998e18` |   2,112 |         1,072 → 1,017 |         170 → 160 |        22 → 17 |         216 → 211 |   6.04 → 6.22 |
| shields/k9s                        | `ff76762d` |   2,029 |             414 → 348 |         504 → 390 |         56 → 4 |         476 → 419 |   2.24 → 2.39 |
| michaelrsweet/pappl                | `5495d919` |   1,727 |               33 → 22 |         138 → 128 |          5 → 0 |         138 → 133 |   1.40 → 1.43 |
| oraios/serena                      | `111a764b` |   1,311 |             315 → 273 |         382 → 318 |         34 → 3 |         393 → 361 |   3.11 → 3.29 |
| taiki-e/cargo-llvm-cov             | `f20c83f4` |   1,195 |               37 → 37 |         287 → 277 |          5 → 0 |         283 → 278 |   1.33 → 1.33 |
| awsdocs/aws-lambda-developer-guide | `7bb9f8ab` |     908 |         2,675 → 1,939 |         640 → 504 |        80 → 17 |         605 → 537 |   6.73 → 7.57 |
| shields/lgtmcp                     | `27e853af` |     366 |               45 → 39 |         172 → 166 |          3 → 0 |         190 → 187 |   1.76 → 1.78 |
| gitcalver/gitcalver.org            | `5448ffde` |     127 |                 0 → 0 |             6 → 6 |          0 → 0 |             6 → 6 |   1.06 → 1.06 |
| gitcalver/rust                     | `73ab4cf8` |      67 |                 2 → 2 |           18 → 16 |          1 → 0 |           17 → 16 |   1.30 → 1.30 |
| gitcalver/sh                       | `7120f81e` |      46 |                 2 → 2 |            10 → 8 |          1 → 0 |             9 → 8 |   1.35 → 1.35 |
| gitcalver/go                       | `78d58676` |      39 |                 1 → 1 |             6 → 6 |          0 → 0 |             7 → 7 |   1.31 → 1.31 |
| gitcalver/python                   | `65e0e589` |      36 |                 3 → 2 |            10 → 8 |          1 → 0 |             9 → 8 |   1.36 → 1.36 |

No repository is worse on crossings, corners, jogs, or graph rows. Each rule’s
effect, applied in order, summed over the 23 repositories:

| Rule                | Crossings | Corners |   Jogs | Graph rows | Mean width |
| ------------------- | --------: | ------: | -----: | ---------: | ---------: |
| First-fit           | 2,329,304 | 393,143 | 27,642 |    367,810 |      8.582 |
| Merge-tap           | 2,326,285 | 392,254 | 27,437 |    367,364 |      8.582 |
| Lazy compaction, 16 | 2,069,932 | 341,707 |  2,164 |    342,092 |      8.735 |
| Lazy compaction, 20 | 2,062,762 | 341,293 |  1,957 |    341,885 |      8.754 |
| Sibling joins       | 1,603,376 | 341,055 |  1,838 |    341,764 |      8.740 |

Overall, crossings fall 31%, corners 13%, jogs 93%, and graph rows 7%, while
mean width, weighted by commits, rises 1.8%. The widest commit gains a column
only on open-webui and loses one on zigbee2mqtt.

The thresholds were chosen on these repositories, so they were also checked on
480 histories generated in four styles: merge trains, git-flow, long feature
branches, and a mix. There, crossings fall 30%, corners 25%, jogs 89%, and graph
rows 13%, for 7.9% more width, and no history is worse than first-fit on any of
the four counts. Against the rules before it, the guarded sibling rule cuts
crossings 5.4% there, is better on 274 histories, and is worse on some count on
78, by small amounts; unguarded, it was worse on 267.

Most of the remaining crossings need the future to avoid. A Python model of
first-fit, which reproduced ghist’s rows exactly, attributed 74–86% of
first-fit’s crossings on the large repositories to nested pairs drawn inside
out, and a pairwise counting argument put the crossings that no layout keeping
column zero and drawing fan-outs before pulls can avoid at 0.3–9.3% of
first-fit’s.

The `GHIST_DIFF_REPOS` oracle in `tests/it/differential.rs` checked every header
and every drawn edge against Git on all of these repositories except
homebrew-cask.

## What lookahead would cost

Lookahead of `K` commits means drawing commit `i` only after Git has produced
commits up to `i + K`. These figures were measured with Git 2.56.0 on the same
machine while it was heavily loaded, so CPU times are more reliable than wall
times, and brew figures use the 43,936-commit checkout in
[the benchmarks](benchmarks.md):

- **Inside the one stream:** without `-p` or `--stat`, a window of `K` parsed
  records costs no extra process and about nothing in time. With `-p`, commit
  `i`’s patch sits between its fields and record `i + 1`, and its prefix depends
  on the layout, so even `K = 1` means holding a whole patch, which ghist does
  not do. One open-webui commit’s patch is 10 MB.
- **A second topology stream:**
  `git rev-list --default HEAD --topo-order --parents --end-of-options …` with
  the same operands produced the same hash and parent sequence as the main
  stream in all 50 repository-and-mode combinations tried, including ranges,
  paths, `-p`, and stats. It costs about two thirds of the main walk, or all of
  it with a path limit. Run concurrently, the first page’s wall time stayed
  within noise, but CPU before the first page roughly doubled (+275 ms on brew).
- **Latency:** without a commit-graph file, Git computes the whole topological
  order before emitting the first commit, so `K` more commits are almost free.
  With one, Git streams, and lookahead waits for Git to find `K` more commits.
  With a path limit that can take seconds: on homebrew-cask, the first matching
  commit for one file arrives after 66 ms, the 65th after 1.7 s.
- **Consistency:** two processes resolve revisions independently, so a commit,
  fetch, or rebase between them makes the streams disagree. Comparing each main
  record with the topology entry at the same position detects that in constant
  time per commit, and ghist would have to fail rather than fall back silently.

The Python model also swept `K` with a rule that reserves holes for lanes the
window proves will open and close inside a new lane. Crossings fell with `K` and
width rose; the knee was around `K = 32`, which removed 30–67% of first-fit’s
crossings on the large repositories at 0.4–19% more mean width. Small `K` was
not a safe partial step: at `K = 1` it drew more crossings than first-fit on
brew and ohmyzsh. A whole-history layout that sorts lanes by where they end and
polishes the result by local search removed 61–80% of crossings and every jog,
at 8–22% more mean width and 16–160 times the layout CPU, and needs the whole
topology before the first row.

ghist uses no lookahead: the gain is real, but it needs a second Git process on
every run and makes first output wait on Git in exactly the cases that are
already slow.

## Theory

The problem sits where several studied problems meet. Each maps onto part of it,
and each mapping says something about what is easy.

**Interval coloring and channel routing.** Lanes alive together need different
columns, so a layout colors an interval graph. Coloring intervals in order of
their left ends is optimal offline. In VLSI channel routing this is the
left-edge algorithm (Hashimoto and Stevens 1971), which is optimal when there
are no vertical constraints, as in a channel with terminals on one side only
(Deutsch 1976). Nesting preferences are vertical constraints. Deutsch’s doglegs
split a net across tracks to satisfy them and cost one or two contacts each;
ghist’s jogs are doglegs. Online, with intervals arriving in any order, at most
`3ω − 2` colors are needed and no algorithm does better, where `ω` is the
largest number of overlapping intervals (Kierstead and Trotter 1981, as stated
by Curbelo 2024). ghist’s lanes arrive in order of their start, so width is not
the hard part; choosing among free columns is.

**Layered drawing.** The Sugiyama framework orders vertices within layers to
reduce crossings. Crossing minimization in two layers is NP-complete (Garey and
Johnson 1983), and stays NP-hard with one layer fixed (Eades and Wormald 1994).
The barycenter and median heuristics, sifting, and exact branch-and-cut methods
all permute each layer freely and straighten edges afterwards; Brandes–Köpf
coordinate assignment bends a long edge at most twice, at its ends (Healy and
Nikolov). In a terminal, a permutation between rows is a visible jog, so each
lane keeps one column and the problem becomes track assignment instead. Online
one-sided crossing minimization in the slotted model is not competitive for any
`k ≥ 2` (Burjons, Fuchs, and Lotze 2022).

**Fixed-order arc diagrams.** With column zero as a spine and every side lane an
arc from its fork row to its join row, a column is an arc’s height. On one page
the crossings are fixed by the order of endpoints, and nested arcs must have the
inner arc lower: the two-lane table above. Choosing between two pages is the
NP-hard fixed linear crossing number problem (Masuda et al. 1990; Buchheim and
Zheng 2006), and ghist’s column-zero rule removes that choice.

**Metro lines.** Ordering metro lines along shared edges is NP-hard on
caterpillars, but polynomial when the side on which each line ends is given
(Fink and Pupyrev 2013). In ghist, every lane leaves toward the lane it joins,
so its side is given: the tractable case. Its metro-map form of the nesting rule
is that a lane about to join another should already sit next to it.

**Storylines.** Characters are curves over time that must be adjacent at
meetings; the objectives are crossings, wiggles, and white space, which are
ghist’s crossings, bends, and width. Crossing minimization is NP-complete in
general and fixed-parameter tractable in the number of characters (Kostitsyna et
al. 2015). Minimizing wiggles is NP-complete even with every order fixed (Dobler
et al. 2025). With a protagonist present at every meeting and drawn on one side,
every instance can be drawn with only unavoidable crossings, in `O(k²n)` time
(Hegemann and Wolff 2024). The protagonist is ghist’s first-parent chain in
column zero; this is the closest published model.

**Online algorithms and lookahead.** Recoloring intervals is the formal version
of moving lanes: maintaining an optimal `k`-coloring needs `Ω(k)` recolorings
per step, amortized, for small `k` (Bosek et al. 2020). Lookahead rarely changes
worst-case bounds: for online coloring of classes such as trees and bipartite
graphs, a lookahead of `O(n / log n)` does not improve the `Θ(log n)`
competitive ratio (Albers and Schraink 2017). That matches the argument above: a
bounded window helps typical histories, not the worst case. Lazy compaction is a
rent-or-buy choice in the sense of ski rental.

**Version-control graphs.** No peer-reviewed algorithm for commit-graph lanes
was found. Isaacs and Gamblin describe Git’s layout as one that “conserves
columns” and is “an unambiguous representation: when one edge is routed into
another, they both exit at the same terminus,” at the cost of many rows (Isaacs
and Gamblin 2019). Courtiel and Pépin formalize “git graphs” as a main branch
with non-interfering feature branches (Courtiel and Pépin 2024), the class for
which the two-lane table is exact.

## Prior art

Tools fall into six families. Behavior was learned from documentation, papers,
black-box output, and the source of permissively licensed tools only. GPL
implementations, including Git’s `graph.c`, tig, gitk, and Sapling, were not
read.

| Family                                   | Lane               | Needs         | Lanes move | Members                                                  |
| ---------------------------------------- | ------------------ | ------------- | ---------- | -------------------------------------------------------- |
| A. Insert at the commit, collapse left   | Shared per parent  | Nothing ahead | Constantly | `git log --graph`                                        |
| B. Sparse fixed columns, first free slot | Shared per parent  | Nothing ahead | Rarely     | ghist, Sapling’s renderdag (jj)                          |
| C. One lane per edge, appended right     | One per child edge | Nothing ahead | Yes        | VS Code’s built-in graph, lazygit, SourceGit, GitX       |
| D. Straight branches, children first     | One per branch     | Children map  | Never      | pvigier’s straight branches, DoltHub, serie              |
| E. Interval scheduling of branch chains  | One per branch     | Whole window  | Never      | git-graph (shortest first), GitLab (650 commits), Fossil |
| F. Global order, rank per row            | One per edge       | Whole graph   | Yes        | IntelliJ                                                 |

- **Git** inserts a merge’s parents next to the commit and pushes every lane on
  the right outward, then slides lanes back as branches end. The newest branch
  is always innermost, so Git nests short branches without foresight and draws
  one crossing on the example above, paid for with a lateral move of the long
  lane at every merge and a graph-only row per commit.
- **jj** draws through Sapling’s renderdag: lanes stay put, a vacated column
  stays empty until reused, and horizontals run over intervening lanes. It has
  no column-zero rule.
- **Per-edge tools** give each child edge its own lane until the parent’s row.
  On the example they need width five and draw six crossings.
- **Whole-history tools** know each branch’s lifetime. git-graph assigns columns
  shortest-lived first, which draws the example with no crossings at width five.
  IntelliJ ranks lanes by a global layout index and cuts edges of 30 rows or
  more to arrows; an issue open since 2015 asks for straight lanes, in the words
  “keeping longer branches to the left, and the shorter branches to the right.”
- **Bounded windows are common:** GitLab lays out at most 650 commits, the VS
  Code Git Graph extension loads 300 and then 100 at a time, Fossil lays out a
  page.
- **Pinning the trunk** is common: serie’s primary branch, Fossil’s trunk on the
  far left, git-graph’s ordered branch groups.

No surveyed tool keeps shared, fixed lanes and also nests by lifetime. The
families that nest either move existing lanes outward (A, F) or know lifetimes
before placing lanes (D, E).

## Sources

Theory:

- Albers and Schraink, “Tight Bounds for Online Coloring of Basic Graph
  Classes,” ESA 2017. <https://arxiv.org/abs/1702.07172>
- Bosek, Disser, Feldmann, Pawlewicz, and Zych-Pawlewicz, “Recoloring Interval
  Graphs with Limited Recourse Budget,” SWAT 2020.
  <https://drops.dagstuhl.de/entities/document/10.4230/LIPIcs.SWAT.2020.17>
- Buchheim and Zheng, “Fixed Linear Crossing Minimization by Reduction to the
  Maximum Cut Problem,” COCOON 2006. <https://doi.org/10.1007/11809678_53>
- Burjons, Fuchs, and Lotze, “The Slotted Online One-Sided Crossing Minimization
  Problem on 2-Regular Graphs,” 2022. <https://arxiv.org/abs/2201.04061>
- Courtiel and Pépin, “Random Generation of Git Graphs,” GASCom 2024.
  <https://arxiv.org/abs/2403.01902>
- Curbelo, “Online coloring of short interval graphs and two-count interval
  graphs,” 2024, which states Kierstead and Trotter’s 1981 bound.
  <https://arxiv.org/abs/2412.17193>
- Deutsch, “A ‘Dogleg’ Channel Router,” DAC 1976, which cites Hashimoto and
  Stevens for the left-edge algorithm.
  <https://my.ece.utah.edu/~kalla/phy_des/doglegcr-1.pdf>
- Dobler, Hegemann, Nöllenburg, and Wolff, “Optimizing Wiggle in Storylines,”
  GD 2025. <https://arxiv.org/abs/2508.19802>
- Eades and Wormald, “Edge crossings in drawings of bipartite graphs,”
  Algorithmica 11(4), 1994.
  <https://research.monash.edu/en/publications/edge-crossings-in-drawings-of-bipartite-graphs/>
- Fink and Pupyrev, “Metro-Line Crossing Minimization: Hardness, Approximations,
  and Tractable Cases,” GD 2013. <https://arxiv.org/abs/1306.2079>
- Garey and Johnson, “Crossing Number is NP-Complete,” SIAM J. Algebraic
  Discrete Methods 4(3), 1983.
- Healy and Nikolov, “Hierarchical Drawing Algorithms,” chapter 13 of the
  Handbook of Graph Drawing and Visualization.
  <https://cs.brown.edu/people/rtamassi/gdhandbook/chapters/hierarchical.pdf>
- Hegemann and Wolff, “Storylines with a Protagonist,” GD 2024.
  <https://drops.dagstuhl.de/storage/00lipics/lipics-vol320-gd2024/LIPIcs.GD.2024.26/LIPIcs.GD.2024.26.pdf>
- Isaacs and Gamblin, “Preserving Command Line Workflow for a Package Management
  System using ASCII DAG Visualization,” IEEE TVCG 25(9), 2019.
  <https://arxiv.org/abs/1908.07544>
- Kostitsyna, Nöllenburg, Polishchuk, Schulz, and Strash, “On Minimizing
  Crossings in Storyline Visualizations,” GD 2015.
  <https://arxiv.org/abs/1509.00442>
- Masuda, Nakajima, Kashiwabara, and Fujisawa, “Crossing Minimization in Linear
  Embeddings of Graphs,” IEEE Trans. Computers 39(1), 1990.
  <https://doi.org/10.1109/12.46286>

Tools:

- Git, “history graph API.” <https://git-scm.com/docs/api-history-graph/2.19.2>
- pvigier, “Commit Graph Drawing Algorithms,” 2019.
  <https://pvigier.github.io/2019/05/06/commit-graph-drawing-algorithms.html>
- DoltHub, “Drawing a commit graph,” 2024.
  <https://www.dolthub.com/blog/2024-08-07-drawing-a-commit-graph/>
- jj’s graph log and renderdag.
  <https://github.com/jj-vcs/jj/blob/main/cli/src/graphlog.rs>,
  <https://docs.rs/sapling-renderdag/latest/renderdag/>
- git-graph’s branch assignment.
  <https://github.com/git-bahn/git-graph/blob/master/docs/branch_assignment.md>
- IntelliJ’s VCS log layout, and issue IJPL-89295.
  <https://github.com/JetBrains/intellij-community/tree/master/platform/vcs-log/graph>,
  <https://youtrack.jetbrains.com/issue/IJPL-89295>
- VS Code’s source-control history graph.
  <https://github.com/microsoft/vscode/blob/main/src/vs/workbench/contrib/scm/browser/scmHistory.ts>
- VS Code Git Graph extension settings.
  <https://github.com/mhutchie/vscode-git-graph/wiki/Extension-Settings>
- lazygit’s graph.
  <https://github.com/jesseduffield/lazygit/blob/master/pkg/gui/presentation/graph/graph.go>
- SourceGit’s commit graph.
  <https://github.com/sourcegit-scm/sourcegit/blob/master/src/Models/CommitGraph.cs>
- GitX’s graphing notes.
  <https://github.com/pieter/gitx/blob/master/Documentation/GraphingAPI.txt>
- serie’s graph calculation.
  <https://github.com/lusingander/serie/blob/master/src/graph/calc.rs>
- GitLab’s network graph.
  <https://gitlab.com/gitlab-org/gitlab-foss/-/blob/master/app/models/network/graph.rb>
- Fossil’s timeline graph. <https://fossil-scm.org/home/file/src/graph.c>
