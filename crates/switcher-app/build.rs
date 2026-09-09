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

    embed_version_and_icon(&manifest_dir);
}

/// Compiles a generated `.rc` carrying the application icon and a VERSIONINFO block.
///
/// Without it Explorer shows a blank "Details" tab and a generic icon — for an unsigned
/// alpha those are the only identity the binary has. The script is generated rather than
/// checked in so the version can never drift from `Cargo.toml`.
fn embed_version_and_icon(manifest_dir: &str) {
    let dir = std::path::Path::new(manifest_dir);
    let icon = dir.join("assets/icons/lang-switcher.ico");
    println!("cargo::rerun-if-changed={}", icon.display());
    if !icon.is_file() {
        println!(
            "cargo::warning=assets/icons/lang-switcher.ico not found; \
             regenerate it with `cargo run -p switcher-app --example make_icon`"
        );
        return;
    }

    let version = env!("CARGO_PKG_VERSION");
    // FILEVERSION takes four numbers, so the pre-release suffix ("0.1.0-alpha.1") cannot
    // go there. The numeric field keeps major.minor.patch.0; the human-readable string
    // below carries the full version, suffix included.
    let numeric: Vec<&str> = version
        .split('-')
        .next()
        .unwrap_or("0.0.0")
        .split('.')
        .collect();
    let (major, minor, patch) = (
        numeric.first().copied().unwrap_or("0"),
        numeric.get(1).copied().unwrap_or("0"),
        numeric.get(2).copied().unwrap_or("0"),
    );

    // No `#include <winresrc.h>`: FILEOS 0x4 is VOS__WINDOWS32 and FILETYPE 0x1 is
    // VFT_APP, and spelling them numerically keeps this independent of SDK headers.
    // 0x0409 is US English and 1200 is the Unicode codepage, the pair every localized
    // Windows reads when it finds no block for its own language.
    let rc = format!(
        r#"1 ICON "{icon}"

1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEOS 0x4L
FILETYPE 0x1L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904B0"
        BEGIN
            VALUE "CompanyName", "evk-soft"
            VALUE "FileDescription", "lang-switcher - keyboard layout indicator"
            VALUE "FileVersion", "{version}"
            VALUE "InternalName", "lang-switcher"
            VALUE "LegalCopyright", "MIT OR Apache-2.0"
            VALUE "OriginalFilename", "lang-switcher.exe"
            VALUE "ProductName", "lang-switcher"
            VALUE "ProductVersion", "{version}"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x409, 1200
    END
END
"#,
        // RC accepts forward slashes in a path, and unlike the backslashes
        // `Path::join` produces on Windows they need no escaping.
        icon = icon.display().to_string().replace('\\', "/"),
    );

    let out_dir = std::env::var("OUT_DIR").expect("cargo always sets OUT_DIR");
    let script = std::path::Path::new(&out_dir).join("lang-switcher.rc");
    std::fs::write(&script, rc).expect("the build script can write into OUT_DIR");
    embed_resource::compile(&script, embed_resource::NONE)
        .manifest_required()
        .expect("rc.exe ships with the MSVC toolchain this target already links with");
}
