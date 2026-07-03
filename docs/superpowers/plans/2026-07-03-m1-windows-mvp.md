# M1 — Windows MVP: план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Цель:** резидентная утилита для Windows 11 — транзиентный бейдж «RU/EN» у курсора при смене раскладки, звуковая подсказка, трей, автозапуск, TOML-конфиг; ~0% CPU в простое.

**Архитектура:** cargo workspace c портами/адаптерами (ADR-0002): чистое ядро-автомат `(state, event, now) -> effects` в `switcher-core`; трейты в `switcher-platform`; весь unsafe Win32 — в `switcher-windows`; сборка и wiring — в `switcher-app`. Всё событийно (ADR-0003), оверлей — сырое layered-окно (ADR-0004).

**Стек:** Rust stable, windows-rs, tray-icon + muda, rodio, tiny-skia + ab_glyph, serde + toml, crossbeam-channel, tracing.

## Глобальные ограничения

Требования каждой задачи неявно включают этот раздел.

- **Качество:** перед каждым коммитом чисто проходят `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (скил `quality-gates`).
- **Unsafe:** только в `crates/switcher-windows`; каждый `unsafe`-блок несёт `// SAFETY:`-комментарий с инвариантами. В `switcher-core` и `switcher-platform` — `#![forbid(unsafe_code)]`.
- **Нулевой поллинг (ADR-0003):** никаких таймеров/опросов, кроме двух взводимых исключений: (1) фолбэк-опрос раскладки 500 мс — только пока передний план elevated/UWP/консоль; (2) Raw Input курсора — только пока бейдж видим. Таймер скрытия бейджа — единственный таймер ядра, живёт как deadline в `recv_timeout` цикла рантайма.
- **Потоки:** каждый Win32-хук — свой поток со скрытым окном и циклом сообщений; TSF — отдельный STA-поток; ядро — выделенный поток с блокирующим `recv`; главный поток — трей + его цикл сообщений. Через границы потоков — только плоские данные по каналам crossbeam (никаких HWND/колбэков).
- **Native API:** код windows-rs в задачах уже верифицирован по vendor-документации (workflow `m1-api-research`, 2026-07-03); при любом отклонении от приведённых сигнатур исполнитель обязан перепроверить контракт по context7/learn.microsoft.com (скил `platform-api-work`).
- **Язык:** код, идентификаторы, комментарии, коммиты — английский; docs/ — русский.
- **Коммиты:** после каждой задачи; сообщение в стиле `feat(scope): ...`/`test(scope): ...`/`chore: ...`.

## Карта файлов

```
lang-switcher/
├── Cargo.toml                          # workspace: members, workspace.package, workspace.dependencies, lints
├── .gitignore                          # target/
├── crates/
│   ├── switcher-core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  # forbid(unsafe), pub mod config|content|engine
│   │       ├── config.rs               # Config: serde-модель, defaults, версия схемы, validate/clamp
│   │       ├── content.rs              # BadgeContent::for_lang, палитра, SoundCue::for_lang
│   │       └── engine.rs               # Engine: (state, Event, now_ms) -> Vec<Effect>; вся оркестрация
│   ├── switcher-platform/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  # forbid(unsafe), pub mod events|ports
│   │       ├── events.rs               # Point, LayoutId, LangTag, LayoutSource, BadgeImage, Placement
│   │       └── ports.rs                # трейты LayoutMonitor, PointerTracker, CaretLocator, OverlayWindow, Autostart, SoundPlayer, PlatformError
│   ├── switcher-windows/
│   │   ├── Cargo.toml                  # только target.'cfg(windows)' зависимости
│   │   ├── build.rs                    # embed-manifest: PMv2 DPI awareness
│   │   ├── examples/
│   │   │   ├── overlay_smoke.rs        # риск №1: бейдж следует за курсором, mixed-DPI
│   │   │   └── layout_smoke.rs         # риск №2: печать событий 3 источников
│   │   └── src/
│   │       ├── lib.rs                  # pub mod overlay|layout_monitor|pointer|tsf|autostart|win_util
│   │       ├── win_util.rs             # HiddenWindow (RAII), hook-поток с циклом сообщений, wide-строки
│   │       ├── overlay.rs              # layered-окно: show/move/hide/scale_at, поток оверлея
│   │       ├── pointer.rs              # Raw Input: arm/disarm, коалесценция, cursor_pos()
│   │       ├── layout_monitor.rs       # shell hook + WinEvent(FOREGROUND) + взводимый фолбэк-опрос
│   │       ├── tsf.rs                  # STA-поток, TSF-sink (источник №3)
│   │       └── autostart.rs            # HKCU\...\Run
│   └── switcher-app/
│       ├── Cargo.toml                  # bin; адаптеры по #[cfg(target_os)]
│       ├── assets/fonts/<шрифт>.ttf    # встроенный OFL-шрифт для бейджа (см. Задачу 9)
│       └── src/
│           ├── main.rs                 # сборка: конфиг, логи, потоки, трей, цикл ядра
│           ├── paths.rs                # directories: config/data пути
│           ├── logging.rs              # tracing → файл в data dir
│           ├── render.rs               # растеризация бейджа (tiny-skia + ab_glyph) — чистая, тестируемая
│           ├── sound.rs                # rodio: синтез двух кью, SoundPlayer impl
│           ├── tray.rs                 # tray-icon + muda: меню, обновление
│           └── runtime.rs              # цикл ядра: recv_timeout-deadline, диспетчер Effect → порты
└── docs/smoke/m1-windows.md            # ручной smoke-чеклист (заполняется задачами 10–15, 18–19)
```

Направление зависимостей: `switcher-platform` ← `switcher-core`, ← `switcher-windows`, ← `switcher-app`; `switcher-app` → все. Адаптеры не знают про ядро; ядро не знает про ОС.

---

### Задача 1: каркас workspace

**Файлы:**
- Создать: `Cargo.toml`, `.gitignore`
- Создать: `crates/switcher-core/{Cargo.toml,src/lib.rs}`
- Создать: `crates/switcher-platform/{Cargo.toml,src/lib.rs}`
- Создать: `crates/switcher-windows/{Cargo.toml,src/lib.rs}`
- Создать: `crates/switcher-app/{Cargo.toml,src/main.rs}`

**Интерфейсы:**
- Потребляет: —
- Производит: собирающийся пустой workspace; все последующие задачи добавляют код в эти крейты и берут версии зависимостей из `[workspace.dependencies]`.

Крейты `switcher-macos`/`-linux` НЕ создаются — они появятся в M3/M4 (YAGNI).

- [ ] **Шаг 1: создать файлы**

`.gitignore`:

```gitignore
/target
```

`Cargo.toml` (корень):

```toml
[workspace]
resolver = "3"
members = ["crates/*"]

[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "%%MSRV%%"
license = "MIT OR Apache-2.0"

[workspace.dependencies]
switcher-core = { path = "crates/switcher-core" }
switcher-platform = { path = "crates/switcher-platform" }
switcher-windows = { path = "crates/switcher-windows" }

serde = { version = "%%SERDE%%", features = ["derive"] }
toml = "%%TOML%%"
thiserror = "%%THISERROR%%"
anyhow = "%%ANYHOW%%"
crossbeam-channel = "%%CROSSBEAM%%"
tracing = "%%TRACING%%"
tracing-subscriber = { version = "%%TRACING_SUB%%", features = ["env-filter"] }
tracing-appender = "%%TRACING_APP%%"
directories = "%%DIRECTORIES%%"
tiny-skia = "%%TINY_SKIA%%"
ab_glyph = "%%AB_GLYPH%%"
rodio = "%%RODIO%%"
tray-icon = "%%TRAY_ICON%%"
muda = "%%MUDA%%"
windows = "%%WINDOWS%%"
embed-manifest = "%%EMBED_MANIFEST%%"

[workspace.lints.rust]
unsafe_op_in_unsafe_fn = "deny"
missing_debug_implementations = "warn"

[workspace.lints.clippy]
undocumented_unsafe_blocks = "deny"
```

