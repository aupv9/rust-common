# Security Policy

## Supported versions

The project is pre-1.0. Security fixes go to the latest commit on `main` and, once releases are tagged, to the most recent release.

| Version | Supported |
|---|---|
| `main` | ✓ |
| older revisions | ✗ |

## Reporting a vulnerability

**Do not open a public issue for security problems.**

Report vulnerabilities privately through [GitHub Security Advisories](https://github.com/aupv9/rust-common/security/advisories/new). Include:

- The affected crate(s) and revision
- A description of the issue and its impact
- Steps or code to reproduce it
- A suggested fix, if you have one

You can expect an acknowledgement within 3 business days and an assessment within 7. Once a fix is available, an advisory is published and you are credited unless you prefer otherwise.

## Dependencies

CI runs [`cargo-deny`](https://github.com/EmbarkStudios/cargo-deny) against the [RustSec advisory database](https://rustsec.org) on every change, and Dependabot keeps dependencies current. If an advisory affects a dependency of this project, a pull request that bumps it is welcome.
