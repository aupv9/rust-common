## What does this change?

<!-- A short description of the change and why it is needed. Link the issue it resolves, e.g. "Closes #12". -->

## How was it tested?

<!-- Commands you ran, new tests, or manual steps. -->

## Checklist

- [ ] `cargo fmt`, `cargo clippy -- -D warnings` and `cargo test --workspace --all-features` pass
- [ ] Docker-backed tests pass (`cargo test --workspace --all-features -- --ignored`) if `common-db`, `common-test` or `common-http-client` changed
- [ ] `cargo hack check --workspace --each-feature --no-dev-deps` passes if features changed
- [ ] The crate README is updated for new or changed behavior
- [ ] `CHANGELOG.md` has an entry under `Unreleased` for changes users will notice