`crates/switcher-platform/Cargo.toml`:

```toml
[package]
name = "switcher-platform"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
thiserror = { workspace = true }

[lints]
workspace = true
```

`crates/switcher-platform/src/lib.rs`:

```rust
#![forbid(unsafe_code)]
//! Ports (traits) and flat event/data types shared by the core, adapters and the app shell.
```

`crates/switcher-core/Cargo.toml`:

```toml
[package]
name = "switcher-core"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
switcher-platform = { workspace = true }
serde = { workspace = true }
toml = { workspace = true }
thiserror = { workspace = true }

[lints]
workspace = true
```

`crates/switcher-core/src/lib.rs`:

```rust
#![forbid(unsafe_code)]
//! OS-independent domain: layout state machine, badge content, config model.
```

`crates/switcher-windows/Cargo.toml` (фичи `windows` уточняются задачами 11–16; стартовый набор ниже собирается):

```toml
[package]
name = "switcher-windows"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[target.'cfg(windows)'.dependencies]
switcher-platform = { workspace = true }
crossbeam-channel = { workspace = true }
tracing = { workspace = true }
windows = { workspace = true, features = [
    "Win32_Foundation",
    "Win32_UI_WindowsAndMessaging",
] }

[lints]
workspace = true
```

`crates/switcher-windows/src/lib.rs`:

```rust
//! Win32 adapters. The only crate (besides future -macos/-linux) where `unsafe` is allowed;
//! every unsafe block carries a `// SAFETY:` comment (workspace lint enforces it).
#![cfg(windows)]
```

`crates/switcher-app/Cargo.toml`:

```toml
[package]
name = "switcher-app"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[[bin]]
name = "lang-switcher"
path = "src/main.rs"

[dependencies]
switcher-core = { workspace = true }
switcher-platform = { workspace = true }
anyhow = { workspace = true }
crossbeam-channel = { workspace = true }
serde = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
tracing-appender = { workspace = true }
directories = { workspace = true }
tiny-skia = { workspace = true }
ab_glyph = { workspace = true }
rodio = { workspace = true }
tray-icon = { workspace = true }
muda = { workspace = true }

[target.'cfg(windows)'.dependencies]
switcher-windows = { workspace = true }

[lints]
workspace = true
```

`crates/switcher-app/src/main.rs`:

```rust
fn main() {}
```

- [ ] **Шаг 2: проверить сборку**

Запустить: `cargo build --workspace`
Ожидание: успех, без предупреждений.

- [ ] **Шаг 3: линт и формат**

Запустить: `cargo clippy --workspace --all-targets -- -D warnings`, затем `cargo fmt --all -- --check`
Ожидание: оба чистые.

- [ ] **Шаг 4: коммит**

```bash
git add -A
git commit -m "chore: scaffold cargo workspace (core, platform, windows, app)"
```

---

### Задача 2: switcher-platform — типы событий и порты

**Файлы:**
- Создать: `crates/switcher-platform/src/events.rs`
- Создать: `crates/switcher-platform/src/ports.rs`
- Изменить: `crates/switcher-platform/src/lib.rs`
- Тесты: unit-тесты внутри `events.rs`

**Интерфейсы:**
- Потребляет: —
- Производит (используют ВСЕ последующие задачи):
  - `events::Point { x: i32, y: i32 }`, `events::LayoutId(pub u64)`, `events::LangTag` (`new(impl Into<String>)`, `as_str() -> &str`, `primary() -> String`),
  - `events::LayoutSource { ShellHook, ForegroundChange, ForegroundPoll, Tsf, Initial }`,
  - `events::BadgeImage { width: u32, height: u32, rgba_premul: Vec<u8> }`, `events::Placement { At(Point), PrimaryBottomRight }`,
  - `events::PlatformEvent { LayoutChanged { layout, lang, source }, PointerMoved { pos }, OverlayScaleChanged { dpi } }`,
  - `ports::PlatformError(pub String)`, `ports::SoundCue { Ru, En, Neutral }`,
  - трейты: `LayoutMonitor::current() -> Result<(LayoutId, LangTag), PlatformError>`; `PointerTracker::{set_active(&self, bool), cursor_pos(&self) -> Option<Point>}`; `CaretLocator::caret_point(&self) -> Option<Point>`; `OverlayWindow::{show(&self, &BadgeImage, Placement), move_to(&self, Point), hide(&self), dpi_at(&self, Point) -> u32}`; `Autostart::{is_enabled(&self) -> Result<bool, PlatformError>, set_enabled(&self, bool) -> Result<(), PlatformError>}`; `SoundPlayer::play(&self, SoundCue, volume: f32)`; все трейты `: Send`,
  - `ports::NullCaretLocator` (всегда `None`).

- [ ] **Шаг 1: написать падающий тест на `LangTag::primary`**

В конец `crates/switcher-platform/src/events.rs` (файл создаётся сразу с тестом и заглушкой типа):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lang_tag_primary_extracts_lowercase_primary_subtag() {
        assert_eq!(LangTag::new("ru-RU").primary(), "ru");
        assert_eq!(LangTag::new("EN_us").primary(), "en");
        assert_eq!(LangTag::new("de").primary(), "de");
        assert_eq!(LangTag::new("").primary(), "");
    }
}
```

Временная заглушка над тестами, чтобы файл компилировался, но тест падал:

```rust
/// Language tag like "ru-RU".
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LangTag(String);

impl LangTag {
    pub fn new(tag: impl Into<String>) -> Self {
        Self(tag.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn primary(&self) -> String {
        String::new() // deliberately wrong: TDD red step
    }
}
```

В `lib.rs` добавить:

```rust
pub mod events;
```

- [ ] **Шаг 2: убедиться, что тест падает**

Запустить: `cargo test -p switcher-platform`
Ожидание: FAIL `lang_tag_primary_extracts_lowercase_primary_subtag` (пустая строка вместо "ru").

- [ ] **Шаг 3: минимальная реализация**

Заменить тело `primary`:

```rust
    pub fn primary(&self) -> String {
        self.0
            .split(['-', '_'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
    }
```

- [ ] **Шаг 4: убедиться, что тест проходит**

Запустить: `cargo test -p switcher-platform`
Ожидание: PASS (1 passed).

- [ ] **Шаг 5: дописать остальные типы и порты (декларации, проверяются компилятором)**

`crates/switcher-platform/src/events.rs` — добавить над тестами:

```rust
/// Screen coordinate in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// Opaque platform identity of a keyboard layout (the HKL value on Windows).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LayoutId(pub u64);

/// Which OS mechanism reported a layout change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutSource {
    ShellHook,
    ForegroundChange,
    ForegroundPoll,
    Tsf,
    /// One-shot read at startup: updates the tray but never shows a badge or plays a sound.
    Initial,
}

/// Premultiplied-alpha RGBA image, row-major, top-down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadgeImage {
    pub width: u32,
    pub height: u32,
    pub rgba_premul: Vec<u8>,
}

/// Where the overlay places the badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// Top-left corner of the badge, physical pixels.
    At(Point),
    /// Fixed fallback anchor: bottom-right of the primary monitor with a margin.
    PrimaryBottomRight,
}

/// Flat events adapters push into the app channel. Data only — no handles, no callbacks.
#[derive(Debug, Clone, PartialEq)]
pub enum PlatformEvent {
    LayoutChanged {
        layout: LayoutId,
        lang: LangTag,
        source: LayoutSource,
    },
    PointerMoved {
        pos: Point,
    },
    /// DPI under the visible badge changed (monitor crossing or WM_DPICHANGED).
    OverlayScaleChanged {
        dpi: u32,
    },
}
```

`crates/switcher-platform/src/ports.rs` (новый файл):

