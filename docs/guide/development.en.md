# Development

[Русский](development.ru.md) · [Installation](installation.en.md) · [Translations](translations.en.md)

## Environment

- Rust stable, MSVC toolchain. MSRV is **1.88** (`rust-version` in the root `Cargo.toml`,
  enforced by its own CI job). Why that particular floor:
  [ADR-0023](../architecture/adr/0023-msrv-1-88-for-time-advisory.md) (Russian).
- Release target triple: `x86_64-pc-windows-msvc`. A `*-windows-gnu` build compiles but is
  not equivalent: the Per-Monitor-V2 manifest and the version resource are embedded for
  MSVC only.
- Python 3.11+ for the licence collector (`tomllib`).
- Inno Setup 6.3+ to build the installer only. It is preinstalled on GitHub runners, so a
  local copy is not always needed.

## Build and gates

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings   # must be clean
cargo fmt --all -- --check
```

All three gates are required before a commit (skill `quality-gates`) and are mirrored in
[CI](../../.github/workflows/ci.yml): the gates on Windows, a separate "the core builds and
tests off Windows" check, and an MSRV job.

To run: `cargo run --release -p switcher-app`.

## Layout

| Crate | Role |
| --- | --- |
| `switcher-core` | domain: layout state machine, badge anchor decision, config model. No OS dependencies, `#![forbid(unsafe_code)]` |
| `switcher-platform` | ports (traits) and flat event types |
| `switcher-windows` | Win32/COM adapters — the only place `unsafe` is allowed |
| `switcher-app` | shell: tray, main loop, badge rasterization, localization, sound, wiring |

Project rules are in [CLAUDE.md](../../CLAUDE.md), decisions in
[ADR](../architecture/adr/) (Russian), and the overall picture in the
[architecture overview](../architecture/overview.md) (Russian).

## Localization

Adding an interface language is described in [translations](translations.en.md). In short:
a new `.ftl` plus one row in `SUPPORTED`.

## Manual checks

OS hooks cannot be automated, so they have checklists and scripts:

- [M1 smoke checklist](../smoke/m1-windows.md) — expectations and what is still unverified;
- `.\scripts\manual-check.ps1` — a step-by-step check from PowerShell 7;
- `.\scripts\start-fallback-check.ps1` — a short check of the layout fallback read;
- `cargo run -p switcher-windows --example overlay_smoke` — overlay prototype: click
  transparency, DPI, monitor crossing, fixed corner.

## Building release artifacts

```powershell
pwsh -File packaging/build-release.ps1
```

The script builds the release binary for an explicit triple, generates
`THIRD-PARTY-LICENSES.md`, packs the portable archive, compiles the installer and computes
SHA-256 sums. Output lands in `target/packaging`. Without Inno Setup: `-SkipInstaller`.

The [release workflow](../../.github/workflows/release.yml) calls the same script, so a
local build and a release build produce the same artifacts.

### Licence check

```powershell
python packaging/collect-licenses.py --output target/packaging/THIRD-PARTY-LICENSES.md
```

It takes the dependency set from `cargo tree --edges normal` for the Windows triple, that
is, only what actually ends up in the shipped binary. A non-zero exit means some crate
could not be read; a release must not ship in that state.

## Icon

```sh
cargo run -p switcher-app --example make_icon
```

Regenerates `crates/switcher-app/assets/icons/lang-switcher.ico` from the same rasterizer
that draws the badge. The result is committed; the build never runs this.

## Releasing

1. The three gates and MSRV are green; manual checks are done and recorded.
2. Update `workspace.package.version` and the [CHANGELOG](../../CHANGELOG.md) entry.
3. Merge into `main` through a PR.
4. Tag `vX.Y.Z...` and push the tag — the release workflow builds the artifacts and creates
   a **draft** release.
5. Download the draft's artifacts, verify the SHA-256 sums, test the installation.
6. Publish the release.

Alpha and beta releases are published as prereleases.
