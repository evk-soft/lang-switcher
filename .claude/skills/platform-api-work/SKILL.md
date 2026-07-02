---
name: platform-api-work
description: Use when writing or modifying OS-specific code (windows-rs/Win32/COM, objc2/AppKit, x11rb/XKB, zbus/D-Bus) — docs-first workflow with unsafe and threading rules
---

# Platform API work

## Before writing code

1. Look up the exact API contract with **context7** (windows-rs, objc2, x11rb, zbus crates) or vendor docs (learn.microsoft.com, developer.apple.com, freedesktop.org). Never code native APIs from memory — signatures, flags and lifetime rules drift between versions.
2. Check `docs/architecture/overview.md` for the agreed technique in the area you touch (layout hooks, overlay, caret tracking, tray). If your approach differs from the documented one — stop and write/update an ADR first (skill `adr`).

## While writing

- `unsafe` is allowed only in `switcher-windows` / `switcher-macos` / `switcher-linux`. Every `unsafe` block gets a `// SAFETY:` comment naming the invariants that make it sound.
- Threading contracts (violations here are the #1 source of "works on my machine" bugs):
  - **Windows:** hooks live on a dedicated thread owning a hidden message-only window with a message pump; TSF / UI Automation COM on an STA thread; never block the pump; `SetWinEventHook` with `WINEVENT_OUTOFCONTEXT`.
  - **macOS:** TIS notifications require a thread running a `CFRunLoop` (results go stale without one); AppKit UI on the main thread only.
  - **Linux:** one thread owns the X connection; one thread owns the D-Bus connection.
- Cross thread boundaries only via channels carrying plain data events (`LayoutChanged`, `PointerMoved`, `CaretMoved`) — no handles, no callbacks.
- Every handle/hook/subscription is RAII-wrapped: unhook/close/unsubscribe in `Drop`.
- DPI: all Windows overlay code assumes Per-Monitor-V2 awareness; handle `WM_DPICHANGED` and monitor-crossing coordinate conversion explicitly.

## Verification

- The platform call must sit behind a `switcher-platform` trait; unit-test consumers against a mock implementation.
- Manual smoke checklist for Windows changes (record what you actually ran):
  - [ ] layout switch detected in a normal Win32 app, an elevated app, a UWP app, Windows Terminal
  - [ ] foreground-app switch updates the badge (layouts are per-thread on Windows)
  - [ ] overlay stays click-through and topmost; no focus steal (`WS_EX_NOACTIVATE`)
  - [ ] mixed-DPI monitor crossing: badge size/position correct on both monitors
  - [ ] idle: 0% CPU, no timers running while overlay hidden (check Task Manager / ETW)