```rust
//! Ports: the only surface adapters expose to the app shell. All handles are `Send`
//! because effects are dispatched from the core loop thread; implementations forward
//! calls to their owning threads (e.g. via channels + window messages) internally.

use crate::events::{BadgeImage, LangTag, LayoutId, Placement, Point};

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct PlatformError(pub String);

/// Streams `PlatformEvent::LayoutChanged` into the channel supplied at construction.
pub trait LayoutMonitor: Send {
    /// One-shot read of the current layout (used at startup for the Initial event).
    fn current(&self) -> Result<(LayoutId, LangTag), PlatformError>;
}

/// Streams `PlatformEvent::PointerMoved` while armed. Disarmed by default: zero idle cost.
pub trait PointerTracker: Send {
    fn set_active(&self, active: bool);
    /// One-shot cursor position for anchor resolution.
    fn cursor_pos(&self) -> Option<Point>;
}

/// Best-effort caret position; always allowed to return `None`.
pub trait CaretLocator: Send {
    fn caret_point(&self) -> Option<Point>;
}

/// Click-through, topmost, non-activating badge window.
pub trait OverlayWindow: Send {
    fn show(&self, image: &BadgeImage, placement: Placement);
    fn move_to(&self, pos: Point);
    fn hide(&self);
    /// Effective DPI at a screen point (96 = 100%).
    fn dpi_at(&self, pos: Point) -> u32;
}

pub trait Autostart: Send {
    fn is_enabled(&self) -> Result<bool, PlatformError>;
    fn set_enabled(&self, enabled: bool) -> Result<(), PlatformError>;
}

/// Which cue to play on a layout switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundCue {
    Ru,
    En,
    Neutral,
}

pub trait SoundPlayer: Send {
    fn play(&self, cue: SoundCue, volume: f32);
}

/// M1 caret stub: caret anchoring arrives in M2 (progressive enhancement, never promised).
#[derive(Debug, Clone, Copy, Default)]
pub struct NullCaretLocator;

impl CaretLocator for NullCaretLocator {
    fn caret_point(&self) -> Option<Point> {
        None
    }
}
```

`crates/switcher-platform/src/lib.rs` — итоговое содержимое:

```rust
#![forbid(unsafe_code)]
//! Ports (traits) and flat event/data types shared by the core, adapters and the app shell.

pub mod events;
pub mod ports;
```

- [ ] **Шаг 6: прогнать гейты**

Запустить: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
Ожидание: всё чисто.

- [ ] **Шаг 7: коммит**

```bash
git add crates/switcher-platform
git commit -m "feat(platform): event types and adapter ports"
```

---

### Задача 3: switcher-core — контент бейджа и звуковой кью

**Файлы:**
- Создать: `crates/switcher-core/src/content.rs`
- Изменить: `crates/switcher-core/src/lib.rs`
- Тесты: unit-тесты внутри `content.rs`

**Интерфейсы:**
- Потребляет: `switcher_platform::events::LangTag`, `switcher_platform::ports::SoundCue` (Задача 2).
- Производит (используют задачи 4, 5–7, 9):
  - `content::BadgeStyle { Text, Color }` (serde, snake_case),
  - `content::Rgb8 { r: u8, g: u8, b: u8 }`, `content::BADGE_FG: Rgb8`,
  - `content::parse_hex_rgb(&str) -> Option<Rgb8>` — строго `#RRGGBB`,
  - `content::BadgeContent { label: String, bg: Rgb8, fg: Rgb8, style: BadgeStyle }` и `BadgeContent::for_lang(lang: &LangTag, style: BadgeStyle, colors: &BTreeMap<String, String>) -> BadgeContent`,
  - `content::cue_for(lang: &LangTag) -> SoundCue`.

- [ ] **Шаг 1: написать падающие тесты**

