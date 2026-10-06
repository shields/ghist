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

pub mod ansi;
pub mod env;
#[cfg(not(coverage_nightly))]
pub mod graph_oracle;
pub mod graph_text;
pub mod history;
#[cfg(all(test, not(coverage_nightly)))]
pub mod oracle;
#[cfg(all(test, not(coverage_nightly)))]
pub mod parse;
pub mod repo;
