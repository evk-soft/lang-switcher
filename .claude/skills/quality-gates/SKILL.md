---
name: quality-gates
description: Use before claiming any Rust change in this repo is complete, fixed, or passing — runs fmt/clippy/tests and requires evidence before assertions
---

# Quality gates

Run all three, in order, from the repo root. All must pass before you say "done", commit, or open a PR.

1. `cargo fmt --all -- --check` — if it fails, run `cargo fmt --all` and re-check.
2. `cargo clippy --workspace --all-targets -- -D warnings` — fix warnings; never silence with `#[allow]` unless a one-line comment justifies why the lint is wrong here.
3. `cargo test --workspace` — all tests green.

Rules:

- **Evidence before claims.** Quote the actual output (test count, "clippy clean") when reporting; never say "should pass".
- If a gate cannot run (missing toolchain, broken build), say so explicitly — never imply it passed.
- Platform adapter code that cannot be unit-tested gets a **manual smoke checklist** in the commit/PR description instead: what you ran, on which OS/monitor setup, what you observed. The checklist template lives in skill `platform-api-work`.
- New core logic without tests does not pass the gate: `switcher-core` is test-first (see CLAUDE.md rule 8).