`crates/switcher-core/src/content.rs` (новый файл — сразу весь тестовый модуль; выше него временно только объявления-заглушки из Шага 3 с телом `todo!()` у функций):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use switcher_platform::events::LangTag;
    use switcher_platform::ports::SoundCue;

    #[test]
    fn parse_hex_rgb_accepts_only_hash_rrggbb() {
        assert_eq!(
            parse_hex_rgb("#D64545"),
            Some(Rgb8 { r: 0xD6, g: 0x45, b: 0x45 })
        );
        assert_eq!(
            parse_hex_rgb("#d64545"),
            Some(Rgb8 { r: 0xD6, g: 0x45, b: 0x45 })
        );
        assert_eq!(parse_hex_rgb("D64545"), None);
        assert_eq!(parse_hex_rgb("#D6454"), None);
        assert_eq!(parse_hex_rgb("#D645451"), None);
        assert_eq!(parse_hex_rgb("#GGGGGG"), None);
        assert_eq!(parse_hex_rgb(""), None);
    }

    #[test]
    fn known_languages_get_labels_and_builtin_palette() {
        let ru = BadgeContent::for_lang(&LangTag::new("ru-RU"), BadgeStyle::Text, &BTreeMap::new());
        assert_eq!(ru.label, "RU");
        assert_eq!(ru.bg, Rgb8 { r: 0xD6, g: 0x45, b: 0x45 });
        assert_eq!(ru.fg, BADGE_FG);

        let en = BadgeContent::for_lang(&LangTag::new("en-US"), BadgeStyle::Text, &BTreeMap::new());
        assert_eq!(en.label, "EN");
        assert_eq!(en.bg, Rgb8 { r: 0x3D, g: 0x6F, b: 0xD9 });
    }

    #[test]
    fn config_color_overrides_builtin_palette() {
        let mut colors = BTreeMap::new();
        colors.insert("ru".to_owned(), "#112233".to_owned());
        let ru = BadgeContent::for_lang(&LangTag::new("ru-RU"), BadgeStyle::Color, &colors);
        assert_eq!(ru.bg, Rgb8 { r: 0x11, g: 0x22, b: 0x33 });
        assert_eq!(ru.style, BadgeStyle::Color);
    }

    #[test]
    fn invalid_config_color_falls_back_to_builtin() {
        let mut colors = BTreeMap::new();
        colors.insert("ru".to_owned(), "not-a-color".to_owned());
        let ru = BadgeContent::for_lang(&LangTag::new("ru-RU"), BadgeStyle::Text, &colors);
        assert_eq!(ru.bg, Rgb8 { r: 0xD6, g: 0x45, b: 0x45 });
    }

    #[test]
    fn unknown_language_gets_uppercased_two_letter_label_and_fallback_bg() {
        let de = BadgeContent::for_lang(&LangTag::new("de-DE"), BadgeStyle::Text, &BTreeMap::new());
        assert_eq!(de.label, "DE");
        assert_eq!(de.bg, Rgb8 { r: 0x66, g: 0x66, b: 0x66 });

        let empty = BadgeContent::for_lang(&LangTag::new(""), BadgeStyle::Text, &BTreeMap::new());
        assert_eq!(empty.label, "??");
    }

    #[test]
    fn cue_follows_primary_language() {
        assert_eq!(cue_for(&LangTag::new("ru-RU")), SoundCue::Ru);
        assert_eq!(cue_for(&LangTag::new("en-GB")), SoundCue::En);
        assert_eq!(cue_for(&LangTag::new("de-DE")), SoundCue::Neutral);
    }
}
```

В `lib.rs` добавить `pub mod content;`.

- [ ] **Шаг 2: убедиться, что тесты падают**

Запустить: `cargo test -p switcher-core`
Ожидание: FAIL (паники `todo!()` в заглушках).

- [ ] **Шаг 3: реализация**

Содержимое `content.rs` над тестовым модулем:

```rust
//! What the badge shows for a given language: label, colors, sound cue.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use switcher_platform::events::LangTag;
use switcher_platform::ports::SoundCue;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BadgeStyle {
    Text,
    Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

pub const BADGE_FG: Rgb8 = Rgb8 { r: 0xFF, g: 0xFF, b: 0xFF };
const RU_BG: Rgb8 = Rgb8 { r: 0xD6, g: 0x45, b: 0x45 };
const EN_BG: Rgb8 = Rgb8 { r: 0x3D, g: 0x6F, b: 0xD9 };
const FALLBACK_BG: Rgb8 = Rgb8 { r: 0x66, g: 0x66, b: 0x66 };

/// Parses "#RRGGBB" (case-insensitive). Anything else is `None`.
pub fn parse_hex_rgb(s: &str) -> Option<Rgb8> {
    let hex = s.strip_prefix('#')?;
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let n = u32::from_str_radix(hex, 16).ok()?;
    Some(Rgb8 {
        r: (n >> 16) as u8,
        g: (n >> 8) as u8,
        b: n as u8,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct BadgeContent {
    pub label: String,
    pub bg: Rgb8,
    pub fg: Rgb8,
    pub style: BadgeStyle,
}

impl BadgeContent {
    /// Config colors win over the built-in palette; unknown languages get
    /// a two-letter uppercase label on a neutral background.
    pub fn for_lang(lang: &LangTag, style: BadgeStyle, colors: &BTreeMap<String, String>) -> Self {
        let primary = lang.primary();
        let label = if primary.is_empty() {
            "??".to_owned()
        } else {
            primary.chars().take(2).collect::<String>().to_ascii_uppercase()
        };
        let bg = colors
            .get(&primary)
            .and_then(|hex| parse_hex_rgb(hex))
            .unwrap_or(match primary.as_str() {
                "ru" => RU_BG,
                "en" => EN_BG,
                _ => FALLBACK_BG,
            });
        Self {
            label,
            bg,
            fg: BADGE_FG,
            style,
        }
    }
}

/// Distinct cues for the two languages the product is about; neutral for the rest.
pub fn cue_for(lang: &LangTag) -> SoundCue {
    match lang.primary().as_str() {
        "ru" => SoundCue::Ru,
        "en" => SoundCue::En,
        _ => SoundCue::Neutral,
    }
}
```

- [ ] **Шаг 4: убедиться, что тесты проходят**

Запустить: `cargo test -p switcher-core`
Ожидание: PASS (6 passed).

- [ ] **Шаг 5: гейты и коммит**

Запустить: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
Ожидание: чисто.

```bash
git add crates/switcher-core
git commit -m "feat(core): badge content mapping and sound cue selection"
```

---

### Задача 4: switcher-core — модель конфига

**Файлы:**
- Создать: `crates/switcher-core/src/config.rs`
- Изменить: `crates/switcher-core/src/lib.rs`
- Тесты: unit-тесты внутри `config.rs`

**Интерфейсы:**
- Потребляет: `content::{parse_hex_rgb, BadgeStyle}` (Задача 3).
- Производит (используют задачи 5–7, 9–10, 17–19):
  - `config::CONFIG_VERSION: u32 = 1`, `config::{MIN_SHOW_MS, MAX_SHOW_MS, DEFAULT_SHOW_MS}`,
  - `config::BadgeMode { Transient, Follow }`, `config::AnchorPref { Auto, Cursor, Fixed }` (serde, snake_case),
  - `config::BadgeConfig { mode, anchor, style: BadgeStyle, show_ms: u64, colors: BTreeMap<String, String> }`,
  - `config::SoundConfig { enabled: bool, volume: f32 }`,
  - `config::Config { version, badge, sound, autostart: bool, log_level: String, ui_language: String }` + `Default`,
  - `Config::from_toml_str(&str) -> Result<(Config, Vec<String>), ConfigError>` (warnings — что пришлось поправить), `Config::to_toml_string(&self) -> String`,
  - `config::ConfigError { Parse(String), TooNew { found, supported } }`.

Дефолты: mode=transient, anchor=auto, style=text, show_ms=1500, colors={}, sound.enabled=true, volume=0.4, autostart=false, log_level="info", ui_language="ru". Поле `badge.style` принимает только `text|color`: вид «флаг» — M2, войдёт миграцией схемы (v2), НЕ добавлять сейчас.

- [ ] **Шаг 1: написать падающие тесты**

Тестовый модуль `config.rs` (выше — заглушки из Шага 3 с `todo!()` в телах `from_toml_str`/`to_toml_string`/`sanitize`):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_matches_spec() {
        let cfg = Config::default();
        assert_eq!(cfg.version, CONFIG_VERSION);
        assert_eq!(cfg.badge.mode, BadgeMode::Transient);
        assert_eq!(cfg.badge.anchor, AnchorPref::Auto);
        assert_eq!(cfg.badge.style, crate::content::BadgeStyle::Text);
        assert_eq!(cfg.badge.show_ms, DEFAULT_SHOW_MS);
        assert!(cfg.badge.colors.is_empty());
        assert!(cfg.sound.enabled);
        assert_eq!(cfg.sound.volume, 0.4);
        assert!(!cfg.autostart);
        assert_eq!(cfg.log_level, "info");
        assert_eq!(cfg.ui_language, "ru");
    }

    #[test]
    fn empty_and_partial_toml_fill_defaults() {
        let (cfg, warnings) = Config::from_toml_str("").unwrap();
        assert_eq!(cfg, Config::default());
        assert!(warnings.is_empty());

        let (cfg, warnings) = Config::from_toml_str("[badge]\nmode = \"follow\"\n").unwrap();
        assert_eq!(cfg.badge.mode, BadgeMode::Follow);
        assert_eq!(cfg.badge.show_ms, DEFAULT_SHOW_MS);
        assert!(warnings.is_empty());
    }

    #[test]
    fn roundtrip_preserves_config() {
        let mut cfg = Config::default();
        cfg.badge.mode = BadgeMode::Follow;
        cfg.badge.anchor = AnchorPref::Cursor;
        cfg.badge.show_ms = 900;
        cfg.badge
            .colors
            .insert("ru".to_owned(), "#112233".to_owned());
        cfg.sound.volume = 0.75;
        cfg.autostart = true;
        let (parsed, warnings) = Config::from_toml_str(&cfg.to_toml_string()).unwrap();
        assert_eq!(parsed, cfg);
        assert!(warnings.is_empty());
    }

    #[test]
    fn newer_schema_version_is_rejected() {
        let err = Config::from_toml_str("version = 99\n").unwrap_err();
        assert!(matches!(
            err,
            ConfigError::TooNew {
                found: 99,
                supported: CONFIG_VERSION
            }
        ));
    }

    #[test]
    fn malformed_toml_is_a_parse_error() {
        assert!(matches!(
            Config::from_toml_str("badge = {"),
            Err(ConfigError::Parse(_))
        ));
    }

    #[test]
    fn out_of_range_values_are_clamped_with_warnings() {
        let text = "[badge]\nshow_ms = 50\n[sound]\nvolume = 3.5\n";
        let (cfg, warnings) = Config::from_toml_str(text).unwrap();
        assert_eq!(cfg.badge.show_ms, MIN_SHOW_MS);
        assert_eq!(cfg.sound.volume, 1.0);
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn non_finite_volume_resets_to_default() {
        let (cfg, warnings) = Config::from_toml_str("[sound]\nvolume = nan\n").unwrap();
        assert_eq!(cfg.sound.volume, 0.4);
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn invalid_badge_colors_are_removed_with_warning() {
        let text = "[badge.colors]\nru = \"#112233\"\nen = \"red\"\n";
        let (cfg, warnings) = Config::from_toml_str(text).unwrap();
        assert_eq!(cfg.badge.colors.get("ru").map(String::as_str), Some("#112233"));
        assert!(!cfg.badge.colors.contains_key("en"));
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let (cfg, _) = Config::from_toml_str("future_field = 42\n").unwrap();
        assert_eq!(cfg, Config::default());
    }
}
```

В `lib.rs` добавить `pub mod config;`.

- [ ] **Шаг 2: убедиться, что тесты падают**

Запустить: `cargo test -p switcher-core`
Ожидание: FAIL (новые тесты падают на `todo!()`; тесты Задачи 3 проходят).

- [ ] **Шаг 3: реализация**

Содержимое `config.rs` над тестами:

```rust
//! Config model: schema v1. Parsing is tolerant (missing fields -> defaults, unknown
//! fields ignored); sanitize() clamps bad values and reports what it fixed. Schema
//! migrations will live in from_toml_str as `if cfg.version < N` blocks.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::content::{parse_hex_rgb, BadgeStyle};

pub const CONFIG_VERSION: u32 = 1;
pub const MIN_SHOW_MS: u64 = 200;
pub const MAX_SHOW_MS: u64 = 10_000;
pub const DEFAULT_SHOW_MS: u64 = 1_500;
const DEFAULT_VOLUME: f32 = 0.4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BadgeMode {
    Transient,
    Follow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnchorPref {
    Auto,
    Cursor,
    Fixed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BadgeConfig {
    pub mode: BadgeMode,
    pub anchor: AnchorPref,
    pub style: BadgeStyle,
    pub show_ms: u64,
    /// Primary language subtag -> "#RRGGBB" badge background override.
    pub colors: BTreeMap<String, String>,
}

impl Default for BadgeConfig {
    fn default() -> Self {
        Self {
            mode: BadgeMode::Transient,
            anchor: AnchorPref::Auto,
            style: BadgeStyle::Text,
            show_ms: DEFAULT_SHOW_MS,
            colors: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SoundConfig {
    pub enabled: bool,
    pub volume: f32,
}

impl Default for SoundConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            volume: DEFAULT_VOLUME,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    pub badge: BadgeConfig,
    pub sound: SoundConfig,
    pub autostart: bool,
    pub log_level: String,
    pub ui_language: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            badge: BadgeConfig::default(),
            sound: SoundConfig::default(),
            autostart: false,
            log_level: "info".to_owned(),
            ui_language: "ru".to_owned(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to parse config: {0}")]
    Parse(String),
    #[error("config version {found} is newer than supported {supported}")]
    TooNew { found: u32, supported: u32 },
}

impl Config {
    /// Parses TOML, migrates old schema versions, clamps invalid values.
    /// Returns the config plus a human-readable warning per fixed value.
    pub fn from_toml_str(text: &str) -> Result<(Self, Vec<String>), ConfigError> {
        let mut cfg: Config =
            toml::from_str(text).map_err(|e| ConfigError::Parse(e.to_string()))?;
        if cfg.version > CONFIG_VERSION {
            return Err(ConfigError::TooNew {
                found: cfg.version,
                supported: CONFIG_VERSION,
            });
        }
        cfg.version = CONFIG_VERSION;
        let warnings = cfg.sanitize();
        Ok((cfg, warnings))
    }

    pub fn to_toml_string(&self) -> String {
        toml::to_string_pretty(self).expect("config model always serializes")
    }

    fn sanitize(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();
        if !(MIN_SHOW_MS..=MAX_SHOW_MS).contains(&self.badge.show_ms) {
            warnings.push(format!(
                "badge.show_ms {} out of range {MIN_SHOW_MS}..={MAX_SHOW_MS}, clamped",
                self.badge.show_ms
            ));
            self.badge.show_ms = self.badge.show_ms.clamp(MIN_SHOW_MS, MAX_SHOW_MS);
        }
        if !self.sound.volume.is_finite() {
            warnings.push("sound.volume is not a number, reset to default".to_owned());
            self.sound.volume = DEFAULT_VOLUME;
        } else if !(0.0..=1.0).contains(&self.sound.volume) {
            warnings.push(format!(
                "sound.volume {} out of range 0..=1, clamped",
                self.sound.volume
            ));
            self.sound.volume = self.sound.volume.clamp(0.0, 1.0);
        }
        let invalid: Vec<String> = self
            .badge
            .colors
            .iter()
            .filter(|(_, hex)| parse_hex_rgb(hex).is_none())
            .map(|(lang, _)| lang.clone())
            .collect();
        for lang in invalid {
            self.badge.colors.remove(&lang);
            warnings.push(format!("badge.colors.{lang}: invalid #RRGGBB value removed"));
        }
        warnings
    }
}
```

- [ ] **Шаг 4: убедиться, что тесты проходят**

Запустить: `cargo test -p switcher-core`
Ожидание: PASS (тесты задач 3 и 4).

- [ ] **Шаг 5: гейты и коммит**

Запустить: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
Ожидание: чисто.

```bash
git add crates/switcher-core
git commit -m "feat(core): TOML config model with defaults, clamping and schema version"
```

---

### Задача 5: switcher-core — engine: приём смены раскладки и дедупликация

**Файлы:**
- Создать: `crates/switcher-core/src/engine.rs`
- Изменить: `crates/switcher-core/src/lib.rs`
- Тесты: unit-тесты внутри `engine.rs`

**Интерфейсы:**
- Потребляет: `content::{cue_for, BadgeContent}` (Задача 3), `config::{BadgeMode, AnchorPref, Config}` (Задача 4), типы Задачи 2.
- Производит (контракты стабильны для задач 6, 7, 19):
  - `engine::STALE_ECHO_WINDOW_MS: u64 = 150`,
  - `engine::Event { Layout { layout: LayoutId, lang: LangTag, source: LayoutSource }, AnchorResolved { caret: Option<Point>, cursor: Option<Point> }, Pointer { pos: Point }, HideTimerFired, SetMode(BadgeMode), SetSoundEnabled(bool), SetAutostart(bool) }`,
  - `engine::ResolvedAnchor { Caret(Point), Cursor(Point), Fixed }`,
  - `engine::Effect { QueryAnchor, ShowBadge { content: BadgeContent, anchor: ResolvedAnchor }, MoveBadge { pos: Point }, HideBadge, ArmHideTimer { after_ms: u64 }, CancelHideTimer, SetPointerTracking(bool), PlaySound { cue: SoundCue, volume: f32 }, UpdateTray { label: String, lang: LangTag }, ApplyAutostart(bool), PersistConfig }`,
  - `engine::Engine`: `new(Config) -> Engine`, `config(&self) -> &Config`, `handle(&mut self, Event, now_ms: u64) -> Vec<Effect>`.

Семантика приёма события раскладки (правила ядра из спеки):
1. Тот же `LayoutId`, что текущий, — игнор (это же схлопывает дубли от трёх источников).
2. `LayoutId` равен предыдущему И `now_ms - last_change_ms < STALE_ECHO_WINDOW_MS` — «запоздавшее эхо» отставшего источника, игнор. (Осознанный компромисс: реальный двойной переклик туда-обратно быстрее 150 мс тоже съедается.)
3. Иначе — принять: обновить трей; для `source == Initial` на этом всё (ни бейджа, ни звука при старте); иначе — звук (если включён) и `QueryAnchor` (бейдж покажется в Задаче 6 по `AnchorResolved`).

- [ ] **Шаг 1: написать падающие тесты**

Тестовый модуль `engine.rs` (выше — код Шага 3, но с `todo!("task 5")` в теле `on_layout`; остальные ветки `handle` — `todo!("task 6")`/`todo!("task 7")` как в листинге):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use switcher_platform::events::{LangTag, LayoutId, LayoutSource, Point};
    use switcher_platform::ports::SoundCue;

    use crate::config::Config;
    use crate::content::{BadgeContent, BadgeStyle};

    pub(super) const RU_ID: LayoutId = LayoutId(0x0419_0419);
    pub(super) const EN_ID: LayoutId = LayoutId(0x0409_0409);

    pub(super) fn ru() -> LangTag {
        LangTag::new("ru-RU")
    }

    pub(super) fn en() -> LangTag {
        LangTag::new("en-US")
    }

    pub(super) fn layout(id: LayoutId, lang: LangTag, source: LayoutSource) -> Event {
        Event::Layout {
            layout: id,
            lang,
            source,
        }
    }

    pub(super) fn default_content(lang: &LangTag) -> BadgeContent {
        BadgeContent::for_lang(lang, BadgeStyle::Text, &BTreeMap::new())
    }

    /// Engine already past startup: Initial EN at t=0.
    pub(super) fn engine_after_initial() -> Engine {
        let mut e = Engine::new(Config::default());
        e.handle(layout(EN_ID, en(), LayoutSource::Initial), 0);
        e
    }

    #[test]
    fn initial_layout_updates_tray_only() {
        let mut e = Engine::new(Config::default());
        let fx = e.handle(layout(EN_ID, en(), LayoutSource::Initial), 0);
        assert_eq!(
            fx,
            vec![Effect::UpdateTray {
                label: "EN".to_owned(),
                lang: en(),
            }]
        );
    }

    #[test]
    fn layout_change_updates_tray_plays_sound_and_queries_anchor() {
        let mut e = engine_after_initial();
        let fx = e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 1000);
        assert_eq!(
            fx,
            vec![
                Effect::UpdateTray {
                    label: "RU".to_owned(),
                    lang: ru(),
                },
                Effect::PlaySound {
                    cue: SoundCue::Ru,
                    volume: 0.4,
                },
                Effect::QueryAnchor,
            ]
        );
    }

    #[test]
    fn first_event_from_any_source_is_accepted() {
        let mut e = Engine::new(Config::default());
        let fx = e.handle(layout(RU_ID, ru(), LayoutSource::Tsf), 5);
        assert!(fx.contains(&Effect::QueryAnchor));
    }

    #[test]
    fn same_layout_from_another_source_is_deduplicated() {
        let mut e = engine_after_initial();
        e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 1000);
        let fx = e.handle(layout(RU_ID, ru(), LayoutSource::Tsf), 1020);
        assert_eq!(fx, vec![]);
        let fx = e.handle(layout(RU_ID, ru(), LayoutSource::ForegroundChange), 9000);
        assert_eq!(fx, vec![]);
    }

    #[test]
    fn stale_echo_of_previous_layout_within_window_is_ignored() {
        let mut e = engine_after_initial();
        e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 1000);
        let fx = e.handle(layout(EN_ID, en(), LayoutSource::ForegroundChange), 1100);
        assert_eq!(fx, vec![]);
    }

    #[test]
    fn toggle_back_after_stale_window_is_accepted() {
        let mut e = engine_after_initial();
        e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 1000);
        let fx = e.handle(layout(EN_ID, en(), LayoutSource::ShellHook), 1150);
        assert!(fx.contains(&Effect::QueryAnchor));
    }

    #[test]
    fn sound_disabled_suppresses_play_sound() {
        let mut cfg = Config::default();
        cfg.sound.enabled = false;
        let mut e = Engine::new(cfg);
        e.handle(layout(EN_ID, en(), LayoutSource::Initial), 0);
        let fx = e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 1000);
        assert!(!fx.iter().any(|f| matches!(f, Effect::PlaySound { .. })));
        assert!(fx.contains(&Effect::QueryAnchor));
    }
}
```

В `lib.rs` добавить `pub mod engine;`.

- [ ] **Шаг 2: убедиться, что тесты падают**

Запустить: `cargo test -p switcher-core`
Ожидание: FAIL — новые тесты падают на `todo!("task 5")`.

- [ ] **Шаг 3: реализация**

Содержимое `engine.rs` над тестами:

```rust
//! The core state machine: pure `(state, event, now) -> effects`. No OS calls, no
//! clocks, no channels — the runtime supplies `now_ms` and executes the effects.

