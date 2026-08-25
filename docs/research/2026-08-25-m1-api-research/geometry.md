# Геометрия бейджа и DPI: владение решениями

> Отчёт исследования от 2026-08-25. Каждое утверждение об API сверено по вендоренным исходникам
> в `~/.cargo/registry/src/index.crates.io-*/<крейт>-<версия>/` (context7 в той сессии был недоступен)
> и по learn.microsoft.com для семантики Win32; источник указан рядом с фактом.
> На основе этих отчётов приняты ADR-0005…ADR-0010. **При расхождении отчёта и ADR побеждает ADR** —
> отчёт фиксирует, что было известно на момент решения, и не переписывается вслед за ним.

## Ответ на вопрос

1. **Смещение от точки якоря до левого-верхнего угла бейджа принадлежит адаптеру оверлея** (`switcher-windows`), а не ядру.
2. **Клампинг в рабочую область монитора — тоже адаптеру.** Геометрию мониторов не знает никто, кроме адаптера: ни `switcher-core`, ни `switcher-app` никогда не видят ни одного прямоугольника монитора.
3. **Выбор монитора для `Fixed` — адаптеру.** `Placement::PrimaryBottomRight` удаляется: он вшивает в *тип порта* сразу две политики (какой монитор + какой угол), а политика — дело адаптера (монитор) и конфига (угол, M2).
4. **DPI никогда не доходит до ядра. `Engine::Event` для DPI не нужен и добавлять его нельзя.** Но DPI *не* целиком внутренний для адаптера: он нужен оболочке для выбора масштаба растеризации. Поэтому: `PlatformEvent::OverlayScaleChanged` **сохраняется**, единственный потребитель — `switcher-app` (перерастеризовать и вызвать `show` заново); `OverlayWindow::dpi_at(pos)` **не удаляется, но меняет форму** на `dpi_for(anchor)` — единственный потребитель — растеризатор в `switcher-app`. Текущая форма `dpi_at(pos: Point)` неверна по сигнатуре: для `ResolvedAnchor::Fixed` точки нет вообще, а монитор известен только адаптеру.
5. **Пиксельный размер выбирает оболочка** (`switcher-app`) чистой функцией `rasterize(&BadgeContent, dpi) -> BadgeImage` с кэшем по `(label, bg, fg, style, dpi)`. Для `Effect::ShowBadge` это значит: он **остаётся ровно таким, как сейчас** — `BadgeContent` логичен, DPI-независим, кэшируем и сравним; это не недостаток, а необходимое условие того, что ядро тестируется на любой ОС.
6. Минимальная поверхность: `Placement` удаляется, `ResolvedAnchor` переезжает в `switcher-platform::events`, `BadgeImage` получает поле `dpi`, `Effect::MoveBadge` начинает нести `ResolvedAnchor` вместо голого `Point`. Ломается ровно **1** из 42 тестов ядра.

## Рекомендованное решение (точно и проверяемо)

