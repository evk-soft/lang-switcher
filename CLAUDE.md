# lang-switcher

Cross-platform keyboard-layout indicator (RU/EN): a small badge near the mouse cursor / text caret plus an optional sound cue, so the user always knows the active layout. Windows 11 first; macOS and Linux later. Design docs are the source of truth: `docs/architecture/overview.md`, `docs/superpowers/specs/`.

## Language

- Talk to the user in Russian.
- Code, identifiers, code comments, commit messages: English. Docs under `docs/` are in Russian.

## Stack

Pure native Rust, no webview (ADR-0001). Cargo workspace (ADR-0002):

- `crates/switcher-core` — domain: layout state machine, badge anchor decision, config model. No OS deps, `#![forbid(unsafe_code)]`.
- `crates/switcher-platform` — ports (traits): `LayoutMonitor`, `PointerTracker`, `CaretLocator`, `OverlayWindow`, `Autostart`, `SoundPlayer`.
- `crates/switcher-windows` / `-macos` / `-linux` — per-OS adapters (windows-rs / objc2 / x11rb + zbus). The only crates where `unsafe` is allowed.
- `crates/switcher-app` — shell: tray (tray-icon + muda), main event loop, on-demand egui settings window, wiring.

## Commands

- Build: `cargo build --workspace`
- Test: `cargo test --workspace`
- Lint (must be clean): `cargo clippy --workspace --all-targets -- -D warnings`
- Format: `cargo fmt --all` (check mode: `cargo fmt --all -- --check`)

## Rules

1. **Quality gates** — before claiming any change complete: fmt-check, clippy, tests all pass (skill `quality-gates`). Evidence before assertions.
2. **Unsafe policy** — `unsafe` only inside platform adapter crates; every `unsafe` block carries a `// SAFETY:` comment naming the invariants that make it sound.
3. **Zero polling** — subscribe to OS events. Any fallback poll must be armed only while strictly needed (e.g., elevated/UWP foreground window on Windows) and documented in `docs/architecture/overview.md` (ADR-0003).
4. **Platform isolation** — OS-specific code lives only behind `switcher-platform` traits; `switcher-core` compiles and is fully tested on any OS.
5. **Threading** — each OS hook owns a dedicated thread with its own message loop (Win32 hidden message-only window / CFRunLoop / X event loop); COM work (TSF, UIA) on an STA thread; cross-thread communication via channels carrying plain data events only.
6. **ADR discipline** — any decision changing architecture, dependencies, or a platform technique gets an ADR (skill `adr`) before or with the change.
7. **Native API work** — before writing or altering windows-rs / objc2 / x11rb / zbus code, follow skill `platform-api-work`: verify the API contract via context7 or vendor docs; never code native APIs from memory.
8. **Process** — features go through superpowers: brainstorm → spec → plan → TDD. Core logic is developed test-first; platform adapters get manual smoke checklists where automation is impossible.
9. **Review** — before committing a substantive change, run the project workflow `rust-adversarial-review` (Workflow tool, `.claude/workflows/`).

## Environment expectations

Rust stable toolchain (MSRV pinned in `Cargo.toml` once scaffolded). Claude Code user-level plugins expected: `superpowers`, `context7`.