use switcher_platform::events::{LangTag, LayoutId, LayoutSource, Point};
use switcher_platform::ports::SoundCue;

use crate::config::{AnchorPref, BadgeMode, Config};
use crate::content::{cue_for, BadgeContent};

/// A source reporting the layout we just switched AWAY from within this window
/// is treated as a stale echo of the same physical switch, not a new switch.
pub const STALE_ECHO_WINDOW_MS: u64 = 150;

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Layout {
        layout: LayoutId,
        lang: LangTag,
        source: LayoutSource,
    },
    /// Runtime's synchronous answer to `Effect::QueryAnchor`.
    AnchorResolved {
        caret: Option<Point>,
        cursor: Option<Point>,
    },
    Pointer {
        pos: Point,
    },
    HideTimerFired,
    SetMode(BadgeMode),
    SetSoundEnabled(bool),
    SetAutostart(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedAnchor {
    Caret(Point),
    Cursor(Point),
    Fixed,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Ask the runtime to query caret/cursor availability and feed back AnchorResolved.
    QueryAnchor,
    ShowBadge {
        content: BadgeContent,
        anchor: ResolvedAnchor,
    },
    /// New anchor position for the visible badge (runtime applies the offset).
    MoveBadge {
        pos: Point,
    },
    HideBadge,
    ArmHideTimer {
        after_ms: u64,
    },
    CancelHideTimer,
    SetPointerTracking(bool),
    PlaySound {
        cue: SoundCue,
        volume: f32,
    },
    UpdateTray {
        label: String,
        lang: LangTag,
    },
    ApplyAutostart(bool),
    /// Config changed: runtime saves it and re-syncs tray checkmarks.
    PersistConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BadgeState {
    Hidden,
    /// QueryAnchor issued, waiting for AnchorResolved.
    AwaitingAnchor,
    Visible {
        tracking: bool,
    },
}

#[derive(Debug)]
pub struct Engine {
    cfg: Config,
    layout: Option<(LayoutId, LangTag)>,
    prev_layout: Option<LayoutId>,
    last_change_ms: u64,
    badge: BadgeState,
}

impl Engine {
    pub fn new(cfg: Config) -> Self {
        Self {
            cfg,
            layout: None,
            prev_layout: None,
            last_change_ms: 0,
            badge: BadgeState::Hidden,
        }
    }

    pub fn config(&self) -> &Config {
        &self.cfg
    }

    pub fn handle(&mut self, event: Event, now_ms: u64) -> Vec<Effect> {
        match event {
            Event::Layout {
                layout,
                lang,
                source,
            } => self.on_layout(layout, lang, source, now_ms),
            Event::AnchorResolved { .. } => todo!("task 6"),
            Event::Pointer { .. }
            | Event::HideTimerFired
            | Event::SetMode(_)
            | Event::SetSoundEnabled(_)
            | Event::SetAutostart(_) => todo!("task 7"),
        }
    }

    fn on_layout(
        &mut self,
        layout: LayoutId,
        lang: LangTag,
        source: LayoutSource,
        now_ms: u64,
    ) -> Vec<Effect> {
        if self.layout.as_ref().map(|(id, _)| *id) == Some(layout) {
            return vec![];
        }
        let stale_echo = self.prev_layout == Some(layout)
            && now_ms.saturating_sub(self.last_change_ms) < STALE_ECHO_WINDOW_MS;
        if stale_echo {
            return vec![];
        }
        self.prev_layout = self.layout.take().map(|(id, _)| id);
        self.layout = Some((layout, lang.clone()));
        self.last_change_ms = now_ms;

        let content = BadgeContent::for_lang(&lang, self.cfg.badge.style, &self.cfg.badge.colors);
        let mut fx = vec![Effect::UpdateTray {
            label: content.label,
            lang: lang.clone(),
        }];
        if source == LayoutSource::Initial {
            return fx;
        }
        if self.cfg.sound.enabled {
            fx.push(Effect::PlaySound {
                cue: cue_for(&lang),
                volume: self.cfg.sound.volume,
            });
        }
        self.badge = BadgeState::AwaitingAnchor;
        fx.push(Effect::QueryAnchor);
        fx
    }
}
```

- [ ] **Шаг 4: убедиться, что тесты проходят**

Запустить: `cargo test -p switcher-core`
Ожидание: PASS (все тесты задач 3–5).

- [ ] **Шаг 5: гейты и коммит**

Запустить: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
Ожидание: чисто (`todo!` линтами не запрещён).

```bash
git add crates/switcher-core
git commit -m "feat(core): engine skeleton with layout acceptance and dedup"
```

---

### Задача 6: switcher-core — engine: выбор якоря и показ бейджа

**Файлы:**
- Изменить: `crates/switcher-core/src/engine.rs`
- Тесты: unit-тесты внутри `engine.rs`

**Интерфейсы:**
- Потребляет: всё из Задачи 5 (типы НЕ меняются).
- Производит: реализованную ветку `Event::AnchorResolved`; правило якоря: caret > cursor > fixed с учётом `config.badge.anchor` (auto/cursor/fixed) и автопонижением; слежение за курсором включается ТОЛЬКО при якоре Cursor.

- [ ] **Шаг 1: написать падающие тесты**

Добавить в тестовый модуль `engine.rs`:

```rust
    pub(super) fn resolved(caret: Option<Point>, cursor: Option<Point>) -> Event {
        Event::AnchorResolved { caret, cursor }
    }

    pub(super) fn p(x: i32, y: i32) -> Point {
        Point { x, y }
    }

    /// Engine with a visible cursor-anchored RU badge (transient mode), t=1000.
    pub(super) fn engine_with_visible_badge() -> Engine {
        let mut e = engine_after_initial();
        e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 1000);
        let fx = e.handle(resolved(None, Some(p(10, 10))), 1000);
        assert!(fx
            .iter()
            .any(|f| matches!(f, Effect::ShowBadge { .. })));
        e
    }

    #[test]
    fn auto_anchor_prefers_caret_and_does_not_track_pointer() {
        let mut e = engine_after_initial();
        e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 1000);
        let fx = e.handle(resolved(Some(p(5, 6)), Some(p(9, 9))), 1001);
        assert_eq!(
            fx,
            vec![
                Effect::ShowBadge {
                    content: default_content(&ru()),
                    anchor: ResolvedAnchor::Caret(p(5, 6)),
                },
                Effect::SetPointerTracking(false),
                Effect::ArmHideTimer { after_ms: 1500 },
            ]
        );
    }

    #[test]
    fn auto_anchor_falls_back_to_cursor_and_tracks() {
        let mut e = engine_after_initial();
        e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 1000);
        let fx = e.handle(resolved(None, Some(p(9, 9))), 1001);
        assert_eq!(
            fx,
            vec![
                Effect::ShowBadge {
                    content: default_content(&ru()),
                    anchor: ResolvedAnchor::Cursor(p(9, 9)),
                },
                Effect::SetPointerTracking(true),
                Effect::ArmHideTimer { after_ms: 1500 },
            ]
        );
    }

    #[test]
    fn auto_anchor_falls_back_to_fixed_when_nothing_available() {
        let mut e = engine_after_initial();
        e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 1000);
        let fx = e.handle(resolved(None, None), 1001);
        assert_eq!(
            fx,
            vec![
                Effect::ShowBadge {
                    content: default_content(&ru()),
                    anchor: ResolvedAnchor::Fixed,
                },
                Effect::SetPointerTracking(false),
                Effect::ArmHideTimer { after_ms: 1500 },
            ]
        );
    }

    #[test]
    fn cursor_pref_ignores_caret() {
        let mut cfg = Config::default();
        cfg.badge.anchor = crate::config::AnchorPref::Cursor;
        let mut e = Engine::new(cfg);
        e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 1000);
        let fx = e.handle(resolved(Some(p(5, 6)), Some(p(9, 9))), 1001);
        assert!(fx.contains(&Effect::ShowBadge {
            content: default_content(&ru()),
            anchor: ResolvedAnchor::Cursor(p(9, 9)),
        }));
    }

    #[test]
    fn fixed_pref_ignores_caret_and_cursor() {
        let mut cfg = Config::default();
        cfg.badge.anchor = crate::config::AnchorPref::Fixed;
        let mut e = Engine::new(cfg);
        e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 1000);
        let fx = e.handle(resolved(Some(p(5, 6)), Some(p(9, 9))), 1001);
        assert!(fx.contains(&Effect::ShowBadge {
            content: default_content(&ru()),
            anchor: ResolvedAnchor::Fixed,
        }));
        assert!(fx.contains(&Effect::SetPointerTracking(false)));
    }

    #[test]
    fn follow_mode_cancels_timer_instead_of_arming() {
        let mut cfg = Config::default();
        cfg.badge.mode = crate::config::BadgeMode::Follow;
        let mut e = Engine::new(cfg);
        e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 1000);
        let fx = e.handle(resolved(None, Some(p(9, 9))), 1001);
        assert!(fx.contains(&Effect::CancelHideTimer));
        assert!(!fx.iter().any(|f| matches!(f, Effect::ArmHideTimer { .. })));
    }

    #[test]
    fn anchor_resolved_while_not_awaiting_is_ignored() {
        let mut e = engine_after_initial();
        let fx = e.handle(resolved(None, Some(p(9, 9))), 500);
        assert_eq!(fx, vec![]);
    }
