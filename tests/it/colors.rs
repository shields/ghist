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
        repo::TestRepo,
    };
    use ghist::Exit;

    #[test]
    fn invalid_colors_fail_even_with_color_disabled() {
        for key in [
            "color.decorate.branch",
            "color.decorate.head",
            "log.graphColors",
            "color.diff.commit",
            "color.diff.new",
            "color.diff.old",
        ] {
            let repo = TestRepo::new("sha1").unwrap();
            let mut history = History::default();
            history.push(Commit::default());
            repo.import(&history).unwrap();
            repo.git(["config", "color.ui", "never"]).unwrap();
            repo.git(["config", key, "not-a-color"]).unwrap();
            let mut out = Vec::new();
            let mut err = Vec::new();
            assert_eq!(
                ghist::run(&repo.context(&[]), &mut out, &mut err),
                Exit::Code(128),
                "{key}"
            );
            assert_eq!(out, b"");
            assert!(err.starts_with(b"ghist: invalid configuration"));
        }
    }

    #[test]
    fn one_color_decision_controls_the_git_invocation() {
        let repo = TestRepo::new("sha1").unwrap();
        let mut ctx = repo
            .fake_git(
                br#"
if test "$2" = config; then printf 'color.ui\n%s\000' "$TEST_COLOR"; exit 0; fi
for arg in "$@"; do
    case "$arg" in
        --color=*) test "$arg" = "--color=$TEST_EXPECT" || exit 91; exit 0;;
    esac
done
exit 92
"#,
            )
            .unwrap();
        for (setting, terminal, term, no_color, expected) in [
            ("always", false, "dumb", "1", "always"),
            ("never", true, "xterm", "", "never"),
            ("auto", true, "xterm", "", "always"),
            ("auto", true, "xterm", "1", "never"),
            ("auto", false, "xterm", "", "never"),
            ("auto", true, "dumb", "", "never"),
        ] {
            ctx.stdout_is_terminal = terminal;
            ctx.env.retain(|(key, _)| {
                !["TEST_COLOR", "TEST_EXPECT", "TERM", "NO_COLOR"]
                    .iter()
                    .any(|name| key == name)
            });
            ctx.env.extend(
                [
                    ("TEST_COLOR", setting),
                    ("TEST_EXPECT", expected),
                    ("TERM", term),
                    ("NO_COLOR", no_color),
                ]
                .map(|(key, value)| (key.into(), value.into())),
            );
            let mut err = Vec::new();
            assert_eq!(
                ghist::run(&ctx, &mut Vec::new(), &mut err),
                Exit::Code(0),
                "{setting}: {err:?}"
            );
            assert_eq!(err, b"");
        }
    }
}
