# M1 — Windows MVP: план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Цель:** резидентная утилита для Windows 11 — транзиентный бейдж «RU/EN» у курсора при смене раскладки, звуковая подсказка, трей, автозапуск, TOML-конфиг; ~0% CPU в простое.

**Архитектура:** cargo workspace c портами/адаптерами (ADR-0002): чистое ядро-автомат `(state, event, now) -> effects` в `switcher-core`; трейты в `switcher-platform`; весь unsafe Win32 — в `switcher-windows`; сборка и wiring — в `switcher-app`. Всё событийно (ADR-0003), оверлей — сырое layered-окно (ADR-0004).

**Стек:** Rust stable, windows-rs, tray-icon + muda, rodio, tiny-skia + ab_glyph, serde + toml, crossbeam-channel, tracing.

## Прогресс

Единственный источник истины о состоянии M1. Обновляется в том же коммите, что и задача.

**Аудит 2026-09-07:** проверена рабочая копия `feat/m1-task10-overlay`, база `d855bea`.
На начало аудита были реализованы задачи 1–10. Продолжение в основной папке
довело код до задачи 20; ручная приёмка задачи 21 остаётся открытой. Исправления описаны в
[отчёте](../../research/2026-09-07-implementation-audit.md). Примеры кода завершённых задач
1–20 — исторические шаги TDD; их нельзя копировать поверх текущих файлов. Актуальный
контракт раскладки — [ADR-0011](../../architecture/adr/0011-authoritative-layout-snapshots.md).

| # | Задача | Статус | Коммит |
|---|---|---|---|
| 1 | каркас workspace | ✅ сделано | `2d9313e` |
| 2 | switcher-platform — события и порты | ✅ сделано | `de018bb` |
| 3 | switcher-core — контент бейджа и звуковой кью | ✅ сделано | `7e32782` |
| 4 | switcher-core — модель конфига | ✅ сделано | `abf7b69` |
| 5 | switcher-core — engine: приём раскладки и дедуп | ✅ сделано | `45003ac` |
| 6 | switcher-core — engine: выбор якоря и показ | ✅ сделано | `97109a5` |
| 7 | switcher-core — engine: жизненный цикл, режимы, трей | ✅ сделано | `37efa92`, `af4be79` |
| 8 | порты и ядро под ADR-0005…0008, урезание зависимостей | ✅ сделано | ветка `feat/m1-task8-contracts` |
| 9 | фундамент: манифест PMv2, win_util, supervise | ✅ сделано | ветка `feat/m1-task9-foundation` |
| 10 | overlay.rs + geometry.rs + overlay_smoke — **риск №1 (1/2)** | ✅ сделано | ветка `feat/m1-task10-overlay`; визуальные пункты чеклиста ждут человека |
| 11 | pointer.rs (Raw Input): бейдж следует за курсором — **риск №1 (2/2)** | ✅ код и native-тесты; визуальный smoke ожидает проверки | `a1aa1f5` |
| 12 | layout_monitor.rs: 2 источника + взводимый фолбэк — **риск №2** | ✅ код и native-тесты; доставка смен языка требует smoke | `53a397a` |
| 13 | tsf.rs — третий источник (STA/COM) | ✅ код и native-тесты; глобальная доставка требует smoke | `98bca13` |
| 14 | autostart.rs (HKCU\Run) | ✅ код; roundtrip в изолированном ключе реестра | `4d53567` |
| 15 | switcher-app — render.rs: растеризация и кэш бейджей | ✅ код и тесты; визуальная приёмка впереди | `9025bbc` |
| 16 | switcher-app — sound.rs: синтез двух кью | ✅ код и тесты; прослушивание впереди | `9025bbc` |
| 17 | switcher-app — paths.rs + logging.rs | ✅ включая безопасное сохранение конфига | `9025bbc` |
| 18 | switcher-app — tray.rs | ✅ код и native-контракты; интерактивная приёмка впереди | `5c659c7` |
| 19 | switcher-app — runtime.rs: цикл ядра и диспетчер эффектов | ✅ код и 14 интеграционных тестов на портах | `fb94562` |
| 20 | switcher-app — main.rs: сборка всего, end-to-end | ✅ сборка, реальный старт/выход; смена языка и UI ждут приёмки | `eed318f` |
| 21 | приёмка M1: smoke-чеклист и замер NFR | 🟨 чеклист/замеры/review проведены; ручная матрица, CPU/RSS и hover-таймер открыты | этот коммит |

**Порядок riskiest-first.** Задачи 1–7 закрыли ядро, но не сняли ни одного технического риска проекта: спека требует прототипировать сначала «layered click-through оверлей на mixed-DPI» и «связку трёх источников событий раскладки». Поэтому после задачи 8 (она чисто контрактная и разблокирует всё остальное) идут задачи 9–13: к их концу оба главных риска либо сняты, либо честно перезаписаны в ADR — и только потом строится оболочка (15–20).

**Решения, принятые по итогам аудита 2026-08-25** и обязательные к соблюдению задачами 8+: [ADR-0005](../../architecture/adr/0005-badge-geometry-ownership.md) (владение геометрией и DPI), [ADR-0006](../../architecture/adr/0006-badge-rasterization-and-cache.md) (растеризация, premultiplied BGRA, кэш), [ADR-0007](../../architecture/adr/0007-capability-degradation-contract.md) (деградация, автозапуск как зеркало реестра, супервизия), [ADR-0008](../../architecture/adr/0008-dependency-features-trimming.md) (урезание зависимостей), [ADR-0009](../../architecture/adr/0009-shell-threading-model.md) (потоковая модель оболочки), [ADR-0010](../../architecture/adr/0010-pmv2-manifest-via-linker.md) (манифест PMv2).

## Глобальные ограничения

Требования каждой задачи неявно включают этот раздел.

- **Качество:** перед каждым коммитом чисто проходят `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (скил `quality-gates`).
- **Unsafe:** только в `crates/switcher-windows`; каждый `unsafe`-блок несёт `// SAFETY:`-комментарий с инвариантами. В `switcher-core` и `switcher-platform` — `#![forbid(unsafe_code)]`.
- **Нулевой поллинг (ADR-0003):** никаких таймеров/опросов, кроме двух взводимых исключений: (1) фолбэк-опрос раскладки 500 мс — только пока передний план elevated/UWP/консоль; (2) Raw Input курсора — только пока бейдж видим. Таймер скрытия бейджа — единственный таймер ядра, живёт как deadline в `recv_timeout` цикла рантайма.
- **Потоки:** каждый Win32-хук — свой поток со скрытым окном и циклом сообщений; TSF — отдельный STA-поток; ядро — выделенный поток с блокирующим `recv`; главный поток — трей + его цикл сообщений. Через границы потоков — только плоские данные по каналам crossbeam (никаких HWND/колбэков).
- **Native API:** сигнатуры, приведённые в задачах 8+, сверены 2026-08-25 по **вендоренным исходникам** крейтов (`~/.cargo/registry/src/index.crates.io-.../<крейт>-<версия>/`) и по learn.microsoft.com для семантики; context7 в той сессии был недоступен. Всё, что помечено **UNVERIFIED**, обязано быть сверено до написания кода (скил `platform-api-work`), и любое отклонение от приведённой сигнатуры — тоже повод перепроверить контракт, а не подогнать код.
- **Трекинг:** таблица «Прогресс» в начале этого файла обновляется в том же коммите, что и задача. Отметки в чекбоксах — рабочее состояние внутри задачи; строка таблицы — факт для внешнего наблюдателя.
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
│   │       ├── events.rs               # Point, LayoutId, LangTag, LayoutSource, ResolvedAnchor, BadgeImage, Capability*
│   │       └── ports.rs                # трейты LayoutMonitor, PointerTracker, CaretLocator, OverlayWindow, Autostart, SoundPlayer, PlatformError
│   ├── switcher-windows/
│   │   ├── Cargo.toml                  # только target.'cfg(windows)' зависимости; фичи windows по ADR-0005/0007
│   │   ├── examples/
│   │   │   ├── overlay_smoke.rs        # риск №1: бейдж следует за курсором, mixed-DPI
│   │   │   └── layout_smoke.rs         # риск №2: печать событий 3 источников
│   │   └── src/
│   │       ├── lib.rs                  # pub mod dpi|win_util|supervise|overlay|pointer|layout_monitor|tsf|autostart
│   │       ├── dpi.rs                  # ensure_per_monitor_v2() + warn! о фактической awareness (ADR-0010)
│   │       ├── win_util.rs             # HiddenWindow (RAII), hook-поток с циклом сообщений, wide-строки
│   │       ├── supervise.rs            # catch_unwind + backoff 250/1000/4000 + CapabilityChanged (ADR-0007)
│   │       ├── overlay.rs              # layered-окно: show/move_to/hide/dpi_for, поток оверлея
│   │       ├── overlay/geometry.rs     # ЧИСТАЯ арифметика: place + clamp + scaled, без unsafe (ADR-0005)
│   │       ├── pointer.rs              # Raw Input: arm/disarm, коалесценция, cursor_pos()
│   │       ├── layout_monitor.rs       # shell hook + WinEvent(FOREGROUND) + взводимый фолбэк-опрос
│   │       ├── layout_monitor/classify.rs  # ЧИСТАЯ лестница классификации переднего окна, без unsafe
│   │       ├── tsf.rs                  # STA-поток, TSF-sink (источник №3)
│   │       └── autostart.rs            # HKCU\...\Run
│   └── switcher-app/
│       ├── Cargo.toml                  # bin; адаптеры и трей по cfg(windows)
│       ├── build.rs                    # флаги линкера: встраивание манифеста PMv2 (ADR-0010)
│       ├── lang-switcher.manifest      # рукописный RT_MANIFEST: dpiAwareness = PerMonitorV2
│       ├── assets/fonts/               # статический шрифт-сабсет + текст лицензии + README (см. Задачу 15)
│       └── src/
│           ├── main.rs                 # сборка: обработчики трея → PMv2 → конфиг → логи → потоки → насос
│           ├── menu.rs                 # MenuCommand, ids, command_for
│           ├── capability.rs           # CapabilityMap, compose_tooltip, compose_status
│           ├── paths.rs                # directories: config/data пути (каталоги создаём сами)
│           ├── logging.rs              # tracing → rolling-файл, WorkerGuard живёт до конца main
│           ├── render.rs               # растеризация бейджа (premultiplied BGRA) + BadgeCache — чистая
│           ├── sound.rs                # rodio: синтез двух кью, SoundPlayer impl
│           ├── tray.rs                 # трей на главном потоке, TrayCommand, иконка straight-RGBA
│           └── runtime.rs              # цикл ядра: recv_timeout-deadline, диспетчер Effect → порты, CapabilityMap
├── .github/workflows/ci.yml            # три гейта на Windows + «ядро вне Windows» + MSRV
├── LICENSE-MIT, LICENSE-APACHE         # двойная лицензия из Cargo.toml
└── docs/smoke/m1-windows.md            # ручной smoke-чеклист (наполняется задачами 9–20, собирается в 21)
```

Направление зависимостей: `switcher-platform` ← `switcher-core`, ← `switcher-windows`, ← `switcher-app`; `switcher-app` → все. Адаптеры не знают про ядро; ядро не знает про ОС.

---

### Задача 1: каркас workspace

> **Статус: сделано** — коммит `2d9313e`. Шаги ниже сохранены как история решения.

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

> **Статус: сделано** — коммит `de018bb`. Шаги ниже сохранены как история решения.

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

> **Статус: сделано** — коммит `7e32782`. Шаги ниже сохранены как история решения.

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

> **Статус: сделано** — коммит `abf7b69`. Шаги ниже сохранены как история решения.

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

> **Статус: сделано** — коммит `45003ac`. Шаги ниже сохранены как история решения.

**Файлы:**
- Создать: `crates/switcher-core/src/engine.rs`
- Изменить: `crates/switcher-core/src/lib.rs`
- Тесты: unit-тесты внутри `engine.rs`

**Интерфейсы:**
- Потребляет: `content::{cue_for, BadgeContent}` (Задача 3), `config::{BadgeMode, AnchorPref, Config}` (Задача 4), типы Задачи 2.
- Производит (контракты стабильны для задач 6, 7, 19):
  - повтор текущего `LayoutId` подавляется; временного окна дедупликации нет (ADR-0011),
  - `engine::Event { Layout { layout: LayoutId, lang: LangTag, source: LayoutSource }, AnchorResolved { caret: Option<Point>, cursor: Option<Point> }, Pointer { pos: Point }, HideTimerFired, SetMode(BadgeMode), SetSoundEnabled(bool), SetAutostart(bool) }`,
  - `engine::ResolvedAnchor { Caret(Point), Cursor(Point), Fixed }`,
  - `engine::Effect { QueryAnchor, ShowBadge { content: BadgeContent, anchor: ResolvedAnchor }, MoveBadge { pos: Point }, HideBadge, ArmHideTimer { after_ms: u64 }, CancelHideTimer, SetPointerTracking(bool), PlaySound { cue: SoundCue, volume: f32 }, UpdateTray { label: String, lang: LangTag }, ApplyAutostart(bool), PersistConfig }`,
  - `engine::Engine`: `new(Config) -> Engine`, `config(&self) -> &Config`, `handle(&mut self, Event, now_ms: u64) -> Vec<Effect>`.

Семантика приёма события раскладки (правила ядра из спеки):
1. Тот же `LayoutId`, что текущий, — игнор (это же схлопывает дубли от трёх источников).
2. Возврат к предыдущему `LayoutId` принимается при любом интервале. Рантайм подтверждает снимок через `LayoutMonitor::current()`; старый payload уведомления не является источником истины (ADR-0011).
3. Принятое событие обновляет трей. `Initial` не играет звук и не показывает транзиентный бейдж, но восстанавливает Follow через `QueryAnchor`. Обычная смена даёт звук (если включён) и `QueryAnchor`.

- [ ] **Шаг 1: написать падающие тесты**

Тестовый модуль `engine.rs` (выше — код Шага 3, но с `todo!("task 5")` в теле `on_layout`; остальные ветки `handle` — `todo!("task 6")`/`todo!("task 7")` как в листинге):

Актуальные реализация и регрессионные тесты: [engine.rs](../../../crates/switcher-core/src/engine.rs). Старый листинг с временным подавлением удалён аудитом 2026-09-07; контракт — ADR-0011.

В `lib.rs` добавить `pub mod engine;`.

- [ ] **Шаг 2: убедиться, что тесты падают**

Запустить: `cargo test -p switcher-core`
Ожидание: FAIL — новые тесты падают на `todo!("task 5")`.

- [ ] **Шаг 3: реализация**

Содержимое `engine.rs` над тестами:

Актуальные реализация и регрессионные тесты: [engine.rs](../../../crates/switcher-core/src/engine.rs). Старый листинг с временным подавлением удалён аудитом 2026-09-07; контракт — ADR-0011.

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

> **Статус: сделано** — коммит `97109a5`. Шаги ниже сохранены как история решения.

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

> **Статус: сделано** — коммиты `37efa92` (задача) и `af4be79` (утечка таймера скрытия, найдена ревью). Шаги ниже сохранены как история решения.

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

### Задача 8: порты и ядро под ADR-0005…0008, урезание зависимостей

**Файлы:**
- Изменить: `crates/switcher-platform/src/events.rs` — удалить `Placement`, принять `ResolvedAnchor`, переписать `BadgeImage`, добавить `Capability`/`CapabilityState`/`CapabilityReport`/`PlatformEvent::CapabilityChanged`
- Изменить: `crates/switcher-platform/src/ports.rs` — `PlatformError { code, detail }`, новая форма `OverlayWindow`
- Изменить: `crates/switcher-core/src/engine.rs` — `Effect::MoveBadge { anchor }`, `Effect::SyncTrayMenu`, `Event::AutostartApplied`, ре-экспорт `ResolvedAnchor`, дедуп `SetSoundEnabled`
- Изменить: `crates/switcher-core/src/content.rs` — `Hash` к `Rgb8`/`BadgeStyle`, `Eq + Hash` к `BadgeContent`
- Изменить: `crates/switcher-core/src/config.rs` — валидация `log_level` и `ui_language` в `sanitize()`
- Изменить: `Cargo.toml`, `crates/switcher-app/Cargo.toml`, `crates/switcher-windows/Cargo.toml`, `Cargo.lock`

**Интерфейсы:**
- Потребляет: текущее ядро (задачи 3–7); решения [ADR-0005](../../architecture/adr/0005-badge-geometry-ownership.md), [ADR-0006](../../architecture/adr/0006-badge-rasterization-and-cache.md), [ADR-0007](../../architecture/adr/0007-capability-degradation-contract.md), [ADR-0008](../../architecture/adr/0008-dependency-features-trimming.md).
- Производит (обязательный вход для задач 9–20): `events::ResolvedAnchor`, `events::BadgeImage { width, height, bgra_premul, dpi }`, `events::{Capability, CapabilityState, CapabilityReport}`, `PlatformEvent::CapabilityChanged`, `ports::PlatformError { code, detail }`, `OverlayWindow::{show, move_to, hide, dpi_for}`, `engine::Effect::{MoveBadge { anchor }, SyncTrayMenu}`, `engine::Event::AutostartApplied`, урезанный граф зависимостей.

- [ ] **Шаг 1: написать/поправить тесты (они обязаны не компилироваться)**

В `engine.rs` **переписать** `pointer_moves_visible_tracking_badge` — ожидание становится `vec![Effect::MoveBadge { anchor: ResolvedAnchor::Cursor(p(50, 60)) }]`. **Удалить** `set_autostart_applies_and_persists` и добавить вместо него шесть тестов (пять на автозапуск + один на дедуп звука):

```rust
    #[test]
    fn set_autostart_requests_the_os_before_touching_the_config() {
        let mut e = engine_after_initial();
        assert_eq!(e.handle(Event::SetAutostart(true), 1200), vec![Effect::ApplyAutostart(true)]);
        assert!(!e.config().autostart, "config must not claim what the OS has not confirmed");
        let fx = e.handle(Event::AutostartApplied { requested: true, ok: true }, 1210);
        assert_eq!(fx, vec![Effect::PersistConfig]);
        assert!(e.config().autostart);
    }

    #[test]
    fn refused_autostart_never_reaches_the_config() {
        let mut e = engine_after_initial();
        e.handle(Event::SetAutostart(true), 1200);
        let fx = e.handle(Event::AutostartApplied { requested: true, ok: false }, 1210);
        assert_eq!(fx, vec![Effect::SyncTrayMenu]);
        assert!(!e.config().autostart);
    }

    #[test]
    fn autostart_confirmation_matching_config_is_a_noop() {
        let mut e = engine_after_initial(); // default autostart == false
        let fx = e.handle(Event::AutostartApplied { requested: false, ok: true }, 1210);
        assert_eq!(fx, vec![]);
    }

    /// Startup reconciliation: the registry wins over the config file.
    #[test]
    fn startup_reconciliation_adopts_the_os_value() {
        let mut e = engine_after_initial();
        let fx = e.handle(Event::AutostartApplied { requested: true, ok: true }, 5);
        assert_eq!(fx, vec![Effect::PersistConfig]);
        assert!(e.config().autostart);
    }

    #[test]
    fn repeated_toggle_re_asserts_the_registry() {
        let mut e = engine_after_initial();
        e.handle(Event::SetAutostart(true), 1200);
        e.handle(Event::AutostartApplied { requested: true, ok: true }, 1210);
        let fx = e.handle(Event::SetAutostart(true), 1300);
        assert_eq!(fx, vec![Effect::ApplyAutostart(true)], "the Run key may have drifted");
    }

    #[test]
    fn set_sound_enabled_same_value_is_a_noop() {
        let mut e = engine_after_initial(); // sound.enabled == true by default
        assert_eq!(e.handle(Event::SetSoundEnabled(true), 1200), vec![]);
        assert!(e.config().sound.enabled);
    }
```

В `config.rs` добавить три теста: `unknown_log_level_resets_to_default_with_warning` (`log_level = "verbose"` → `"info"`, ровно одно предупреждение), `log_level_is_accepted_case_insensitively_and_normalized` (`"WARN"` → `"warn"`, предупреждений нет), `unknown_ui_language_resets_to_default_with_warning` (`"fr"` → `"ru"` с предупреждением; `"EN"` → `"en"` без него).

В `content.rs` добавить `badge_content_is_usable_as_a_cache_key`: положить в `HashSet` два разных `BadgeContent`, убедиться, что повторная вставка равного возвращает `false`, а `set.len() == 2` (это и есть проверка `Eq + Hash` для ключа кэша ADR-0006).

В `events.rs` (switcher-platform) добавить `capability_keys_are_unique_and_stable` (обойти `Capability::ALL`, собрать `key()`, отсортировать, `dedup()`, длина не изменилась; плюс `assert_eq!(Capability::LayoutTsf.key(), "layout.tsf")`) и `only_the_three_layout_sources_are_layout_sources`.

- [ ] **Шаг 2: убедиться, что тесты падают**

Запустить: `cargo test --workspace`
Ожидание: FAIL на этапе компиляции — `Effect::MoveBadge` не имеет поля `anchor`, `Event::AutostartApplied`/`Effect::SyncTrayMenu`/`Capability` не существуют. Ошибка компиляции — корректный сигнал «тест падает» для чисто типовых изменений; никакого `todo!()` тут не нужно.

- [ ] **Шаг 3: `switcher-platform::events`**

Удалить `enum Placement` целиком. Перенести сюда `ResolvedAnchor` (точная копия варианта из `engine.rs`) с док-комментарием: ядро решает, **к чему** привязан бейдж; адаптер оверлея владеет всеми пиксельными решениями — смещение, масштабирование смещения по DPI, выбор монитора для `Fixed`, кламп в рабочую область (ADR-0005). Переписать `BadgeImage`:

```rust
/// A badge rasterized for one specific DPI, ready for `UpdateLayeredWindow`.
///
/// Byte order is **BGRA with premultiplied alpha** (ADR-0006): index 0 = blue, 1 = green,
/// 2 = red, 3 = alpha, and every colour channel is already multiplied by alpha
/// (`b <= a && g <= a && r <= a` holds for every pixel). This is exactly what a 32bpp
/// `BI_RGB` DIB expects on little-endian Windows together with `AC_SRC_ALPHA` — do NOT
/// demultiply. Rows are top-down with stride `width * 4` and no padding, so an adapter
/// copies the whole buffer into a `biHeight = -height` DIB in one `copy_from_slice`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadgeImage {
    pub width: u32,
    pub height: u32,
    pub bgra_premul: Vec<u8>,
    /// DPI the image was rendered for (96 = 100%).
    pub dpi: u32,
}
```

Добавить `Capability` (8 вариантов из ADR-0007), `pub const ALL: [Capability; 8]`, `pub const fn key(self) -> &'static str` со стабильными ключами (`"layout.shell_hook"`, `"layout.foreground_hook"`, `"layout.tsf"`, `"pointer"`, `"caret"`, `"overlay"`, `"sound"`, `"autostart"`), `pub const fn is_layout_source(self) -> bool`; `CapabilityState { Ok, Degraded, Off }`; `CapabilityReport { capability, state, code: &'static str, detail: String }`; вариант `PlatformEvent::CapabilityChanged(CapabilityReport)` с док-комментарием «потребитель — только `switcher-app`; ядру не передаётся, потому что из этого факта у ядра нет ни одного решения». `OverlayScaleChanged { dpi }` **оставить** как есть.

- [ ] **Шаг 4: `switcher-platform::ports`**

```rust
#[derive(Debug, Clone, thiserror::Error)]
#[error("{detail}")]
pub struct PlatformError {
    /// Stable machine key authored by the adapter ("registry_write_denied",
    /// "hook_register_failed"): lets the shell build a CapabilityReport without
    /// knowing a single thing about the OS.
    pub code: &'static str,
    pub detail: String,
}

