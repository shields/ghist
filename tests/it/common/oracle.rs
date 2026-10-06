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

use ghist::Exit;

use super::{parse, repo::TestRepo};

pub fn fuller(repo: &TestRepo, args: &[&str]) -> std::io::Result<()> {
    let mut invocation = vec![
        "log",
        "--pretty=fuller",
        "--date=iso",
        "--topo-order",
        "--parents",
        "--abbrev=7",
        "--no-abbrev-commit",
        "--no-decorate",
        "--no-patch",
        "--no-show-signature",
        "--no-follow",
        "--color=never",
        "--end-of-options",
    ];
    invocation.extend_from_slice(args);
    let expected = repo.git(invocation)?;
    let mut actual = Vec::new();
    let mut err = Vec::new();
    let exit = ghist::run(&repo.context(args), &mut actual, &mut err);
    if exit != Exit::Code(0) || !err.is_empty() {
        return Err(std::io::Error::other(format!(
            "{exit:?}: {}",
            String::from_utf8_lossy(&err)
        )));
    }
    let observed = parse::commits(&actual).map_err(std::io::Error::other)?;
    let wanted = parse::commits(&expected).map_err(std::io::Error::other)?;
    if observed != wanted {
        return Err(std::io::Error::other(format!(
            "{args:?}\nactual: {observed:?}\nexpected: {wanted:?}"
        )));
    }
    for line in actual.split(|&byte| byte == b'\n') {
        if line.ends_with(b" ") || line.ends_with(b"\t") {
            return Err(std::io::Error::other(format!(
                "trailing whitespace: {line:?}"
            )));
        }
    }
    Ok(())
}
