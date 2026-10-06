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
        history::{Commit, History},
        oracle,
        repo::TestRepo,
    };

    #[test]
    fn every_dag_through_four_nodes() {
        for count in 1..=4 {
            let edges = count * (count - 1) / 2;
            for mask in 0..(1 << edges) {
                let repo = TestRepo::new(if mask % 8 == 0 { "sha256" } else { "sha1" }).unwrap();
                let mut history = History::default();
                let mut bit = 0;
                for node in 0..count {
                    let parents = (0..node)
                        .filter_map(|parent| {
                            let include = mask & (1 << bit) != 0;
                            bit += 1;
                            include.then_some(parent + 1)
                        })
                        .collect();
                    let mark = history.push(Commit {
                        parents,
                        message: format!("node {node}\n").into_bytes(),
                        ..Commit::default()
                    });
                    history.refs.push((format!("refs/heads/node-{node}"), mark));
                }
                repo.import(&history).unwrap();
                let args: Vec<_> = history.refs.iter().map(|(name, _)| name.as_str()).collect();
                oracle::fuller(&repo, &args).unwrap();
            }
        }
    }
}
