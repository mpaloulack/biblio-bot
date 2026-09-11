## What this changes

<!-- One or two sentences. Link the issue if there is one. -->

## Why

<!-- The reasoning a reviewer cannot get from the diff. -->

## Checks

- [ ] `cargo fmt` and `cargo clippy --all-targets -- -D warnings` are clean
- [ ] `cargo test` passes
- [ ] `./scripts/coverage.sh` still holds above the gate
- [ ] User-facing strings were added to `src/i18n.rs` in both languages
