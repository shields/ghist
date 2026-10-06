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
| Git record framing and hostile configuration        | T      | `git::log::tests`, `tests/it/hostile.rs`             |
| SHA-1 and SHA-256, unique-prefix dimming            | G      | Stream and color implementation pending              |
| Headers, mailmap, dates, decorations, message bytes | T      | `render::*::tests`, `tests/it/headers.rs`            |
| Graph edges, lane colors, ranges, compaction        | G      | Graph implementation pending                         |
| Patches, stats, binary sizes, width                 | G      | Diff implementation pending                          |
| Pager environment, exit statuses, stderr, signals   | G      | Process lifecycle implementation pending             |
| Repository mutations during a run                   | U      | Subprocesses may observe different repository states |
| Ambiguous-width Unicode terminals                   | U      | Display cell widths depend on the terminal           |

## Fixture construction

These tests verify the inputs used by the integration harness. The application
behavior entries above stay marked as gaps until ghist itself is tested.

| Dimension                                                               | Status | Evidence                                              |
| ----------------------------------------------------------------------- | ------ | ----------------------------------------------------- |
| SHA-1 and SHA-256 histories from one fast-import stream                 | T      | `tests/it/fixtures.rs`                                |
| Message bytes, NUL, invalid UTF-8, Latin-1 encoding                     | T      | `imports_byte_exact_histories_in_both_object_formats` |
| Binary data, byte paths, renames, deletions, symlinks, executable modes | T      | `imports_byte_exact_histories_in_both_object_formats` |
| Tags, remote refs, independent roots, ordered octopus parents           | T      | `tests/it/fixtures.rs`                                |
| Shallow clones, stash, replace refs, mailmap                            | T      | `supports_shallow_stash_replace_and_mailmap_fixtures` |
| Allowlisted process environment and Git setup failures                  | T      | `isolates_environment_and_git_failures`               |

## Configuration and subprocess setup

| Dimension                                                      | Status | Evidence                                                   |
| -------------------------------------------------------------- | ------ | ---------------------------------------------------------- |
| Last-entry precedence across aliases; subsection case          | T      | `git::config::tests`                                       |
| Bare, empty, multiline, and non-UTF-8 configuration values     | T      | `git::config::tests`                                       |
| Boolean keywords, numeric bases, scale suffixes, signed bounds | T      | `env::tests`, `tests/it/config.rs`                         |
| Invalid `log.mailmap` values exit 128                          | T      | `reads_real_config_and_rejects_invalid_boolean`            |
| Explicit argv, allowlisted environment, null stdin             | T      | `config_subprocess_has_explicit_arguments_and_environment` |
| Raw stderr, nonzero status, signal status, and spawn failures  | T      | `tests/it/config.rs`                                       |
| Large stderr does not block stdout                             | T      | `drains_large_stderr_without_deadlocking`                  |
| Git failures take precedence over malformed protocol output    | T      | `prioritizes_git_failures_over_malformed_stdout`           |
| Help and version avoid spawning Git                            | T      | `informational_options_do_not_start_git`                   |
| Stderr write and flush failures remain errors                  | T      | `surfaces_stderr_write_failures`                           |

## Record stream

| Dimension                                                         | Status | Evidence                                                                              |
| ----------------------------------------------------------------- | ------ | ------------------------------------------------------------------------------------- |
| SHA-1 and SHA-256 IDs, parent counts, abbreviation prefixes       | T      | `git::log::tests`, `oid::tests`                                                       |
| Arbitrary buffer boundaries, marker bytes inside messages         | T      | `message_marker_and_chunk_boundaries`                                                 |
| Every truncated record prefix and every read failure boundary     | T      | `empty_and_truncated_streams`, `io_errors_at_each_byte_boundary`                      |
| Adjacent records, patch/stat sections, byte-exact diff lines      | T      | `diffs_and_adjacent_records`, `streams_both_formats_with_hostile_log_and_diff_config` |
| Signature verification and external diffs disabled                | T      | `tests/it/hostile.rs`                                                                 |
| Invalid diff color, unborn HEAD, bad revision, outside repository | T      | `tests/it/hostile.rs`, `tests/it/errors.rs`                                           |
| Empty walks, malformed framing, Git error precedence              | T      | `tests/it/errors.rs`                                                                  |
| Raw Git warnings retained with protocol errors                    | T      | `retains_git_warnings_when_protocol_validation_fails`                                 |

## Plain rendering

The Unicode-width comparison covers every scalar value except NUL, LF, CR, and
TAB, which have separate message/framing cases.

| Dimension                                                                      | Status | Evidence                                                                   |
| ------------------------------------------------------------------------------ | ------ | -------------------------------------------------------------------------- |
| Full hashes and object format labels; detached HEAD                            | T      | `full_hashes_conditional_headers_and_dates_in_both_formats`                |
| Conditional committer identity and dates; same epoch with different timezones  | T      | `render::header::tests`, `tests/it/headers.rs`                             |
| Mailmap affects identity comparison and honors `log.mailmap=false`             | T      | `mailmap_controls_identity_comparison_and_full_decorations`                |
| Decoration order, shortened refs, commas and non-UTF-8 ref bytes               | T      | `merge_abbreviations_and_decoration_kinds`, `tests/it/headers.rs`          |
| Merge parents show at least seven digits or their longer unique prefix         | T      | `merge_abbreviations_and_decoration_kinds`                                 |
| Empty messages, blank lines, trailing whitespace, tab stops and Unicode widths | T      | `render::message::tests`, `message_bytes_and_encoding_match_git_fuller`    |
| Invalid UTF-8 and control bytes stop tab expansion                             | T      | `tabs_use_message_columns_and_stop_at_invalid_text`, `tests/it/headers.rs` |
| Git output encodings and NUL-truncated messages                                | T      | `message_bytes_and_encoding_match_git_fuller`                              |
| Commit separators and every partial output write                               | T      | `separation_empty_messages_and_every_partial_write`                        |
| Unicode character widths and tab stops across the scalar range                 | T      | `unicode_tab_stops_match_git_across_scalar_values`                         |
