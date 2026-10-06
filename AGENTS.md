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

# Agents

Project conventions live in
[`shields/right-answers`](https://github.com/shields/right-answers). Follow
those by default; `shields/freshl` is the precedent where that guide is silent.

## Git owns the semantics

Use a `git log` subprocess for walking, revision syntax, path simplification,
mailmap, encodings, and diffs. When semantics differ from git, change the git
invocation instead of reinterpreting its output. Do not translate git's GPL
source into this Apache-licensed project.

Stream commit records and patch lines. Do not buffer the complete history or a
complete patch. The negative-revision boundary pre-pass may retain its hidden
parent set.

## Repository races

Repository changes during a run are unspecified. ghist is a read-only display
tool, and its subprocesses can observe different repository states.

## Tests and documentation

Tests spawn git only through `tests/it/common`, with an allowlisted environment.
Never mutate the process environment. Update `docs/edge-cases.md` in the same
commit as a new edge case.

## Commit workflow

Before every commit:

1. Review the diff for correctness, regressions, missing tests, and unnecessary
   comments. After a fix, review the changed portion again.
2. Run `make lint test coverage`; all tests and the 100% line gate must pass.
3. Submit through LGTMCP (`review_and_commit`).

Before closing each implementation milestone, run
`PROPTEST_CASES=1024 make test`. Keep each change and its tests together. Do not
push without a request.
