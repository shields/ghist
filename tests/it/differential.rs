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

#[cfg(test)]
mod tests {
    use crate::common::{
        history::{Change, Commit, History, Identity},
        oracle,
        repo::TestRepo,
    };
    use proptest::{
        prelude::*,
        test_runner::{Config, RngAlgorithm, TestRng, TestRunner},
    };

    #[test]
    fn opted_in_repositories_match_fuller() {
        if let Some(paths) = std::env::var_os("GHIST_DIFF_REPOS") {
            for path in std::env::split_paths(&paths) {
                oracle::fuller(&TestRepo::existing(path).unwrap(), &[]).unwrap();
            }
        }
    }

    #[test]
    fn generated_histories_match_fuller() {
        let strategy = prop::collection::vec(
            (
                any::<u16>(),
                prop::collection::vec(any::<u8>(), 0..80),
                any::<bool>(),
                -720_i16..841,
            ),
            1..13,
        );
        let config = Config {
            source_file: Some(file!()),
            test_name: Some("generated_histories_match_fuller"),
            ..Config::default()
        };
        for case in 0..config.cases {
            let mut seed = [0x6a; 32];
            seed[..4].copy_from_slice(&case.to_le_bytes());
            let mut runner = TestRunner::new_with_rng(
                Config {
                    cases: 1,
                    max_shrink_iters: config.max_shrink_iters(),
                    ..config.clone()
                },
                TestRng::from_seed(RngAlgorithm::ChaCha, &seed),
            );
            let format = if case % 8 == 0 { "sha256" } else { "sha1" };
            runner
                .run(&strategy, |nodes| {
                    let repo = TestRepo::new(format).unwrap();
                    let mut history = History::default();
                    for (index, (mask, message, different, zone)) in nodes.into_iter().enumerate() {
                        let author = Identity {
                            date: format!(
                                "{} {}{:02}{:02}",
                                1_700_000_000 + index * 61,
                                if zone < 0 { '-' } else { '+' },
                                zone.abs() / 60,
                                zone.abs() % 60
                            ),
                            ..Identity::default()
                        };
                        let mut committer = author.clone();
                        if different {
                            committer.name = b"Another author".to_vec();
                            committer.date = "1700000000 -0330".into();
                        }
                        let mark = history.push(Commit {
                            parents: (0..index)
                                .filter(|parent| mask & (1 << parent) != 0)
                                .map(|parent| parent + 1)
                                .collect(),
                            author,
                            committer,
                            message,
                            encoding: Some("ISO-8859-1".into()),
                            changes: vec![Change::Write {
                                mode: 0o100_644,
                                path: format!("file{}", index % 3).into_bytes(),
                                data: format!("content {index}\n").into_bytes(),
                            }],
                            ..Commit::default()
                        });
                        history
                            .refs
                            .push((format!("refs/heads/node-{index}"), mark));
                    }
                    repo.import(&history).unwrap();
                    let args: Vec<_> = history.refs.iter().map(|(name, _)| name.as_str()).collect();
                    oracle::fuller(&repo, &args)
                        .map_err(|error| TestCaseError::fail(error.to_string()))?;
                    oracle::fuller(&repo, &["HEAD", "--", "file0"])
                        .map_err(|error| TestCaseError::fail(error.to_string()))?;
                    for color in ["never", "always"] {
                        repo.git(["config", "color.diff", color]).unwrap();
                        crate::common::patches::compare(&repo, &args)
                            .map_err(|error| TestCaseError::fail(error.to_string()))?;
                    }
                    repo.git([
                        "config",
                        "color.diff",
                        if case % 2 == 0 { "never" } else { "always" },
                    ])
                    .unwrap();
                    let stat_compare = if case % 2 == 0 {
                        crate::common::stats::compare
                    } else {
                        crate::common::stats::combined
                    };
                    stat_compare(&repo, &args, 20 + usize::try_from(case % 181).unwrap())
                        .map_err(|error| TestCaseError::fail(error.to_string()))?;
                    Ok(())
                })
                .unwrap_or_else(|error| panic!("case {case} ({format}): {error}"));
        }
    }
}
