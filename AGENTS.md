# Agent Instructions

- Ignore the `IGNORE/` folder entirely: do not read, search, index, reference, or modify anything inside it.
- Exclude `IGNORE/` from all code searches, refactors, and analysis.

# Project Instructions

- Use stable Rust.
- Prefer safe Rust.
- Avoid unnecessary dependencies.
- Document all public APIs.
- Add unit tests for new functionality.
- Do not change public APIs without approval.

## Validation

Run these commands before committing changes:

1. cargo fmt --all -- --check
2. cargo check --workspace --all-targets
3. cargo clippy --workspace --all-targets
4. cargo test --workspace