impl PlatformError {
    pub fn new(code: &'static str, detail: impl Into<String>) -> Self { /* … */ }
}

/// Click-through, topmost, non-activating badge window. **Owns all badge geometry**
/// (ADR-0005): the offset from the anchor point, its DPI scaling, the monitor chosen for
/// `Fixed`, and clamping to the work area. The core never sees a pixel.
pub trait OverlayWindow: Send {
    fn show(&self, image: &BadgeImage, anchor: ResolvedAnchor);
    fn move_to(&self, anchor: ResolvedAnchor);
    fn hide(&self);
    /// Effective DPI of the monitor this adapter *would* place `anchor` on (including the
    /// monitor it picks itself for `Fixed`). Answers **synchronously on the caller's
    /// thread** — no hop into the overlay thread, otherwise the core loop deadlocks
    /// against the overlay's message pump.
    fn dpi_for(&self, anchor: ResolvedAnchor) -> u32;
}
```

Импорт в шапке файла меняется с `Placement` на `ResolvedAnchor`. Остальные пять трейтов — без изменений (ADR-0007: контракт деградации не добавил портам ни одного метода).

- [ ] **Шаг 5: `switcher-core`**

`engine.rs`: удалить локальное определение `ResolvedAnchor`, вместо него `pub use switcher_platform::events::ResolvedAnchor;` — **ре-экспорт обязателен**, без него ломаются шесть тестов якоря вместо одного (ADR-0005). `Effect::MoveBadge { anchor: ResolvedAnchor }` с док-комментарием «adapter recomputes offset and clamping». Добавить `Effect::SyncTrayMenu` («re-sync tray checkmarks from `Engine::config()` **without** writing the file») и `Event::AutostartApplied { requested: bool, ok: bool }`. Ветка `Pointer` становится `vec![Effect::MoveBadge { anchor: ResolvedAnchor::Cursor(pos) }]`. Ветки автозапуска и звука:

```rust
            // `cfg.autostart` mirrors a registry value, so the OS — not the click — decides.
            // No dedup on the request: the Run key may have drifted (cleaner tool, manual
            // edit, another copy of the app), so a repeated toggle must re-assert it.
            Event::SetAutostart(enabled) => vec![Effect::ApplyAutostart(enabled)],
            Event::AutostartApplied { requested, ok } => {
                if !ok {
                    // Refused: the config keeps the old value and the menu is snapped back;
                    // the reason travels separately as CapabilityChanged(Autostart, ..).
                    return vec![Effect::SyncTrayMenu];
                }
                if self.cfg.autostart == requested {
                    return vec![];
                }
                self.cfg.autostart = requested;
                vec![Effect::PersistConfig]
            }
            Event::SetSoundEnabled(enabled) => {
                // Dedup like on_set_mode: the config is the only source of truth for this
                // checkbox, so a no-op click must not rewrite the file on disk.
                if self.cfg.sound.enabled == enabled {
                    return vec![];
                }
                self.cfg.sound.enabled = enabled;
                vec![Effect::PersistConfig]
            }
```

`content.rs`: добавить **только `Hash`** в `derive` у `BadgeStyle` (`content.rs:9`) и `Rgb8` (`:16`) — `Eq` у них уже выведен, повторный в том же `derive` даст `error[E0119]`; у `BadgeContent` (`:58`) нет ни того, ни другого, ему нужны `Eq, Hash`. Всё аддитивно, ни один существующий тест не ломается.

`config.rs`: константы и нормализация в `sanitize()`.

```rust
/// Levels `tracing`'s LevelFilter parses, case-insensitively
/// (verified: tracing-core-0.1.36/src/metadata.rs:799-804 — error|warn|info|debug|trace|off).
/// Hardcoded on purpose: `switcher-core` must not take a dependency on `tracing`.
const LOG_LEVELS: [&str; 6] = ["error", "warn", "info", "debug", "trace", "off"];
const UI_LANGUAGES: [&str; 2] = ["ru", "en"];

fn normalize_enum(value: &str, allowed: &[&str], default: &str, field: &str,
                  warnings: &mut Vec<String>) -> String {
    let lower = value.to_ascii_lowercase();
    if allowed.contains(&lower.as_str()) { return lower; }
    warnings.push(format!("{field} {value:?} is not one of {allowed:?}, reset to {default:?}"));
    default.to_owned()
}
```

Вызвать её в `sanitize()` для `log_level` (default `"info"`) и `ui_language` (default `"ru"`). Мотив в комментарии: сейчас это свободные строки, и мусор из файла доходит до инициализации логов.

Там же — **нормализация ключей `badge.colors` к нижнему регистру**, тот же класс тихой опечатки: `BadgeContent::for_lang` ищет `colors.get(&primary)`, а `LangTag::primary()` возвращает нижний регистр, поэтому ключ `RU` проходит валидацию значения, не даёт ни одного предупреждения и при этом не применяется никогда. Ключ переименовывается с предупреждением; если нижнерегистровый дубликат уже есть — mixed-case вариант отбрасывается, тоже с предупреждением (детерминированно, без зависимости от порядка обхода). Два теста: `badge_color_keys_are_normalized_to_lower_case` и `duplicate_badge_color_keys_keep_the_lower_case_one`.

- [ ] **Шаг 6: убедиться, что тесты проходят, и посчитать их**

Запустить: `cargo test --workspace`
Ожидание: PASS. Арифметика обязана сойтись: было 43 (42 core + 1 platform). `engine.rs` 27 → 32 (27 − 1 удалённый + 6 новых; ещё один переписан на месте, счёт не меняя), `config.rs` 9 → 14 (три теста на `log_level`/`ui_language` + два на нормализацию ключей `badge.colors`, см. шаг 5), `content.rs` 6 → 7, итого core 53; `events.rs` 1 → 3. **56 тестов в воркспейсе.** Если цифра другая — сверить со списком выше, а не «округлить».

- [ ] **Шаг 7: зависимости по ADR-0008**

В `[workspace.dependencies]` корневого `Cargo.toml`: **удалить** строки `muda = "0.19"` (используется как `tray_icon::menu` — реэкспорт `pub use muda::*` проверен в `tray-icon-0.24.1/src/lib.rs:144-145`) и `embed-manifest = "1.5"` (никем не используется; манифест встраивается по ADR-0010). Заменить четыре строки на:

```toml
tiny-skia  = { version = "0.12", default-features = false, features = ["std", "simd"] }
ab_glyph   = { version = "0.2",  default-features = false, features = ["std"] }
rodio      = { version = "0.22", default-features = false, features = ["playback"] }
tray-icon  = { version = "0.24", default-features = false }
```

`windows = "0.62"` **не трогать**: его единственная default-фича — `std` (`windows-0.62.2/Cargo.toml:711`), и она нужна. Добавить перед `[workspace.lints.rust]`:

```toml
# ADR-0007: `panic = "abort"` is forbidden in every profile — unwinding is a precondition
# of the hook-thread supervisor (switcher-windows/src/supervise.rs). Note that the outer
# catch_unwind only fires if each extern "system" callback guards its own body: an unwind
# reaching a non-unwind extern boundary aborts the process (Rust >= 1.81).
```

`crates/switcher-app/Cargo.toml`: удалить `muda`, добавить `thiserror = { workspace = true }` (для `RenderError`, ADR-0006), перенести `tray-icon = { workspace = true }` из `[dependencies]` в `[target.'cfg(windows)'.dependencies]`.

`crates/switcher-windows/Cargo.toml`: **фичи `windows` здесь не добавляются.** ADR-0008 требует «фича приходит в том же коммите, что и код, который её требует», а в этой задаче ни строки Win32-кода нет — добавленная заранее фича не будет ничем проверена. Каждую свою фичу включает задача, которой она нужна; ниже — целевая карта, чтобы исполнитель этих задач не искал заново:

| Фича | Кто требует | Задача |
|---|---|---|
| `Win32_Foundation`, `Win32_UI_WindowsAndMessaging` | уже включены каркасом | — |
| `Win32_Graphics_Gdi` | `WNDCLASSW`/`RegisterClassW`, `MonitorFrom*`, `GetMonitorInfoW`, `UpdateLayeredWindow`, DIB | 9 |
| `Win32_UI_HiDpi` | `SetProcessDpiAwarenessContext`, `GetDpiForMonitor` | 9 |
| `Win32_System_Threading` | `GetCurrentThreadId`, `GetCurrentProcess`, `OpenProcess` | 9 |
| `Win32_System_LibraryLoader` | `GetModuleHandleW` | 9 |
| `Win32_UI_Input` | `RegisterRawInputDevices`, `RAWINPUTDEVICE` | 11 |
| `Win32_UI_Accessibility` | `SetWinEventHook` | 12 |
| `Win32_UI_Input_KeyboardAndMouse` | `GetKeyboardLayout` | 12 |
| `Win32_Globalization` | `LCIDToLocaleName` | 12 |
| `Win32_Storage_Packaging_Appx` | `GetPackageFullName` | 12 |
| `Win32_System_Com`, `Win32_UI_TextServices` | COM/STA + TSF-синк | 13 |
| `Win32_System_Registry` | `HKCU\...\Run` | 14 |

Три неочевидных гейта, проверенных по исходникам (иначе их ищут отладкой): `GetDpiForMonitor` объявлена под **двумя** cfg сразу — `Win32_UI_HiDpi` и `Win32_Graphics_Gdi` (`UI/HiDpi/mod.rs:37-38`); `UpdateLayeredWindow` требует `Win32_Graphics_Gdi` вопреки своему расположению в `WindowsAndMessaging` (`:2437-2443`); `RegisterClassW`/`WNDCLASSW` тоже гейтятся `Win32_Graphics_Gdi` (`WindowsAndMessaging/mod.rs:1919-1921`, `:7198-7200`).

- [ ] **Шаг 8: проверить, что граф действительно урезан**

Запустить: `cargo check --workspace --all-targets`, затем `cargo tree -p switcher-app -e normal`
Ожидание: сборка зелёная (логика фич выведена по исходникам, но **фактом** является только зелёный `check`), и в дереве нет ни одного из: `symphonia*`, `rand`, `rand_distr`, `rtrb`, `png`, `flate2`, `miniz_oxide`, `fdeflate`, `crc32fast`, `adler2`, `muda` как отдельного корня, `embed-manifest`. Отдельно убедиться, что `grep -rn 'panic' Cargo.toml` не находит ни одного `panic = "abort"`.

- [ ] **Шаг 9: правило 4 — ядро собирается вне Windows**

Запустить: `cargo check -p switcher-core -p switcher-platform --target x86_64-unknown-linux-gnu`
Ожидание: чисто. Это единственная защита от того, что новый тип из `events.rs` случайно потянет OS-специфику; цель уже установлена в этой машине (`rustup target list --installed`).

- [ ] **Шаг 10: гейты и коммит**

Запустить: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
Ожидание: чисто, 56 тестов.

```bash
git add Cargo.toml Cargo.lock crates/switcher-core crates/switcher-platform crates/switcher-app/Cargo.toml crates/switcher-windows/Cargo.toml
git commit -m "refactor(platform,core): ADR-0005..0008 contracts and dependency trimming"
```

---

### Задача 9: фундамент switcher-windows — манифест PMv2, оконный насос, супервизор

> **Статус: сделано.** Отклонения от шагов ниже, принятые при исполнении и **обязательные к учёту задачами 10–14**:
>
> 1. **Гашение насоса — приватным сообщением, а не `WM_QUIT` через `PostThreadMessageW`.** Сверка страницы `PostThreadMessageW` на learn.microsoft.com показала, что про `WM_QUIT` там не сказано **ничего**: посылать его между потоками — общинная идиома, а не контракт. Подтверждено при этом другое: `MSG.hwnd == NULL` у thread-сообщений и их нельзя отдавать в `DispatchMessage`, а также требование, чтобы у потока-адресата уже существовала очередь (иначе `ERROR_INVALID_THREAD_ID`). Поэтому `PumpThread::shutdown` посылает приватное `WM_PUMP_STOP`, которое распознаёт сам цикл насоса, а `WM_QUIT` приходит только от `post_quit()` на собственном потоке — этот путь документирован.
> 2. **Рукопожатие форсирует очередь явно.** После `setup` поток делает `PeekMessageW(.., PM_NOREMOVE)` — ровно тот приём, который Microsoft предписывает против этой гонки, — и только потом публикует свой `thread_id`. Иначе первая же побудка потерялась бы для хендлера без окна.
> 3. **`HiddenWindow::new` подставляет свой `default_wndproc`, если передан `None`.** Найдено вживую: класс с NULL `lpfnWndProc` роняет процесс с `STATUS_FATAL_USER_CALLBACK_EXCEPTION` прямо внутри `CreateWindowExW`. Поскольку `WNDPROC` в windows-rs — это `Option`, ошибка была в одном нажатии клавиши, и она закрыта кодом, а не комментарием.
> 4. **К `guard_callback` добавлен `callback_panic_outcome()`.** Без него схема из шага 6 тихо неверна: `guard_callback` гасит насос, тело потока возвращает `Ok(())`, и супервизор читает это как «попросили остановиться» и **не перезапускает**. Теперь пойманная на границе FFI паника ставит потоковый флаг, а `callback_panic_outcome()` в конце тела превращает его в `Err`. Задачи 12 и 13 обязаны заканчивать свой `run` этим вызовом.
> 5. **Тестов 11, а не 7** (в воркспейсе 67, а не 63): добавлены мемоизация `dpi`, `wide`, два теста на `guard_callback` и проверка потока, на котором дропается владелец окна.
> 6. **Негативный контроль манифеста требует `touch`.** Переименование сохраняет mtime, поэтому после возврата файла Cargo не перезапускает `build.rs`, и прогон продолжает показывать `from_manifest=false`. Записано в `docs/smoke/m1-windows.md`.
>
> **Правки по итогам `rust-adversarial-review`** (линзы дали 18 замечаний, верификаторы отклонили все; при чтении четыре оказались настоящими — сигнатуры ниже финальные, задачи 10–14 опираются на них):
>
> 7. **`guard_callback` принимает `on_panic` замыканием, а не значением.** Естественная запись wndproc — `guard_callback(cap, || DefWindowProcW(..), || { .. })`; при энергично вычисляемом default `DefWindowProcW` вызывался бы на **каждом** сообщении, включая обработанные телом, то есть сообщение обрабатывалось бы дважды на счастливом пути. Тест на это добавлен.
> 8. **`spawn_supervised` возвращает `Result<JoinHandle<()>, PlatformError>`.** Было `.expect()` на отказ ОС создать поток — прямое противоречие смыслу ADR-0007 («деградировать, а не умирать»).
> 9. **`run_pump` открыт как `pump_with_handler` и это обязательная форма для супервизируемых хуков.** `guard_callback` помнит панику в thread-local, поэтому `spawn_pump` **внутри** тела `spawn_supervised` рвёт цепочку: флаг ставится на внутреннем потоке, а `callback_panic_outcome()` на супервизируемом рапортует успех — источник замолкает и не перезапускается никогда. Запрет записан в док-комментарии `spawn_pump`.
> 10. **`CALLBACK_PANICKED` сбрасывается в начале каждой попытки** супервизора: иначе флаг от паники, которую тело не успело потребить, превратил бы законный чистый выход следующей попытки в перезапуск.
> 11. **Имя класса окна — предусловие, а не деталь.** Классы процессные, и при совпадении имени переиспользуется уже зарегистрированный класс *с его* wndproc: два разных обработчика под одним именем молча поделили бы первый. Зафиксировано в док-комментарии `HiddenWindow::new` + `debug!` при переиспользовании.

**Файлы:**
- Создать: `crates/switcher-app/build.rs` — флаги линкера, встраивающие манифест в `lang-switcher.exe`
- Создать: `crates/switcher-app/lang-switcher.manifest` — рукописный `RT_MANIFEST` с PerMonitorV2
- Создать: `crates/switcher-windows/src/dpi.rs` — `ensure_per_monitor_v2()`
- Создать: `crates/switcher-windows/src/win_util.rs` — `HiddenWindow`, `spawn_pump`, `PumpThread`, wide-строки
- Создать: `crates/switcher-windows/src/supervise.rs` — `RestartBudget` + `spawn_supervised`
- Создать: `docs/smoke/m1-windows.md` — первый пункт чеклиста (фактическая DPI-awareness)
- Изменить: `crates/switcher-windows/src/lib.rs` — объявить три модуля
- Изменить: `crates/switcher-windows/Cargo.toml` — фичи `windows`, которые требует именно этот код: `Win32_Graphics_Gdi` (`WNDCLASSW`/`RegisterClassW` гейтятся именно ей), `Win32_UI_HiDpi` (`dpi.rs`), `Win32_System_Threading` (`GetCurrentThreadId`, `GetCurrentProcess`), `Win32_System_LibraryLoader` (`GetModuleHandleW`)
- Изменить: `crates/switcher-app/src/main.rs` — временное тело, доказывающее встраивание манифеста (задача 20 его заменит)

**Интерфейсы:**
- Потребляет: `switcher_platform::events::{Capability, CapabilityReport, CapabilityState, PlatformEvent}` и `ports::PlatformError` (задача 8); `crossbeam-channel`, `tracing`.
- Производит (используют задачи 10–14): `dpi::ensure_per_monitor_v2()`, `win_util::{HiddenWindow, PumpHandler, PumpVerdict, PumpThread, spawn_pump, wide}`, `supervise::{RestartBudget, spawn_supervised, BACKOFF_MS, HEALTHY_AFTER_MS}`.
- Производит (используют задачи 18–20): `win_util::{pump_messages, PumpWaker, current_thread_id, post_quit}` — насос **на вызывающем потоке** для трея, в отличие от `spawn_pump`, который поднимает новый поток. Обе формы возвращают один и тот же `PumpVerdict`; имени `PumpControl` в проекте нет.

- [ ] **Шаг 1: манифест и build.rs**

Сначала сверка: директива называется `cargo::rustc-link-arg-bins=FLAG` и действует «only when building a binary target», а `cargo::rerun-if-changed=PATH` — стандартная форма; синтаксис `cargo::KEY=VALUE` требует MSRV 1.77, у нас 1.87 (сверено по Cargo Book, «Build Scripts → Outputs of the build script», 2026-08-25). Само `/MANIFEST:EMBED` + `/MANIFESTINPUT:<file>` проверено по Microsoft Learn (выписка — `docs/research/2026-08-25-m1-api-research/apis.md`), ограничение `MAX_PATH` на путь манифеста — оттуда же.

`crates/switcher-app/lang-switcher.manifest` — XML из ADR-0010 дословно: `asmv3:windowsSettings` с `<dpiAware>true</dpiAware>` (схема SMI/2005) **и** `<dpiAwareness>PerMonitorV2</dpiAwareness>` (схема SMI/2016). PerMonitorV2 через `<dpiAware>` выразить нельзя — только через `<dpiAwareness>`.

`crates/switcher-app/build.rs` — гейт по `CARGO_CFG_WINDOWS` и `CARGO_CFG_TARGET_ENV == "msvc"`, путь строится от `CARGO_MANIFEST_DIR`, три `println!`: `cargo::rustc-link-arg-bins=/MANIFEST:EMBED`, `cargo::rustc-link-arg-bins=/MANIFESTINPUT:{path}`, `cargo::rerun-if-changed=lang-switcher.manifest`. На не-Windows и на `*-windows-gnu` скрипт не делает ничего. Две обязательные проверки: (1) если длина абсолютного пути > 250 — `println!("cargo::warning=…")`, потому что `/MANIFESTINPUT` ограничен `MAX_PATH`, а мы работаем в `.claude/worktrees/...`; (2) **если файла манифеста нет — флаги не выдавать вообще**, а напечатать `cargo::warning=manifest not found, PMv2 will be set at runtime`. Без второй проверки отсутствующий манифест даёт не «бинарник без манифеста», а падение линковки (`mt.exe` не находит входной файл → `LNK1327`), и негативный контроль шага 7 стал бы невыполнимым. Cargo подхватывает `build.rs` автоматически, правки `Cargo.toml` не нужны.

Запустить: `cargo build -p switcher-app`
Ожидание: сборка проходит; линкер не жалуется на неизвестный флаг. Флаг применяется только к бинарной цели — значит примеры `switcher-windows` манифеста не получают (это и есть причина шага 2).

- [ ] **Шаг 2: `dpi.rs` — страховка и, главное, диагностика**

Сначала добавить в `Cargo.toml` те фичи `windows`, которые требует код этой задачи: `Win32_UI_HiDpi` (этот модуль), `Win32_System_Threading` (`GetCurrentProcess` здесь, `GetCurrentThreadId` в шаге 4), `Win32_Graphics_Gdi` и `Win32_System_LibraryLoader` (`WNDCLASSW`/`RegisterClassW` и `GetModuleHandleW` в шаге 4). Каждая — с комментарием, какой вызов её требует: фича приходит вместе с кодом (правило ADR-0008), поэтому задача 8 их сознательно не добавляла. Проверка правила бесплатна: удаление любой из четырёх обязано ломать сборку.

Модуль отвечает на один вопрос: **манифест встроился или нет**, и только потом страхует. Проверенные API (`windows-0.62.2`, фича `Win32_UI_HiDpi` + `Win32_System_Threading`): `GetDpiAwarenessContextForProcess(HANDLE) -> DPI_AWARENESS_CONTEXT` (`UI/HiDpi/mod.rs:32-36`), `AreDpiAwarenessContextsEqual(a, b) -> BOOL` (`:8-11`), `GetAwarenessFromDpiAwarenessContext(ctx) -> DPI_AWARENESS` (`:17-20`, тип derive'ит `Debug`), `SetProcessDpiAwarenessContext(ctx) -> Result<()>` (`:136-140`), `DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2` (`:252`), `GetCurrentProcess() -> HANDLE` (`System/Threading/mod.rs:679`).

```rust
pub struct DpiAwarenessReport { pub from_manifest: bool, pub per_monitor_v2: bool }

/// Idempotent (OnceLock): safe to call from `main` and as the first line of every example.
/// MUST run before any HWND exists in the process — Learn: "Once a window (an HWND) has
/// been created in your process, changing the DPI awareness mode is no longer supported".
pub fn ensure_per_monitor_v2() -> DpiAwarenessReport
```

Порядок строгий: **сначала прочитать** контекст процесса и сравнить его с PMv2 (`from_manifest`), и лишь если это не PMv2 — вызвать `SetProcessDpiAwarenessContext`. Иначе `warn!` перестанет быть детектором отсутствия манифеста. При `from_manifest` — `debug!("dpi awareness from manifest: per-monitor-v2")`; иначе `warn!(target: "switcher_windows::dpi", awareness = ?GetAwarenessFromDpiAwarenessContext(ctx), applied = ?res, "process is NOT Per-Monitor-V2 from a manifest; setting it at runtime")`. `DPI_AWARENESS` различает только unaware/system/per-monitor, поэтому точный V2 определяется именно `AreDpiAwarenessContextsEqual`, а не сравнением указателей. `// SAFETY:` каждого блока обязан назвать: (1) `GetCurrentProcess` возвращает псевдо-хэндл, который не закрывается; (2) обе Get-функции только читают состояние процесса и не имеют предусловий кроме валидного хэндла; (3) вызов Set происходит до создания любого окна в процессе — инвариант, который держит контракт функции, а не компилятор.

Запустить: `cargo clippy -p switcher-windows --all-targets -- -D warnings`
Ожидание: чисто.

- [ ] **Шаг 3: падающие тесты `win_util.rs`**

Три теста, все автоматические (крейт `#![cfg(windows)]`, тесты идут на Windows):

1. `thread_message_reaches_the_handler_with_null_hwnd` — `spawn_pump` с хендлером, пишущим `(msg, hwnd.is_null())` в `Arc<Mutex<Vec<_>>>`; `post(WM_APP + 1)`, затем `post(WM_APP + 2)`, на который хендлер возвращает `PumpVerdict::Quit`; `shutdown()`; ассерт по вектору. Проверяет ровно тот факт, что posted thread messages приходят с `MSG.hwnd == NULL` и **не** могут быть отданы в `DispatchMessageW`.
2. `window_message_is_dispatched_to_the_wndproc` — setup создаёт `HiddenWindow`, отдаёт числовое значение `hwnd.0 as usize` тестовому потоку через `crossbeam_channel::bounded(1)` (через границу потока идёт число, окном по-прежнему владеет и разрушает его поток-создатель); тест делает `PostMessageW` на этот HWND, wndproc инкрементит `static AtomicUsize`; после `shutdown()` счётчик равен 1.
3. `hidden_window_is_dropped_on_the_pump_thread` — хендлер в своём `Drop` пишет `GetCurrentThreadId()` в `Arc<AtomicU32>`; после `shutdown()` записанный id равен `PumpThread::thread_id()`. Это тест потокового контракта, а не косметика: `DestroyWindow` обязан исполниться на потоке-владельце.

Запустить: `cargo test -p switcher-windows`
Ожидание: FAIL — модуля `win_util` ещё нет.

- [ ] **Шаг 4: реализация `win_util.rs`**

Поверхность (`pub`, а не `pub(crate)`, иначе `-D warnings` завалится на `dead_code` до задачи 10; в док-комментарии модуля прямо сказать, что он adapter-internal по смыслу):

```rust
pub fn wide(s: &str) -> Vec<u16>;                    // NUL-terminated UTF-16
pub struct HiddenWindow { /* hwnd */ }               // !Send/!Sync автоматически: HWND — сырой указатель
impl HiddenWindow {
    pub fn new(class_name: &str, wndproc: WNDPROC, create_param: Option<*const c_void>)
        -> Result<Self, PlatformError>;
    pub fn hwnd(&self) -> HWND;
}
pub enum PumpVerdict { Continue, Quit }
pub trait PumpHandler { fn on_thread_message(&mut self, msg: u32, wparam: WPARAM, lparam: LPARAM) -> PumpVerdict; }
pub struct PumpThread { /* thread_id, join */ }
impl PumpThread {
    pub fn thread_id(&self) -> u32;
    pub fn post(&self, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Result<(), PlatformError>;
    pub fn shutdown(&mut self);          // PostThreadMessageW(WM_QUIT) + join; вызывается и из Drop
}
pub fn current_thread_id() -> u32;
/// Pumps GetMessageW/TranslateMessage/DispatchMessageW on the CALLING thread (the tray, task 18).
/// `on_iter` runs before each dispatch, on this same thread; Quit leaves the loop via PostQuitMessage.
pub fn pump_messages(on_iter: impl FnMut() -> PumpVerdict) -> Result<(), PlatformError>;
/// Send: wakes a pump owned by another thread (core loop -> tray). Wraps PostThreadMessageW.
pub struct PumpWaker(u32);
pub fn post_quit();

pub fn spawn_pump<S, H>(name: &'static str, setup: S) -> Result<PumpThread, PlatformError>
where S: FnOnce() -> Result<H, PlatformError> + Send + 'static, H: PumpHandler;
```

Точные API и фичи (все сверены в `windows-0.62.2`): `RegisterClassW(*const WNDCLASSW) -> u16` и `WNDCLASSW` — фича `Win32_Graphics_Gdi` (`WindowsAndMessaging/mod.rs:1919-1924`, `:7198-7211`); `CreateWindowExW(WINDOW_EX_STYLE, PCWSTR, PCWSTR, WINDOW_STYLE, i32×4, Option<HWND>, Option<HMENU>, Option<HINSTANCE>, Option<*const c_void>) -> Result<HWND>` (`:430-436`); `HWND_MESSAGE = HWND(-3)` (`:4060`) — родитель, делающий окно message-only; `WS_OVERLAPPED = WINDOW_STYLE(0)` (`:7297`); `DestroyWindow(HWND) -> Result<()>` (`:534-537`); `DefWindowProcW` (`:475`); `GetMessageW(*mut MSG, Option<HWND>, u32, u32) -> BOOL` (`:997`), `TranslateMessage` (`:2402`), `DispatchMessageW` (`:575`), `PostMessageW` (`:1857`), `PostThreadMessageW(u32, u32, WPARAM, LPARAM) -> Result<()>` (`:1872`), `WM_QUIT = 18` (`:7095`), `WM_APP = 32768` (`:6897`), `MSG { hwnd, message, wParam, lParam, time, pt }` (`:5096-5103`); `GetCurrentThreadId() -> u32` (`System/Threading/mod.rs:709`); `GetModuleHandleW(None) -> Result<HMODULE>` (`System/LibraryLoader/mod.rs:224-231`) плюс `impl From<HMODULE> for HINSTANCE` (`Foundation/mod.rs:5548`); `ERROR_CLASS_ALREADY_EXISTS = 1410` (`Foundation/mod.rs:1374`); `GetLastError()` (`Foundation/mod.rs:27`); `windows_core::BOOL(pub i32)` с `.as_bool()`/`.ok()` (`windows-result-0.4.1/src/bool.rs:7-24`).

Правила реализации, каждое из которых обязано быть в коде:

- **Класс не разрегистрируется.** `RegisterClassW` возвращает 0 при ошибке; `ERROR_CLASS_ALREADY_EXISTS` считается успехом (класс уже зарегистрирован другим окном того же назначения). `UnregisterClassW` в `Drop` **не** вызывается: он провалится, пока живо хоть одно окно класса, а классы освобождаются при выгрузке модуля. Причина — комментарием в коде, чтобы это не выглядело утечкой.
- **Насос.** `GetMessageW(&mut msg, None, 0, 0)`: `.0 == -1` → `error!` и выход из цикла; `.0 == 0` → пришёл `WM_QUIT`, штатный выход; иначе — **если `msg.hwnd` нулевой, сообщение отдаётся `PumpHandler` и НЕ передаётся в `DispatchMessageW`**. Это не догадка: Learn (`nf-winuser-postthreadmessagew`, Remarks) — «The **hwnd** member of the returned MSG structure is NULL… Messages posted by PostThreadMessage are not associated with a window. As a general rule, messages that are not associated with a window cannot be dispatched by the DispatchMessage function».
- **Рукопожатие обязательно.** Тот же источник: «The function fails if the specified thread does not have a message queue… GetLastError returns ERROR_INVALID_THREAD_ID». Поэтому `spawn_pump` возвращает управление только после того, как новый поток отправил свой `GetCurrentThreadId()` (или ошибку setup) через `crossbeam_channel::bounded(1)`. Отдавать id раньше создания окна нельзя — гонка тихо съест первую побудку.
- **`// SAFETY:` обязаны называть:** для `RegisterClassW`/`CreateWindowExW` — что `class_name`/`wide`-буфер жив дольше вызова и NUL-терминирован, а `wndproc` — `extern "system"` функция с корректной ABI, живущая всё время жизни класса; для `DestroyWindow` — что вызов происходит на потоке-владельце окна (гарантия типа: `HiddenWindow` не `Send`) и что HWND не был разрушен раньше; для `GetMessageW`/`DispatchMessageW` — что `MSG` инициализирован системой перед чтением; для `PostThreadMessageW` — что `thread_id` получен рукопожатием, то есть очередь у потока уже существует.
- `PumpThread::shutdown` идемпотентен (`Option<JoinHandle>`), `Drop` вызывает его — RAII-контракт, из-за которого `std::process::exit` в оболочке запрещён (ADR-0009).

Запустить: `cargo test -p switcher-windows`
Ожидание: PASS, три теста.

- [ ] **Шаг 5: падающие табличные тесты `supervise.rs`**

Чистая часть супервизора — расчёт задержки и сброс бюджета — выносится в `RestartBudget`, чтобы её можно было проверить без потоков:

```rust
    #[test] fn backoff_walks_the_schedule_then_gives_up() {
        let mut b = RestartBudget::new();
        assert_eq!(b.on_exit(10), Some(250));
        assert_eq!(b.on_exit(10), Some(1_000));
        assert_eq!(b.on_exit(10), Some(4_000));
        assert_eq!(b.on_exit(10), None);
    }
    #[test] fn exhausted_budget_stays_exhausted_and_never_panics() { /* ещё два on_exit → None, None:
        сторож против индексации BACKOFF_MS за границей массива */ }
    #[test] fn a_healthy_run_resets_the_budget() { /* два падения, затем on_exit(HEALTHY_AFTER_MS) == Some(250) */ }
    #[test] fn just_under_the_healthy_threshold_does_not_reset() { /* одно падение, затем on_exit(HEALTHY_AFTER_MS - 1) == Some(1_000): на свежем бюджете было бы Some(250) */ }
```

Запустить: `cargo test -p switcher-windows`
Ожидание: FAIL — модуля `supervise` нет.

- [ ] **Шаг 6: реализация `supervise.rs`**

```rust
pub const BACKOFF_MS: [u64; 3] = [250, 1_000, 4_000];
/// A thread that ran this long without dying is healthy: reset its restart budget.
pub const HEALTHY_AFTER_MS: u64 = 60_000;

pub struct RestartBudget { attempt: usize }
impl RestartBudget {
    pub const fn new() -> Self;
    /// `ran_ms` — how long the run that just ended lasted. Returns the backoff to sleep
    /// before the next attempt, or `None` when the budget is spent.
    pub fn on_exit(&mut self, ran_ms: u64) -> Option<u64>;
}

pub fn spawn_supervised<F>(cap: Capability, tx: Sender<PlatformEvent>, run: F) -> JoinHandle<()>
where F: Fn(&Sender<PlatformEvent>) -> Result<(), PlatformError> + Send + 'static;
```

Цикл: `Instant::now()` → `std::panic::catch_unwind(AssertUnwindSafe(|| run(&tx)))` (`AssertUnwindSafe` нужен потому, что `Sender` не `UnwindSafe`) → на `Err(payload)` (паника; текст достаётся через `downcast_ref::<&str>()` / `::<String>()`) либо `Ok(Err(e))` (штатный сбой, есть `e.code`) — `warn!` с `cap.key()`, номером попытки и кодом; затем `budget.on_exit(elapsed_ms)`: `Some(ms)` → `sleep`, повтор; `None` → отправить `PlatformEvent::CapabilityChanged(CapabilityReport { capability: cap, state: CapabilityState::Off, code: "restart_budget_exhausted", detail })` **тем же каналом**, что и остальные события (порядок относительно `LayoutChanged` значим, ADR-0007), и выйти. `Ok(Ok(()))` — штатное завершение, выход без события.

В шапке модуля — комментарий, что весь механизм мёртв при `panic = "abort"` (см. запрет в корневом `Cargo.toml`, ADR-0007), и что константы 250/1000/4000/60 с — инженерное суждение, проверяемое эмпирически (убить `explorer.exe` и замерить восстановление shell-hook), а не выведенное из документации.

**Границу FFI закрыть обязательно, иначе внешний `catch_unwind` бесполезен.** Сигнатуры `WNDPROC` (`UI/WindowsAndMessaging/mod.rs:7249`) и `WINEVENTPROC` (`UI/Accessibility/mod.rs:21016`) объявлены `extern "system"`, а **не** `extern "system-unwind"`, а с Rust 1.81 разворачивание, дошедшее до такой границы, аварийно завершает процесс (MSRV проекта 1.87). Работа хуков исполняется именно внутри этих колбэков, поэтому:

- каждый наш `extern "system"` колбэк (wndproc из шага 4, `WINEVENTPROC` и `WM_TIMER`-ветка задачи 12, TSF-колбэк задачи 13) оборачивает **всё своё тело** в `std::panic::catch_unwind(AssertUnwindSafe(...))`;
- при поймённой панике колбэк пишет `error!` с `cap.key()`, возвращает безопасный результат (`DefWindowProcW(...)` для wndproc, ничего для `WINEVENTPROC`) и просит поток завершиться (`PostQuitMessage` на своём потоке), чтобы уже внешний `catch_unwind` супервизора увидел штатный выход и применил backoff;
- добавить хелпер `guard_callback` в `supervise.rs`, чтобы это не копировалось в четырёх местах, и тест на него: паника внутри переданного замыкания не разворачивается наружу, а возвращает подставленный default.

Без этого шага обещание ADR-0007 «источник перезапускается, а при исчерпании бюджета гаснет с сообщением в трей» не выполняется ни для одного реального места паники: процесс просто аборт, без флаша лога.

Запустить: `cargo test -p switcher-windows`
Ожидание: PASS, 7 тестов в крейте (3 + 4); в воркспейсе 63.

- [ ] **Шаг 7: smoke — доказать, что манифест встроился**

Во `crates/switcher-app/src/main.rs` — временное тело (задача 20 его заменит; отметить это комментарием `// TODO(task 20)`): инициализировать `tracing_subscriber` с `EnvFilter`, под `#[cfg(windows)]` вызвать `switcher_windows::dpi::ensure_per_monitor_v2()` первой строкой и напечатать полученный `DpiAwarenessReport`.

Запустить: `RUST_LOG=debug cargo run -p switcher-app`
Ожидание: строка `debug` «dpi awareness from manifest: per-monitor-v2» и **ни одного `warn!`** от `switcher_windows::dpi`; отчёт `from_manifest: true, per_monitor_v2: true`.

Негативный контроль (без него отсутствие `warn!` ничего не доказывает): временно переименовать `crates/switcher-app/lang-switcher.manifest`, снова `cargo run -p switcher-app` (`rerun-if-changed` заставит пересобрать), убедиться, что теперь появляется `warn!` c `from_manifest: false`, вернуть имя файла и убедиться, что `warn!` исчез.

Завести `docs/smoke/m1-windows.md` с первым пунктом чеклиста: что запускали, на какой конфигурации мониторов, какая строка лога наблюдалась, и явная отметка про негативный контроль. Дальше файл дополняют задачи 10–20, а задача 21 сводит его в прогоняемый чеклист.

- [ ] **Шаг 8: гейты и коммит**

Запустить: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
Ожидание: чисто, 63 теста.

```bash
git add crates/switcher-app/build.rs crates/switcher-app/lang-switcher.manifest crates/switcher-app/src/main.rs crates/switcher-windows docs/smoke/m1-windows.md
git commit -m "feat(windows): PMv2 manifest, hidden-window pump and hook supervisor"
```

---

### Задача 10: overlay.rs + overlay/geometry.rs + overlay_smoke — риск №1 (1/2)

> **Статус: сделано** (код и лог-наблюдаемая часть smoke; визуальные пункты секции C чеклиста
> требуют человека за клавиатурой и отмечены там как непроверенные). Отклонения от шагов ниже,
> принятые при исполнении и **обязательные к учёту задачами 11–20**:
>
> 1. **`HiddenWindow` для оверлея не подходит, поэтому регистрация класса вынесена в
>    `win_util::register_class`.** `HiddenWindow` жёстко создаёт message-only окно
>    (`HWND_MESSAGE` в родителях), а такое окно **не рисует** — прямая противоположность
>    бейджу. Дублировать 40 строк идемпотентной регистрации класса с разбором
>    `ERROR_CLASS_ALREADY_EXISTS` было нельзя, поэтому она стала отдельной функцией, которую
>    зовут оба; `HiddenWindow::new` унаследовал её предусловие про уникальность имени класса.
>    Задачи 12–13 и 18 берут `register_class`, если им нужно окно с собственной wndproc.
> 2. **`Overlay` не хранит `thread_id` отдельно** — он уже внутри `PumpThread` (форма задачи 9),
>    и `PumpThread::post` его использует. Существенное свойство сохранено: `HWND` границу
>    потока не пересекает вообще.
> 3. **Инвалидация кэша DPI — через thread-local флаг, а не через `GWLP_USERDATA`.** Это не
>    упрощение, а требование безопасности, и обнаружилось оно прогоном: `WM_DPICHANGED`
>    приходит **реентрантно внутри нашего же `SetWindowPos`**, то есть wndproc исполняется,
>    пока `move_to` держит `&mut self`. Состояние через сырой указатель было бы алиасингом
>    живой мутабельной ссылки — UB на обычном пути. Задачи 12–13, если заведут состояние для
>    своих wndproc, обязаны учитывать ту же реентрантность.
> 4. **`hdcDst` у `UpdateLayeredWindow` — `None`, а не экранный DC.** API документирует NULL как
>    «использовать палитру по умолчанию», поэтому пара `GetDC`/`ReleaseDC` не нужна вовсе — на
>    один освобождаемый ресурс меньше.
> 5. **Добавлена дедупликация `OverlayScaleChanged` по паре `(image_dpi, monitor_dpi)`.** Первый
>    прогон показал 13 событий на один переход границы мониторов: рассогласование живёт до
>    перерисовки, а `move_to` в это время идёт каждые 150 мс. Ожидание шага 6 («ровно одна
>    строка на переход») без этого просто не выполнялось.
> 6. **Пример масштабирует синтетический бейдж по DPI и перерисовывает его на событие.** В
>    варианте из шага 6 (бейдж фиксированного размера, без реакции на событие) пункт чеклиста
>    «одинаковый видимый размер на 100 % и 150 %» проверить нечем, а утверждение ADR-0005 «цикл
>    сходится за один шаг» остаётся недоказанным. Теперь оба наблюдаемы: 66×39 px при 144 DPI →
>    одно событие → 44×26 px при 96 DPI → тишина до конца прогона.
> 7. **Заглушка `scaled` в красном шаге — `panic!`, а не `todo!`:** `todo!()` в `const fn` даёт
>    E0015 (сборка не проходит), то есть красный шаг был бы ошибкой компиляции вместо падающих
>    тестов.
> 8. **Тестов 12, а не «таблица + один»** (в крейте 23, в воркспейсе 79): к таблице добавлены
>    кламп на мониторе левее первичного, бейдж шире и выше рабочей области, `dpi == 0`,
>    сатурация на предельных значениях, инвариант «`place` никогда не возвращает
>    неклампленное», `dib_bytes` (предусловие `copy_nonoverlapping` — единственное место в
>    модуле, где ошибка в числе была бы UB) и `Overlay: Send + Sync`.
>
> **Правки по итогам `rust-adversarial-review`** (17 агентов, два подтверждённых замечания — оба
> настоящие, проверены самостоятельно перед исправлением):
>
> 9. **`WM_DPICHANGED`/`WM_DISPLAYCHANGE` были инертны.** Сообщения ставили флаг, а флаг
>    читался только внутри `facts()`, то есть **лишь когда оболочка присылает команду**. А
>    бейдж законно висит неподвижно неограниченно долго: при `AnchorPref::Fixed` ядро выключает
>    слежение (`tracking` истинно только для `Cursor`), а `BadgeMode::Follow` отменяет таймер
>    скрытия. Значит сценарий «сменили масштаб в Параметрах, пока бейдж стоит» — ровно тот,
>    для которого сообщение и оставлено (ADR-0005, `overview.md`) — не работал вовсе. Исправлено:
>    `OverlayThread` запоминает `last_anchor`, wndproc после установки флага **будит насос**
>    (`PostThreadMessageW` — единственное, что безопасно делать из реентрантного колбэка), а
>    обработчик переставляет бейдж и сообщает о рассогласовании. `lParam` с предложенным
>    прямоугольником сознательно игнорируется: он для окон, которые раскладывает система, а
>    это окно позиционирует себя само по `rcWork`. Задачи 12–13, 18 обязаны учитывать тот же
>    приём: из wndproc — только `post`, вся работа с состоянием в обработчике.
> 10. **`GetDpiForMonitor` зависит от awareness, а research-док утверждал обратное.** Проверено
>    по Learn: *«This API is not DPI aware… you will receive different DPI values depending on
>    the DPI awareness of the calling application»*, `PROCESS_DPI_UNAWARE` → 96 для любого
>    монитора. Ложное утверждение в `geometry.md` исправлено с цитатой. Следствия в коде:
>    `OverlayHwnd::new` сам зовёт `dpi::ensure_per_monitor_v2()` (PMv2 — предусловие
>    правильности, а не пожелание) и предупреждает, если уже поздно; тест `dpi_for` больше не
>    выдаётся за доказательство настоящих значений — он доказывает отсутствие зависания и
>    совпадение ответов, а настоящие пер-мониторные значения (144 и 96) доказывает smoke-прогон.
>    Попутно с той же страницы закрыт ещё один UNVERIFIED-пункт: *«The values of \*dpiX and
>    \*dpiY are identical»* — теперь цитата, а не вывод из `WM_DPICHANGED`.
> 11. **Пункт чеклиста про смену масштаба переписан.** Он просил найти строку в логе — и прошёл
>    бы, пока бейдж стоит не на месте. Пункт, проверяющий наличие лога вместо наблюдаемого
>    поведения, провалить нельзя, поэтому теперь он требует смотреть на бейдж.
>
> **Снятые UNVERIFIED-вопросы** (были в `docs/research/2026-08-25-m1-api-research/geometry.md`):
> `WM_DPICHANGED` доходит до окна `WS_EX_NOACTIVATE|WS_EX_TOOLWINDOW` и доходит при нашем
> собственном перемещении, синхронно; потоковая аффинность `MonitorFromPoint`/`MonitorFromWindow`/
> `GetMonitorInfoW`/`GetDpiForMonitor`/`GetForegroundWindow` подтверждена тестом с двумя потоками.

**Файлы:**
- Создать: `crates/switcher-windows/src/overlay/geometry.rs` — чистая арифметика размещения (`WorkArea`, `MonitorFacts`, `scaled`, `place`, `clamp`) и её табличные тесты; `#![deny(unsafe_code)]` в шапке модуля
- Создать: `crates/switcher-windows/src/overlay.rs` — layered-окно, DIB-секция, поток оверлея, `impl OverlayWindow`
- Создать: `crates/switcher-windows/examples/overlay_smoke.rs` — прогон синтетического бейджа по скриптованному пути
- Изменить: `docs/smoke/m1-windows.md` — секция «Оверлей» (файл создан задачей 9)
- Изменить: `crates/switcher-windows/src/lib.rs` — `pub mod overlay;`
- Изменить: `crates/switcher-windows/Cargo.toml` — секция `[target.'cfg(windows)'.dev-dependencies]` с `tracing-subscriber` (нужен примерам). Новых фич `windows` не требуется: `Win32_Graphics_Gdi` и `Win32_UI_HiDpi` уже включены задачей 9 — если сборка на них жалуется, значит задача 9 их не добавила, и добавить надо там

**Интерфейсы:**
- Потребляет: задача 8 — `ResolvedAnchor`, `BadgeImage { width, height, bgra_premul, dpi }`, `OverlayWindow` с `dpi_for(anchor) -> u32`, `PlatformEvent::{OverlayScaleChanged, CapabilityChanged}`, `PlatformError::new(code, detail)`, `Capability::Overlay`; задача 9 — `win_util` (wide-строки, RAII класса окна и message-only окна, запуск потока с насосом), `dpi::ensure_per_monitor_v2()`.
- Производит: `overlay::Overlay::new(tx: Sender<PlatformEvent>) -> Result<Overlay, PlatformError>` (реализует `OverlayWindow`); `overlay::geometry::{WorkArea, MonitorFacts, place, clamp, scaled, DEFAULT_DPI}` — задача 11 переиспользует их без изменений.

- [ ] **Шаг 1: табличные тесты геометрии (red)**

Создать `overlay/geometry.rs` с типами из ADR-0005, заглушками `place`/`clamp`/`scaled` (тела `todo!("task 10")`) и тестовым модулем. Константы: `CURSOR_GAP_96 = 12`, `CARET_GAP_96 = 6`, `FIXED_MARGIN_96 = 16`, `DEFAULT_DPI = 96` (ADR-0005). Таблица обязана покрыть:

| случай | вход | ожидание |
|---|---|---|
| курсор, 100 % | `Cursor(100,100)`, 40×24, dpi 96 | `(112,112)` |
| курсор, 200 % | то же, dpi 192 | `(124,124)` — зазор физический |
| дробный масштаб | `scaled(12, 110)` | `13` — целочисленное усечение зафиксировано тестом |
| курсор у правого-нижнего края | `Cursor(1900,1030)`, 40×24, work `0,0..1920,1040` | `(1880,1016)` |
| каретка | `Caret(100,100)`, dpi 96 | `(106,106)` |
| `Fixed`, 100 % | work `0,0..1920,1040`, 40×24, dpi 96 | `(1864,1000)` |
| `Fixed`, 200 % | та же work, 80×48, dpi 192 | `(1808,960)` |
| монитор слева от первичного | work `-1920,0..0,1080`, кламп точки `(-2000,-50)` | `x >= -1920`, `y >= 0` |
| бейдж шире рабочей области | 2000×24, work шириной 1920 | `x == work.left` (переполнение принимается) |

Заметка в док-комментарии `place`: ветка `Caret` в M1 недостижима (`NullCaretLocator`), направление «над строкой vs под строкой» — продуктовый вопрос M2 вместе с `Effect::SetCaretTracking`; в M1 реализуется так же, как курсор, но с `CARET_GAP_96`.

Запустить: `cargo test -p switcher-windows`
Ожидание: FAIL — все табличные тесты падают на `todo!("task 10")`.

- [ ] **Шаг 2: реализовать чистую геометрию**

`scaled(logical_96, dpi) = logical_96 * dpi / 96`; `place` даёт левый-верхний угол по виду якоря; `clamp(top_left, size, work)` = `x.min(work.right - w).max(work.left)` и то же по Y — порядок `min` затем `max` и есть та ветка, которая прижимает слишком большой бейдж к left/top. Ни одного вызова ОС: все факты приходят в `MonitorFacts`.

Запустить: `cargo test -p switcher-windows`
Ожидание: PASS.

- [ ] **Шаг 3: «добыча фактов» о мониторе + кэш DPI**

В `overlay.rs` — приватная `unsafe`-часть `monitor_facts(anchor) -> MonitorFacts`. Точные API и фичи (проверено по исходникам `windows-0.62.2`):

- `MonitorFromPoint(POINT, MONITOR_DEFAULTTONEAREST)` → `Graphics/Gdi/mod.rs:1474`, флаг `= MONITOR_FROM_FLAGS(2)` (`:5863`); для `Fixed` — `MonitorFromWindow(GetForegroundWindow(), MONITOR_DEFAULTTOPRIMARY)` (`:1484`, флаг `= 1`), нулевой HWND обрабатывать явно.
- `GetMonitorInfoW(hmon, *mut MONITORINFO) -> BOOL` (`:1102`) — **не** `Result`, проверять руками; `MONITORINFO::cbSize` выставлять вручную несмотря на `Default` (`:5833`). Кламп по `rcWork`, никогда по `rcMonitor`.
- `GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dpix, &mut dpiy) -> Result<()>` (`UI/HiDpi/mod.rs:37`) — требует **обе** фичи `Win32_UI_HiDpi` и `Win32_Graphics_Gdi`. Использовать `dpix`; расхождение с `dpiy` логировать `debug!` (равенство осей — вывод, не цитата).
- Кэш `HMONITOR -> dpi` внутри потока оверлея, сброс на `WM_DPICHANGED` (`= 736`) и `WM_DISPLAYCHANGE` (`= 126`). Это не поллинг: все вызовы внутри обработки уже пришедшего события (ADR-0003).

Каждый `unsafe`-блок несёт `// SAFETY:`, называющий: (1) переданный указатель указывает на живой стековый `MONITORINFO` с корректным `cbSize`; (2) `HMONITOR` получен из вызова, только что вернувшего непустое значение, и не переживает обработку текущего сообщения; (3) вызов не требует потока-владельца нашего окна (ни одна из четырёх функций не принимает HWND нашего окна).

Запустить: `cargo clippy -p switcher-windows --all-targets -- -D warnings`
Ожидание: чисто; `undocumented_unsafe_blocks` (deny в воркспейсе) не срабатывает.

- [ ] **Шаг 4: окно и DIB-бэкинг**

Класс регистрируется через `WNDCLASSW` + `RegisterClassW` — **обе сущности под `#[cfg(feature = "Win32_Graphics_Gdi")]`** (проверено: `UI/WindowsAndMessaging/mod.rs:7198` и `:1919`), `RegisterClassW -> u16`, ноль = отказ, проверять руками. Окно: `CreateWindowExW(WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST, …, WS_POPUP, …) -> Result<HWND>` (`:430`); значения флагов проверены (`:7270, :7288, :7276, :7286, :7287, :7299`).

DIB: `CreateCompatibleDC(None) -> HDC` (ноль = отказ, без `Result`, `Gdi:207`), `CreateDIBSection(Some(hdc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) -> Result<HBITMAP>` (`Gdi:242`), `SelectObject` (`Gdi:1756`). Заполнение `BITMAPINFOHEADER`: `biBitCount = 32`, `biPlanes = 1`, `biHeight = -(height as i32)` (top-down), `biCompression = BI_RGB.0` — поле объявлено `u32`, а `BI_RGB` это `BI_COMPRESSION(0)` (`Gdi:2336`, `:2402`), поэтому нужен `.0`. Stride 32bpp DIB = `width * 4` без выравнивания, `bgra_premul.len() == width * height * 4` в точности — прямой `copy_from_slice` в биты секции.

Вывод: `UpdateLayeredWindow(hwnd, Some(hdc_screen), Some(&pt_dst), Some(&size), Some(hdc_mem), Some(&pt_src_zero), COLORREF(0), Some(&blend), ULW_ALPHA)` (`WAM:2437`, `ULW_ALPHA = 2` `:6646`), где `BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 }` — поля `u8`, константы объявлены `u32` (`Gdi:2410`, `:2174`). Один вызов меняет позицию, размер и содержимое, поэтому смена DPI не даёт промежуточного кадра. `move_to` без смены размера — `SetWindowPos(hwnd, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE)`; `SWP_ASYNCWINDOWPOS` запрещён (ADR-0005).

RAII: `OverlayWindow`-ресурсы обёрнуты структурами с `Drop`, порядок освобождения строго такой: **восстановить прежний `HGDIOBJ` в DC → `DeleteObject(bitmap)` → `DeleteDC` → `DestroyWindow`**. Порядок не косметика: `DeleteObject` документирован как возвращающий ноль, если объект «currently selected into a DC», поэтому удаление битмапа до восстановления прежнего объекта провалится и утечёт DIB-секция (`width*height*4` байт плюс GDI-объект) на каждое пересоздание оверлея. `UnregisterClassW` **не** вызывается — правило задачи 9 (класс живёт до выгрузки модуля). `DeleteDC`/`DeleteObject` возвращают `BOOL` (`Gdi:448`, `:463`) — ненулевой отказ логировать `warn!` (иначе утечка останется тихой), но не паниковать в `Drop`.

Запустить: `cargo build -p switcher-windows`
Ожидание: успех; при удалении `Win32_Graphics_Gdi` из фич сборка падает (правило ADR-0008: фича добавляется в тот же коммит, что и код, который её требует).

- [ ] **Шаг 5: поток оверлея, `Send`-безопасная проводка, эмиссия `OverlayScaleChanged`**

`Overlay` хранит **`thread_id: u32`** (`GetCurrentThreadId`, `System/Threading/mod.rs:709`) и `Sender<OverlayCommand>`; HWND через границу потока не проходит вообще — это то, что делает `Overlay` честно `Send`. `show`/`move_to`/`hide` кладут команду в канал и делают `PostThreadMessageW(thread_id, WM_APP_WAKE, 0, 0)` (`WAM:1872`, `WM_APP = 32768` `:6897`); насос при сообщении с нулевым HWND дренирует канал до `DispatchMessageW`. Оверлей **не** супервизируется в M1 — по тому же основанию, что записано в ADR-0007 для оверлея: его состояние (видимый бейдж) пришлось бы восстанавливать реплеем эффектов. Сбой конструирования → `Err(PlatformError::new("overlay_create_failed", …))`, а `CapabilityChanged(Overlay, Off, …)` формирует оболочка.

После каждого `show`/`move_to`: `if image.dpi != facts.dpi { tx.send(OverlayScaleChanged { dpi: facts.dpi }) }` — единственный триггер, без опоры на `WM_DPICHANGED` (ADR-0005). При расхождении бейдж всё равно ставится по фактическим `image.width/height`, поэтому не обрезается.

`dpi_for(anchor)` исполняется **синхронно на потоке вызывающего**, без hop'а: он не трогает HWND нашего окна.

Запустить: `cargo test -p switcher-windows`
Ожидание: PASS, включая новый тест `dpi_for_answers_from_a_foreign_thread` — `Arc<Overlay>` передаётся в `std::thread::spawn`, оба потока зовут `dpi_for(Fixed)`, значения совпадают и вызов не зависает (это и есть проверка UNVERIFIED-пункта о потоковой аффинности из `docs/research/2026-08-25-m1-api-research/geometry.md`).

- [ ] **Шаг 6: smoke-пример и чеклист**

`examples/overlay_smoke.rs`: (1) **первой строкой** `dpi::ensure_per_monitor_v2()` и `info!` с фактической awareness — примеры манифеста не получают (ADR-0010), без этого прототип снимал бы риск на виртуализованных координатах; (2) синтетический `BadgeImage`: сплошной непрозрачный прямоугольник 44×26 (при `a = 255` премультипликация тождественна) плюс полностью нулевой угловой блок 8×8 — прозрачный угол доказывает, что `ULW_ALPHA` работает, а не просто рисуется квадрат; байты укладывать как `[B, G, R, A]` (ADR-0006); (3) путь курсора считается из `GetSystemMetrics(SM_XVIRTUALSCREEN/SM_YVIRTUALSCREEN/SM_CXVIRTUALSCREEN/SM_CYVIRTUALSCREEN)` (`WAM:1086`, `:6077`, `:6078`, `:6013`, `:6045`) — 40 шагов по диагонали всего виртуального рабочего стола с `move_to(Cursor(p))` и паузой 150 мс, `dpi_for` печатается на каждом шаге; (4) в конце `hide()` и штатный выход.

Дополнить `docs/smoke/m1-windows.md` (создан задачей 9) секцией «Оверлей» с наблюдаемыми ожиданиями: бейдж поверх всех окон, включая максимизированное; клик по бейджу попадает в окно под ним (click-through); фокус не уходит с активного окна (заголовок остаётся активным); на 100 % и 200 % мониторах бейдж одного видимого размера, зазор 12 логических px; при пересечении границы мониторов в логе появляется ровно одна строка `OverlayScaleChanged { dpi }` на переход; прозрачный угол виден насквозь; в простое (`hide()` вызван, пример ещё жив) Task Manager показывает 0 % CPU. Отдельным пунктом — UNVERIFIED-вопрос из `docs/research/2026-08-25-m1-api-research/geometry.md`: приходит ли `WM_DPICHANGED` нашему `WS_EX_NOACTIVATE|WS_EX_TOOLWINDOW` окну (сменить масштаб в Параметрах при стоящем бейдже и посмотреть, есть ли строка лога); ответ записать в чеклист, поведение от него не зависит.

Запустить: `cargo run -p switcher-windows --example overlay_smoke`
Ожидание: все пункты секции «Оверлей» отмечены с записанными наблюдениями; если click-through или topmost не работают — это отказ ADR-0004, и он требует правки ADR, а не подгонки флагов наугад.

- [ ] **Шаг 7: гейты и коммит**

Запустить: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
Ожидание: чисто.

```bash
git add crates/switcher-windows docs/smoke/m1-windows.md
git commit -m "feat(windows): layered overlay window with pure placement geometry"
```

---

### Задача 11: pointer.rs (Raw Input) — бейдж следует за курсором, риск №1 (2/2)

**Реализовано 2026-09-07 в основной папке.** Четыре теста курсора включают фактическую регистрацию/снятие через Windows и регрессию `WM_QUIT`. Независимое ревью выявило поглощение `WM_QUIT` фильтрованным `PeekMessageW`; исправлено возвратом `PumpVerdict::Quit`. Шаг 5 реализован как интерактивный режим, но визуальная и нагрузочная приёмка пока не выполнена.

**Файлы:**
- Создать: `crates/switcher-windows/src/pointer.rs` — Raw Input: взвод/снятие, коалесценция, `cursor_pos()`
- Изменить: `crates/switcher-windows/src/lib.rs` — `pub mod pointer;`
- Изменить: `crates/switcher-windows/Cargo.toml` — фича `windows`: `Win32_UI_Input`
- Изменить: `crates/switcher-windows/examples/overlay_smoke.rs` — второй режим: слежение за живым курсором
- Изменить: `docs/smoke/m1-windows.md` — секция «Курсор»

**Интерфейсы:**
- Потребляет: задача 8 — `PointerTracker`, `PlatformEvent::PointerMoved`, `Point`; задача 9 — `win_util` (message-only окно + насос); задача 10 — `Overlay` (для примера).
- Производит: `pointer::Pointer::new(tx) -> Result<Pointer, PlatformError>` (реализует `PointerTracker`).

- [x] **Шаг 1: тест на главный футган — `RIDEV_REMOVE` с NULL-целью (red)**

Единственное место `pointer.rs`, которое можно проверить unit-тестом, и ровно то, где ошибка тихая: Learn документирует «If **RIDEV_REMOVE** is set and the **hwndTarget** member is not set to NULL, then RegisterRawInputDevices function will fail», а для `RIDEV_INPUTSINK` — «hwndTarget must be specified». Перепутать местами — снятие подписки молча провалится, и ADR-0003 будет нарушен без единого симптома. Поэтому конструирование запроса выносится в чистую функцию и тестируется:

```rust
/// `arm == false` MUST clear hwndTarget: RIDEV_REMOVE with a non-NULL target fails.
fn raw_input_request(hwnd: HWND, arm: bool) -> RAWINPUTDEVICE;

#[test]
fn disarm_request_has_a_null_target_and_arm_request_does_not() {
    let hwnd = HWND(0x1234usize as *mut _);
    let arm = raw_input_request(hwnd, true);
    assert_eq!(arm.dwFlags, RIDEV_INPUTSINK);
    assert!(!arm.hwndTarget.is_invalid());
    let off = raw_input_request(hwnd, false);
    assert_eq!(off.dwFlags, RIDEV_REMOVE);
    assert!(off.hwndTarget.is_invalid(), "RIDEV_REMOVE requires a NULL target");
}
```

Проверенные факты: `RAWINPUTDEVICE { usUsagePage, usUsage, dwFlags, hwndTarget }` (`UI/Input/mod.rs:145`), `RIDEV_INPUTSINK = 256` (`:254`), `RIDEV_REMOVE = 1` (`:258`), `RegisterRawInputDevices(&[RAWINPUTDEVICE], u32) -> Result<()>` (`:61`), `HWND::is_invalid()` и `HWND: Default` (нулевой HWND) — `Foundation/mod.rs:5671`, `:5676`. Мышь — `usUsagePage = 0x01`, `usUsage = 0x02`.

Запустить: `cargo test -p switcher-windows pointer`
Ожидание: FAIL — `raw_input_request` ещё `todo!("task 11")`.

- [x] **Шаг 2: реализовать запрос и поток курсора**

`Pointer::new` поднимает выделенный поток с message-only окном и насосом; **при старте подписки нет** — взведение только по `set_active(true)`. `set_active` вызывается с потока ядра, а `RegisterRawInputDevices` обязана исполниться на потоке-владельце очереди сообщений `hwndTarget`, поэтому `set_active` делает `PostThreadMessageW(thread_id, WM_APP_ARM, wparam = arm as usize, 0)`, а регистрация — в обработчике. Хранить в структуре только `thread_id: u32` и `Sender`; HWND границу потока не пересекает.

`// SAFETY:` у блока регистрации обязан назвать: (1) слайс из одного `RAWINPUTDEVICE` жив на время вызова; (2) `cbsize == size_of::<RAWINPUTDEVICE>() as u32`; (3) при `RIDEV_REMOVE` цель нулевая, при `RIDEV_INPUTSINK` — HWND, созданный этим же потоком и живой до его завершения.

`Pointer` в M1 **не** супервизируется — по тому же основанию, что ADR-0007 приводит для оверлея: перезапущенный поток вернулся бы снятым, пока бейдж видим, то есть состояние пришлось бы восстанавливать реплеем. Сбой конструирования → `Err(PlatformError::new("pointer_thread_failed", …))`. Если ревью сочтёт, что курсор обязан супервизироваться — это правка ADR-0007 через скил `adr`, а не решение внутри задачи.

Запустить: `cargo test -p switcher-windows pointer`
Ожидание: PASS.

- [x] **Шаг 3: позиция берётся из `GetCursorPos`, а не из `RAWMOUSE`**

`WM_INPUT` (`= 255`, `WAM:6990`) используется **только как признак «что-то двинулось»**; координаты читаются `GetCursorPos(*mut POINT) -> Result<()>` (`WAM:825`). Основание, а не вкусовщина: `RAWMOUSE.usFlags` бывает `MOUSE_MOVE_RELATIVE = 0` или `MOUSE_MOVE_ABSOLUTE = 1` (`UI/Input/mod.rs:103`, `:101`), то есть сырые данные у большинства мышей — device-relative дельты, минующие ускорение указателя; порту нужны физические экранные пиксели (ADR-0005). Побочная выгода: `GetRawInputData` и разбор `RAWINPUT`-юниона не нужны вовсе — минус один источник unsafe.

`cursor_pos()` (одиночное чтение для разрешения якоря) вызывается синхронно на потоке вызывающего: `GetCursorPos` не принимает HWND нашего окна — то же обоснование, что у `dpi_for`.

- [x] **Шаг 4: коалесценция без таймера**

При `RIDEV_INPUTSINK` `WM_INPUT` приходит на каждый пакет мыши (125–1000 Гц). Коалесценция делается дренажом очереди, а не таймером — иначе появилось бы третье взводимое исключение к ADR-0003: получив `WM_INPUT`, вычерпать все уже стоящие в очереди `WM_INPUT` через `PeekMessageW(&mut msg, Some(hwnd), WM_INPUT, WM_INPUT, PM_REMOVE)` (`WAM:1842`, `PM_REMOVE = 1` `:5493`) и только потом один раз прочитать `GetCursorPos` и отправить одно `PointerMoved`. Всплеск сворачивается в одно событие, последняя позиция никогда не теряется, ни одного таймера не появляется.

Счётчик отправленных `PointerMoved` за секунду печатать `debug!` в примере (шаг 5). Если измерение даст больше ~120 событий/с при быстром движении — добавить нижний порог интервала (`min_interval_ms`) как чистую функцию `should_send(now_ms, last_sent_ms, min_interval_ms) -> bool` с табличным тестом; **до измерения не добавлять** (ADR-0004 требует «до частоты кадров», но само число — предмет замера, а не догадки).

- [ ] **Шаг 5: расширить smoke до живого курсора и доказать нулевой простой**

В `overlay_smoke.rs` добавить второй режим (аргумент `follow`): подписаться на канал `PlatformEvent`, вызвать `pointer.set_active(true)`, показать бейдж и на каждое `PointerMoved` вызывать `overlay.move_to(Cursor(pos))`; раз в секунду печатать `debug!` со счётчиком событий; по первому Enter — `set_active(false)` и `hide()`; второе Enter завершает процесс. Так скрытый режим остаётся доступен для проверки простоя.

Обязательный шаг наблюдаемости (это и есть проверка ADR-0003, а не украшение): в режиме `follow` после `set_active(false)` при **скрытом** бейдже прогнать мышь по всему экрану с `RUST_LOG=switcher_windows=trace`.

Запустить: `cargo run -p switcher-windows --example overlay_smoke -- follow`
Ожидание: (1) бейдж едет за курсором без видимого отставания и без джиттера, включая переход между мониторами 100 %/200 % — на переходе появляется одна строка `OverlayScaleChanged`, а размер бейджа исправляется следующим показом; (2) после снятия — в логе **ноль** строк `PointerMoved` на любой прогон мыши; (3) Task Manager: 0 % CPU при снятой подписке. Ненулевое число `PointerMoved` при снятой подписке = `RIDEV_REMOVE` провалился (почти наверняка ненулевой `hwndTarget`).

- [x] **Шаг 6: гейты и коммит**

Запустить: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
Ожидание: чисто.

```bash
git add crates/switcher-windows docs/smoke/m1-windows.md
git commit -m "feat(windows): raw input pointer tracking armed only while the badge is visible"
```

---

### Задача 12: layout_monitor.rs + layout_smoke — риск №2

**Реализовано 2026-09-07 в основной папке.** Пробник подтвердил регистрацию обоих хуков и shell-сообщения 6/32774; десять переключений языка и elevated-сценарий ещё не проверены. Shell получает `Degraded` до первого `HSHELL_LANGUAGE`. Актуальный код в `layout_monitor/` соблюдает ADR-0011 (нет чтения собственного потока при NULL foreground). Остановка и многоканальные отчёты супервизии зафиксированы в ADR-0012.

**Файлы:**
- Создать: `crates/switcher-windows/src/layout_monitor.rs` — поток раскладки: два источника + взводимый фолбэк-опрос, `impl LayoutMonitor`
- Создать: `crates/switcher-windows/src/layout_monitor/classify.rs` — чистая лестница классификации переднего окна + табличные тесты
- Создать: `crates/switcher-windows/examples/layout_probe.rs` — эксперимент по двум открытым вопросам
- Создать: `crates/switcher-windows/examples/layout_smoke.rs` — печать событий с источником и временем
- Изменить: `crates/switcher-windows/src/lib.rs`, `docs/smoke/m1-windows.md`
- Изменить: `crates/switcher-windows/Cargo.toml` — фичи `windows`: `Win32_UI_Accessibility`, `Win32_UI_Input_KeyboardAndMouse`, `Win32_Globalization`, `Win32_Storage_Packaging_Appx` (`Win32_System_Threading` уже включена задачей 9)

**Интерфейсы:**
- Потребляет: задача 8 — `LayoutMonitor`, `LayoutId`, `LangTag`, `LayoutSource`, `Capability::{LayoutShellHook, LayoutForegroundHook}`, `CapabilityReport`; задача 9 — `win_util`, `supervise::spawn_supervised`.
- Производит: `layout_monitor::LayoutHooks::new(tx) -> Result<LayoutHooks, PlatformError>` (реализует `LayoutMonitor::current`); `classify::{ForegroundFacts, PollDecision, decide}`.

- [ ] **Шаг 1: эксперимент №1 — жив ли `HSHELL_LANGUAGE` на Windows 11**

Открытый вопрос №2 из `docs/research/2026-08-25-m1-api-research/degradation.md`: страница `nc-winuser-shellproc` отдаёт 404, семантика `HSHELL_LANGUAGE` не подтверждена, и есть сомнение, отдаёт ли его современный Windows вообще. Это предмет измерения, не предположения. `examples/layout_probe.rs`: message-only окно, `RegisterWindowMessageW("SHELLHOOK")` (`WAM:1956`) → id сообщения, `RegisterShellHookWindow(hwnd) -> BOOL` (`:1943`, ноль = отказ, проверять руками), `DeregisterShellHookWindow` в `Drop` (`:491`). В wndproc: на зарегистрированное сообщение печатать `wparam` (код `HSHELL_*`), `lparam` в hex и метку времени; `HSHELL_LANGUAGE = 8` (`:4018`).

Запустить: `cargo run -p switcher-windows --example layout_probe`, затем **сначала положительный контроль** — открыть и закрыть Блокнот, убедившись, что в логе есть хотя бы одна строка с `HSHELL_WINDOWCREATED`/`HSHELL_WINDOWACTIVATED`/`HSHELL_WINDOWDESTROYED` (коды 1/4/2). Без этого «нет `code=8`» неотличимо от «сообщения shell-hook вообще не доставляются нашему окну», а `RegisterShellHookWindow != 0` исключает только отказ вызова, не отсутствие доставки. Только при живом канале — 10 раз переключить раскладку (Win+Space и Alt+Shift) в Блокноте.
Ожидание — развилка, обе ветки легальны (и трактуются только после успешного положительного контроля):
- строка с `code=8` появляется в пределах ~1 с после каждого переключения ⇒ источник жив; из напечатанного `lparam` записать в чеклист, HKL это или нет (документация этого не даёт);
- ни одной строки с `code=8` на 10 переключений ⇒ источник мёртв на этой сборке Windows. Тогда: shell hook в задаче не регистрируется, `Capability::LayoutShellHook` не заводится, и **обязателен шаг «правка ADR-0003 и `docs/architecture/overview.md`: источников два, а не три»** через скил `adr` в этом же коммите. Обнаружение «источник жив, но молчит» в M1 не входит (ADR-0007), поэтому мёртвый источник надо именно убрать, а не оставить «на всякий случай».

Результат с датой и `winver` — в `docs/smoke/m1-windows.md`.

- [ ] **Шаг 2: эксперимент №2 — что возвращает `GetKeyboardLayout(tid)` для elevated-потока**

Открытый вопрос №3 из `docs/research/2026-08-25-m1-api-research/degradation.md`, и он определяет объём исключения ADR-0003. В `layout_probe.rs` добавить `SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, None, Some(cb), 0, 0, WINEVENT_OUTOFCONTEXT)` (`UI/Accessibility/mod.rs:222`, `WINEVENTPROC = Option<unsafe extern "system" fn(…)>` `:21016`; `EVENT_SYSTEM_FOREGROUND = 3`, `WINEVENT_OUTOFCONTEXT = 0` — `WAM:3467`, `:6872`) и печатать по каждому событию: `hwnd`, `GetClassNameW` (`WAM:795`), `pid`/`tid` из `GetWindowThreadProcessId` (`:1184`), `GetKeyboardLayout(tid)` в hex (`UI/Input/KeyboardAndMouse/mod.rs:68`, `HKL(pub *mut c_void)` `:319`), а также `GetCurrentThreadId()` внутри колбэка — последнее проверяет предположение «колбэк исполняется на потоке-установщике», от которого зависит вся конструкция с `thread_local!` (шаг 5).

**Второй, независимый от фокуса триггер выборки — обязателен.** `EVENT_SYSTEM_FOREGROUND` не приходит, когда раскладку меняют внутри уже сфокусированного окна, поэтому «синхронность HKL с переключениями» на одном этом событии наблюдать нельзя: наблюдатель попал бы в ветку «HKL залипает» по причине, не связанной с вопросом. Добавить в пробник ручную выборку — по нажатию Enter в консоли печатать `GetKeyboardLayout(tid)` для текущего переднего окна, — и снимать её после каждого переключения раскладки.

Запустить: `cargo run -p switcher-windows --example layout_probe`, затем поднять elevated `cmd.exe`, дать ему фокус, переключать раскладку внутри него
Ожидание — развилка:
- младшее слово HKL меняется 0x0409 ↔ 0x0419 синхронно с переключениями ⇒ `GetKeyboardLayout(tid)` работает и для elevated. Тогда взводимое исключение сужается до UWP/консоли, и **обязателен шаг «отредактировать ADR-0003 в сторону строгости»** (скил `adr`): elevated из формулировки исключения убирается, шаг 4 лестницы (`OpenProcess`) остаётся только как fail-safe для непрозрачных процессов;
- HKL = 0 либо залипает на нашей раскладке ⇒ elevated остаётся в лестнице как есть, ADR-0003 не меняется.

Результат записать в чеклист. Оба эксперимента можно и нужно сделать одним коммитом до основного кода — они дешёвые, а их итог меняет состав задачи.

- [x] **Шаг 3: табличные тесты лестницы классификации (red)**

`classify.rs`: `ForegroundFacts { tid: u32, is_own_process: bool, class_name: String, open_process: OpenOutcome, package: PackageOutcome }` и `decide(&ForegroundFacts) -> PollDecision`, где `PollDecision { Keep, Arm(&'static str), Disarm }`. Чистая функция: все факты ОС приходят аргументами, `unsafe` остаётся в добыче. Лестница из ADR-0007 §4, fail-safe в сторону взвода:

| вход | ожидание |
|---|---|
| `tid == 0` | `Keep` (окно умерло — состояние не менять) |
| `is_own_process` | `Keep` (наш трей/оверлей) |
| класс из «слепого» списка | `Arm("blind_window_class")` |
| `open_process == AccessDenied` | `Arm("opaque_process")` |
| `open_process == Ok`, `package == NoPackage` | `Disarm` |
| `open_process == Ok`, `package == Packaged` | `Arm("packaged_app")` |
| `open_process == Ok`, `package == ProbeFailed` | `Arm("probe_failed")` |
| `open_process == OtherError` | `Arm("probe_failed")` |

Строки классов (`ConsoleWindowClass`, `PseudoConsoleWindow`, `CASCADIA_HOSTING_WINDOW_CLASS`, `ApplicationFrameWindow`, `Windows.UI.Core.CoreWindow`) — **UNVERIFIED**: их нет ни в вендоренных исходниках, ни в проверенной документации. Подтвердить через Spy++ в smoke-чеклисте; именно поэтому шаг 3 лестницы не единственный — шаг 4 закрывает случай неверного имени класса.

Запустить: `cargo test -p switcher-windows classify`
Ожидание: FAIL — `decide` ещё `todo!("task 12")`.

- [x] **Шаг 4: реализовать `decide` и добычу фактов**

Пробник — строго `OpenProcess(PROCESS_QUERY_INFORMATION, false, pid)` (`System/Threading/mod.rs:1192`), **не** `PROCESS_QUERY_LIMITED_INFORMATION`: Learn документирует последнее как намеренно ослабленное подмножество, доступное даже к protected processes, то есть как пробник оно бесполезно. Сопоставление ошибки — `WIN32_ERROR::from_error(&e) == Some(ERROR_ACCESS_DENIED)` (`extensions/Win32/Foundation/WIN32_ERROR.rs`, `ERROR_ACCESS_DENIED = WIN32_ERROR(5)`). Успешный хэндл обернуть в `windows::core::Owned<HANDLE>` (RAII, `impl Free for HANDLE` есть; путь через реэкспорт `windows::core`, чтобы не заводить прямую зависимость на `windows-core` ради одного типа) и передать в `GetPackageFullName(handle, &mut len, None) -> WIN32_ERROR` (`Storage/Packaging/Appx/mod.rs:206`): `APPMODEL_ERROR_NO_PACKAGE = 15700` ⇒ обычный desktop, `ERROR_INSUFFICIENT_BUFFER = 122` ⇒ упакованное приложение, иное ⇒ `probe_failed`. `IsImmersiveProcess` не использовать: в `windows-0.62.2` она сгенерирована как `BOOL → Result<()>`, поэтому `Err` неотличим от «не immersive».

Запустить: `cargo test -p switcher-windows classify`
Ожидание: PASS.

- [x] **Шаг 5: источник 2 — WinEvent-хук и приведение HKL к тегу языка**

Поток раскладки: message-only окно + насос + `SetWinEventHook(EVENT_SYSTEM_FOREGROUND, …, WINEVENT_OUTOFCONTEXT)`, `UnhookWinEvent` в `Drop` (`:675`, `BOOL`). `WINEVENTPROC` — обычная `extern "system" fn` без захвата, поэтому `Sender<PlatformEvent>` держится в `thread_local!` этого потока; корректность этого приёма опирается на «колбэк исполняется на потоке-установщике, который качает сообщения» — формулировка **UNVERIFIED**, сверить по Learn перед кодом (скил `platform-api-work`), а фактически подтверждается напечатанным в шаге 2 `GetCurrentThreadId`.

Раскладка на Windows per-thread, поэтому чтение — `GetKeyboardLayout(GetWindowThreadProcessId(GetForegroundWindow(), None))`; при нулевом HWND — ошибка `foreground_unavailable`, без чтения потока 0 (ADR-0011). `LayoutId` = `hkl.0 as usize as u64`. Тег языка: младшее слово HKL как LANGID → `LCIDToLocaleName(langid as u32, Some(&mut buf), 0) -> i32` (`Globalization/mod.rs:684`), ноль = отказ. Два предупреждения: «младшее слово HKL — это LANGID» и «LANGID годится как LCID» — **UNVERIFIED**, обязательны к сверке по Learn до кода; `LOCALE_NAME_MAX_LENGTH` в `windows-0.62.2` есть (`System/SystemServices/mod.rs:2785`, значение 85), но лежит за фичей `Win32_System_SystemServices`, которой в карте фич нет: тянуть целую фичу ради одной константы не обязательно — допустимо захардкодить 85 со ссылкой на это место. Отказ конверсии ⇒ `LangTag::new("")`, что ядро уже корректно превращает в метку `"??"` (тест `unknown_language_gets_uppercased_two_letter_label_and_fallback_bg`) — деградация без специального кода.

Событие уходит как `LayoutChanged { source: LayoutSource::ForegroundChange }`; дедуп ядра глотает повторы от других источников (тест `same_layout_from_another_source_is_deduplicated`), поэтому отправлять можно безусловно.

- [x] **Шаг 6: принять решение и записать ADR — супервизия потока с двумя возможностями**

Пробел, который ADR-0007 оставил: `supervise::spawn_supervised(cap, tx, run)` принимает **одну** `Capability`, а поток раскладки по ADR-0009 хостит две (`LayoutShellHook` + `LayoutForegroundHook`) — при исчерпании бюджета вторая осталась бы в карте как `Ok`, будучи мёртвой. Не решать это молча. Варианты: (а) расширить внутренний хелпер до `&[Capability]` и слать `Off` по всем при исчерпании бюджета; (б) разнести источники на два потока, каждый со своим окном и своей супервизией (расходится с картой потоков ADR-0009). Выбрать, зафиксировать через скил `adr` (правка ADR-0007 либо новый ADR) и только потом писать проводку. Если шаг 1 показал, что shell hook мёртв, вопрос снимается сам — записать и это.

- [x] **Шаг 7: взводимый фолбэк-опрос 500 мс и три слоя наблюдаемости**

Решение о взводе принимается в обработчике уже существующего `EVENT_SYSTEM_FOREGROUND`, поэтому «решить, опрашивать ли» не стоит ни одного опроса. Механика: `SetTimer(Some(hwnd), IDT_FALLBACK, 500, None) -> usize` (ноль = отказ, `WAM:2238`) и `KillTimer(Some(hwnd), IDT_FALLBACK) -> Result<()>` (`:1365`) на окне **потока раскладки** (ADR-0009); в `WM_TIMER` (`= 275`, `:7128`) — то же чтение, что в шаге 5, с `source: LayoutSource::ForegroundPoll`. Взвод/снятие идемпотентны: повторный `Arm` при уже взведённом таймере ничего не делает и ничего не логирует.

Три слоя из ADR-0007, все обязательны:
1. система типов: `LayoutSource::ForegroundPoll` не должен появляться в логе, пока переднее окно — обычное Win32-приложение;
2. `info!(target: "switcher_windows::layout", …)` ровно на взвод (с `hwnd`, `class`, `reason`, `interval_ms = 500`) и на снятие (с `ticks = n`); сам тик — `trace!`, поэтому простой даёт ноль лог-трафика, а счётчик тиков печатается только в строке снятия;
3. `typeperf "\Thread(layout_smoke/*)\Context Switches/sec"` (или Process Explorer → Threads) на потоке раскладки: живой `SetTimer` даёт ~2 переключения в секунду, снятый — ноль. Это единственный слой, который наше логирование подделать не может. **Имя экземпляра — по имени образа**, а здесь запускается пример (`target/debug/examples/layout_smoke.exe`), а не `lang-switcher.exe`; в задаче 21 та же проверка идёт уже по `lang-switcher`. Маска `/*` агрегирует все потоки процесса, поэтому конкретный поток надёжнее опознавать в Process Explorer по Context Switch Delta.

- [ ] **Шаг 8: layout_smoke и чеклист**

`examples/layout_smoke.rs`: поднять `LayoutHooks`, печатать каждое событие как `+123ms source=ForegroundChange layout=0x4190419 lang=ru-RU` (миллисекунды от старта процесса), плюс строку по `LayoutMonitor::current()` на старте.

Запустить: `cargo run -p switcher-windows --example layout_smoke`
Ожидание, по пунктам чеклиста (секция «Раскладка» в `docs/smoke/m1-windows.md`): переключение в Блокноте даёт событие с временем < 300 мс и **без** `ForegroundPoll`; переключение фокуса между окнами с разными раскладками даёт `ForegroundChange`; в elevated `cmd` и в UWP-приложении — событие приходит (источник записать, для этого и печатается), а взвод/снятие опроса видны ровно двумя строками `info!`; после возврата фокуса в Блокнот `typeperf` показывает ноль переключений контекста. Имена классов окон из шага 3 подтвердить в Spy++ и записать фактические.

- [x] **Шаг 9: гейты и коммит**

Запустить: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
Ожидание: чисто.

```bash
git add crates/switcher-windows docs/smoke/m1-windows.md docs/architecture
git commit -m "feat(windows): layout sources with armed foreground fallback poll"
```

---

### Задача 13: tsf.rs — третий источник раскладки (COM/STA)

**Реализовано 2026-09-07.** Выбран `ITfInputProcessorProfileActivationSink`, прямой windows-core 0.62.2; контракт и отказ от автоматической agility — ADR-0013. Четыре теста TSF прошли. 45-секундный прогон на пользовательском desktop подтвердил подписку/завершение; глобальная доставка смен RU/EN пока не проверена.

**Файлы:**
- Создать: `crates/switcher-windows/src/tsf.rs` — STA-поток, COM-синк, RAII времени жизни
- Изменить: `crates/switcher-windows/src/lib.rs` — `pub mod tsf;`
- Изменить: `crates/switcher-windows/Cargo.toml` — фичи `windows`: `Win32_System_Com`, `Win32_UI_TextServices`; **плюс прямая зависимость `windows-core`** (см. шаг 3а)
- Изменить: `Cargo.toml` воркспейса — строка `windows-core = { version = "0.62", default-features = false, features = ["std"] }`
- Изменить: `crates/switcher-windows/examples/layout_smoke.rs` — поднимать и третий источник
- Изменить: `docs/smoke/m1-windows.md` — пункты по TSF
- Возможно создать: новый ADR (см. шаг 2) — номер брать следующий свободный по `ls docs/architecture/adr/`, не жёстко 0011: задача 15 тоже заводит ADR, и порядок выполнения решает, кому какой номер достанется

**Интерфейсы:**
- Потребляет: задачу 12 — читатель «HKL → (LayoutId, LangTag)» переиспользуется как есть; `Capability::LayoutTsf`, `supervise::spawn_supervised`.
- Производит: `tsf::TsfSource::new(tx) -> Result<TsfSource, PlatformError>` — третий поставщик `LayoutChanged { source: LayoutSource::Tsf }`.

- [x] **Шаг 1: сверка контракта TSF — до единой строки кода**

Контракт TSF в исследовании M1 **не проверялся**. Перед кодом обязательна сверка по vendor-документации (скил `platform-api-work`; context7 в этой сессии недоступен — использовать learn.microsoft.com и вендоренные исходники). Что именно сверить, по пунктам:

1. Семантика `ITfActiveLanguageProfileNotifySink::OnActivated` — стреляет ли она при переключении **обычной раскладки клавиатуры** (не TSF-профиля). Это ключевой вопрос: если нет, источник бесполезен.
2. Обязателен ли `ITfThreadMgr::Activate` на потоке до `AdviseSink`, и в каком порядке освобождать (`UnadviseSink` → `Deactivate` → release).
3. Откуда брать `ITfSource` для выбранного синка — из `ITfThreadMgr` (cast) или из `ITfInputProcessorProfiles`.
4. Требует ли поток насоса сообщений при живом синке.

Уже проверено мной по исходникам `windows-0.62.2` (использовать как отправную точку, не как замену сверке):
- `ITfActiveLanguageProfileNotifySink`, IID `b246cb75-a93e-4652-bf8c-b3fe0cfd7e57`, метод `OnActivated(clsid, guidprofile, factivated: BOOL)` → `UI/TextServices/mod.rs:4415, 4428`. **Ни langid, ни HKL в колбэк не приходят.**
- `ITfInputProcessorProfileActivationSink`, IID `71c6e74e-0f28-11d8-a82a-00065b84435c`, `OnActivated(dwprofiletype: u32, langid: u16, clsid, catid, guidprofile, hkl: HKL, dwflags: u32)` → `:8340`; трейт `_Impl` под `#[cfg(feature = "Win32_UI_Input_KeyboardAndMouse")]` (фича уже добавлена задачей 12). **Несёт и `langid`, и `hkl` напрямую.**
- `ITfSource::AdviseSink(riid, punk) -> Result<u32>` (cookie), `UnadviseSink(cookie)` → `:12558`; `ITfThreadMgr::Activate() -> Result<u32>`, `Deactivate() -> Result<()>` → `:13174`; `CLSID_TF_ThreadMgr = 529a9e6b-6587-4f23-ab9e-9c7d683e3c50` → `:65`.
- `windows::core::implement` доступен без фич: `windows-implement` 0.60.2 — жёсткая зависимость `windows-core` 0.62.2, реэкспорт в `windows-core-0.62.2/src/lib.rs:58`, а `windows-0.62.2/src/lib.rs:18` даёт `pub use windows_core as core`.

**UNVERIFIED (все четыре пункта выше):** ни семантика колбэков, ни правила времени жизни, ни требование насоса из исходников не выводятся.

Ожидание: короткая записка с ответами и ссылками — в теле коммита или в `docs/smoke/m1-windows.md`.

- [x] **Шаг 2: выбрать синк и записать ADR**

ADR-0009 в карте потоков называет `ITfActiveLanguageProfileNotifySink`. Проверенные подписи показывают, что `ITfInputProcessorProfileActivationSink` доставляет `langid` и `hkl` прямо в колбэк, тогда как названный в ADR — нет и потребовал бы второго чтения через читатель задачи 12. Это выбор платформенной техники, значит — правило 6 CLAUDE.md: выбрать по итогам шага 1 и **записать через скил `adr`** (правка ADR-0009 либо новый ADR со следующим свободным номером), и только затем писать код. Не решать молча ни в ту, ни в другую сторону.

Ожидание: в `docs/architecture/adr/` лежит зафиксированное решение с обоснованием; карта потоков в ADR-0009 согласована с ним.

- [x] **Шаг 3а: сделать `windows-core` прямой зависимостью — иначе `#[implement]` не соберётся**

Макрос `implement` раскрывается в **абсолютные** пути `::windows_core::…` (`windows-implement-0.60.2/src/gen.rs`), а абсолютный путь требует крейт в extern prelude, то есть прямую строку в `Cargo.toml`. Сейчас `windows-core` только транзитивный (`Cargo.lock`), у `switcher-windows` в зависимостях его нет. Добавить в `[workspace.dependencies]` строку `windows-core = { version = "0.62", default-features = false, features = ["std"] }` и в `[target.'cfg(windows)'.dependencies]` крейта — `windows-core = { workspace = true }`. Версия обязана совпадать с той, что тянет `windows` 0.62, иначе в графе окажутся два `windows-core` и типы перестанут совпадать; проверить `cargo tree -p switcher-windows -i windows-core` — должна быть ровно одна версия.

Отдельно: в задачах 12 и 14 путь `windows::core::Owned` берётся через реэкспорт (`windows-0.62.2/src/lib.rs` — `pub use windows_core as core`) и прямой зависимости не требует; она нужна только для раскрытия макроса здесь.

Запустить: `cargo tree -p switcher-windows -i windows-core`
Ожидание: одна версия, 0.62.x.

- [x] **Шаг 3: STA-поток и правило «MTA в процессе нет»**

Поток TSF поднимается отдельно и инициализирует COM как STA: `CoInitializeEx(None, COINIT_APARTMENTTHREADED) -> HRESULT`. `S_OK` и `S_FALSE` — успех; каждый такой вызов требует одного `CoUninitialize` на том же потоке после освобождения интерфейсов. **`RPC_E_CHANGED_MODE` — ошибка:** требуемый STA не установлен. Вернуть `PlatformError::new("com_init_failed", …)`, не создавать синк и не вызывать `CoUninitialize` за неудачную попытку. Исправлено по [контракту Microsoft](https://learn.microsoft.com/en-us/windows/win32/api/combaseapi/nf-combaseapi-coinitializeex). Проверить ветви `S_OK`, `S_FALSE`, `RPC_E_CHANGED_MODE` и прочей ошибки на границе RAII-гарда.

TSF получает собственный STA-поток по ADR-0009. Поведение `cpal`, допускающего `RPC_E_CHANGED_MODE`, нельзя переносить в этот гард. Апартмент определяется для каждого потока: наличие STA у звука само по себе не запрещает MTA на другом потоке; правило M1 «не заводить MTA» — ограничение проекта, не требование COM ко всему процессу.

Насос `GetMessageW` на этом потоке держать по умолчанию (правило проекта; необходимость подтверждается шагом 1).

- [x] **Шаг 4: синк, его время жизни и отправка события**

Реализация синка — `#[windows_core::implement(<выбранный интерфейс>)]` над структурой, хранящей `Sender<PlatformEvent>` (плоские данные, никаких HWND). В колбэке: собрать `LayoutChanged { layout, lang, source: LayoutSource::Tsf }` и отправить; при выбранном `ITfActiveLanguageProfileNotifySink` — предварительно прочитать раскладку читателем задачи 12 (синк говорит «когда», читатель — «что»); при `ITfInputProcessorProfileActivationSink` — взять `hkl`/`langid` из аргументов. Дедуп ядра уже гасит совпадения с двумя другими источниками.

RAII обязательна и парна: гард держит cookie от `AdviseSink` и в `Drop` делает `UnadviseSink` → `Deactivate` → отпускает интерфейсы → `CoUninitialize`, строго на том же потоке. Ни один COM-указатель не покидает поток TSF — иначе понадобилась бы маршализация, которой в проекте нет. `// SAFETY:` у каждого блока называет: (1) COM на этом потоке инициализирован и ещё не деинициализирован; (2) интерфейс получен из вызова, вернувшего `Ok`; (3) переданный `riid` указывает на статический GUID, живущий дольше вызова.

Поток супервизируется как источник раскладки: `spawn_supervised(Capability::LayoutTsf, tx, run)` (ADR-0007: backoff 250/1000/4000, сброс бюджета после 60 с, затем `CapabilityChanged(LayoutTsf, Off, "restart_budget_exhausted")`).

Запустить: `cargo build -p switcher-windows && cargo clippy -p switcher-windows --all-targets -- -D warnings`
Ожидание: чисто; при удалении `Win32_UI_TextServices` или `Win32_System_Com` из фич сборка падает.

- [ ] **Шаг 5: smoke — источник действительно стреляет**

В `layout_smoke.rs` поднимать `TsfSource` рядом с хуками задачи 12 (все три источника пишут в один канал, порядок сохраняется).

Запустить: `cargo run -p switcher-windows --example layout_smoke`, переключить раскладку 10 раз в Блокноте, затем в Word/браузере (полноценный TSF-клиент)
Ожидание: строки с `source=Tsf` появляются, и по времени они либо опережают, либо догоняют `ForegroundChange` — обе картины валидны, важно, что источник не молчит. Ноль строк `source=Tsf` на 10 переключений ⇒ выбранный синк не покрывает переключение раскладки: вернуться к шагу 1, а не «дожимать» код. Отдельный пункт чеклиста: при завершении примера нет ни зависания на выходе, ни сообщений COM в отладочном выводе — это наблюдаемое доказательство парности `Drop`.

- [x] **Шаг 6: гейты и коммит**

Запустить: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
Ожидание: чисто.

```bash
git add crates/switcher-windows docs/architecture docs/smoke/m1-windows.md
git commit -m "feat(windows): TSF layout source on a dedicated STA thread"
```

---

### Задача 14: autostart.rs — `HKCU\...\Run`

**Файлы:**
- Создать: `crates/switcher-windows/src/autostart.rs` — `impl Autostart` + чистые хелперы и их тесты
- Создать: `crates/switcher-windows/examples/autostart_smoke.rs` — реальная запись/удаление с восстановлением исходного состояния
- Изменить: `crates/switcher-windows/src/lib.rs` — `pub mod autostart;`
- Изменить: `crates/switcher-windows/Cargo.toml` — фича `windows`: `Win32_System_Registry`
- Изменить: `docs/smoke/m1-windows.md` — секция «Автозапуск»

**Интерфейсы:**
- Потребляет: задачу 8 — `Autostart`, `PlatformError::new(code, detail)`.
- Производит: `autostart::RegistryAutostart::new() -> RegistryAutostart` (реализует `Autostart`); коды ошибок `"registry_write_denied"`, `"registry_error"`, `"exe_path_unavailable"` — их потребляет задача 19 при построении `CapabilityReport`. Четвёртый код, `"registry_value_missing"`, существует только внутри `code_for` и наружу не выходит: `ERROR_FILE_NOT_FOUND` трактуется как `Ok` (значение просто отсутствует), поэтому задаче 19 ветки под него не нужно.

- [ ] **Шаг 1: тесты чистых хелперов (red)**

Два места, где ошибка не видна глазами, оба чистые и потому тестируемые:

```rust
/// REG_SZ payload: UTF-16LE, quoted, NUL-terminated. Explicit little-endian bytes
/// instead of a transmute — the registry stores LE and `unsafe` buys nothing here.
fn run_value_bytes(exe: &Path) -> Result<Vec<u8>, PlatformError>;
/// Adapter-authored stable code for a registry failure (ADR-0007).
fn code_for(err: WIN32_ERROR) -> &'static str;

#[test]
fn run_value_is_quoted_utf16le_with_terminator() {
    let b = run_value_bytes(Path::new(r"C:\Program Files\ls\lang-switcher.exe")).unwrap();
    assert_eq!(&b[0..2], &[b'"', 0], "path must be quoted: it contains spaces");
    assert_eq!(&b[b.len() - 4..], &[b'"', 0, 0, 0], "closing quote then the NUL");
    assert_eq!(b.len() % 2, 0);
}

#[test]
fn registry_errors_map_to_stable_codes() {
    assert_eq!(code_for(ERROR_ACCESS_DENIED), "registry_write_denied");
    assert_eq!(code_for(ERROR_FILE_NOT_FOUND), "registry_value_missing");
    assert_eq!(code_for(WIN32_ERROR(1234)), "registry_error");
}
```

Путь брать через `std::os::windows::ffi::OsStrExt::encode_wide` (без потерь на не-UTF-8), внутренний нулевой символ ⇒ `Err(PlatformError::new("exe_path_unavailable", …))`. Кавычки обязательны: без них путь с пробелами Windows разбирает как команду с аргументами.

Запустить: `cargo test -p switcher-windows autostart`
Ожидание: FAIL — оба хелпера `todo!("task 14")`.

- [ ] **Шаг 2: сверить путь ключа и реализовать хелперы**

Подключ `Software\Microsoft\Windows\CurrentVersion\Run` и семантику `Run`-ключа сверить по Microsoft Learn («Run and RunOnce Registry Keys») перед кодом — **UNVERIFIED**, в исследовании M1 этой страницы нет. Имя значения — константа `"lang-switcher"`.

Запустить: `cargo test -p switcher-windows autostart`
Ожидание: PASS.

- [ ] **Шаг 3: `is_enabled` — разбор `WIN32_ERROR`, а не `Result`**

Ключевое отличие реестрового API от остального Win32 в `windows-0.62.2`: функции возвращают **`WIN32_ERROR`**, а не `Result`, поэтому `?` не работает и каждый код разбирается явно. Проверенные подписи: `RegOpenKeyExW(hkey, subkey, uloptions: Option<u32>, samdesired: REG_SAM_FLAGS, phkresult: *mut HKEY) -> WIN32_ERROR` (`System/Registry/mod.rs:376`), `RegQueryValueExW(hkey, name, lpreserved, lptype, lpdata: Option<*mut u8>, lpcbdata: Option<*mut u32>) -> WIN32_ERROR` (`:489`), `RegCloseKey -> WIN32_ERROR` (`:12`). Константы: `HKEY_CURRENT_USER` (`:737`), `KEY_READ = 131097` (`:756`), `KEY_SET_VALUE = 2` (`:757`), `REG_SZ = 1` (`:1806`).

Логика: отсутствие подключа или значения (`ERROR_FILE_NOT_FOUND = 2`) ⇒ `Ok(false)`; значение есть ⇒ `Ok(true)`; `ERROR_ACCESS_DENIED = 5` и прочее ⇒ `Err` с кодом из `code_for`. **Путь в значении с `current_exe` в M1 не сравнивается**: «значение есть = включено». Иначе переезд exe читался бы как «выключено», и по двухфазному контракту ADR-0007 («реестр побеждает») конфиг молча перезаписался бы в `false`. Сохранённый путь логировать `debug!`, чтобы расхождение было диагностируемо; самолечение при переезде — M2.

RAII: открытый ключ оборачивать в `windows::core::Owned<HKEY>` — `impl Free for HKEY` существует (`System/Registry/mod.rs:719`), то есть `RegCloseKey` в `Drop` даётся бесплатно и утечка хэндла на путях ошибок исключена. `// SAFETY:` называет: (1) `phkresult` указывает на живой стековый `HKEY`; (2) `Owned` создаётся только после `is_ok()`; (3) буфер под значение живёт всю длительность вызова, а `lpcbdata` содержит его размер в **байтах**.

- [ ] **Шаг 4: `set_enabled` — запись и удаление, идемпотентно**

Включение: `std::env::current_exe()` (ошибка ⇒ `"exe_path_unavailable"`), `run_value_bytes`, затем `RegCreateKeyExW(HKEY_CURRENT_USER, subkey, None, PCWSTR::null(), REG_OPTION_NON_VOLATILE, KEY_SET_VALUE, None, &mut hkey, None) -> WIN32_ERROR` (`:84`, `REG_OPTION_NON_VOLATILE = 0` `:1702`) и `RegSetValueExW(hkey, name, None, REG_SZ, Some(&bytes)) -> WIN32_ERROR` (`:626`) — параметр объявлен как `Option<&[u8]>`, `cbdata` крейт выводит из длины слайса, поэтому передавать нужно **весь** массив вместе с двумя нулевыми байтами терминатора. Включение при уже существующем значении перезаписывает путь — это и есть «повторный клик перезаявляет реестр» из ADR-0007.

Выключение: открыть с `KEY_SET_VALUE`; `ERROR_FILE_NOT_FOUND` ⇒ `Ok(())` (уже выключено); `RegDeleteValueW(hkey, name) -> WIN32_ERROR` (`:211`), его `ERROR_FILE_NOT_FOUND` ⇒ тоже `Ok(())`. Текст `detail` строить как `windows_core::Error::from(err).message()` — `impl From<WIN32_ERROR> for windows_core::Error` существует (`extensions/Win32/Foundation/WIN32_ERROR.rs`).

**Границы ответственности, обязательные к соблюдению (ADR-0007, двухфазный контракт):** адаптер **не** отправляет `CapabilityChanged` и вообще не владеет каналом — `Autostart` конструируется без `Sender`. Он только возвращает `Result<_, PlatformError>` с машинным кодом; преобразование в `Event::AutostartApplied { requested, ok }`, `Effect::SyncTrayMenu` и `CapabilityChanged(Autostart, …)` делает `switcher-app` (задача 19). Зафиксировать это док-комментарием у `impl Autostart` — тестом это не проверяется, значит должно быть написано словами для ревью.

Запустить: `cargo clippy -p switcher-windows --all-targets -- -D warnings`
Ожидание: чисто; при удалении `Win32_System_Registry` из фич сборка падает.

- [x] **Шаг 5: безопасная проверка чтения и изолированный roundtrip**

Исправление аудита 2026-09-07: восстановление одного boolean после записи в настоящий Run теряло исходную команду существующего значения. Поэтому `examples/autostart_smoke.rs` только читает настоящее состояние, а native-тест создаёт уникальный временный ключ вне Run, проверяет точные байты REG_SZ, повторные включение/выключение и удаление временного ключа. Настоящая запись автозапуска тестами не меняется.

Чтение: `cargo run -p switcher-windows --example autostart_smoke`. Native roundtrip: `cargo test -p switcher-windows autostart::tests::native_registry_roundtrip -- --nocapture` (требует доступ к записи HKCU). Проверка запуска при входе в Windows остаётся ручным пунктом после задачи 20.

Реализация дополнительно проверяет лимит Run в 260 UTF-16 единиц для всей команды с кавычками; путь сохраняется через `encode_wide`. Для `RegCreateKeyExW` windows-rs требует также feature `Win32_Security`. Гейты 2026-09-07: fmt, clippy и 111 тестов workspace прошли, native roundtrip выполнен вне sandbox с последующей проверкой удаления ключа. Независимое ревью: замечаний нет.

- [ ] **Шаг 6: гейты и коммит**

Запустить: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
Ожидание: чисто.

```bash
git add crates/switcher-windows docs/smoke/m1-windows.md
git commit -m "feat(windows): HKCU Run autostart with explicit WIN32_ERROR mapping"
```

---

### Задача 15: switcher-app — render.rs: растеризация бейджа и кэш

**Реализовано 2026-09-07:** Inter v4.1, статический сабсет 15 304 байта, [ADR-0014](../../architecture/adr/0014-embedded-badge-font.md). Модули оболочки экспортирует `src/lib.rs` с `forbid(unsafe_code)` для тестов и wiring. Метки ядра могут содержать Unicode; при любом отсутствующем глифе рисуется цветной квадрат. Семь тестов инвариантов, без golden-байтов.

**Файлы:**
- Создать: `crates/switcher-app/src/render.rs` — `BadgeMetrics`, `BadgeKey`, `RenderError`, `render_badge`, `measure_label`, `BadgeCache`, `MAX_ENTRIES`
- Создать: `crates/switcher-app/assets/fonts/<Font>-subset.ttf` — статический (не variable) сабсет `A–Z` + `?` + `0–9`
- Создать: `crates/switcher-app/assets/fonts/<файл лицензии шрифта>` — дословно из скачанного релиза
- Создать: `crates/switcher-app/assets/fonts/README.md` — источник, версия релиза, лицензия, точная команда сабсеттинга
- Создать: `docs/architecture/adr/00NN-embedded-badge-font.md` — номер следующий свободный (0011, либо 0012, если задача 13 уже заняла 0011)
- Изменить: `crates/switcher-app/src/main.rs` — `mod render;`
- Изменить: `crates/switcher-app/Cargo.toml` — добавить `thiserror = { workspace = true }`

**Интерфейсы:**
- Потребляет: `switcher_core::content::{BadgeContent, BadgeStyle, Rgb8}` с `Eq + Hash` (задача 8); `switcher_platform::events::BadgeImage { width, height, bgra_premul, dpi }` (задача 8); фичи `tiny-skia = ["std","simd"]`, `ab_glyph = ["std"]` (ADR-0008, задача 8).
- Производит: весь модуль `render` — для задачи 18 (второй растеризатор иконки трея) и задачи 19 (кэш во владении рантайма).

- [ ] **Шаг 1: выбрать шрифт, скачать, прочитать лицензию, сделать сабсет, закоммитить артефакт**

Кандидат — **Inter, статический инстанс SemiBold** (альтернативы того же класса: Noto Sans, Fira Sans, Roboto). **Лицензию не принимать по памяти:** скачать релиз с официальной страницы проекта, открыть вложенный файл лицензии (`OFL.txt` / `LICENSE.txt`), прочитать его целиком и положить в `assets/fonts/` без изменений. Если во вложенном файле лицензия не разрешает свободное распространение в составе бинарника — взять следующий кандидат.

Проверить, что файл статический, а не variable (иначе фича `ab_glyph/variable-fonts`, выключенная по ADR-0008, нам бы понадобилась):

```bash
python -c "from fontTools.ttLib import TTFont; print('fvar' in TTFont(r'Inter-SemiBold.ttf'))"
# ожидание: False
```

Сабсет (точные флаги `pyftsubset` **UNVERIFIED** — fonttools в этой сессии не проверялся; сверить по `pyftsubset --help` перед запуском и записать в README ровно ту команду, которая была реально выполнена):

```bash
pyftsubset Inter-SemiBold.ttf --unicodes=U+0030-0039,U+003F,U+0041-005A \
  --output-file=crates/switcher-app/assets/fonts/Inter-SemiBold-subset.ttf
```

`README.md` рядом с артефактом обязан содержать: URL и версию релиза, имя лицензии + имя вложенного файла, дословную команду сабсеттинга, размер результата. Без этого артефакт невоспроизводим.

Запустить: `ls -l crates/switcher-app/assets/fonts/`
Ожидание: три файла; `.ttf` — 10–40 КБ (полный Inter — сотни КБ, поэтому размер сам доказывает, что сабсет сработал).

- [ ] **Шаг 2: записать ADR о встроенном шрифте**

Скил `adr`, файл `docs/architecture/adr/00NN-embedded-badge-font.md` (номер — следующий свободный: задача 13 могла уже занять 0011). Это новая вшитая зависимость с лицензионными обязательствами, поэтому решение фиксируется отдельно от ADR-0006. Контекст: бейдж рисуется в рантайме (ADR-0006), нужен один вес, нужен `include_bytes!`. Решение: конкретное имя шрифта, версия, лицензия — **со ссылкой на вложенный файл, прочитанный на шаге 1**, статический инстанс, сабсет A–Z + `?` + 0–9 (метка всегда ASCII-uppercase, `BadgeContent::for_lang` это гарантирует, включая `"??"`). Последствия: обязательство собрать файл сторонних лицензий при упаковке (M2) — лицензия шрифта + BSD-3-Clause у `tiny-skia` + Apache-2.0 у `ab_glyph`.

- [ ] **Шаг 3: написать падающие тесты (инварианты, не golden-байты)**

Побайтовая детерминированность между сборками (разные `target-feature`, `simd`, версия растеризатора глифов) **не гарантируется** — golden-тестов не писать (ADR-0006). Тестовый модуль в `render.rs`, над ним заглушки с `todo!()`:

- `size_scales_with_dpi`: `render_badge(.., dpi=192)` даёт `height == 2 * height(96)`; `width(192) >= 2 * width(96) - 2`; `img.dpi == key.dpi`; `bgra_premul.len() == (w*h*4) as usize`.
- `corner_pixel_is_transparent`: alpha пикселя (0,0) (байт 3) `== 0` — доказывает, что скругление действительно есть.
- `center_pixel_is_background_in_bgra_order`: центральный пиксель `== [bg.b, bg.g, bg.r, 255]`. Это единственный тест, который поймает перевёрнутый канал.
- `all_pixels_are_premultiplied`: для каждого пикселя `b <= a && g <= a && r <= a`.
- `text_style_has_foreground_ink`: при `BadgeStyle::Text` и белом `fg` есть хотя бы 8 пикселей с `b,g,r >= 200 && a == 255`.
- `color_style_at_96_dpi_is_square_and_filled`: `BadgeStyle::Color`, `dpi = 96` → `w == h`, центр — фон, углы прозрачны, не все байты нули. **Это тест нижней границы:** `Pixmap::fill_path` тихо выходит (`log::warn!` + `return`) при почти нулевых bounds пути (`tiny-skia-0.12.0/src/painter.rs:229-233`), и без этого теста «пустой бейдж на 100 %» прошёл бы незамеченным.
- `label_without_glyphs_falls_back_to_swatch`: метка `"ЖЖ"` (в сабсете глифов нет) → `w == h` (откат на сплошной swatch) и непустое множество непрозрачных пикселей. Пустой бейдж невозможен.
- `measure_label_ink_grows_with_text`: `measure_label(font, "RU", 16.0).width > measure_label(font, "R", 16.0).width > 0.0`.
- `cache_hits_do_not_grow`: два `cache.image(&content, 96)` → `cache.len() == 1`.
- `cache_clears_when_full`: 17 разных `dpi` → `cache.len() <= MAX_ENTRIES`.
- `cache_clear_empties`: после `clear()` → `len() == 0`.

- [ ] **Шаг 4: убедиться, что тесты падают**

Запустить: `cargo test -p switcher-app`
Ожидание: FAIL на `todo!()` во всех новых тестах; тесты ядра зелёные.

- [ ] **Шаг 5: реализация**

Конвейер строго по ADR-0006 (все вызовы сверены с `tiny-skia 0.12.0` / `ab_glyph 0.2.32`):

1. `let sc = font.as_scaled(PxScale::from((metrics.text_px_dip * scale).round()))`, где `scale = dpi / 96.0`.
2. `measure_label`: перо `pen = 0.0`; для каждого символа `id = sc.glyph_id(c)`, `pen += sc.kern(prev, id)`, `id.with_scale_and_position(sc.scale(), point(pen, 0.0))`, `pen += sc.h_advance(id)`; ink-bbox = union `OutlinedGlyph::px_bounds()`. Центрировать по **ink-bbox**, не по advance (`px_bounds` документирован как «should not be used for layout logic», но нам нужна именно оптическая центровка двух заглавных).
3. `h = round(height_dip*scale)`; `Text`: `w = round(max(min_width_dip*scale, ink.width + 2*pad_x_dip*scale))`; `Color`: `w = h`.
4. `Pixmap::new(w, h)` (инициализируется прозрачным чёрным).
5. Путь скруглённого прямоугольника — **готового rounded-rect в tiny-skia 0.12 нет** (проверено), строить руками:

```rust
const KAPPA: f32 = 0.5523;
fn rounded_rect(w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    let r = r.min(w / 2.0).min(h / 2.0);
    let c = KAPPA * r;
    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(r, 0.0);
    pb.line_to(w - r, 0.0);
    pb.cubic_to(w - r + c, 0.0, w, r - c, w, r);
    pb.line_to(w, h - r);
    pb.cubic_to(w, h - r + c, w - r + c, h, w - r, h);
    pb.line_to(r, h);
    pb.cubic_to(r - c, h, 0.0, h - r + c, 0.0, h - r);
    pb.line_to(0.0, r);
    pb.cubic_to(0.0, r - c, r - c, 0.0, r, 0.0);
    pb.close();
    pb.finish()
}
```

6. `fill_path(&path, &bg_paint, FillRule::Winding, Transform::identity(), None)`, `bg_paint.set_color_rgba8(bg.r, bg.g, bg.b, 255)`, `anti_alias` оставить `true` (дефолт).
7. Текст (только `Text`): `Mask::new(w, h)`, покрытие писать в `mask.data_mut()` из `OutlinedGlyph::draw(|gx, gy, cov| ...)` через `saturating_add((cov * 255.0) as u8)` (bbox кернованных пар могут пересечься); затем **тот же** путь залить fg-цветом через маску — чернила физически не выйдут за скруглённую форму.
8. `let mut data = pixmap.take();` → `for px in data.chunks_exact_mut(4) { px.swap(0, 2); }` → premultiplied **BGRA**. `take_demultiplied()` использовать нельзя: `AC_SRC_ALPHA` требует premultiplied.
9. `Ok(BadgeImage { width: w, height: h, bgra_premul: data, dpi: key.dpi })`.

`BadgeCache { font: FontRef<'static>, metrics: BadgeMetrics, entries: HashMap<BadgeKey, BadgeImage> }`, `new(font_bytes: &'static [u8], metrics) -> Result<Self, InvalidFont>` через `FontRef::try_from_slice` (без копии в heap), `MAX_ENTRIES = 16`, вытеснение — `entries.clear()` при заполнении. Шрифт — `const FONT: &[u8] = include_bytes!("../assets/fonts/<файл>.ttf");`.

Метрики (`height_dip: 26.0`, `min_width_dip: 40.0`, `pad_x_dip: 9.0`, `radius_dip: 7.0`, `text_px_dip: 16.0`) — **дизайн-предложение, а не проверенный факт**; в коде отметить комментарием «подтверждается глазами на 100/150/200 % в задаче 21».

- [ ] **Шаг 6: убедиться, что тесты проходят**

Запустить: `cargo test -p switcher-app`
Ожидание: PASS, все инварианты. Отдельно глянуть: `cargo tree -p switcher-app | grep -c png` → `0` (доказательство, что `png-format` выключен).

- [ ] **Шаг 7: гейты и коммит**

Запустить: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
Ожидание: чисто.

```bash
git add crates/switcher-app docs/architecture/adr
git commit -m "feat(app): badge rasterizer in premultiplied BGRA with DPI-keyed cache"
```

---

### Задача 16: switcher-app — sound.rs: два синтезированных кью на rodio 0.22

**Реализовано 2026-09-07:** `SoundDevice::open(tx)` принимает канал событий. Вместо готового helper используется `from_default_device().with_error_callback(...).open_sink_or_fallback()` для регистрации отказа уже открытого потока. После отказа дальнейшие тоны отбрасываются; конфиг звука сохраняет намерение. Владение устройством ограничено главным потоком через `PhantomData<Rc<()>>`. Тест проверяет также конечность, амплитуду и затухание реальных сэмплов без воспроизведения. Стартовый Sound=Ok устанавливается **до** обработки накопленных событий Off.

**Файлы:**
- Создать: `crates/switcher-app/src/sound.rs` — `SoundDevice`, `RodioSoundPlayer`, `NullSoundPlayer`, `cue_freq_hz`, `effective_gain`
- Изменить: `docs/smoke/m1-windows.md` — секция «Звук» (файл создан задачей 9)
- Изменить: `crates/switcher-app/src/main.rs` — `mod sound;`

**Интерфейсы:**
- Потребляет: `switcher_platform::ports::{SoundPlayer, SoundCue}` (не меняются); `rodio` с `default-features = false, features = ["playback"]` (ADR-0008, задача 8).
- Производит: `sound::SoundDevice::open() -> Result<(SoundDevice, RodioSoundPlayer), rodio::DeviceSinkError>` и `sound::NullSoundPlayer` — для задачи 20.

**Внимание — имена API.** В rodio 0.22.2 **нет** `OutputStream`, `OutputStreamBuilder`, `Sink`. Проверенные имена: `rodio::DeviceSinkBuilder::open_default_sink() -> Result<MixerDeviceSink, DeviceSinkError>` (`stream.rs:233`), `MixerDeviceSink::mixer(&self) -> &Mixer` (`:67`), `MixerDeviceSink::log_on_drop(&mut self, bool)` (`:78`), `Mixer::add<T: Source + Send + 'static>(&self, T)` (`mixer.rs:57`), `rodio::source::SineWave::new(freq: f32)` (`source/sine.rs:25`). **`Mixer` не реэкспортирован из корня крейта** — путь ровно `rodio::mixer::Mixer` (проверено: в `lib.rs` корневые `pub use` его не содержат). Трейт `rodio::Source` обязан быть в scope — иначе `take_duration`/`fade_out`/`amplify` не видны.

- [ ] **Шаг 1: доказать сборкой, что урезанные фичи rodio компилируются**

В исследовании конфигурация выведена из кода (`DecoderImpl::None(Unreachable, …)` «to satisfy the compiler when there are no decoders enabled»), но **сборкой не проверена**.

Запустить: `cargo check -p switcher-app && cargo tree -p switcher-app | grep -ci symphonia`
Ожидание: `cargo check` — чисто; `grep -c` → `0`. Если `check` падает — не добавлять фичи наугад: включить минимально необходимую, проверить `cargo tree` и записать отклонение от ADR-0008 в тот же коммит комментарием в `Cargo.toml`.

- [ ] **Шаг 2: написать падающие тесты на чистые части**

Играющий звук тестом не проверяется; проверяется таблица кью и политика громкости:

```rust
#[test]
fn each_cue_gets_a_distinct_audible_frequency() {
    let (ru, en, n) = (cue_freq_hz(SoundCue::Ru), cue_freq_hz(SoundCue::En), cue_freq_hz(SoundCue::Neutral));
    for f in [ru, en, n] { assert!((200.0..2000.0).contains(&f)); }
    assert_ne!(ru, en); assert_ne!(en, n); assert_ne!(ru, n);
}

#[test]
fn gain_rejects_silence_and_garbage_and_clamps_the_rest() {
    assert_eq!(effective_gain(0.0), None);
    assert_eq!(effective_gain(-1.0), None);
    assert_eq!(effective_gain(f32::NAN), None);
    assert_eq!(effective_gain(2.0), Some(1.0));
    assert_eq!(effective_gain(0.4), Some(0.4));
}

#[test]
fn null_player_accepts_any_call() {
    NullSoundPlayer.play(SoundCue::Ru, 1.0); // не паникует: путь деградации ADR-0007
}
```

- [ ] **Шаг 3: убедиться, что тесты падают**

Запустить: `cargo test -p switcher-app`
Ожидание: FAIL на `todo!()`.

- [ ] **Шаг 4: реализация**

```rust
//! Sound cues. `SoundDevice::open()` MUST be called on the main thread: cpal
//! initializes COM as STA on the calling thread (see ADR-0009), and the main thread
//! is the one that pumps messages. Dropping `SoundDevice` kills all sound, so main
//! keeps it alive until it returns.
pub struct SoundDevice { _sink: rodio::MixerDeviceSink }
pub struct RodioSoundPlayer { mixer: rodio::mixer::Mixer }
```

`open()`: `let mut sink = DeviceSinkBuilder::open_default_sink()?;` → `sink.log_on_drop(false);` (иначе крейт печатает предупреждение в stderr на дропе, а у оконного приложения stderr никто не читает) → `let mixer = sink.mixer().clone();` (`Mixer` — `Clone`, и он `Send`, что доказано самим rodio: `MixerSource` с полем `Mixer` перемещается в cpal-колбэк, требующий `Send`).

`play`: `let Some(gain) = effective_gain(volume) else { return };` затем
`self.mixer.add(SineWave::new(cue_freq_hz(cue)).take_duration(Duration::from_millis(90)).fade_out(Duration::from_millis(90)).amplify(gain))`. Fire-and-forget, без `Player`: `Effect::PlaySound` — разовое кью, очередь не нужна. `fade_out` — против щелчка на обрыве, и его аргумент равен полной длительности кью **не случайно**: в rodio 0.22 `fade_out(d)` — это линейная рампа от начала источника (`source/fadeout.rs:8-15` → `source/linear_ramp.rs:20-28, 76-90`), а не хвост в конце. С `from_millis(40)` при 90-мс тоне получился бы 40-мс блип и 50 мс тишины — на слух это «работает», поэтому smoke-пункт такую ошибку не поймал бы.

Частоты: `Ru => 660.0`, `En => 880.0`, `Neutral => 520.0`.

`NullSoundPlayer` — `impl SoundPlayer` с пустым `play`. Это исполнитель эффекта при мёртвом устройстве: ядро продолжает эмитить `Effect::PlaySound`, а **`cfg.sound.enabled` не трогается никем** (ADR-0007: конфиг хранит намерение, карта возможностей — реальность).

- [ ] **Шаг 5: убедиться, что тесты проходят**

Запустить: `cargo test -p switcher-app`
Ожидание: PASS.

- [ ] **Шаг 6: ручной smoke — звук и его отказ (записать в `docs/smoke/m1-windows.md`)**

Пункты (наблюдаемые ожидания, а не «послушать, что работает»):
1. Переключить RU→EN→DE. Ожидание: **три различимых на слух тона**, ни одного щелчка в конце.
2. `[sound] volume = 0.0` в конфиге → переключить раскладку. Ожидание: тишина, в логе нет ни одной ошибки (путь `effective_gain -> None`).
3. Отключить звуковое устройство (Диспетчер устройств → аудиоустройство → «Отключить»), запустить приложение. Ожидание: приложение **стартует**, в логе ровно одна строка `warn!` с `cap="sound"`, `state=Off`, `code="no_output_device"`; тултип трея содержит предупреждение; `type %APPDATA%\evk-soft\lang-switcher\config\config.toml` показывает `enabled = true` — конфиг не изменился. Приложение не падает и продолжает показывать бейдж.

- [ ] **Шаг 7: гейты и коммит**

Запустить: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
Ожидание: чисто.

```bash
git add crates/switcher-app docs/smoke/m1-windows.md
git commit -m "feat(app): synthesized sound cues with graceful audio-device degradation"
```

---

### Задача 17: switcher-app — paths.rs и logging.rs

**Реализовано 2026-09-07:** также добавлен `config_io.rs`, чтобы чтение/сохранение не дублировались в main/runtime. Запись через уникальный временный файл в том же каталоге, `sync_all` и замену `fs::rename`; успешный Windows roundtrip проверен. Нечитаемый, повреждённый или более новый конфиг защищён от записи до перезапуска с исправленным файлом. Каталог создаётся только при сохранении. `logging::init` использует `try_init`, поэтому занятый глобальный subscriber возвращает ошибку. Гейты оболочки: fmt и clippy чистые, 126 тестов workspace прошли.

**Файлы:**
- Создать: `crates/switcher-app/src/paths.rs` — `AppPaths`, `resolve()`, `layout_from()`, `ensure_config_dir()`
- Создать: `crates/switcher-app/src/logging.rs` — `init()`, `parse_level()`, `LogError`
- Изменить: `crates/switcher-app/src/main.rs` — `mod paths; mod logging;`

**Интерфейсы:**
- Потребляет: `directories 6.0`, `tracing-appender 0.2.5`, `tracing-subscriber 0.3.23` (фича `env-filter` уже в воркспейсе); `switcher_core::config::Config::log_level` — уже провалидированный (задачи 4, 8).
- Производит: `paths::AppPaths { config_file: PathBuf, log_dir: PathBuf }`, `paths::resolve()`, `logging::init(&Path, &str) -> Result<WorkerGuard, LogError>` — для задачи 20.

Проверенные факты, определяющие форму кода: на Windows `ProjectDirs::from` **игнорирует `qualifier`** (`directories-6.0.0/src/win.rs:98-99`), путь = `organization\application`, и крейт **сам добавляет подкаталог** `config`/`data` (`win.rs:75`); **каталоги крейт не создаёт** (во всём крейте нет ни одного `fs::`). `RollingFileAppender::new`/`rolling::daily` **паникуют** при ошибке (`rolling.rs:143-156`), а спека требует деградации — значит только `RollingFileAppender::builder()` + `build(dir) -> Result` (`builder.rs:298`). Каталог логов appender создаёт сам (`rolling.rs:795`). Имя файла — `{prefix}.{date}.{suffix}`, дата **UTC** (`rolling.rs:640-652, 193`). `Builder::latest_symlink` не использовать: под капотом `symlink::symlink_file` (`rolling.rs:805`), на Windows требует привилегий. `WorkerGuard` флашит в `Drop` (`non_blocking.rs:69-107`) ⇒ обязан жить до конца `main` и называться `let _guard`, а не `let _`.

- [ ] **Шаг 1: написать падающие тесты**

Ключ к тестируемости: разделить «спросить у ОС» и «сложить пути». Чистая `layout_from(config_dir: &Path, data_local_dir: &Path) -> AppPaths` тестируется на фейковых путях без единого обращения к ФС.

```rust
#[test]
fn layout_puts_config_and_logs_where_the_spec_says() {
    let p = layout_from(Path::new(r"C:\A\evk-soft\lang-switcher\config"),
                        Path::new(r"C:\L\evk-soft\lang-switcher\data"));
    assert!(p.config_file.ends_with("config.toml"));
    assert_eq!(p.config_file.parent().unwrap().file_name().unwrap(), "config");
    assert!(p.log_dir.ends_with("logs"));
}

#[test]
fn resolve_succeeds_on_this_machine() {
    let p = paths::resolve().expect("APPDATA must resolve");
    assert!(p.config_file.is_absolute() && p.log_dir.is_absolute());
    #[cfg(windows)]
    assert!(p.config_file.to_string_lossy().contains("evk-soft"));
}

#[test]
fn level_falls_back_to_info_on_garbage_and_on_empty() {
    assert_eq!(parse_level("debug"), LevelFilter::DEBUG);
    assert_eq!(parse_level("WARN"), LevelFilter::WARN);
    assert_eq!(parse_level("nonsense"), LevelFilter::INFO);
    assert_eq!(parse_level(""), LevelFilter::INFO); // FromStr отдал бы ERROR — см. Шаг 3
}

#[test]
fn init_returns_err_when_log_dir_is_a_file() {
    let f = std::env::temp_dir().join("lang-switcher-log-dir-is-a-file");
    std::fs::write(&f, b"x").unwrap();
    assert!(logging::init(&f, "info").is_err()); // деградация, не паника
    std::fs::remove_file(&f).ok();
}
```

`init` при успехе ставит глобальный подписчик, поэтому успешный путь тестом не покрывается (повторный `init()` паникует) — он проверяется наблюдаемо в задаче 20.

- [ ] **Шаг 2: убедиться, что тесты падают**

Запустить: `cargo test -p switcher-app`
Ожидание: FAIL на `todo!()`.

- [ ] **Шаг 3: реализация**

`paths::resolve()`: `ProjectDirs::from("", "evk-soft", "lang-switcher")` → `None` превращается в `PathsError::NoHome` (не `unwrap`). Далее `layout_from(dirs.config_dir(), dirs.data_local_dir())`, где `config_file = config_dir.join("config.toml")`, `log_dir = data_local_dir.join("logs")`. Итог на Windows: `%APPDATA%\evk-soft\lang-switcher\config\config.toml` и `%LOCALAPPDATA%\evk-soft\lang-switcher\data\logs`. Конфиг — в roaming (переезжает с профилем), логи — локально (не гонять по сети). `ensure_config_dir()` = `fs::create_dir_all(config_file.parent())` — обязателен вручную, крейт этого не делает.

`logging::init(log_dir, level)`:

```rust
let appender = RollingFileAppender::builder()
    .rotation(Rotation::DAILY)
    .filename_prefix("lang-switcher")
    .filename_suffix("log")          // => lang-switcher.2026-08-25.log (дата UTC)
    .max_log_files(7)
    .build(log_dir)?;                // Result, а не паника
let (writer, guard) = tracing_appender::non_blocking(appender);
tracing_subscriber::fmt()
    .with_env_filter(EnvFilter::builder()
        .with_default_directive(parse_level(level).into())
        .with_env_var("LANG_SWITCHER_LOG")
        .from_env_lossy())
    .with_ansi(false)                // в файл ANSI не нужен
    .with_writer(writer)
    .init();
Ok(guard)
```

`parse_level` — **не** голый `level.parse::<LevelFilter>()`: проверено, что `FromStr` для `LevelFilter` мапит пустую строку в `ERROR` (`tracing-core-0.1.36/src/metadata.rs:797-798`), то есть пустой `log_level` молча заглушил бы `info!`. Поэтому: пустая или неразобранная строка → `LevelFilter::INFO` (имена регистронезависимы, это в том же `FromStr`). Уровень берётся из `cfg.log_level`, уже провалидированного моделью конфига; `LANG_SWITCHER_LOG` перекрывает его для отладки.

- [ ] **Шаг 4: убедиться, что тесты проходят**

Запустить: `cargo test -p switcher-app`
Ожидание: PASS.

- [ ] **Шаг 5: гейты и коммит**

Запустить: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
Ожидание: чисто.

```bash
git add crates/switcher-app
git commit -m "feat(app): resolve config/log paths and set up rolling file logging"
```

---

### Задача 18: switcher-app — трей: меню, иконка, карта возможностей

**Реализовано 2026-09-07:** меню и объекты tray-icon строго на главном потоке; состояние — подменю отдельных строк. Подтверждён native-контракт straight alpha (255,a128 → 128; premul 128,a128 → 64). Дренаж команд дополнен после DispatchMessage, а ограничение sent-modal внутри GetMessage описано в ADR-0009; сам tray-icon посылает WM_NULL после меню. Независимое ревью нашло длинную метку при size=8: добавлена проверка фактических размеров, регрессия red→green. Гейты: fmt/clippy и 133 теста workspace прошли. UI-smoke ожидает задачи 20.

**Файлы:**
- Создать: `crates/switcher-app/src/menu.rs` — `MenuCommand`, `ids::*`, `command_for(&str)`
- Создать: `crates/switcher-app/src/capability.rs` — `CapabilityMap`, `compose_tooltip`, `compose_status`
- Создать: `crates/switcher-app/src/tray.rs` — `#[cfg(windows)]`: `TrayCommand`, `TrayInit`, `run_tray(...)`
- Изменить: `docs/smoke/m1-windows.md` — секция «Трей» (файл создан задачей 9)
- Изменить: `crates/switcher-app/src/render.rs` — добавить `BadgeCache::tray_rgba` (второй растеризатор, straight alpha)
- Изменить: `crates/switcher-app/src/main.rs` — `mod menu; mod capability; #[cfg(windows)] mod tray;`

**Интерфейсы:**
- Потребляет: `tray_icon` (`default-features = false`, только под `cfg(windows)` — ADR-0008), `tray_icon::menu` (реэкспорт `muda`, отдельной зависимости `muda` быть не должно); `switcher_platform::events::{Capability, CapabilityState, CapabilityReport}` (задача 8); `switcher_windows::win_util::{current_thread_id, PumpWaker, pump_messages, post_quit}` (задача 9).
- Производит: `menu::{MenuCommand, command_for}`, `capability::CapabilityMap`, `tray::{TrayCommand, run_tray}`, `render::BadgeCache::tray_rgba` — для задач 19 и 20.

**Потоковый контракт (ADR-0009), нарушать нельзя.** `TrayIcon` и все типы `muda` — `Rc<RefCell<…>>`, то есть `!Send`; `set_icon`/`set_tooltip`/`set_menu` внутри делают `SendMessageW` в собственный HWND, поэтому вызов с непрокачивающего потока залипнет. Значит: трей целиком живёт на **главном** потоке, который сам качает `GetMessageW`; с потока ядра приходят только плоские `TrayCommand`.

**`unsafe` в `switcher-app` запрещён.** Насос сообщений и `PostQuitMessage`/`PostThreadMessageW` берутся безопасными обёртками из `switcher-windows`. Ожидаемая форма (даёт задача 9; если её нет — добавить именно туда, а не здесь):

```rust
// PumpVerdict приходит из задачи 9 вместе с pump_messages; отдельного PumpControl нет.
/// Runs GetMessageW/TranslateMessage/DispatchMessageW on the calling thread.
/// `on_iter` is called before dispatching each message, on this same thread.
pub fn pump_messages(on_iter: impl FnMut() -> PumpVerdict) -> Result<(), PlatformError>;
pub struct PumpWaker(u32);        // Send: thread id + PostThreadMessageW
```
`// SAFETY:`-комментарии в обёртке обязаны называть: `MSG` инициализируется нулями и заполняется только ОС; `GetMessageW` возвращает `-1` как ошибку (обязательная проверка, иначе бесконечный цикл); все вызовы исполняются на потоке-владельце очереди.

- [ ] **Шаг 1: написать падающие тесты на чистые части**

Тестируется всё, что не трогает ОС: маппинг id, компоновка тултипа и статуса, второй растеризатор.

- `menu_ids_map_to_commands`: `command_for(ids::FOLLOW) == Some(MenuCommand::ToggleFollow)`, то же для `SOUND`, `AUTOSTART`, `QUIT`; `command_for("status") == None` (неактивный пункт команд не даёт); `command_for("bogus") == None`.
- `tooltip_is_short_enough_for_the_shell`: `compose_tooltip("RU", &map_with_3_off())` даёт строку, у которой `s.encode_utf16().count() <= 126`. **Обоснование числа:** `tray-icon` копирует в `szTip: [u16; 128]` циклом `for i in 0..tip.len().min(128)`, где `tip` уже содержит завершающий `0` (`platform_impl/windows/mod.rs:216, 586`) — ровно 128 единиц съедают NUL. Держим ≤126.
- `tooltip_aggregates_layout_sources`: три `Capability::is_layout_source()` в состоянии `Off` дают в тултипе **одну** строку-агрегат, не три.
- `status_lists_every_degraded_capability_with_its_code`: `compose_status` содержит `Capability::key()` и `code` каждой недоступной возможности, по строке на каждую (длина не ограничена — это неактивный пункт меню).
- `capability_map_defaults_to_ok_and_records_last_report`: свежая карта — всё `Ok`, `degraded().count() == 0`; после `apply(report)` — состояние и `code` доступны.
- `tray_icon_bytes_are_in_rgba_order`: `tray_rgba(&content, 16)` длиной `16*16*4`; центральный пиксель `== [bg.r, bg.g, bg.b, 255]` — **порядок RGBA**, тогда как `render_badge` для того же контента даёт `[bg.b, bg.g, bg.r, 255]`. Ловит перепутанный порядок каналов.
- `tray_icon_edge_pixel_is_not_premultiplied`: на пикселе сглаженного края (`0 < a < 255`) проверить `max(r,g,b) > a`. Отдельный тест нужен потому, что при `a = 255` премультиплицированный и straight буферы **побайтово совпадают** — центральный пиксель конвенцию альфы не различает вообще, только полупрозрачный край. Само требование «`Icon::from_rgba` ждёт straight alpha» помечено **UNVERIFIED**: в исходниках крейта видно только построение AND-маски и swap каналов, а страница `CreateIcon` про альфу молчит — сверить по Learn или экспериментом (сравнить вид иконки на двух буферах) до того, как опираться на него.
- `tray_icon_corner_is_fully_transparent`: alpha (0,0) `== 0`.

- [ ] **Шаг 2: убедиться, что тесты падают**

Запустить: `cargo test -p switcher-app`
Ожидание: FAIL на `todo!()`.

- [ ] **Шаг 3: реализация второго растеризатора**

`BadgeCache::tray_rgba(&self, content: &BadgeContent, size: u32) -> Result<Vec<u8>, RenderError>`: тот же конвейер, что `render_badge` (скруглённый квадрат `size × size`, радиус `size/4`, текст под ширину), но финал другой — **`pixmap.take_demultiplied()` и никакого swap каналов**. Почему ровно так (ADR-0006): `tray_icon::Icon::from_rgba` строит AND-маску из `a.wrapping_sub(255)` и сам переворачивает каналы в BGRA (`platform_impl/windows/icon.rs:21-54`), то есть ожидает **straight (непремультиплицированный) RGBA**; premultiplied дал бы тёмную кайму по краям иконки. Кэш для иконки не нужен: она меняется только на смену раскладки. Размер в M1 — жёсткие `16`; если smoke на 150/200 % покажет мыло, **не подкручивать молча**: `GetSystemMetrics(SM_CXSMICON)` потребовал бы `windows` в `switcher-app` либо новую обёртку в адаптере — это решение и ADR (скил `adr`).

- [ ] **Шаг 4: реализация трея**

```rust
#[derive(Debug)]
pub enum TrayCommand {
    SetIcon { rgba_straight: Vec<u8>, size: u32 },   // straight alpha, НЕ premultiplied
    SetTooltip(String),
    SetStatus { text: String, autostart_available: bool },
    SyncChecks { follow: bool, sound: bool, autostart: bool },
    Shutdown,
}
```
(ADR-0009 задал форму из четырёх вариантов; `SetStatus` — добавка того же класса, плоские данные, нужная для пункта «Состояние» из ADR-0007.)

Меню (проверенные сигнатуры `muda 0.19.3`, акселераторы везде `None` — на Windows они требуют `TranslateAcceleratorW`, которого у нас нет):

```rust
let follow    = CheckMenuItem::with_id(ids::FOLLOW, "Следовать за курсором", true, checks.follow, None);
let sound     = CheckMenuItem::with_id(ids::SOUND, "Звук", true, checks.sound, None);
let autostart = CheckMenuItem::with_id(ids::AUTOSTART, "Автозапуск", init.autostart_available, checks.autostart, None);
let status    = MenuItem::with_id(ids::STATUS, "Состояние: всё работает", false, None); // inert
let quit      = MenuItem::with_id(ids::QUIT, "Выход", true, None);
let menu = Menu::with_items(&[&follow, &sound, &autostart,
                              &PredefinedMenuItem::separator(), &status,
                              &PredefinedMenuItem::separator(), &quit])?;
let tray = TrayIconBuilder::new()
    .with_menu(Box::new(menu)).with_tooltip(init.tooltip)
    .with_icon(Icon::from_rgba(rgba, size, size)?)
    .with_menu_on_left_click(true).build()?;
```
`muda::Menu::init_for_hwnd` **не вызывать**: это меню-бар окна; трею подписку ставит сам `tray-icon` (`attach_menu_subclass_for_hwnd`).

Цикл — `pump_messages(|| { … })`, где замыкание **на каждой итерации** осушает канал `while let Ok(cmd) = rx.try_recv()` и применяет команды; `Shutdown` → `PumpVerdict::Quit` (обёртка внутри делает `PostQuitMessage`, `std::process::exit` запрещён: он не исполнит `Drop` у `WorkerGuard`, и последние строки лога, включая причину выхода, потеряются). Осушение **до** `DispatchMessageW` и безусловно, а не по коду сообщения-побудки: тогда корректность не зависит от того, как ОС доставляет thread-сообщения — достаточно того, что побудка разбудила `GetMessageW`. (Поведение `PostThreadMessageW` — сообщение без окна, `DispatchMessageW` его не доставляет — **UNVERIFIED** в этой сессии: сверить по Microsoft Learn перед кодом, скил `platform-api-work`. Выбранная схема от этого факта не зависит.)

`Icon::from_rgba` при `Err(BadIcon)` — `warn!` и оставить старую иконку; трей важнее иконки.

- [ ] **Шаг 5: убедиться, что тесты проходят**

Запустить: `cargo test -p switcher-app`
Ожидание: PASS.

- [ ] **Шаг 6: ручной smoke трея (в `docs/smoke/m1-windows.md`)**

1. **Обработчики перехватывают события.** Это выведено из кода `OnceCell` (`set_event_handler` = `OnceCell::set`, а `send` = `get_or_init(|| None)`), но в рантайме не подтверждено. Клик по «Звук». Ожидание: в логе появилась строка о `MenuCommand::ToggleSound`. Если тишина — обработчик зафиксирован в `None`, фолбэк: `crossbeam_channel::Select` по `MenuEvent::receiver()`/`TrayIconEvent::receiver()` (в графе ровно один `crossbeam-channel 0.5.15`, тот же, что в воркспейсе).
2. **Галка чекбокса щёлкается сама.** Проверено в коде: на `WM_COMMAND` muda делает `item.set_checked(!checked)` до отправки события (`platform_impl/windows/mod.rs:1195-1198`). Ожидание: после отказа автозапуска галка **возвращается** назад по `SyncChecks` — визуально видно.
3. **Перезапуск `explorer.exe`** (Диспетчер задач → Проводник → «Перезапустить»). Ожидание: иконка сама вернулась в трей (крейт делает это через `RegisterWindowMessageA("TaskbarCreated")`), меню работает.
4. **Меню открыто → переключить раскладку.** Ожидание: иконка не обновляется до закрытия меню (главный поток в модальном `TrackPopupMenu`) — это известное следствие ADR-0009, а не баг; зафиксировать наблюдение.
5. **«Выход»** — приложение закрывается, иконка исчезает, **последняя строка лога дописана в файл** (доказательство, что `WorkerGuard` дропнулся, а не был убит `process::exit`).
6. Иконка на 100 / 150 / 200 % масштабе панели задач — читаема ли метка.

- [ ] **Шаг 7: гейты и коммит**

Запустить: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
Ожидание: чисто.

```bash
git add crates/switcher-app docs/smoke/m1-windows.md
git commit -m "feat(app): tray icon, menu and capability presentation on the main thread"
```

---

### Задача 19: switcher-app — runtime.rs: цикл ядра и диспетчер эффектов

**Реализовано 2026-09-07:** свежие layout/cursor/DPI-снимки, Initial до чтения очереди, синхронный ответ QueryAnchor, абсолютный дедлайн перед каждым select, отдельный stop-канал. Тест с постоянно готовой очередью курсора и управляемыми часами подтверждает скрытие по дедлайну. Отказ записи автозапуска перечитывает ОС; отказ Overlay снимает tracking, восстановление перепроверяет DPI. Замечания независимого ревью закрыты регрессиями; 147 тестов workspace, fmt и clippy прошли. Исторические фрагменты ниже уточнены текущим кодом и ADR-0007.

**Файлы:**
- Создать: `crates/switcher-app/src/runtime.rs` — `Runtime`, `Ports`, `Shown`, `run(...)`, `MAX_DISPATCH_STEPS`
- Изменить: `crates/switcher-app/src/main.rs` — `mod runtime;`

**Интерфейсы:**
- Потребляет: `switcher_core::engine::{Engine, Event, Effect, ResolvedAnchor}` (задачи 5–8), `switcher_platform::ports::*`, `render::BadgeCache` (задача 15), `capability::CapabilityMap` + `menu::MenuCommand` + `tray::TrayCommand` (задача 18), `switcher_windows::win_util::PumpWaker` (задача 9).
- Производит: `Runtime::new`, `Runtime::handle_platform`, `Runtime::handle_menu`, `Runtime::reconcile_autostart`, `Runtime::fire_hide_timer`, `Runtime::timeout`, `runtime::run` — для задачи 20.

`Ports` обязательно содержит `layout_monitor: Box<dyn LayoutMonitor>` (задача 12).
Его синхронное чтение доступно и при отказе установки отдельного хука, иначе потеря
одного источника отключит обработку всех остальных (ADR-0011). В тестах нужен
`MockLayoutMonitor` с управляемой последовательностью снимков/ошибок.

**Форма, обеспечивающая тестируемость:** дедлайн скрытия хранится как `hide_deadline_ms: Option<u64>` на той же монотонной базе, что `now_ms` ядра, а не как `Instant`. Тогда весь автомат рантайма проверяется моками портов с инъекцией времени, а `run()` остаётся тонкой обёрткой из десяти строк.

- [ ] **Шаг 1: написать падающие тесты на моках портов**

Моки: `MockOverlay { calls: Vec<OverlayCall>, dpi: u32 }` (`OverlayCall::{Show{w,h,dpi,anchor}, MoveTo(ResolvedAnchor), Hide}`), `MockPointer { cursor: Option<Point>, active: Vec<bool> }`, `MockCaret`, `MockSound { played: Vec<(SoundCue, f32)> }`, `MockAutostart { result: Result<(), PlatformError>, enabled: Result<bool, PlatformError> }`; общий вектор — `Arc<Mutex<…>>`, потому что порты `Send` и берут `&self`. `TrayCommand` собирается из тестового `Receiver`.

- `layout_change_shows_badge_in_one_pass`: подать `PlatformEvent::LayoutChanged{..}` → после **одного** вызова `handle_platform` в моке оверлея уже есть `Show`. Это тест на главный инвариант: `Effect::QueryAnchor` обязан быть отвечен синхронно в том же проходе.
- `late_notification_uses_current_snapshot`: payload говорит RU, `LayoutMonitor::current()` уже EN → ядро и трей остаются EN, лишнего звука и показа нет.
- `layout_read_failure_never_uses_queued_payload`: ошибка `current()` → прежнее состояние сохранено, ошибка видна в диагностике; старый payload не передан ядру.
- `rapid_return_is_not_lost`: подтверждённые EN → RU → EN за 100 мс → итоговые трей и бейдж EN.
- `hide_deadline_wins_over_busy_pointer_stream`: канал курсора всегда готов, время пересекло дедлайн → `Hide` и отключение трекинга всё равно происходят.
- `bootstrap_precedes_queued_hook_events`: уведомление попало в канал во время setup; рантайм сначала выполняет начальное чтение и эффекты `Initial`, затем обрабатывает очередь. Follow показан без звука.
- `autostart_failure_rechecks_external_drift`: после отказа записи перечитать `is_enabled()` и примирить конфиг с фактом ОС; если чтение тоже отказало, сообщить неизвестное состояние и отключить галку автозапуска.
- `query_anchor_never_leaves_the_engine_awaiting`: то же событие при `cursor = None, caret = None` → `Show` с `ResolvedAnchor::Fixed`, `pending_anchor == false` по выходе.
- `show_uses_dpi_from_the_port_and_caches`: `MockOverlay { dpi: 144 }` → `Show{dpi: 144}`, `cache.len() == 1`; второе такое же переключение → `cache.len()` не вырос.
- `overlay_scale_changed_equal_to_last_render_dpi_is_ignored`: после показа при 144 подать `OverlayScaleChanged{dpi:144}` → новых `Show` нет (гасит гонку двух событий, ADR-0005).
- `overlay_scale_changed_rerenders_with_show_not_move`: подать `OverlayScaleChanged{dpi:192}` → появился второй `Show{dpi:192}`, `MoveTo` — ни одного. `show`, а не `move_to`, потому что меняется размер окна.
- `overlay_scale_changed_while_hidden_is_ignored`.
- `pointer_moved_while_tracking_moves_badge`: `MoveTo(Cursor(pos))`.
- `arm_hide_timer_sets_deadline_and_fire_hides`: после показа `timeout(now)` == `Some(1500ms)`; `fire_hide_timer(now+1500)` → `Hide` + `set_active(false)`; `timeout` снова `None`.
- `cancel_hide_timer_clears_deadline` (режим `Follow`).
- `autostart_refusal_snaps_menu_back_and_keeps_config_and_file`: `MockAutostart` возвращает `Err(PlatformError::new("registry_write_denied", …))`; подать `MenuCommand::ToggleAutostart` → пришёл `SyncChecks{autostart:false}`, `engine.config().autostart == false`, файла конфига **не появилось**, в `CapabilityMap` у `Autostart` — не `Ok`.
- `autostart_success_persists_config_and_syncs`: `Ok` → файл создан и парсится обратно `Config::from_toml_str`, `cache.len() == 0` (`PersistConfig` чистит кэш по ADR-0006), пришёл `SyncChecks{autostart:true}`.
- `startup_reconcile_lets_the_registry_win`: конфиг `autostart=false`, `is_enabled() == Ok(true)` → после `reconcile_autostart` конфиг `true` и файл записан. При `Err` → `CapabilityMap[Autostart] != Ok` и `SetStatus{autostart_available:false}`.
- `sound_capability_off_does_not_touch_the_config`: подать `CapabilityChanged(Sound, Off)`, затем смену раскладки → `PlaySound` всё равно передан плееру, `engine.config().sound.enabled == true` (ADR-0007).
- `capability_changed_updates_tooltip_and_status`: пришли `SetTooltip` и `SetStatus`, тултип ≤126 UTF-16 единиц.
- `menu_quit_sends_shutdown`: `MenuCommand::Quit` → `TrayCommand::Shutdown` и `run` завершается.
- `shutdown_does_not_wait_for_adapter_senders`: отдельный сигнал остановки завершает
  рантайм, даже пока адаптеры держат клоны `Sender`; закрытие всех источников не является
  предусловием выхода.

Путь записи конфига в тестах — уникальный файл в `std::env::temp_dir()`, удаляется в конце теста.

- [ ] **Шаг 2: убедиться, что тесты падают**

Запустить: `cargo test -p switcher-app`
Ожидание: FAIL на `todo!()`.

- [ ] **Шаг 3: реализация диспетчера**

Диспетчер — **рабочий список**, а не рекурсия: так синхронный ответ на `QueryAnchor` обеспечен конструкцией, а не дисциплиной.

```rust
// INVARIANT (audit finding): Effect::QueryAnchor MUST be answered inside the same
// dispatch pass. If it is ever deferred, the engine stays in BadgeState::AwaitingAnchor
// forever: every later Layout event is swallowed and the badge never appears again.
const MAX_DISPATCH_STEPS: usize = 64;

fn dispatch(&mut self, effects: Vec<Effect>, now_ms: u64) {
    let mut q: VecDeque<Effect> = effects.into();
    let mut steps = 0usize;
    while let Some(fx) = q.pop_front() {
        steps += 1;
        if steps > MAX_DISPATCH_STEPS { error!("effect dispatch did not converge"); break; }
        match fx {
            Effect::QueryAnchor => {
                self.pending_anchor = true;
                let caret = self.ports.caret.caret_point();
                let cursor = self.ports.pointer.cursor_pos();
                q.extend(self.engine.handle(Event::AnchorResolved { caret, cursor }, now_ms));
                self.pending_anchor = false;
            }
            Effect::ApplyAutostart(want) => {
                let res = self.ports.autostart.set_enabled(want);
                if let Err(e) = &res { self.caps_off(Capability::Autostart, e.code, &e.detail); }
                q.extend(self.engine.handle(
                    Event::AutostartApplied { requested: want, ok: res.is_ok() }, now_ms));
            }
            /* … остальные ветки … */
        }
    }
    debug_assert!(!self.pending_anchor, "QueryAnchor left unanswered");
}
```

Остальные ветки:
- `ShowBadge { content, anchor }` → `let dpi = overlay.dpi_for(anchor);` → `cache.image(&content, dpi)` → `overlay.show(img, anchor)` → `self.last = Some(Shown { content, anchor, dpi })`. Обращаться к `self.cache` и `self.ports.overlay` **как к полям, а не через `&mut self`-методы**, иначе borrow checker не даст держать `&BadgeImage` из кэша одновременно с вызовом порта. `Err(RenderError)` → `error!` и не показывать (лучше без бейджа, чем паника).
- `MoveBadge { anchor }` → `overlay.move_to(anchor)` + обновить `last.anchor`. DPI здесь **не** перезапрашивать: расхождение находит адаптер и присылает `OverlayScaleChanged`.
- `HideBadge` → `overlay.hide()`, `self.last = None`.
- `ArmHideTimer { after_ms }` → `hide_deadline_ms = Some(now_ms + after_ms)`; `CancelHideTimer` → `None`. Это **единственный таймер ядра** (ADR-0003).
- `SetPointerTracking(b)` → `pointer.set_active(b)`.
- `PlaySound { cue, volume }` → `sound.play(cue, volume)` безусловно (плеер может быть `NullSoundPlayer`).
- `UpdateTray { label, lang }` → `BadgeContent::for_lang(&lang, cfg.badge.style, &cfg.badge.colors)` → `cache.tray_rgba(&content, 16)` → `SetIcon` + `SetTooltip(compose_tooltip(&label, &caps))`; сохранить `label` для последующих пересборок тултипа.
- `PersistConfig` → записать `engine.config().to_toml_string()` в `cfg_path` (ошибка записи → `error!`, работа продолжается: `Capability` для конфига в контракте нет, и это осознанно), затем `cache.clear()` и `SyncChecks` из `engine.config()`.
- `SyncTrayMenu` → только `SyncChecks`, без записи файла (асимметрия ADR-0007 сохранена намеренно).

Маппинг `PlatformEvent -> Option<engine::Event>`: `LayoutChanged` → синхронное `ports.layout_monitor.current()` → `Event::Layout` с новой парой `layout/lang` и исходным `source` (ADR-0011). Ошибка чтения → диагностика и **`None`**, без подстановки payload. `PointerMoved` → `Some(Event::Pointer{..})`; `OverlayScaleChanged { dpi }` → **`None`** плюс локальная обработка (если бейдж скрыт или `last.dpi == dpi` — выход; иначе перерастеризовать и вызвать `show`); `CapabilityChanged(report)` → **`None`**, диагностика, обновление `CapabilityMap`, `SetTooltip` + `SetStatus`.

`MenuCommand` → `Event`: `ToggleFollow` → `SetMode(если сейчас Follow, то Transient, иначе Follow)`; `ToggleSound` → `SetSoundEnabled(!cfg.sound.enabled)`; `ToggleAutostart` → `SetAutostart(!cfg.autostart)` (дедупа нет намеренно: источник истины автозапуска — реестр, он мог разъехаться); `Quit` → `TrayCommand::Shutdown` и выход из цикла.

`run(rx_platform, rx_menu, …)`: **перед каждым** приёмом проверить абсолютный дедлайн по текущему времени; если он истёк — выполнить `HideTimerFired` и эффекты. Затем вычислить оставшееся время и вызвать `crossbeam_channel::select!` с `default(remaining)`. Одного `default` недостаточно: постоянно готовый канал Raw Input может не дать ему выполниться. Без дедлайна — `select!` без `default`, блокировка до события; периодический опрос не добавляется.

- [ ] **Шаг 4: убедиться, что тесты проходят**

Запустить: `cargo test -p switcher-app`
Ожидание: PASS; в `runtime.rs` не осталось `todo!` (`grep -n "todo!" crates/switcher-app/src/runtime.rs` — пусто).

- [ ] **Шаг 5: гейты и коммит**

Запустить: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
Ожидание: чисто.

```bash
git add crates/switcher-app
git commit -m "feat(app): core loop and effect dispatcher over platform ports"
```

---

### Задача 20: switcher-app — main.rs: сборка end-to-end

> **Реализовано 2026-09-07:** сборка находится в `startup.rs`, `main.rs` вызывает её.
> Начальные capability и Initial применяются до обработки очереди. Логи создают
> каталог до ротации; сохранение конфига — через ConfigStore из задачи 17.
> Отдельное оконное сообщение завершает модальный цикл (ADR-0015). Реальные debug
> и release запуски завершились с exit 0 и финальной строкой `clean=true`.
> Release — 3 078 656 байт; 148 тестов прошли, fmt/Clippy и MSRV 1.87 чистые.
> Шаги ниже — исходный план; интерактивная часть шага 3 ещё не подтверждена.

**Файлы:**
- Изменить: `crates/switcher-app/src/main.rs` — вся сборка вместо `fn main() {}`
- Возможно создать: `crates/switcher-app/build.rs` и `crates/switcher-app/lang-switcher.manifest` — если задача 9 не создала их **в этом пакете** (по ADR-0010 они обязаны лежать именно здесь: флаги линкера из build-скрипта действуют на бинарные цели своего пакета)

**Интерфейсы:**
- Потребляет: всё из задач 15–19 плюс адаптеры `switcher_windows::{overlay, pointer, layout_monitor, tsf, autostart}` (задачи 10–14) и обёртку PMv2/насоса из задачи 9.
- Производит: работающий `lang-switcher.exe`.

- [ ] **Шаг 1: реализовать `main` строго в этом порядке**

Порядок здесь не стилистика: каждый пункт стоит там, где стоит, из-за проверенного ограничения.

1. **Каналы и обработчики событий трея — самые первые строки.** `let (tx_ev, rx_ev) = unbounded::<PlatformEvent>(); let (tx_menu, rx_menu) = unbounded::<MenuCommand>();` затем `MenuEvent::set_event_handler(Some(move |e: MenuEvent| { … }))` и `TrayIconEvent::set_event_handler(Some(|e| trace!(?e)))`. Причина: и `tray-icon`, и `muda` держат обработчик в `OnceCell`, а отправка события делает `get_or_init(|| None)` — одно событие раньше установки фиксирует `None` навсегда. Обработчик меню мапит `MenuId` через `menu::command_for` и посылает `MenuCommand`; при `SendError` (поток ядра мёртв) — сразу `post_quit()`, иначе «Выход» перестал бы работать.
2. **PMv2** — до создания любого HWND: «Once a window (an HWND) has been created in your process, changing the DPI awareness mode is no longer supported», а `TrayIconBuilder::build()` окно создаёт. Вызвать обёртку из `switcher-windows` (`SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)`, затем чтение фактической awareness через `GetThreadDpiAwarenessContext` + `AreDpiAwarenessContextsEqual`). Обёртка **ничего не логирует, а возвращает результат**: подписчика ещё нет, и строка о фактической awareness потерялась бы. Ошибка «режим уже задан манифестом» — ожидаемая, не фатальная.
3. **Пути и конфиг в память.** `paths::resolve()?`, `ensure_config_dir()` (ошибку не глотать: запомнить, `PersistConfig` будет падать), затем прочитать файл и `Config::from_toml_str` → `(cfg, warnings)`. Чтение конфига физически **предшествует** логам, потому что уровень берётся из `cfg.log_level` (задача 17); предупреждения о поправленных значениях накапливаются в `warnings` и печатаются позже. Файла нет → `Config::default()`; `ConfigError` → дефолты + запомнить причину.
4. **Логи.** `let _guard = logging::init(&paths.log_dir, &cfg.log_level)` — имя `_guard`, не `_`: `WorkerGuard` флашит в `Drop`, и он обязан жить до конца `main`. Ошибка `init` — не паника: работа продолжается без файлового лога, а причина уходит в наблюдаемый канал — `CapabilityMap` и пункт «Состояние» трея. `eprintln!` только дополнением под `#[cfg(debug_assertions)]`: release-сборка объявлена `windows_subsystem = "windows"`, консоли у неё нет, и это единственный путь, на котором файлового лога тоже нет.
5. **Выплюнуть накопленное:** фактическую DPI-awareness (`warn!`, если не per-monitor-v2 — это единственное доказательство, что манифест встроился), `warnings` конфига, результат `ensure_config_dir`.
6. **Звук.** `SoundDevice::open()` **на главном потоке** (cpal инициализирует COM как STA на вызывающем потоке; главный поток — тот, что качает сообщения). `Ok` → `SoundDevice` остаётся в `main` живым до конца, плеер уходит в ядро. `Err` → `NullSoundPlayer` + `tx_ev.send(CapabilityChanged(CapabilityReport { capability: Sound, state: Off, code: "no_output_device", detail }))`. Конфиг при этом не трогаем.
7. **Сверка автозапуска с реестром.** `autostart.is_enabled()` — результат передаётся в `Runtime::reconcile_autostart`: при расхождении **реестр побеждает**, при `Err` — `Capability::Autostart` уходит в `Off`, пункт меню становится неактивным.
8. **Адаптеры и их потоки** — каждый со своим `tx_ev.clone()`: оверлей, курсор, монитор раскладки (он же владеет взводимым фолбэк-опросом), TSF на своём STA-потоке. Конструктор вернул `Err` → `CapabilityChanged(<cap>, Off, …)` и Null-заглушка вместо порта; ни один отказ не роняет приложение.
9. **Начальная раскладка.** Рантайм синхронно выполняет `layout_monitor.current()` → `Engine::handle(Event::Layout { source: Initial, … })` и эффекты **до чтения очереди уведомлений хуков**. `Initial` обновляет трей и восстанавливает Follow без звука; транзиентный бейдж при старте скрыт. При ошибке сохранить диагностику; первое успешное чтение по следующему уведомлению выполняет эту же инициализацию. Нельзя ставить `Initial` в хвост уже работающих источников.
10. **Поток ядра.** `thread::spawn` с `runtime::run(...)`: туда уходят `Engine`, `BadgeCache`, `CapabilityMap`, порты, `Sender<TrayCommand>`, `PumpWaker` (id главного потока), путь конфига.
11. **Трей и насос — на главном потоке**, последним: `tray::run_tray(init, rx_tray)`. Возврат из него = `GetMessageW` вернул 0.
12. **Graceful shutdown:** отдельный канал остановки входит в `select!` рантайма; главный поток отправляет сигнал до ожидания завершения. Нельзя полагаться на удаление main-копии `Sender`: клоны остаются у адаптеров. Рантайм прекращает приём событий, останавливает принадлежащие ему адаптеры и подтверждает завершение; `join` выполняется после подтверждения. У `JoinHandle::join` нет встроенного таймаута: предел ожидания задаётся каналом подтверждения, а остановка адаптеров должна быть прерываемой, включая backoff. Проверить выход при живых источниках и при их перезапуске. После завершения потоков `_guard` флашит лог; `std::process::exit` запрещён.

Плюс: `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` — release-сборка не должна открывать консоль, debug-сборка её сохраняет для разработки. Проверяется наблюдаемо на шаге 3.

- [ ] **Шаг 2: манифест PMv2, если его ещё нет**

Проверить `ls crates/switcher-app/lang-switcher.manifest crates/switcher-app/build.rs`. Если чего-то нет — создать по ADR-0010: рукописный XML с `<dpiAware>true</dpiAware>` (SMI/2005) и `<dpiAwareness>PerMonitorV2</dpiAwareness>` (SMI/2016), встраиваемый флагами `/MANIFEST:EMBED` + `/MANIFESTINPUT:<абсолютный путь>` из `build.rs`, под гейтом `CARGO_CFG_WINDOWS` + `CARGO_CFG_TARGET_ENV == "msvc"`, с `cargo::rerun-if-changed`. Директива — `cargo::rustc-link-arg-bins=FLAG` (сверено по Cargo Book активного тулчейна; двойное двоеточие требует Cargo 1.77, у нас MSRV 1.87 — см. «Последствия» ADR-0010). Помнить ограничение `/MANIFESTINPUT`: полный путь ≤ `MAX_PATH` (260) — для пути вида `.claude/worktrees/...` это реальный риск.

Запустить: `cargo build -p switcher-app`
Ожидание: сборка проходит; после запуска в логе строка о per-monitor-v2 **без** `warn!`.

- [ ] **Шаг 3: end-to-end прогон (первый живой запуск всего приложения)**

Запустить: `cargo run -p switcher-app`
Ожидание, по пунктам-доказательствам:
- в трее появилась иконка с меткой текущей раскладки; бейджа на старте **нет** (пункт 9);
- Win+Space → бейдж у курсора, звук, через ~1.5 с бейдж исчез;
- в логе есть строки инициализации в порядке шагов 2–9 и **нет** ни одной `error!`;
- release-сборка (`cargo run --release -p switcher-app`) не открывает окно консоли, debug-сборка открывает.

- [ ] **Шаг 4: гейты и коммит**

Запустить: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
Ожидание: чисто.

```bash
git add crates/switcher-app
git commit -m "feat(app): wire adapters, core loop and tray into a working binary"
```

---

### Задача 21: приёмка M1 — smoke-чеклист и замер NFR

> **Результат 2026-09-07:** заполнены таблицы фактических прогонов и ручная матрица.
> Release стартует и завершается штатно; 620-секундный замер выявил превышение
> CPU/RSS. Подтверждён расход аудиопотока; по коду найден дефект hover-таймера
> зависимости. [Разбор и следующая работа](../../research/2026-09-07-idle-performance.md).
> Независимое read-only ревью заменило недоступный workflow. M1 не закрыт:
> ручной ввод/визуальная приёмка и исправления NFR ещё требуются.

**Файлы:**
- Изменить: `docs/smoke/m1-windows.md` — свести все пункты задач 9–20 в один прогоняемый чеклист с таблицей результатов
- Изменить: `docs/superpowers/plans/2026-07-03-m1-windows-mvp.md` — таблица прогресса
- Возможно создать: `docs/architecture/adr/00NN-*.md` — только если замер потребует решения

**Интерфейсы:**
- Потребляет: работающий бинарник (задача 20) и все smoke-пункты, накопленные задачами 9–20.
- Производит: подписанный чеклист M1 и числа NFR — вход в решение «M1 закрыт».

- [ ] **Шаг 1: собрать полный чеклист**

Структура файла (шаблон из скила `platform-api-work`, расширенный): для каждого пункта — **что запустить / на что смотреть / что именно доказывает успех**. Пункт без наблюдаемого доказательства в чеклист не берётся.

- **A. Старт.** Строка лога о фактической DPI-awareness = per-monitor-v2, `warn!` нет (доказывает, что манифест встроился, а не «наверное встроился»). Конфиг создан по пути `%APPDATA%\evk-soft\lang-switcher\config\config.toml`, лог — `%LOCALAPPDATA%\evk-soft\lang-switcher\data\logs\lang-switcher.<дата UTC>.log`.
- **B. Раскладка, четыре класса окон.** Обычное Win32 (Блокнот), **elevated** (Диспетчер задач от админа), **UWP** (Калькулятор / Параметры), **Windows Terminal**, плюс смена переднего окна между окнами с разными раскладками (раскладка на Windows — на поток). Ожидание: бейдж и иконка трея верны во всех четырёх; в логе видно, какой источник сработал (`ShellHook` / `ForegroundChange` / `Tsf` / `ForegroundPoll`).
- **C. Оверлей.** Click-through (клик сквозь бейдж попадает в окно под ним), topmost (бейдж поверх полноэкранного окна), **отсутствие кражи фокуса** (каретка в Блокноте продолжает мигать, набор текста не прерывается), mixed-DPI (перетащить курсор с монитора 100 % на 200 % в режиме `follow` — размер меняется, бейдж не обрезан и не выезжает за экран), кламп у правого/нижнего края, `AnchorPref::Fixed` (бейдж в правом-нижнем углу монитора активного окна, а не первичного).
- **D. Курсор.** `follow`-режим: бейдж следует без рывков. **Ноль `PointerMoved` при скрытом бейдже:** `LANG_SWITCHER_LOG=switcher_windows=trace`, водить мышью 30 с при скрытом бейдже, затем `Select-String PointerMoved <лог> | Measure-Object` → `Count = 0`.
- **E. Фолбэк-опрос (три слоя наблюдаемости).** (1) при фокусе на Блокноте `LayoutSource::ForegroundPoll` в логе **не появляется** вообще; (2) `info!`-строки «fallback poll armed/disarmed» появляются ровно на взвод/снятие, в строке снятия — счётчик израсходованных тиков; (3) независимая от нашего кода проверка: `typeperf "\Thread(lang-switcher/*)\Context Switches/sec"` на потоке раскладки при фокусе на Блокноте ≈ 0 (живой `SetTimer` 500 мс дал бы ~2/с).
- **F. Трей.** Все шесть пунктов из задачи 18 (обработчики перехватывают события; галка возвращается после отказа; перезапуск `explorer.exe`; иконка не обновляется при открытом меню; «Выход» дописывает лог; читаемость иконки на 100/150/200 %).
- **G. Деградация (ADR-0007).** Отказ записи автозапуска — инъекция отказа через тестовый порт или изолированный ключ реестра, без изменения прав настоящего Run: галка отщёлкивается назад, пункт становится неактивным, тултип получает предупреждение, `config.toml` **не изменился**. Отсутствие звукового устройства (пункт 3 задачи 16). Перезапуск Explorer проверяется отдельным ручным сценарием: восстановление иконки и доставки раскладки. Сам перезапуск Explorer не равен панике потока и не доказывает срабатывание супервизора.
- **H. Конфиг.** `show_ms = 50` → в логе предупреждение о клампе и бейдж живёт 200 мс; `badge.colors.en = "red"` → предупреждение и цвет из встроенной палитры.

- [ ] **Шаг 2: замерить NFR (конкретные числа и способ)**

| NFR | Как мерить | Ожидание |
|---|---|---|
| ~0 % CPU в простое | `typeperf "\Process(lang-switcher)\% Processor Time" -si 5 -sc 12` (1 мин), бейдж скрыт, фокус на Блокноте | среднее `0.0`, единичные выбросы < 0.5 |
| RSS 5–15 МБ | `Get-Process lang-switcher \| Select-Object WorkingSet64, PrivateMemorySize64` через 5 мин работы и 20 переключений раскладки | `WorkingSet64` в 5–15 МБ; после 20 переключений не растёт (кэш ≤ 16 записей) |
| Бинарник 1–4 МБ | `cargo build --release -p switcher-app`, затем `(Get-Item target\release\lang-switcher.exe).Length` | 1 000 000 – 4 200 000 байт |
| Ноль `PointerMoved` при скрытом бейдже | пункт D | `Count = 0` |
| Ноль тиков фолбэк-опроса на обычном окне | пункт E, слои 1 и 3 | ни одной строки `ForegroundPoll`; context switches ≈ 0 |
| Лог-трафик в простое | размер файла лога до и после 10 мин простоя | прирост 0 байт |

Если размер бинарника вне бюджета — **не подкручивать профиль сборки молча**: принять решение и записать ADR (скил `adr`), помня жёсткий запрет `panic = "abort"` из ADR-0007 (он убил бы супервизор потоков). Если mixed-DPI даёт «дрожащую» на 1 px высоту бейджа — это уже предсказано в ADR-0006: квантование `dpi` в ключе кэша до ближайших 25 %, отдельным коммитом и записью в ADR-0006 как последствие. Если метрики бейджа (26/40/9/7/16 dip) визуально плохи на 150/200 % — поправить константы и зафиксировать финальные значения комментарием в `render.rs`.

- [ ] **Шаг 3: заполнить таблицу результатов**

В `docs/smoke/m1-windows.md` — таблица `| Пункт | Дата | Сборка (debug/release) | ОС и конфигурация мониторов | Результат | Заметки |`, заполненная **фактическими** прогонами. Незапущенный пункт помечается «не проверялось», а не «ок»: скил `quality-gates` требует доказательств, а не утверждений. Явно записать, что спека «показывает причину в настройках/трее» выполнена **наполовину**: трей есть, окна настроек нет (M2).

- [ ] **Шаг 4: adversarial-review всего M1**

Запустить проектный workflow `rust-adversarial-review` по накопленному диффу оболочки (задачи 15–20). Найденные замечания разбирать по одному: каждое либо исправляется отдельным коммитом с тестом, либо получает письменное обоснование отказа в чеклисте.

- [ ] **Шаг 5: гейты и коммит**

Запустить: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
Ожидание: чисто (кода не менялось, если шаг 4 не потребовал правок).

```bash
git add docs/smoke/m1-windows.md docs/superpowers/plans/2026-07-03-m1-windows-mvp.md
git commit -m "docs(m1): manual smoke checklist, NFR measurements and progress table"
```