```

- [ ] **Шаг 2: убедиться, что тесты падают**

Запустить: `cargo test -p switcher-core`
Ожидание: FAIL — новые тесты падают на `todo!("task 6")`.

- [ ] **Шаг 3: реализация**

В `handle` заменить ветку:

```rust
            Event::AnchorResolved { caret, cursor } => self.on_anchor(caret, cursor),
```

Добавить метод в `impl Engine`:

```rust
    fn on_anchor(&mut self, caret: Option<Point>, cursor: Option<Point>) -> Vec<Effect> {
        if self.badge != BadgeState::AwaitingAnchor {
            return vec![];
        }
        let anchor = match self.cfg.badge.anchor {
            AnchorPref::Auto => caret
                .map(ResolvedAnchor::Caret)
                .or(cursor.map(ResolvedAnchor::Cursor))
                .unwrap_or(ResolvedAnchor::Fixed),
            AnchorPref::Cursor => cursor
                .map(ResolvedAnchor::Cursor)
                .unwrap_or(ResolvedAnchor::Fixed),
            AnchorPref::Fixed => ResolvedAnchor::Fixed,
        };
        let tracking = matches!(anchor, ResolvedAnchor::Cursor(_));
        let (_, lang) = self
            .layout
            .as_ref()
            .expect("badge is pending only after an accepted layout event");
        let content = BadgeContent::for_lang(lang, self.cfg.badge.style, &self.cfg.badge.colors);
        self.badge = BadgeState::Visible { tracking };
        let mut fx = vec![
            Effect::ShowBadge { content, anchor },
            Effect::SetPointerTracking(tracking),
        ];
        match self.cfg.badge.mode {
            BadgeMode::Transient => fx.push(Effect::ArmHideTimer {
                after_ms: self.cfg.badge.show_ms,
            }),
            BadgeMode::Follow => fx.push(Effect::CancelHideTimer),
        }
        fx
    }
