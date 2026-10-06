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
    use crate::common::history::{Commit, History};
    use crate::common::repo::TestRepo;
    use ghist::{Context, Exit};

    fn capture(ctx: &Context) -> (Exit, Vec<u8>) {
        let mut err = Vec::new();
        let exit = ghist::run(ctx, &mut Vec::new(), &mut err);
        (exit, err)
    }

    #[test]
    fn unborn_head_bad_revision_and_outside_repo() {
        let repo = TestRepo::new("sha1").unwrap();
        let (exit, err) = capture(&repo.context(&[]));
        assert_eq!(exit, Exit::Code(128));
        assert!(err.starts_with(b"fatal:"));
        let mut history = History::default();
        history.push(Commit::default());
        repo.import(&history).unwrap();
        let (exit, err) = capture(&repo.context(&["bad-revision"]));
        assert_eq!(exit, Exit::Code(128));
        assert!(err.starts_with(b"fatal:"));
        let mut ctx = repo.context(&[]);
        ctx.cwd = repo.cwd.parent().unwrap().to_owned();
        let (exit, err) = capture(&ctx);
        assert_eq!(exit, Exit::Code(128));
        assert!(err.starts_with(b"fatal: not a git repository"));
    }

    #[test]
    fn git_failure_precedes_record_protocol_failure() {
        let repo = TestRepo::new("sha1").unwrap();
        let ctx = repo.fake_git(b"if test \"$2\" = config; then exit 0; fi\nprintf 'wrong\\n'; printf 'fatal\\377\\n' >&2; exit 43\n").unwrap();
        assert_eq!(capture(&ctx), (Exit::Code(43), b"fatal\xff\n".to_vec()));
        let ctx = repo.fake_git(b"if test \"$2\" = config; then exit 0; fi\nprintf 'wrong\\n'; i=0; while test $i -lt 20000; do printf 'drain me\\n'; i=$((i+1)); done\n").unwrap();
        assert_eq!(
            capture(&ctx),
            (
                Exit::Code(1),
                b"ghist: invalid git output: invalid record marker\n".to_vec()
            )
        );
    }

    #[test]
    fn empty_walk_and_log_warning() {
        let repo = TestRepo::new("sha1").unwrap();
        let ctx = repo
            .fake_git(b"if test \"$2\" = config; then exit 0; fi\nprintf 'warning\\377\\n' >&2\n")
            .unwrap();
        assert_eq!(capture(&ctx), (Exit::Code(0), b"warning\xff\n".to_vec()));
    }
    #[test]
    fn retains_git_warnings_when_protocol_validation_fails() {
        let repo = TestRepo::new("sha1").unwrap();
        for (script, expected) in [
            (b"printf 'bad'; printf 'warning\\377\\n' >&2\n".as_slice(), b"warning\xff\nghist: invalid git output: unterminated config entry\n".as_slice()),
            (b"if test \"$2\" = config; then exit 0; fi\nprintf 'bad\\n'; printf 'warning\\377\\n' >&2\n".as_slice(), b"warning\xff\nghist: invalid git output: invalid record marker\n".as_slice()),
        ] {
            let ctx = repo.fake_git(script).unwrap();
            assert_eq!(capture(&ctx), (Exit::Code(1), expected.to_vec()));
        }
    }
}
