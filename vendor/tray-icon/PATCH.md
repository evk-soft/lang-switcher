# Local Windows hover opt-out (ADR-0017)

Upstream: tauri-apps/tray-icon 0.24.1 from crates.io, commit
`304c7fb3779ae898c75d1d3d5be5f8f6997f0c4c` (see `.cargo_vcs_info.json`).
Published crate SHA-256:
`65ba1e5f6b9ef9fd87e21b9c6f351554dbd717960089168fcfdef854686961dc`.

Original source, normalized manifest, README and license files are retained.
Changes: Windows creation attribute/builder `hover_tracking` (default true),
stored on the Windows native owner; skip WM_MOUSEMOVE before querying cursor,
tray bounds or arming the hover timer when false. No other behavior is changed.
The upstream hover=true timer issue remains; lang-switcher opts out explicitly.

Native regression: `cargo run -p switcher-windows --example tray_smoke`.
Tests an enabled control and disabled hover, actual WM_TIMER delivery and clicks.
Repeat it when upgrading/removing this patch. See
`docs/architecture/adr/0017-disable-unused-tray-hover.md` in the application repo.
