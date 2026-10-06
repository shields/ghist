# Copyright © 2026 Michael Shields
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

.PHONY: build install test lint fmt coverage print-nightly run clean publish publish-dry-run bench

NIGHTLY := nightly-2026-10-06
PRETTIER ?= bunx --no-install prettier

export BENCH_REPO ?= .
export BENCH_REV ?= HEAD~100..HEAD
export GHIST_BIN := $(CURDIR)/target/release/ghist

build:
	cargo build --release

install:
	cargo install --path .

test:
	cargo test --all-targets

lint:
	cargo fmt --check
	cargo clippy --all-targets -- -D warnings
	$(PRETTIER) --check .

fmt:
	cargo fmt
	$(PRETTIER) --write .

coverage:
	cargo +$(NIGHTLY) llvm-cov --fail-under-lines 100 --ignore-filename-regex '(^|/)tests/'

print-nightly:
	@echo $(NIGHTLY)

run:
	cargo run --release -- $(ARGS)

clean:
	cargo clean

publish publish-dry-run:
	@set -e; \
	src=$$(pwd); \
	tmp=$$(mktemp -d); \
	trap 'rm -rf "$$tmp"' EXIT INT TERM; \
	git archive HEAD | tar -x -C "$$tmp"; \
	ver=$$(gitcalver prepare-publish --prefix 0. --manifest "$$tmp/Cargo.toml" --source-dir "$$src"); \
	$(if $(findstring dry,$@),echo "Would publish version $$ver";) \
	cd "$$tmp"; \
	CARGO_TARGET_DIR="$$src/target" cargo check --quiet; \
	CARGO_TARGET_DIR="$$src/target" cargo publish $(if $(findstring dry,$@),--dry-run,)

bench: build
	hyperfine --warmup 3 --runs 10 \
		--command-name 'ghist: first page' 'cd "$$BENCH_REPO" && "$$GHIST_BIN" | head -n 40 >/dev/null' \
		--command-name 'git: first page' 'cd "$$BENCH_REPO" && git --no-pager log --graph --pretty=fuller --topo-order | head -n 40 >/dev/null'
	hyperfine --warmup 3 --runs 10 \
		--command-name 'ghist: full history' 'cd "$$BENCH_REPO" && "$$GHIST_BIN" >/dev/null' \
		--command-name 'git: full history' 'cd "$$BENCH_REPO" && git --no-pager log --graph --pretty=fuller --topo-order >/dev/null'
	hyperfine --warmup 3 --runs 10 \
		--command-name 'ghist: patches' 'cd "$$BENCH_REPO" && "$$GHIST_BIN" -p "$$BENCH_REV" >/dev/null' \
		--command-name 'git: patches' 'cd "$$BENCH_REPO" && git --no-pager log --graph --pretty=fuller --topo-order --no-diff-merges -p "$$BENCH_REV" >/dev/null'
	hyperfine --warmup 3 --runs 10 \
		--command-name 'ghist: stats' 'cd "$$BENCH_REPO" && "$$GHIST_BIN" --stat "$$BENCH_REV" >/dev/null' \
		--command-name 'git: stats' 'cd "$$BENCH_REPO" && git --no-pager log --graph --pretty=fuller --topo-order --no-diff-merges --stat "$$BENCH_REV" >/dev/null'
