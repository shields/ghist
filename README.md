<!--
Copyright © 2026 Michael Shields

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
-->

# ghist

An opinionated `git log` replacement, with a Unicode graph and git’s own colors.
Implementation is in progress; see [the design](docs/design.md).

## Development

Install [Bun](https://bun.sh/), [Lefthook](https://lefthook.dev/), and
[cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov), then run:

```sh
bun install --frozen-lockfile
rustup toolchain install "$(make print-nightly)" --profile minimal --component llvm-tools-preview
lefthook install
make lint test coverage
```

`rust-toolchain.toml` pins stable Rust. Coverage uses the dated nightly in the
Makefile and requires 100% line coverage. The pre-commit hook runs both lint and
coverage so local commits meet the same gate as CI. `make fmt` formats Rust and
all supported documentation and configuration files.

`make build` builds a release binary; `make install` installs it.
