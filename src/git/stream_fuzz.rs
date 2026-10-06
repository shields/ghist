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
    use crate::git::log::LogReader;
    use proptest::{
        prelude::*,
        test_runner::{Config, RngAlgorithm, TestRng, TestRunner},
    };
    use std::io::BufReader;

    fn runner(seed: u8) -> TestRunner {
        TestRunner::new_with_rng(
            Config::default(),
            TestRng::from_seed(RngAlgorithm::ChaCha, &[seed; 32]),
        )
    }

    #[test]
    fn records_round_trip_across_arbitrary_buffer_boundaries() {
        let payload = prop::collection::vec(1_u8..=255, 0..256);
        let strategy = (
            prop::collection::vec((any::<bool>(), payload.clone(), payload), 0..16),
            1_usize..128,
        );
        runner(0x35)
            .run(&strategy, |(records, capacity)| {
                let mut bytes = Vec::new();
                let mut expected = Vec::new();
                for (sha256, message, diff) in records {
                    let hash = vec![b'a'; if sha256 { 64 } else { 40 }];
                    bytes.extend_from_slice(b"\x1e\x1fghist\n");
                    for field in [
                        &hash,
                        b"aaaa".as_slice(),
                        b"",
                        b"",
                        b"author",
                        b"a@example.com",
                        b"date",
                        b"author",
                        b"a@example.com",
                        b"date",
                        b"",
                        &message,
                    ] {
                        bytes.extend_from_slice(field);
                        bytes.push(0);
                    }
                    bytes.push(b'\n');
                    let mut patch = Vec::new();
                    for line in diff.split(|&byte| byte == b'\n') {
                        patch.push(b'+');
                        patch.extend_from_slice(line);
                        patch.push(b'\n');
                    }
                    bytes.push(b'\n');
                    bytes.extend_from_slice(&patch);
                    expected.push((hash, message, patch));
                }
                let mut source = BufReader::with_capacity(capacity, bytes.as_slice());
                let mut reader = LogReader::new(&mut source);
                for (hash, message, patch) in expected {
                    let record = reader.next_record().unwrap().unwrap();
                    prop_assert_eq!(record.hash, hash);
                    prop_assert_eq!(record.message, message);
                    let mut actual = Vec::new();
                    while let Some(line) = reader.diff_line().unwrap() {
                        actual.extend_from_slice(line);
                    }
                    prop_assert_eq!(actual, patch);
                }
                prop_assert_eq!(reader.next_record().unwrap(), None);
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn arbitrary_streams_terminate_without_panicking() {
        let strategy = (prop::collection::vec(any::<u8>(), 0..4096), 1_usize..128);
        runner(0x82)
            .run(&strategy, |(bytes, capacity)| {
                let mut source = BufReader::with_capacity(capacity, bytes.as_slice());
                let mut reader = LogReader::new(&mut source);
                let mut count = 0;
                while let Ok(Some(_)) = reader.next_record() {
                    count += 1;
                    prop_assert!(count < bytes.len());
                }
                Ok(())
            })
            .unwrap();
    }
}