```

- [ ] **Шаг 4: убедиться, что тесты проходят**

Запустить: `cargo test -p switcher-core`
Ожидание: PASS.

- [ ] **Шаг 5: гейты и коммит**

Запустить: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
Ожидание: чисто.

```bash
git add crates/switcher-core
git commit -m "feat(core): anchor resolution caret>cursor>fixed with auto-downgrade"
```

---

### Задача 7: switcher-core — engine: жизненный цикл бейджа, режимы, команды трея

**Файлы:**
- Изменить: `crates/switcher-core/src/engine.rs`
- Тесты: unit-тесты внутри `engine.rs`

**Интерфейсы:**
- Потребляет: всё из задач 5–6 (типы НЕ меняются).
- Производит: полностью реализованный `Engine::handle` — ветки `Pointer`, `HideTimerFired`, `SetMode`, `SetSoundEnabled`, `SetAutostart`; в `engine.rs` не остаётся ни одного `todo!`.

- [ ] **Шаг 1: написать падающие тесты**

Добавить в тестовый модуль `engine.rs`:

```rust
    #[test]
    fn pointer_moves_visible_tracking_badge() {
        let mut e = engine_with_visible_badge();
        let fx = e.handle(Event::Pointer { pos: p(50, 60) }, 1100);
        assert_eq!(fx, vec![Effect::MoveBadge { pos: p(50, 60) }]);
    }

    #[test]
    fn pointer_is_ignored_when_badge_hidden() {
        let mut e = engine_after_initial();
        let fx = e.handle(Event::Pointer { pos: p(50, 60) }, 1100);
        assert_eq!(fx, vec![]);
    }

    #[test]
    fn pointer_is_ignored_when_caret_anchored() {
        let mut e = engine_after_initial();
        e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 1000);
        e.handle(resolved(Some(p(5, 6)), Some(p(9, 9))), 1001);
        let fx = e.handle(Event::Pointer { pos: p(50, 60) }, 1100);
        assert_eq!(fx, vec![]);
    }

    #[test]
    fn hide_timer_hides_badge_and_stops_tracking() {
        let mut e = engine_with_visible_badge();
        let fx = e.handle(Event::HideTimerFired, 2500);
        assert_eq!(fx, vec![Effect::HideBadge, Effect::SetPointerTracking(false)]);
        // Once hidden, pointer noise is ignored.
        let fx = e.handle(Event::Pointer { pos: p(1, 1) }, 2600);
        assert_eq!(fx, vec![]);
    }

    #[test]
    fn stale_hide_timer_in_follow_mode_is_ignored() {
        let mut cfg = Config::default();
        cfg.badge.mode = crate::config::BadgeMode::Follow;
        let mut e = Engine::new(cfg);
        e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 1000);
        e.handle(resolved(None, Some(p(9, 9))), 1001);
        let fx = e.handle(Event::HideTimerFired, 2500);
        assert_eq!(fx, vec![]);
    }

    #[test]
    fn second_change_while_visible_requeries_anchor_and_rearms() {
        let mut e = engine_with_visible_badge();
        let fx = e.handle(layout(EN_ID, en(), LayoutSource::ShellHook), 2000);
        assert!(fx.contains(&Effect::QueryAnchor));
        let fx = e.handle(resolved(None, Some(p(20, 20))), 2001);
        assert!(fx.contains(&Effect::ShowBadge {
            content: default_content(&en()),
            anchor: ResolvedAnchor::Cursor(p(20, 20)),
        }));
        assert!(fx.contains(&Effect::ArmHideTimer { after_ms: 1500 }));
    }

    #[test]
    fn set_mode_follow_while_hidden_shows_badge_and_persists() {
        let mut e = engine_after_initial();
        let fx = e.handle(Event::SetMode(crate::config::BadgeMode::Follow), 3000);
        assert_eq!(fx, vec![Effect::PersistConfig, Effect::QueryAnchor]);
        assert_eq!(e.config().badge.mode, crate::config::BadgeMode::Follow);
    }

    #[test]
    fn set_mode_follow_before_any_layout_only_persists() {
        let mut e = Engine::new(Config::default());
        let fx = e.handle(Event::SetMode(crate::config::BadgeMode::Follow), 10);
        assert_eq!(fx, vec![Effect::PersistConfig]);
    }

    #[test]
    fn set_mode_follow_while_visible_cancels_timer() {
        let mut e = engine_with_visible_badge();
        let fx = e.handle(Event::SetMode(crate::config::BadgeMode::Follow), 1200);
        assert_eq!(fx, vec![Effect::PersistConfig, Effect::CancelHideTimer]);
    }

    #[test]
    fn set_mode_transient_while_visible_arms_timer() {
        let mut cfg = Config::default();
        cfg.badge.mode = crate::config::BadgeMode::Follow;
        let mut e = Engine::new(cfg);
        e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 1000);
        e.handle(resolved(None, Some(p(9, 9))), 1001);
        let fx = e.handle(Event::SetMode(crate::config::BadgeMode::Transient), 1200);
        assert_eq!(
            fx,
            vec![Effect::PersistConfig, Effect::ArmHideTimer { after_ms: 1500 }]
        );
    }

    #[test]
    fn set_mode_same_is_noop() {
        let mut e = engine_after_initial();
        let fx = e.handle(Event::SetMode(crate::config::BadgeMode::Transient), 1200);
        assert_eq!(fx, vec![]);
    }

    #[test]
    fn set_sound_enabled_updates_config_and_persists() {
        let mut e = engine_after_initial();
        let fx = e.handle(Event::SetSoundEnabled(false), 1200);
        assert_eq!(fx, vec![Effect::PersistConfig]);
        assert!(!e.config().sound.enabled);
        let fx = e.handle(layout(RU_ID, ru(), LayoutSource::ShellHook), 2000);
        assert!(!fx.iter().any(|f| matches!(f, Effect::PlaySound { .. })));
    }

    #[test]
    fn set_autostart_applies_and_persists() {
        let mut e = engine_after_initial();
        let fx = e.handle(Event::SetAutostart(true), 1200);
        assert_eq!(fx, vec![Effect::ApplyAutostart(true), Effect::PersistConfig]);
        assert!(e.config().autostart);
    }
