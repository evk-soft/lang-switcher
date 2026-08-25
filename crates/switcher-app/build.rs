//! Embeds the Per-Monitor-V2 DPI manifest into `lang-switcher.exe` (ADR-0010).
//!
//! This lives in the binary crate on purpose: `cargo::rustc-link-arg-bins` applies to the
//! binary targets of *this* package, so a build script in `switcher-windows` could not
//! reach the executable. It also means the examples in `switcher-windows` never get the
//! manifest — they call `dpi::ensure_per_monitor_v2()` first thing instead.

fn main() {
    // The flags below are `link.exe` options, so they are MSVC-only. On any other target
    // (including `*-windows-gnu`, which would need a `.rc` file instead) this is a no-op.
    let is_windows = std::env::var_os("CARGO_CFG_WINDOWS").is_some();
    let is_msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if !is_windows || !is_msvc {
        return;
    }

    println!("cargo::rerun-if-changed=lang-switcher.manifest");

    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("cargo always sets CARGO_MANIFEST_DIR");
    let manifest = std::path::Path::new(&manifest_dir).join("lang-switcher.manifest");

    // Without this check a missing manifest would not produce "a binary without a
    // manifest" — it would fail the link, because `/MANIFESTINPUT` hands the path to
    // mt.exe, which errors out when the file is not there. That also makes the negative
    // control in the smoke checklist (rename the manifest, expect a runtime warning)
    // impossible to run.
    if !manifest.is_file() {
        println!(
            "cargo::warning=lang-switcher.manifest not found; \
             Per-Monitor-V2 will be set at runtime instead"
        );
        return;
    }

    let path = manifest.display().to_string();
    // `/MANIFESTINPUT` is documented as limited to MAX_PATH (260) for the fully qualified
    // name. Worth warning about early: this repository is often built from a worktree
    // under `.claude/worktrees/...`, which eats a lot of that budget.
    if path.chars().count() > 250 {
        println!(
            "cargo::warning=manifest path is {} characters, close to the MAX_PATH limit of \
             260 that /MANIFESTINPUT imposes; the link may fail",
            path.chars().count()
        );
    }

    println!("cargo::rustc-link-arg-bins=/MANIFEST:EMBED");
    println!("cargo::rustc-link-arg-bins=/MANIFESTINPUT:{path}");
}
