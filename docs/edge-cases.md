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

# Edge-case matrix

- **T** — tested by the named suite.
- **U** — intentionally unspecified.
- **G** — a gap that remains to be implemented or verified.

Line coverage measures exercised code; this matrix tracks behavior. A fixture
alone does not establish that ghist handles a case correctly.

## Arguments and output errors

| Dimension                                                          | Status | Evidence                                            |
| ------------------------------------------------------------------ | ------ | --------------------------------------------------- |
| Empty arguments, revisions, ranges, exclusions, and implicit paths | T      | `args::tests` checks byte-preserving pass-through   |
| `-p`, `--stat`, repetition, and bundled `-ph`                      | T      | `args::tests`                                       |
| Help and version terminate argument parsing                        | T      | `args::tests`, `tests::informational_output`        |
| Explicit `--`, including no paths and option-like paths            | T      | `args::tests`                                       |
| Non-UTF-8 operands and unsupported options                         | T      | `args::tests`                                       |
| Usage diagnostics and exit 2                                       | T      | `error::tests`, `tests::usage_error_goes_to_stderr` |
| Partial stdout and stderr writes, flush failures                   | T      | `tests::handles_*`                                  |
| Quiet broken pipe                                                  | G      | Planned with output and pager lifecycle             |

## History and display

| Dimension                                           | Status | Evidence                                             |
| --------------------------------------------------- | ------ | ---------------------------------------------------- |
| Git record framing and hostile configuration        | G      | Stream implementation pending                        |
| SHA-1 and SHA-256, unique-prefix dimming            | G      | Stream and color implementation pending              |
| Headers, mailmap, dates, decorations, message bytes | G      | Rendering implementation pending                     |
| Graph edges, lane colors, ranges, compaction        | G      | Graph implementation pending                         |
| Patches, stats, binary sizes, width                 | G      | Diff implementation pending                          |
| Pager environment, exit statuses, stderr, signals   | G      | Process lifecycle implementation pending             |
| Repository mutations during a run                   | U      | Subprocesses may observe different repository states |
| Ambiguous-width Unicode terminals                   | U      | Display cell widths depend on the terminal           |