```

- [ ] **Шаг 2: убедиться, что тесты падают**

Запустить: `cargo test -p switcher-core`
Ожидание: FAIL — новые тесты падают на `todo!("task 7")`.

- [ ] **Шаг 3: реализация**

В `handle` заменить оставшиеся ветки:

```rust
            Event::Pointer { pos } => match self.badge {
                BadgeState::Visible { tracking: true } => vec![Effect::MoveBadge { pos }],
                _ => vec![],
            },
            Event::HideTimerFired => match self.badge {
                BadgeState::Visible { .. } if self.cfg.badge.mode == BadgeMode::Transient => {
                    self.badge = BadgeState::Hidden;
                    vec![Effect::HideBadge, Effect::SetPointerTracking(false)]
                }
                _ => vec![],
            },
            Event::SetMode(mode) => self.on_set_mode(mode),
            Event::SetSoundEnabled(enabled) => {
                self.cfg.sound.enabled = enabled;
                vec![Effect::PersistConfig]
            }
            Event::SetAutostart(enabled) => {
                self.cfg.autostart = enabled;
                vec![Effect::ApplyAutostart(enabled), Effect::PersistConfig]
            }
```

Добавить метод:

```rust
    fn on_set_mode(&mut self, mode: BadgeMode) -> Vec<Effect> {
        if self.cfg.badge.mode == mode {
            return vec![];
        }
        self.cfg.badge.mode = mode;
        let mut fx = vec![Effect::PersistConfig];
        match (mode, self.badge) {
            (BadgeMode::Follow, BadgeState::Hidden) if self.layout.is_some() => {
                self.badge = BadgeState::AwaitingAnchor;
                fx.push(Effect::QueryAnchor);
            }
            (BadgeMode::Follow, BadgeState::Visible { .. }) => fx.push(Effect::CancelHideTimer),
            (BadgeMode::Transient, BadgeState::Visible { .. }) => fx.push(Effect::ArmHideTimer {
                after_ms: self.cfg.badge.show_ms,
            }),
            _ => {}
        }
        fx
    }
```

- [ ] **Шаг 4: убедиться, что тесты проходят**

Запустить: `cargo test -p switcher-core`
Ожидание: PASS — весь табличный набор ядра зелёный, `todo!` в `engine.rs` не осталось (проверить: `grep -n "todo!" crates/switcher-core/src/engine.rs` — пусто).

- [ ] **Шаг 5: гейты и коммит**

Запустить: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
Ожидание: чисто.

```bash
git add crates/switcher-core
git commit -m "feat(core): badge lifecycle, hide timer, modes and tray commands"
```

---
