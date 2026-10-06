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

.PHONY: build install test lint fmt coverage print-nightly run clean publish publish-dry-run

NIGHTLY := nightly-2026-10-06
PRETTIER ?= bunx --no-install prettier

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
