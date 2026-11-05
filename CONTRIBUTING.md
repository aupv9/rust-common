# Contributing to rust-common

Thanks for taking the time to contribute. This guide covers how to set up the project, the conventions the code follows, and what a pull request needs before it can be merged.

By participating you agree to follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## Ways to contribute

- **Report a bug** with the [bug report template](https://github.com/aupv9/rust-common/issues/new?template=bug_report.yml). A minimal reproduction helps most.
- **Propose a feature** with the [feature request template](https://github.com/aupv9/rust-common/issues/new?template=feature_request.yml). For anything larger than a small addition, open an issue to agree on the design before writing code.
- **Improve documentation.** READMEs and examples count as much as code.
- **Report a vulnerability** privately, as described in [SECURITY.md](SECURITY.md).

## Development setup

You need:

- Rust **stable**; `rust-toolchain.toml` selects it and installs `rustfmt` and `clippy`
- A C toolchain and CMake for `aws-lc-sys`, the TLS crypto provider. The first build takes a few minutes because of it
- Docker (or [colima](https://github.com/abiosoft/colima)) for integration tests
- Optional: [`cargo-deny`](https://github.com/EmbarkStudios/cargo-deny) and [`cargo-hack`](https://github.com/taiki-e/cargo-hack)

```sh
git clone https://github.com/aupv9/rust-common
cd rust-common
cargo test --workspace --all-features
```

## Before opening a pull request

Run the same checks CI runs:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check
cargo hack check --workspace --each-feature --no-dev-deps
```

Integration tests that need Docker are marked `#[ignore]`:

```sh
cargo test --workspace --all-features -- --ignored
```

With colima, export these first:

```sh
export DOCKER_HOST=unix://$HOME/.colima/default/docker.sock
export TESTCONTAINERS_DOCKER_SOCKET_OVERRIDE=/var/run/docker.sock
```

## Conventions

- **No comments in source code.** Names and types should make the code readable. Explanations belong in the crate README, the PR description, or tests that show the behavior.
- **Layering.** Dependencies only point down the tiers described in [docs/architecture.md](docs/architecture.md). Tier 0 crates (`common-error`, `common-types`, `common-config`) must not depend on tokio.
- **Features.** Optional integrations go behind a feature flag and are re-exported through the `common` facade. Every feature must build on its own; `cargo hack` checks this.
- **Errors.** Public functions return `common_error::Result`. Pick the `ErrorKind` that gives the caller the right HTTP status and retry behavior.
- **No `unsafe`.** It is forbidden across the workspace.
- **Lints.** Clippy `pedantic` is on. Allow a lint locally only with a good reason.
- **Tests.** New behavior comes with tests. Tests that need external services are marked `#[ignore = "requires docker"]`.
- **Dependencies.** Add them to `[workspace.dependencies]` with `default-features = false` where practical. New licenses must be accepted by `deny.toml`.

### Adding a crate

1. `cargo new --lib crates/common-<name>`
2. In its `Cargo.toml`, inherit `version`, `edition`, `rust-version`, `license`, `repository` and `publish` from the workspace, set `readme = "README.md"`, and add `[lints] workspace = true`
3. Add it to `[workspace.dependencies]` in the root `Cargo.toml`
4. Add a feature and a `pub use` in `crates/common`, and list it in the root README
5. Write `crates/common-<name>/README.md`

## Commits and pull requests

- Keep each pull request focused on one change. Split unrelated changes.
- Write commit subjects in the imperative mood ("Add cursor pagination", not "Added ...").
- Fill in the pull request template, including how you tested the change.
- Add an entry under `Unreleased` in [CHANGELOG.md](CHANGELOG.md) for any change users will notice.
- CI must pass before review.

## License

By contributing, you agree that your contributions are dual licensed under the MIT and Apache-2.0 licenses, as stated in the [README](README.md#license), without any additional terms or conditions.
