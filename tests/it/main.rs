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

mod colors;
mod common;
mod config;
#[cfg(not(coverage_nightly))]
mod differential;
mod diffs;
mod errors;
#[cfg(not(coverage_nightly))]
mod exhaustive;
mod fixtures;
#[cfg(not(coverage_nightly))]
mod gaps;
mod graph_colors;
mod headers;
mod hostile;
mod paging;
mod pipes;
mod ranges;
mod signals;

#[cfg(not(coverage_nightly))]
mod properties;