**Инвариант владения.** Ядро отвечает на вопрос «к чему привязан бейдж» (`ResolvedAnchor`) и «что на нём написано» (`BadgeContent`). Оболочка отвечает на вопрос «сколько это в пикселях» (`BadgeImage` + `dpi`). Адаптер отвечает на вопрос «где именно на экране левый-верхний угол» (offset + DPI-масштаб offset'а + монитор + клампинг). Три ответа, три владельца, ни одного пересечения.

**Почему смещение — у адаптера (п.1).** Четыре независимых причины, каждая достаточна:
- Смещение измеряется в **физических пикселях** и обязано масштабироваться с DPI (зазор 12 px при 100 % = 24 px при 200 %). Если бы смещение выдавало ядро, оно было бы либо логическим (тогда адаптер всё равно его домножает — число ядра не ответ), либо физическим (тогда ядру нужен DPI — противоречие с п.4).
- Смещение зависит от **собственного пиксельного размера бейджа** (чтобы у нижней кромки экрана перевернуть бейдж «над» каретку, нужна его высота). Размер известен только после растеризации, т. е. на стороне оболочки/адаптера.
- Смещение зависит от **вида якоря**: у каретки бейдж встаёт рядом со строкой (не закрывая набираемый текст), у курсора — как тултип ниже-правее hotspot'а. Это OS-конвенция (на macOS геометрия hotspot и направление оси Y другие) — правило 4 CLAUDE.md требует держать это за портами.
- Тестируемость **не теряется, а появляется**: в `switcher-windows` заводится чистый модуль `overlay::geometry` без `unsafe` и без вызовов ОС, куда все факты ОС приходят аргументами. Он покрывается табличными unit-тестами, которые гоняются на Windows-CI. `unsafe` остаётся только в «добыче фактов».

```rust
// crates/switcher-windows/src/overlay/geometry.rs — без unsafe, unit-тестируется
use switcher_platform::events::{Point, ResolvedAnchor};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorkArea { pub left: i32, pub top: i32, pub right: i32, pub bottom: i32 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MonitorFacts { pub work: WorkArea, pub dpi: u32 }

pub(crate) const DEFAULT_DPI: u32 = 96;      // USER_DEFAULT_SCREEN_DPI
const CURSOR_GAP_96: i32 = 12;               // логические px при 96 dpi
const CARET_GAP_96: i32 = 6;
const FIXED_MARGIN_96: i32 = 16;

#[inline]
pub(crate) fn scaled(logical_96: i32, dpi: u32) -> i32 {
    (logical_96 * dpi as i32) / DEFAULT_DPI as i32
}

/// Левый-верхний угол бейджа в физических экранных px. Чистая функция:
/// каждый факт ОС передан аргументом, поэтому тестируется таблично.
pub(crate) fn place(anchor: ResolvedAnchor, size: (u32, u32), m: MonitorFacts) -> Point { /* … */ }

/// Гарантирует, что весь прямоугольник бейджа лежит внутри `work`.
/// Если бейдж больше рабочей области — прижимается к left/top (переполнение принимается).
pub(crate) fn clamp(top_left: Point, size: (u32, u32), work: WorkArea) -> Point { /* … */ }
```

**Почему клампинг — у адаптера (п.2).** Прямоугольник для клампинга — это `MONITORINFO.rcWork` (рабочая область без панели задач; `rcMonitor` дал бы бейдж под панелью). Кламп делается **по рабочей области того монитора, который содержит точку якоря** (`MonitorFromPoint(..., MONITOR_DEFAULTTONEAREST)`), а не по объединению всех мониторов: бейдж, лежащий на шве двух мониторов с разным масштабом, растеризован под один DPI и половиной выглядит неправильно, а объединение позволило бы бейджу оказаться в «мёртвой зоне» неправоугольного рабочего стола. `MONITOR_DEFAULTTONEAREST` заодно закрывает случай, когда каретка отрапортована за пределами всех мониторов.

**Почему выбор монитора для `Fixed` — у адаптера, и какой это монитор (п.3).** `Fixed` достигается двумя путями: пользователь явно выбрал `AnchorPref::Fixed`, либо и каретка, и курсор недоступны (на Windows `GetCursorPos` практически всегда работает, кроме secure desktop, где рисовать всё равно нельзя). В обоих случаях «экран, на котором пользователь работает» лучше всего приближается **монитором активного окна**, а не первичным монитором: на связке «ноутбук + внешний» первичный — обычно не тот экран. Рекомендация: `MonitorFromWindow(GetForegroundWindow(), MONITOR_DEFAULTTOPRIMARY)`, при нулевом HWND — первичный монитор напрямую; угол — правый-нижний `rcWork` минус `scaled(FIXED_MARGIN_96, dpi)`. Это продуктовое решение, ему нужна строка в ADR. Угол становится полем конфига в M2 (`#[serde(default)]` — без бампа версии схемы), и тогда ядро передаёт его как `ResolvedAnchor::Fixed { corner }`.

**Почему DPI не доходит до ядра, но нужен оболочке (п.4, п.5).** Ядро с DPI перестало бы быть OS-независимым и потеряло бы главное свойство — «компилируется и полностью тестируется на любой ОС». Технически можно было бы завести `Engine::Event::ScaleChanged`, но он не порождал бы ни одного *решения*: ядро не растеризует и не считает пиксели, оно бы просто переслало число обратно как эффект. Это чистый шум в автомате и +N состояний в таблице тестов. Значит — **нет**.

Но рисовать пиксели должен кто-то, у кого есть `tiny-skia` + `ab_glyph` + байты шрифта, и это `switcher-app` (правило распределения крейтов). Замыкание получается такое, без единого хэндла и без единого колбэка через границу потока:

```
Effect::ShowBadge { content, anchor }
  → shell: dpi = overlay.dpi_for(anchor)                     // синхронно, без hop'а в поток оверлея
  → shell: img = raster_cache.get_or_render(&content, dpi)   // чистая функция + кэш
  → shell: overlay.show(&img, anchor)                        // плоские данные через канал
  → adapter (в своём потоке): facts = monitor_facts(anchor)
                              pos = geometry::place(anchor, (img.width, img.height), facts)
                              UpdateLayeredWindow(hwnd, .., pptDst=pos, psize=size, .., ULW_ALPHA)
                              if img.dpi != facts.dpi { send(OverlayScaleChanged { dpi: facts.dpi }) }
```

Ключевой ход — **поле `dpi` внутри `BadgeImage`**. Адаптер сравнивает `img.dpi` с DPI монитора, который он сам вывел, и *расхождение* — единственный триггер `OverlayScaleChanged`. Из этого следуют три хороших свойства:
- Пересечение мониторов при слежении за курсором **не зависит от `WM_DPICHANGED` вообще**. `WM_DPICHANGED` остаётся только для случая «пользователь сменил масштаб в Параметрах, пока бейдж стоит на месте» — т. е. для сценария, где никакого события мыши/раскладки нет.
- Цикл не самоподдерживается: при `img.dpi == facts.dpi` событие не отправляется, поэтому обратная связь сходится за один шаг после того, как курсор остановился. Оболочка обязана дополнительно игнорировать `OverlayScaleChanged { dpi }`, равный DPI последнего отрендеренного изображения (гасит гонку двух событий).
- Худший видимый артефакт — **один кадр** с картинкой «правильного размера для прошлого монитора»: адаптер всегда клампит по *фактическому* `img.width/height`, поэтому бейдж никогда не обрезан и никогда не вылезает за экран, он лишь на кадр не того масштаба. `move_to` при расхождении **перемещает сейчас, а размер меняет следующим событием** — никакого мигания.

**Обязательное предусловие всей геометрии.** Утверждение `Point` = «физические экранные пиксели» верно **только** при Per-Monitor-V2: без манифеста DPI-виртуализация подменит и `GetCursorPos`, и `rcWork`, и всё тихо разъедется. Рекомендация: на старте прочитать `GetDpiAwarenessContextForProcess` → `GetAwarenessFromDpiAwarenessContext` и залогировать `warn!`, если это не per-monitor; манифест — по overview.md, `SetProcessDpiAwarenessContext` — только как аварийный ран-тайм фолбэк (и он обязан выполниться до создания любого окна).

**Потоки.** `show`/`move_to`/`hide` форвардятся в поток оверлея (канал + `PostMessage`), как требует правило 5. `SWP_ASYNCWINDOWPOS` — **не** использовать: он решает ту же проблему в обход правила и делает порядок операций ненаблюдаемым. `dpi_for` наоборот **обязан** отвечать синхронно на потоке вызывающего, без hop'а в поток оверлея, иначе главный цикл получит взаимную блокировку с обработкой сообщений оверлея; это возможно потому, что `MonitorFromPoint`/`MonitorFromWindow`/`GetMonitorInfoW`/`GetDpiForMonitor`/`GetForegroundWindow` не принимают HWND нашего окна и не требуют владения им.

**Кэш DPI.** `MonitorFromPoint` + `GetDpiForMonitor` на каждое перемещение курсора (коалесцированное до частоты кадров) допустимо, но правильно кэшировать `HMONITOR -> dpi` и пересчитывать только при смене `HMONITOR`; кэш сбрасывается на `WM_DPICHANGED` и `WM_DISPLAYCHANGE`. Это не поллинг — все вызовы происходят строго внутри обработки уже пришедшего события (ADR-0003 соблюдён).

**Фичи `windows`, которые надо добавить в `switcher-windows`:** к текущим `Win32_Foundation` + `Win32_UI_WindowsAndMessaging` нужны `Win32_Graphics_Gdi` и `Win32_UI_HiDpi`. Точная разбивка — в разделе проверенных фактов; отдельно отмечу два неочевидных момента: `GetDpiForMonitor` требует **обе** фичи (`Win32_UI_HiDpi` и `Win32_Graphics_Gdi`), `UpdateLayeredWindow` требует `Win32_Graphics_Gdi` вопреки своему расположению в `WindowsAndMessaging`, а `MONITORINFOF_PRIMARY` лежит не в `Gdi`, а в `WindowsAndMessaging`.

## Рассмотренные альтернативы (по 1-2 строки каждая, с причиной отклонения)

- **Смещение считает ядро (`Effect::ShowBadge { top_left: Point }`).** Отклонено: смещение физическое и зависит от DPI и от пиксельного размера бейджа — ядру пришлось бы знать оба, что убивает OS-независимость и тестируемость на любой ОС.
- **Ядро получает топологию дисплеев (список `rcWork` + DPI) событием и само клампит.** Отклонено: ядро превращается в мини-оконный менеджер (подключение/отключение мониторов, смена разрешения и масштаба), таблица тестов раздувается кратно, а пользователь не получает ничего.
- **`Engine::Event::ScaleChanged { dpi }` + `Effect::RerenderBadge`.** Отклонено: событие не порождает ни одного решения ядра — оно бы транслировало число обратно; чистый шум в автомате.
- **Адаптер сам растеризует (`show(&BadgeContent, anchor)`).** Отклонено: тянет `tiny-skia` + `ab_glyph` + байты шрифта в единственный крейт с `unsafe`, и дублирует растеризацию трижды (Windows/macOS/Linux); `BadgeContent` при этом пришлось бы выселять из ядра.
- **Адаптер получает `Arc<dyn BadgeRenderer>` и тянет пиксели лениво, когда узнал DPI.** Отклонено прямым правилом проекта: это хэндл/колбэк через границу потока (рендер исполнился бы на потоке оверлея), что запрещено контрактом `switcher-platform`.
- **Оболочка передаёт набор изображений на все DPI сразу (`BadgeImageSet`), адаптер выбирает.** Отклонено: чтобы знать список DPI, оболочке нужен новый порт перечисления мониторов + событие смены конфигурации дисплеев — поверхность растёт, а не сокращается; выигрыш (удаление `OverlayScaleChanged`) меньше цены.
- **Рендер один раз в большом масштабе, адаптер уменьшает.** Отклонено: мыло на тексте, ADR-0004 требует именно пререндер под масштаб.
- **Оставить `Placement` как отдельный «платформенный» тип, оболочка маппит `ResolvedAnchor -> Placement`.** Отклонено: два параллельных типа об одном и том же, причём `Placement::At(Point)` теряет вид якоря, который адаптеру нужен для выбора смещения.
- **Оставить `Placement::PrimaryBottomRight`.** Отклонено: вшивает политику «первичный монитор» + «правый-нижний угол» в словарь порта; на «ноутбук + внешний» первичный монитор — почти всегда неправильный экран.
- **Кламп по объединению всех мониторов.** Отклонено: разрешает бейджу лечь на шов двух мониторов с разным масштабом и попасть в «мёртвую зону» неправоугольного рабочего стола.
- **Кламп по `rcMonitor` вместо `rcWork`.** Отклонено: бейдж уезжает под панель задач, т. е. становится невидимым — ровно то, от чего продукт лечит.
- **Полагаться на `WM_DPICHANGED` как на единственный источник смены масштаба.** Отклонено как единственный: доставка сообщения нашему `WS_EX_NOACTIVATE|WS_EX_TOOLWINDOW` окну при перемещении *нами самими* документально не гарантирована (см. UNVERIFIED); сравнение `img.dpi != monitor.dpi` не зависит ни от какого сообщения.
- **`SWP_ASYNCWINDOWPOS` вместо форварда в поток-владелец окна.** Отклонено: обходит правило 5 и делает порядок show/move/hide ненаблюдаемым.
- **`SetWindowPos` для смены и позиции, и размера при смене DPI.** Отклонено: `UpdateLayeredWindow` меняет позицию, размер и содержимое **одним** вызовом (документировано), т. е. без промежуточного кадра.

## Последствия: изменения в портах/событиях/эффектах (точные сигнатуры Rust)

`crates/switcher-platform/src/events.rs` — удалить `Placement`, добавить `ResolvedAnchor`, добавить `dpi` в `BadgeImage`:

```rust
/// Screen coordinate in physical pixels. Physical only because the process is
/// manifested Per-Monitor-V2; without that, DPI virtualization breaks every
/// coordinate in this file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point { pub x: i32, pub y: i32 }

/// Premultiplied-alpha RGBA image (byte order R,G,B,A), row-major, top-down,
/// already rasterized by the app shell for exactly `dpi` (96 = 100%).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadgeImage {
    pub width: u32,
    pub height: u32,
    pub rgba_premul: Vec<u8>,
    /// The DPI this image was rendered for. The overlay adapter compares it with
    /// the DPI of the monitor it actually places the badge on; a mismatch is the
    /// sole trigger for `PlatformEvent::OverlayScaleChanged`.
    pub dpi: u32,
}

/// What the badge is anchored to. Turning this into a top-left pixel position is
/// entirely the overlay adapter's job: it owns the anchor->corner offset, the DPI
/// scaling of that offset, the monitor choice for `Fixed`, and work-area clamping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedAnchor {
    Caret(Point),
    Cursor(Point),
    /// No usable anchor, or the user asked for a fixed corner: the adapter picks
    /// the monitor (active window's, else primary) and the corner itself.
    Fixed,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlatformEvent {
    LayoutChanged { layout: LayoutId, lang: LangTag, source: LayoutSource },
    PointerMoved { pos: Point },
    /// The visible badge's image was rendered for a different DPI than the monitor
    /// it now sits on (monitor crossing, or WM_DPICHANGED while it sat still).
    /// Consumed by `switcher-app` ONLY: re-rasterize at `dpi`, then call
    /// `OverlayWindow::show` again with the last known anchor. It is deliberately
    /// NOT mapped to any `switcher_core::engine::Event` — the core never sees DPI.
    OverlayScaleChanged { dpi: u32 },
}
```

`crates/switcher-platform/src/ports.rs`:

```rust
use crate::events::{BadgeImage, LangTag, LayoutId, Point, ResolvedAnchor};

/// Click-through, topmost, non-activating badge window. Owns ALL geometry:
/// anchor->top-left offset, DPI scaling of that offset, monitor selection for
/// `Fixed`, and clamping into the work area of the monitor holding the anchor.
pub trait OverlayWindow: Send {
    /// `image.dpi` should equal `self.dpi_for(anchor)`. If it does not, the badge
    /// is still shown at the image's real pixel size (never clipped, never off
    /// screen) and `OverlayScaleChanged` is emitted so the shell can re-render.
    fn show(&self, image: &BadgeImage, anchor: ResolvedAnchor);

    /// Re-place the visible badge. Never re-rasterizes: on a DPI mismatch it moves
    /// now with the current pixel size and emits `OverlayScaleChanged`.
    fn move_to(&self, anchor: ResolvedAnchor);

    fn hide(&self);

    /// Effective DPI (96 = 100%) of the monitor this adapter would place `anchor`
    /// on — including the monitor it picks for `Fixed`. Answered synchronously on
    /// the caller's thread (no hop into the overlay thread, so it cannot deadlock
    /// the main loop). Sole caller: the app shell's rasterizer, to choose scale.
    fn dpi_for(&self, anchor: ResolvedAnchor) -> u32;
}
```

`crates/switcher-core/src/engine.rs` — тип переезжает и ре-экспортируется, `MoveBadge` несёт якорь:

```rust
// ResolvedAnchor now lives in switcher-platform (the overlay adapter must see it),
// re-exported here so `switcher_core::engine::ResolvedAnchor` keeps resolving.
pub use switcher_platform::events::ResolvedAnchor;

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    QueryAnchor,
    /// Logical, DPI-free content plus the anchor. Pixels are the shell's job.
    ShowBadge { content: BadgeContent, anchor: ResolvedAnchor },   // UNCHANGED
    /// New anchor for the visible badge; the adapter re-derives offset, DPI and
    /// clamping. Carrying the anchor kind (not a bare Point) keeps `move_to`
    /// stateless and is what M2 caret tracking will need.
    MoveBadge { anchor: ResolvedAnchor },                          // CHANGED
    HideBadge,
    ArmHideTimer { after_ms: u64 },
    CancelHideTimer,
    SetPointerTracking(bool),
    PlaySound { cue: SoundCue, volume: f32 },
    UpdateTray { label: String, lang: LangTag },
    ApplyAutostart(bool),
    PersistConfig,
}
```

Единственное изменение в логике ядра — ветка `Event::Pointer` (engine.rs:115-118):

```rust
Event::Pointer { pos } => match self.badge {
    BadgeState::Visible { tracking: true } => {
        vec![Effect::MoveBadge { anchor: ResolvedAnchor::Cursor(pos) }]
    }
    _ => vec![],
},
```

Новый инвариант ядра (уже выполняется, но теперь его стоит записать в док-комментарий): `tracking == true` возможно только при `ResolvedAnchor::Cursor(_)` (engine.rs:198), поэтому `MoveBadge` всегда несёт `Cursor`. Вариант `Fixed` в `MoveBadge` при этом не является недопустимым состоянием — адаптер просто перевычислит фиксированный угол.

Состояние, которое обязана держать оболочка (`switcher-app`), и это надо назвать явно: `last_content: Option<BadgeContent>`, `last_anchor: Option<ResolvedAnchor>`, `last_render_dpi: Option<u32>`. Диспетчер `PlatformEvent -> Option<engine::Event>` возвращает `None` для `OverlayScaleChanged` и вместо этого выполняет локальное действие «перерастеризовать и `show` заново».

Изменения в `switcher-windows/Cargo.toml`:

```toml
windows = { workspace = true, features = [
    "Win32_Foundation",
    "Win32_UI_WindowsAndMessaging",
    "Win32_Graphics_Gdi",   # MonitorFromPoint/Window, GetMonitorInfoW, MONITORINFO,
                            # BLENDFUNCTION, CreateDIBSection; also gates UpdateLayeredWindow
    "Win32_UI_HiDpi",       # GetDpiForMonitor, MONITOR_DPI_TYPE, GetDpiForWindow
] }
```

## Ломающиеся тесты ядра (файл::тест -> что изменить)

Ровно **1 из 42**. Полный список мест, где вообще упоминаются затронутые типы, проверен `grep` по `crates/`: `Placement` — только `switcher-platform` (events.rs:55, ports.rs:5, ports.rs:31), ни одного использования вне крейта; `ResolvedAnchor` — только `engine.rs`; `MoveBadge` — три места (engine.rs:51 определение, :116 конструирование, :476 тест); интеграционных каталогов `tests/` в воркспейсе нет.

- **`crates/switcher-core/src/engine.rs::tests::pointer_moves_visible_tracking_badge`** (строка 476) — ассерт `assert_eq!(fx, vec![Effect::MoveBadge { pos: p(50, 60) }]);` меняется на `assert_eq!(fx, vec![Effect::MoveBadge { anchor: ResolvedAnchor::Cursor(p(50, 60)) }]);`.

Не ломаются, хотя выглядят рискованно (важно для оценки объёма работ):

- `engine.rs::tests::auto_anchor_prefers_caret_and_does_not_track_pointer`, `auto_anchor_falls_back_to_cursor_and_tracks`, `auto_anchor_falls_back_to_fixed_when_nothing_available`, `cursor_pref_ignores_caret`, `fixed_pref_ignores_caret_and_cursor`, `second_change_while_visible_requeries_anchor_and_rearms` — все шесть называют `ResolvedAnchor::{Caret,Cursor,Fixed}` по короткому пути и берут его из `use super::*`; благодаря `pub use switcher_platform::events::ResolvedAnchor;` в `engine.rs` путь не меняется. **Если ре-экспорт не сделать — сломаются эти шесть плюс тот, что выше, итого 7.**
- `pointer_is_ignored_when_badge_hidden`, `pointer_is_ignored_when_caret_anchored`, `stale_hide_timer_in_follow_mode_is_ignored` — ассертят `vec![]`, форма `MoveBadge` не важна.
- `hide_timer_hides_badge_and_stops_tracking`, `engine_with_visible_badge` — `MoveBadge` не встречается.
- `content.rs` (6 тестов) и `config.rs` (9 тестов) — не затрагиваются вообще: `BadgeContent` остаётся логическим, `Config` не получает новых полей (угол для `Fixed` — только в M2).
- `switcher-platform/src/events.rs::tests::lang_tag_primary_extracts_lowercase_primary_subtag` — не затрагивается.

## Проверенные факты API (утверждение -> путь к файлу-источнику)

Корень вендоренных исходников: `C:\Users\work\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\`

**windows-0.62.2, сигнатуры**

- `pub unsafe fn MonitorFromPoint(pt: POINT, dwflags: MONITOR_FROM_FLAGS) -> HMONITOR`, линкуется из `user32.dll` → `windows-0.62.2\src\Windows\Win32\Graphics\Gdi\mod.rs:1474-1477`
- `pub unsafe fn MonitorFromWindow(hwnd: HWND, dwflags: MONITOR_FROM_FLAGS) -> HMONITOR`, `user32.dll` → `...\Graphics\Gdi\mod.rs:1484-1488`
- `MONITOR_DEFAULTTONULL = MONITOR_FROM_FLAGS(0)`, `MONITOR_DEFAULTTOPRIMARY = 1`, `MONITOR_DEFAULTTONEAREST = 2`; `#[repr(transparent)] pub struct MONITOR_FROM_FLAGS(pub u32)` → `...\Graphics\Gdi\mod.rs:5863-5867`
- `pub unsafe fn GetMonitorInfoW(hmonitor: HMONITOR, lpmi: *mut MONITORINFO) -> windows_core::BOOL`, `user32.dll` — возвращает `BOOL`, **не** `Result`, т. е. проверять руками → `...\Graphics\Gdi\mod.rs:1102-1105`
- `#[repr(C)] #[derive(Clone, Copy, Debug, Default, PartialEq)] pub struct MONITORINFO { cbSize: u32, rcMonitor: RECT, rcWork: RECT, dwFlags: u32 }` — `Default` есть, но `cbSize` всё равно надо выставить вручную → `...\Graphics\Gdi\mod.rs:5833-5840`
- `pub const MONITORINFOF_PRIMARY: u32 = 1u32` лежит **не** в `Gdi`, а в `WindowsAndMessaging` → `windows-0.62.2\src\Windows\Win32\UI\WindowsAndMessaging\mod.rs:5076`
- `pub unsafe fn GetDpiForMonitor(hmonitor: HMONITOR, dpitype: MONITOR_DPI_TYPE, dpix: *mut u32, dpiy: *mut u32) -> windows_core::Result<()>`; линкуется из `api-ms-win-shcore-scaling-l1-1-1.dll`, внутри `HRESULT.ok()` → `windows-0.62.2\src\Windows\Win32\UI\HiDpi\mod.rs:37-42`
- `MDT_EFFECTIVE_DPI = MONITOR_DPI_TYPE(0)` (численно совпадает с `MDT_DEFAULT`), `MDT_ANGULAR_DPI = 1`, `MDT_RAW_DPI = 2`; `#[repr(transparent)] pub struct MONITOR_DPI_TYPE(pub i32)` → `...\UI\HiDpi\mod.rs:266-273`
- `pub unsafe fn GetDpiForWindow(hwnd: HWND) -> u32`, `user32.dll`; **без** cfg на `Win32_Graphics_Gdi` → `...\UI\HiDpi\mod.rs:48-52`
- `pub unsafe fn SetProcessDpiAwarenessContext(value: DPI_AWARENESS_CONTEXT) -> windows_core::Result<()>`, `user32.dll` → `...\UI\HiDpi\mod.rs:136-140`; `GetDpiAwarenessContextForProcess` → `...\UI\HiDpi\mod.rs:32-36`; `GetAwarenessFromDpiAwarenessContext` → `...\UI\HiDpi\mod.rs:17-20`
- `pub unsafe fn SetWindowPos(hwnd: HWND, hwndinsertafter: Option<HWND>, x: i32, y: i32, cx: i32, cy: i32, uflags: SET_WINDOW_POS_FLAGS) -> windows_core::Result<()>` — `hwndinsertafter` именно `Option`, возврат `Result` → `...\UI\WindowsAndMessaging\mod.rs:2278-2282`
- `SWP_NOSIZE = 1`, `SWP_NOMOVE = 2`, `SWP_NOZORDER = 4`, `SWP_NOACTIVATE = 16`, `SWP_SHOWWINDOW = 64`, `SWP_HIDEWINDOW = 128`; `#[repr(transparent)] pub struct SET_WINDOW_POS_FLAGS(pub u32)` → `...\UI\WindowsAndMessaging\mod.rs:6396-6406` и `:5905-5907`
- `pub unsafe fn UpdateLayeredWindow(hwnd: HWND, hdcdst: Option<HDC>, pptdst: Option<*const POINT>, psize: Option<*const SIZE>, hdcsrc: Option<HDC>, pptsrc: Option<*const POINT>, crkey: COLORREF, pblend: Option<*const BLENDFUNCTION>, dwflags: UPDATE_LAYERED_WINDOW_FLAGS) -> windows_core::Result<()>`, `user32.dll`, помечена `#[cfg(feature = "Win32_Graphics_Gdi")]` → `...\UI\WindowsAndMessaging\mod.rs:2437-2443`
- `ULW_COLORKEY = 1`, `ULW_ALPHA = 2`, `ULW_OPAQUE = 4`; `pub struct UPDATE_LAYERED_WINDOW_FLAGS(pub u32)` → `...\UI\WindowsAndMessaging\mod.rs:6646-6649` и `:6673-6675`
- `#[repr(C)] pub struct BLENDFUNCTION { BlendOp: u8, BlendFlags: u8, SourceConstantAlpha: u8, AlphaFormat: u8 }` → `...\Graphics\Gdi\mod.rs:2410-2417`; `AC_SRC_OVER: u32 = 0`, `AC_SRC_ALPHA: u32 = 1` → `...\Graphics\Gdi\mod.rs:2174-2175`
- `pub const WM_DPICHANGED: u32 = 736u32` (0x02E0) → `...\UI\WindowsAndMessaging\mod.rs:6934`
- `pub unsafe fn GetForegroundWindow() -> HWND`, `user32.dll` → `...\UI\WindowsAndMessaging\mod.rs:865-869`
- Вспомогательное для DIB-бэкинга оверлея: `CreateCompatibleDC` → `...\Graphics\Gdi\mod.rs:207`, `CreateDIBSection` → `:242`, `SelectObject` → `:1756`, `DeleteDC` → `:448`, `DeleteObject` → `:463`, `EnumDisplayMonitors` → `:559`; `MONITORENUMPROC` → `:5832`

**windows-0.62.2, фичи (крейт feature-gated)**

- `Win32_Graphics_Gdi = ["Win32_Graphics"]`, `Win32_Graphics = ["Win32"]`, `Win32 = ["Win32_Foundation"]`, `Win32_Foundation = ["Win32"]`, `Win32_UI_HiDpi = ["Win32_UI"]`, `Win32_UI_WindowsAndMessaging = ["Win32_UI"]`, `Win32_UI = ["Win32"]` → `windows-0.62.2\Cargo.toml:376, 416, 419, 440, 681, 688, 707`
- Модуль `Gdi` объявлен под `#[cfg(feature = "Win32_Graphics_Gdi")]` → `windows-0.62.2\src\Windows\Win32\Graphics\mod.rs:32-33`; модули `HiDpi` и `WindowsAndMessaging` — под `Win32_UI_HiDpi` / `Win32_UI_WindowsAndMessaging` → `windows-0.62.2\src\Windows\Win32\UI\mod.rs:9-10, 27-28`
- Итог по каждому запрошенному API: `MonitorFromPoint`, `GetMonitorInfoW`, `MONITORINFO` → `Win32_Graphics_Gdi`. `GetDpiForMonitor` + `MONITOR_DPI_TYPE` → `Win32_UI_HiDpi` **и** `Win32_Graphics_Gdi` (двойной cfg, `...\UI\HiDpi\mod.rs:37-38`). `GetDpiForWindow` → только `Win32_UI_HiDpi`. `SetWindowPos` + `SWP_*` и `WM_DPICHANGED` → `Win32_UI_WindowsAndMessaging` (уже включена). `UpdateLayeredWindow` → `Win32_UI_WindowsAndMessaging` **и** `Win32_Graphics_Gdi`.

**tiny-skia-0.12.0**

- `Pixmap` владеет **премультиплицированными** RGBA-пикселями, порядок байт R,G,B,A → `tiny-skia-0.12.0\src\pixmap.rs:26` («A container that owns premultiplied RGBA pixels») и `:317` («premultiplied RGBA pixels (byteorder: RGBA)»); `data()` → `:230`, `take()` → `:262`, `width()/height()` → `:203/:209`. Следствие: формат совпадает с контрактом `BadgeImage.rgba_premul`, но 32-битный DIB под `UpdateLayeredWindow` — BGRA, т. е. в адаптере нужен swizzle R↔B (это вычислительный шаг, не API-факт).

**Microsoft Learn (семантика, не сигнатуры)**

- `WM_DPICHANGED` посылается когда «The window is moved to a new monitor that has a different DPI» либо «The DPI of the monitor hosting the window changes»; `HIWORD(wParam)`/`LOWORD(wParam)` = Y/X DPI (для Windows-приложений совпадают), `lParam` = `RECT*` с предлагаемыми позицией и размером; «This message is only relevant for PROCESS_PER_MONITOR_DPI_AWARE applications or DPI_AWARENESS_PER_MONITOR_AWARE threads»; `USER_DEFAULT_SCREEN_DPI = 96` → https://learn.microsoft.com/en-us/windows/win32/hidpi/wm-dpichanged
- `GetDpiForWindow`: `DPI_AWARENESS_UNAWARE` → 96, `SYSTEM_AWARE` → системный DPI, `PER_MONITOR_AWARE` → «The DPI of the monitor where the window is located»; невалидный `hwnd` → 0. Т. е. для геометрии надёжнее `GetDpiForMonitor` (он не зависит от awareness), а `GetDpiForWindow` — только как сверка внутри `WM_DPICHANGED` → https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getdpiforwindow
- `UpdateLayeredWindow` — «Updates the position, size, shape, content, and translucency of a layered window»: `pptDst` = «the new screen position» (NULL если позиция не меняется), `psize` = «the new size» (NULL если размер не меняется), `hdcSrc` NULL если содержимое не меняется, `ULW_ALPHA` = «Use pblend as the blend function»; «UpdateLayeredWindow always updates the entire window». Подтверждает: позиция + размер + содержимое меняются **одним** вызовом → https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-updatelayeredwindow
- `SetWindowPos`: `SWP_NOSIZE` — «Retains the current size (ignores the cx and cy parameters)», `SWP_NOZORDER` — «Retains the current Z order (ignores the hWndInsertAfter parameter)», `SWP_NOACTIVATE` — «Does not activate the window», `SWP_SHOWWINDOW` — «Displays the window», `SWP_HIDEWINDOW` — «Hides the window»; `SWP_ASYNCWINDOWPOS` — «If the calling thread and the thread that owns the window are attached to different input queues, the system posts the request to the thread that owns the window» → https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowpos
- `MONITORINFO`: `cbSize` — «Set this member to sizeof(MONITORINFO) before calling GetMonitorInfo»; `rcWork` — «the work area rectangle of the display monitor, expressed in virtual-screen coordinates… if the monitor is not the primary display monitor, some of the rectangle's coordinates may be negative values»; `MONITORINFOF_PRIMARY` — «This is the primary display monitor» → https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-monitorinfo

## UNVERIFIED / открытые вопросы

- **Доставка `WM_DPICHANGED` при нашем собственном перемещении окна.** Learn говорит только «The window is moved to a new monitor that has a different DPI», не различая, кто переместил, и не уточняя, приходит ли сообщение синхронно (реентрантно) внутри нашего `SetWindowPos`/`UpdateLayeredWindow` или постится. **UNVERIFIED.** Предложенная архитектура намеренно от этого не зависит: пересечение мониторов ловится сравнением `img.dpi != monitor.dpi`. Нужен пункт в ручном smoke-чеклисте.
- **Приходит ли `WM_DPICHANGED` окну `WS_EX_LAYERED|WS_EX_TRANSPARENT|WS_EX_NOACTIVATE|WS_EX_TOOLWINDOW` вообще.** Документально не исключено, но и не подтверждено. **UNVERIFIED.** Деградация честная: без него ломается только сценарий «масштаб сменили в Параметрах, пока бейдж стоит и мышь не двигается»; следующий `show`/`move_to` всё исправит.
- **Потоковая аффинность `MonitorFromPoint` / `MonitorFromWindow` / `GetMonitorInfoW` / `GetDpiForMonitor` / `GetForegroundWindow`.** Ни одна из этих функций не принимает HWND *нашего* окна, и требования вызывать их на потоке-владельце в документации нет — но это вывод, а не цитата. **UNVERIFIED** для синхронного `dpi_for` без hop'а; проверять smoke-тестом с двумя потоками.
- **Может ли `GetForegroundWindow()` вернуть `NULL` и что именно об этом говорит Learn** — страницу не запрашивал. **UNVERIFIED.** Код всё равно обязан обрабатывать нулевой HWND (→ первичный монитор).
- **Гарантия `dpix == dpiy` у `GetDpiForMonitor`.** Learn утверждает равенство осей для `WM_DPICHANGED` («The values of the X-axis and the Y-axis are identical for Windows apps»); распространение этого на `GetDpiForMonitor` — вывод. **UNVERIFIED.** Использовать `dpix`, `dpiy` игнорировать (или залогировать расхождение).
- **`embed-manifest` 0.1.5/1.5 API** — крейт не вендорен в этой сессии, код эмиссии манифеста Per-Monitor-V2 описать нельзя. **UNVERIFIED.** Проверить перед написанием `build.rs`.
- **Стоимость `GetDpiForMonitor` на кадр** — предположение «дешёво, но лучше кэшировать по `HMONITOR`» не измерено. **UNVERIFIED.**
- **Продуктовое решение**: «`Fixed` = монитор активного окна, иначе первичный; правый-нижний угол `rcWork` с отступом 16 логических px» — нигде не зафиксировано, требует подтверждения владельцем и строки в ADR (кандидат: расширить ADR-0004 либо новый ADR «владение геометрией бейджа»). Числа зазоров (12/6/16 логических px) — тоже продуктовые, не проверенные.
- **Открытый архитектурный вопрос**: чистая функция «кламп прямоугольника в рабочую область» будет продублирована в трёх адаптерах. Правило `switcher-platform` («ports + flat data only») запрещает положить её туда. Варианты на будущее: отдельный крейт `switcher-geometry` без OS-зависимостей, либо явное ослабление правила. Для M1 — осознанный non-goal (у macOS перевёрнутая ось Y, у X11 рабочая область существует только через EWMH `_NET_WORKAREA`, так что общего кода там меньше, чем кажется).
- **Открытый вопрос по M2**: когда появится слежение за кареткой (`EVENT_OBJECT_LOCATIONCHANGE`), понадобится `Effect::SetCaretTracking(bool)` рядом с `SetPointerTracking`, и `Engine::Event::CaretMoved { pos }`. Предложенная форма `MoveBadge { anchor }` это уже выдерживает без изменения порта — это и есть основная причина не оставлять `MoveBadge { pos }`.