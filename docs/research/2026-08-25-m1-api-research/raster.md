# Растеризация бейджа, формат пикселей и кэш

> Отчёт исследования от 2026-08-25. Каждое утверждение об API сверено по вендоренным исходникам
> в `~/.cargo/registry/src/index.crates.io-*/<крейт>-<версия>/` (context7 в той сессии был недоступен)
> и по learn.microsoft.com для семантики Win32; источник указан рядом с фактом.
> На основе этих отчётов приняты ADR-0005…ADR-0010. **При расхождении отчёта и ADR побеждает ADR** —
> отчёт фиксирует, что было известно на момент решения, и не переписывается вслед за ним.

## Ответ на вопрос

Растеризация живёт **только** в `crates/switcher-app/src/render.rs` как свободная чистая функция `render_badge(font, key, metrics) -> Result<BadgeImage, RenderError>`; кэш — там же, отдельным типом `BadgeCache`, ключ = `BadgeKey { label, bg, fg, style, dpi }`.

Главный вывод по байт-порядку: **конверсия нужна, и имя поля `rgba_premul` неверно.** `tiny_skia::Pixmap` отдаёт premultiplied-**RGBA** (байт 0 = R), а 32bpp `BI_RGB` DIB, который требует `UpdateLayeredWindow`, — это DWORD `0x00RRGGBB` + альфа в старшем байте, то есть на little-endian порядок байт в памяти **B, G, R, A**. Требуется swap каналов R↔B (премультипликация при этом сохраняется — демультиплицировать НЕЛЬЗЯ). Swap принадлежит `render.rs`, а не `overlay.rs`, потому что кэш должен хранить уже готовые к `memcpy` байты: иначе swap выполнялся бы на каждый `show()` (каждое переключение), а не на каждый cache-miss. Поле переименовать в `bgra_premul`.

`Effect::ShowBadge` менять **не нужно**: DPI runtime получает сам через порт (`OverlayWindow`), ядро о DPI не знает. Ломающихся тестов ядра — **0**.

## Рекомендованное решение (точно и проверяемо)

### 1. Где растеризация и её сигнатура

`crates/switcher-app/src/render.rs` — без OS-вызовов, без окна, без GDI, тестируется на любой ОС (`tiny-skia` и `ab_glyph` платформо-независимы).

```rust
//! crates/switcher-app/src/render.rs
use ab_glyph::{Font, FontRef, InvalidFont, PxScale, ScaleFont, point};
use std::collections::HashMap;
use switcher_core::content::{BadgeContent, BadgeStyle, Rgb8};
use switcher_platform::events::BadgeImage;
use tiny_skia::{Color, FillRule, Mask, Paint, PathBuilder, Pixmap, Rect, Transform};

/// Метрики бейджа в device-independent pixels при 96 dpi. Физика = round(dip * dpi / 96).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BadgeMetrics {
    pub height_dip: f32,     // 26.0
    pub min_width_dip: f32,  // 40.0
    pub pad_x_dip: f32,      // 9.0
    pub radius_dip: f32,     // 7.0
    pub text_px_dip: f32,    // 16.0  -> PxScale (высота текста ascent-descent)
}

impl Default for BadgeMetrics {
    fn default() -> Self {
        Self { height_dip: 26.0, min_width_dip: 40.0, pad_x_dip: 9.0, radius_dip: 7.0, text_px_dip: 16.0 }
    }
}

/// Всё, что меняет пиксели. Он же — ключ кэша.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BadgeKey {
    pub label: String,
    pub bg: Rgb8,
    pub fg: Rgb8,
    pub style: BadgeStyle,
    /// Эффективный DPI монитора-хозяина (96 = 100%).
    pub dpi: u32,
}

impl BadgeKey {
    pub fn new(content: &BadgeContent, dpi: u32) -> Self { /* клонирует поля content + dpi */ }
    fn scale(&self) -> f32 { self.dpi as f32 / 96.0 }
}

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("pixmap allocation failed for {width}x{height}")]
    PixmapAlloc { width: u32, height: u32 },
    #[error("badge geometry is degenerate at dpi {dpi}")]
    Geometry { dpi: u32 },
}

/// Ink-габариты уложенной строки: чистая, измеряемая отдельно.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LabelInk { pub width: f32, pub height: f32, pub left: f32, pub top: f32 }

/// Чистая: одинаковые входы -> побайтово одинаковый выход. Ни окна, ни GDI, ни I/O.
/// Выход: premultiplied **BGRA**, top-down, stride == width * 4.
pub fn render_badge(
    font: &FontRef<'_>,
    key: &BadgeKey,
    metrics: &BadgeMetrics,
) -> Result<BadgeImage, RenderError>;

/// Чистый помощник: измеряет 2-буквенную строку по advance+kern и по union px_bounds.
pub fn measure_label(font: &FontRef<'_>, label: &str, px: f32) -> LabelInk;
```

Внутренний конвейер `render_badge` (все вызовы проверены по исходникам, ссылки ниже):

1. `let sc = font.as_scaled(PxScale::from((metrics.text_px_dip * scale).round()))`.
2. `measure_label`: перо `pen = 0.0`, для каждого символа `id = sc.glyph_id(c)`, `pen += sc.kern(prev, id)`, глиф `id.with_scale_and_position(sc.scale(), point(pen, 0.0))`, `pen += sc.h_advance(id)`; ink-bbox = union `OutlinedGlyph::px_bounds()`.
3. Размеры: `h = round(height_dip*scale)`, для `Text` — `w = round(max(min_width_dip*scale, ink.width + 2*pad_x_dip*scale))`, для `Color` — `w = h`.
4. `Pixmap::new(w, h)` (полностью прозрачный (0,0,0,0) по умолчанию).
5. Rounded-rect путь: `PathBuilder` + `move_to/line_to/cubic_to` (κ = 0.5523 · r) + `close` + `finish`; заливка `pixmap.fill_path(&path, &bg_paint, FillRule::Winding, Transform::identity(), None)`, `bg_paint.set_color_rgba8(bg.r, bg.g, bg.b, 255)`, `anti_alias = true` (default).
6. Текст (только `BadgeStyle::Text`): `Mask::new(w, h)`, в `mask.data_mut()` пишем покрытие из `OutlinedGlyph::draw(|gx, gy, c| ...)` (`saturating_add` по `(c*255)` — на случай пересечения bbox кернованных пар), позиция глифов сдвинута так, чтобы ink-bbox был центрирован в бейдже. Затем **тот же** rounded-rect путь заливается fg-цветом через маску: `pixmap.fill_path(&path, &fg_paint, FillRule::Winding, Transform::identity(), Some(&mask))` — текст физически не может вылезти за скруглённую форму.
7. `let mut data = pixmap.take();` (premultiplied RGBA) → `for px in data.chunks_exact_mut(4) { px.swap(0, 2); }` → premultiplied BGRA. Никакого `take_demultiplied()`.
8. `Ok(BadgeImage { width: w, height: h, bgra_premul: data })`.

Правило устойчивости: если для метки не получилось ни одного ink-пикселя (нелатинский subtag → `glyph_id` = `GlyphId(0)`, `outline_glyph` = `None`), функция откатывается к рендеру `BadgeStyle::Color` (сплошной swatch) — пустой бейдж невозможен.

### 2. Байт-порядок (главное)

| Звено | Формат |
|---|---|
| `tiny_skia::Pixmap::data()/take()` | premultiplied, **RGBA** (байт 0 = R) |
| DIB 32bpp `BI_RGB` (`UpdateLayeredWindow`) | DWORD `0x00RRGGBB`, LE ⇒ байты **B,G,R,X**; при `AC_SRC_ALPHA` старший байт = alpha ⇒ **BGRA** |
| Премультипликация | требуется обоими — конверсии по альфе НЕТ |

Итого: единственная необходимая операция — swap байтов 0↔2. Она в `render.rs` (safe Rust, один проход, результат кэшируется). `overlay.rs` в `switcher-windows` получает уже готовый буфер и делает **прямой `copy_from_slice`**: для 32bpp stride DIB = `((w*32 + 31) & !31) >> 3 = w*4`, то есть padding отсутствует и `bgra_premul.len() == stride * h` в точности. `biHeight = -(height as i32)` даёт top-down DIB, что совпадает с документированным «row-major, top-down» у `BadgeImage`.

Вывод: **имя `rgba_premul` неверно** и должно стать `bgra_premul` с уточнённым doc-комментарием. Это не косметика: имя — единственная защита от того, что адаптер поверит ему и получит синие «RU» вместо красных.

### 3. Кэш

- **Ключ:** `BadgeKey { label: String, bg: Rgb8, fg: Rgb8, style: BadgeStyle, dpi: u32 }`. Это исчерпывающий список входов, меняющих пиксели: `label`/`bg`/`fg`/`style` — все поля `BadgeContent`; `dpi` — единственное, что приходит извне. `BadgeMetrics` в ключ не входит, потому что живёт в самом `BadgeCache` и константен на время процесса (если в M2 метрики станут настраиваемыми — их изменение обрабатывается `clear()`, а не расширением ключа).
- **Где живёт:** `crates/switcher-app/src/render.rs`, тип `BadgeCache`, владелец — структура runtime главного потока (та же, что исполняет `Effect`). Не в `switcher-core` (там нет пикселей), не в `switcher-windows` (иначе каждый адаптер дублирует кэш и swap).
- **Тип:** `HashMap<BadgeKey, BadgeImage>` внутри
  ```rust
  pub struct BadgeCache {
      font: FontRef<'static>,
      metrics: BadgeMetrics,
      entries: HashMap<BadgeKey, BadgeImage>,
  }
  impl BadgeCache {
      pub fn new(font_bytes: &'static [u8], metrics: BadgeMetrics) -> Result<Self, InvalidFont>;
      pub fn image(&mut self, content: &BadgeContent, dpi: u32) -> Result<&BadgeImage, RenderError>;
      pub fn clear(&mut self);
      pub fn len(&self) -> usize;
  }
  pub const MAX_ENTRIES: usize = 16;
  ```
- **Ёмкость в худшем случае для M1:** (число различных primary-subtag среди установленных раскладок) × (число различных DPI, встреченных за сессию) × (число стилей за сессию, обычно 1). Реалистично ≤ 4 × 3 = 12. Арифметика памяти: при 96 dpi бейдж ≈ 42×26 = 1092 px ≈ 4.3 КБ; при 384 dpi (400 %) ≈ 168×104 = 17 472 px ≈ 68 КБ. При `MAX_ENTRIES = 16` теоретический потолок ≈ 1.1 МБ, реалистично ≤ 100 КБ — вписывается в бюджет 5–15 МБ RSS.
- **Вытеснение:** LRU не нужен. Достаточно `if entries.len() >= MAX_ENTRIES { entries.clear(); }` перед вставкой (clear-on-full): промах стоит один re-render (< 1 мс), а сценария с 16 живыми ключами на реальной машине не бывает.
- **Что инвалидирует:** формально — ничего. Изменение конфига (`badge.style`, `badge.colors`) даёт другой `BadgeContent` ⇒ другой ключ; смена DPI даёт другой ключ; смена раскладки даёт другой ключ. Это вопрос гигиены памяти, а не корректности. Практика: вызывать `BadgeCache::clear()` при исполнении `Effect::PersistConfig` (он срабатывает на любую мутацию конфига) — так мёртвые записи от старой палитры/стиля не накапливаются.
- **DPI-событие:** `PlatformEvent::OverlayScaleChanged { dpi }` в ядро **не** идёт. Runtime держит `last_shown: Option<(BadgeContent, Placement)>` и при смене DPI сам делает `cache.image(content, new_dpi)` + `overlay.show(...)` — нужен именно `show`, а не `move_to`, потому что меняется размер окна (`SetWindowPos(SWP_NOSIZE)` его не поменяет).

### 4. Измерение текста и шрифт

Нужный минимум API `ab_glyph` (всё — из проверенных исходников): `FontRef::try_from_slice`, трейт `Font` (`glyph_id`, `outline_glyph`, `as_scaled`, `units_per_em`, `ascent_unscaled`, `descent_unscaled`), трейт `ScaleFont` (`scale`, `h_advance`, `h_side_bearing`, `kern`, `ascent`, `descent`, `height`, `scaled_glyph`), `GlyphId::with_scale_and_position`, `PxScale::from(f32)`, `OutlinedGlyph::px_bounds()` + `OutlinedGlyph::draw(|x, y, coverage|)`, `point()`.

Важный контракт: `px_bounds()` — консервативный целочисленный bbox чернил, ровно того размера, в который пишет `draw` (координаты callback — `0..width`, `0..height` относительно `px_bounds().min`); он **не** совпадает с layout-bounds (`glyph_bounds`). Для оптического центрирования двух заглавных букв использовать union `px_bounds`, а не advance.

Что встраивается и как:

- Файл: `crates/switcher-app/assets/fonts/Inter-SemiBold-subset.ttf` — **статический** инстанс (не variable), сабсет на `A–Z` + `?` (+ `0–9` на будущее), ~10–20 КБ, сделан `pyftsubset` и закоммичен как артефакт.
- Рядом: `assets/fonts/OFL.txt` (SIL Open Font License 1.1 — Inter) и `assets/fonts/README.md` с точной командой сабсеттинга и версией исходного шрифта (воспроизводимость).
- Загрузка: `const FONT: &[u8] = include_bytes!("../assets/fonts/Inter-SemiBold-subset.ttf");` → `FontRef::try_from_slice(FONT)?` в `BadgeCache::new` (даёт `FontRef<'static>` без копии в heap; `FontVec::try_from_vec` копировал бы). Никаких `static`/`LazyLock`: `BadgeCache` живёт на потоке главного цикла, `Sync` не требуется.
- Метка всегда ASCII (`BadgeContent::for_lang` делает `to_ascii_uppercase`), поэтому сабсет A–Z + `?` покрывает всё, включая `"??"` для пустого lang-tag.
- Лицензионная гигиена: сам `ab_glyph` — Apache-2.0, `tiny-skia` — BSD-3-Clause; и то и другое, плюс OFL шрифта, должны попасть в NOTICE/THIRD-PARTY файл сборки.

### 5. Feature-флаги (важно для размера бинарника)

Сейчас в `Cargo.toml` workspace обе библиотеки подключены с default-фичами. Нужно урезать:

```toml
tiny-skia = { version = "0.12", default-features = false, features = ["std", "simd"] }
ab_glyph  = { version = "0.2",  default-features = false, features = ["std"] }
```

- `tiny-skia`: `std` обязателен (иначе `compile_error!`), `simd` даёт SSE2/NEON-путь на стабильном компиляторе; отключение `png-format` выкидывает из графа `png`, `flate2`, `miniz_oxide`, `fdeflate`, `crc32fast`, `adler2` — они не нужны ни для одного шага конвейера (мы не декодируем PNG).
- `ab_glyph`: отключение `variable-fonts` + `gvar-alloc` корректно ровно потому, что встраивается статический инстанс, а не variable-шрифт.
- `switcher-windows` дополнительно нужны фичи `windows`: `Win32_Graphics_Gdi` и `Win32_UI_HiDpi` (сейчас объявлены только `Win32_Foundation`, `Win32_UI_WindowsAndMessaging`).
- `switcher-app` нужен `thiserror` (для `RenderError`) — в workspace он уже есть, в крейт не подключён.

## Рассмотренные альтернативы (по 1-2 строки каждая, с причиной отклонения)

- **Растеризация в `switcher-windows`** (D2D/DirectWrite или GDI `DrawText`). Отклонено: нарушает изоляцию платформы, тянет unsafe и COM в путь рендера и делает бейдж непроверяемым unit-тестом на любой ОС.
- **Растеризация в `switcher-core`.** Отклонено: `tiny-skia`/`ab_glyph` — тяжёлые зависимости; ядро обязано остаться доменом без графики (правило CLAUDE.md).
- **Swap R↔B в `overlay.rs`.** Отклонено: swap выполнялся бы на каждый `show()`, а не на каждый cache-miss, и был бы продублирован в каждом OS-адаптере; плюс пиксельная арифметика уехала бы в единственный крейт, где разрешён unsafe.
- **Оставить `rgba_premul` и «просто помнить» про swap.** Отклонено: имя поля — контракт порта; неверное имя гарантированно даст перевёрнутый канал при следующей реализации.
- **`Pixmap::take_demultiplied()` перед отдачей.** Отклонено: `BLENDFUNCTION.AlphaFormat = AC_SRC_ALPHA` требует именно premultiplied; демультипликация сломала бы композицию и добавила потери округления.
- **Вшитые PNG вместо рантайм-рендера** (вариант из ADR-0004). Отклонено: цвета настраиваются через `badge.colors`, а DPI бывает произвольным (кастомное масштабирование) — заранее заготовить все комбинации нельзя; плюс PNG вернул бы `png-format` и его 6 транзитивных крейтов.
- **Предрендер всех бейджей на старте (eager, «per language × per DPI»).** Отклонено: набор языков и DPI известен только в рантайме; лениво-заполняемый кэш даёт тот же результат с быстрым старом (требование «мгновенный старт»).
- **LRU/`lru`-crate для кэша.** Отклонено: 16 записей по ≤ 68 КБ, промах стоит < 1 мс — политика вытеснения не окупает зависимость.
- **`Pixmap::fill_rect(full, fg, .., Some(&mask))` для текста.** Отклонено в пользу `fill_path(rounded_rect, fg, .., Some(&mask))`: второй вариант дополнительно клипует чернила по скруглённой форме бесплатно.
- **`Mask::from_pixmap(MaskType::Alpha)` через отдельный Pixmap с глифами.** Отклонено: лишняя аллокация 4×; `Mask::new` + `data_mut()` + `OutlinedGlyph::draw` пишет покрытие напрямую в 1 байт/пиксель.
- **Проведение `PlatformEvent::OverlayScaleChanged` через ядро новым `Event`.** Отклонено: добавило бы состояние и тесты в ядро ради чисто презентационной перерисовки; runtime и так владеет `last_shown`.
- **Добавить `dpi` в `Effect::ShowBadge`.** Отклонено: ядро стало бы знать про физические пиксели, и это единственная альтернатива, ломающая существующие тесты (список — ниже).
- **`FontVec::try_from_vec(FONT.to_vec())`.** Отклонено: копия ~20 КБ в heap без выгоды; `include_bytes!` даёт `'static` и `FontRef<'static>` хранится в структуре.
- **Variable Inter + фича `variable-fonts`.** Отклонено: нужен один вес, а фича тянет `gvar-alloc` и рантайм-интерполяцию.

## Последствия: изменения в портах/событиях/эффектах (точные сигнатуры Rust)

**`crates/switcher-platform/src/events.rs`** — переименование поля + уточнение контракта:

```rust
/// Premultiplied-alpha BGRA image: per pixel bytes are `[B, G, R, A]`
/// (little-endian `0xAARRGGBB`), row-major, top-down, stride == `width * 4`.
/// This is exactly the layout a 32bpp `BI_RGB` top-down DIB section expects,
/// so an adapter can `copy_from_slice` it without touching channels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadgeImage {
    pub width: u32,
    pub height: u32,
    pub bgra_premul: Vec<u8>,
}
```

**`crates/switcher-platform/src/ports.rs`** — `dpi_at(pos)` заменяется на `dpi_for(placement)`, чтобы shell мог получить DPI и для `ResolvedAnchor::Fixed` (у которого нет точки), не зная про мониторы:

```rust
pub trait OverlayWindow: Send {
    fn show(&self, image: &BadgeImage, placement: Placement);
    fn move_to(&self, pos: Point);
    fn hide(&self);
    /// Effective DPI of the monitor that will host `placement` (96 = 100%).
    /// For `Placement::PrimaryBottomRight` this is the primary monitor's DPI.
    fn dpi_for(&self, placement: Placement) -> u32;
}
```

**`crates/switcher-core/src/content.rs`** — только аддитивные derive (нужны, чтобы `BadgeContent` мог быть частью ключа `HashMap`):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BadgeStyle { Text, Color }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgb8 { pub r: u8, pub g: u8, pub b: u8 }

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BadgeContent { pub label: String, pub bg: Rgb8, pub fg: Rgb8, pub style: BadgeStyle }
```

**`crates/switcher-core/src/engine.rs`** — **без изменений**. `Effect::ShowBadge { content, anchor }` остаётся как есть. Порядок вызовов в runtime:

```rust
Effect::ShowBadge { content, anchor } => {
    let placement = match anchor {
        ResolvedAnchor::Caret(p) | ResolvedAnchor::Cursor(p) => Placement::At(p), // shell добавит offset
        ResolvedAnchor::Fixed => Placement::PrimaryBottomRight,
    };
    let dpi = overlay.dpi_for(placement);
    let img = cache.image(&content, dpi)?;
    let placement = offset_for(placement, img, dpi); // сдвиг вправо-вниз от якоря, в dip*scale
    overlay.show(img, placement);
    self.last_shown = Some((content, placement));
}
```

Клэмп по границам монитора (чтобы бейдж не выехал за экран) остаётся внутри `switcher-windows` — только адаптер знает `MonitorFromPoint` и work area; порт от этого не меняется.

## Ломающиеся тесты ядра (файл::тест -> что изменить)

**Рекомендованный набор изменений не ломает ни один из 42 тестов.** Проверено: `BadgeImage`/`rgba_premul`/`Placement` встречаются только в `crates/switcher-platform/src/{events.rs, ports.rs}` и нигде в `switcher-core`; добавление `Eq`/`Hash` аддитивно.

- `switcher-platform/src/events.rs::lang_tag_primary_extracts_lowercase_primary_subtag` -> не затрагивается (не касается `BadgeImage`).
- `switcher-core/src/content.rs::*` (6 тестов) -> не затрагиваются: `assert_eq!` на `Rgb8`/`String`/`BadgeStyle` работает и с добавленными derive.
- `switcher-core/src/engine.rs::*` (все) -> не затрагиваются: `Effect` не меняется.

Что бы сломалось при отклонённой альтернативе «добавить `dpi: u32` в `Effect::ShowBadge`» — для полноты, ровно 6 тестов + 1 хелпер в `crates/switcher-core/src/engine.rs`:

| Тест | Что пришлось бы менять |
|---|---|
| `engine.rs::auto_anchor_prefers_caret_and_does_not_track_pointer` | литерал `Effect::ShowBadge { content, anchor }` в `assert_eq!` — добавить `dpi` |
| `engine.rs::auto_anchor_falls_back_to_cursor_and_tracks` | то же |
| `engine.rs::auto_anchor_falls_back_to_fixed_when_nothing_available` | то же |
| `engine.rs::cursor_pref_ignores_caret` | литерал внутри `fx.contains(&...)` — добавить `dpi` |
| `engine.rs::fixed_pref_ignores_caret_and_cursor` | то же |
| `engine.rs::second_change_while_visible_requeries_anchor_and_rearms` | то же (два литерала `ShowBadge`) |
| `engine.rs::default_content` (хелпер, стр. 360) | пришлось бы вернуть пару `(BadgeContent, dpi)` или добавить второй хелпер |

Плюс концептуальный урон: ядру понадобился бы источник DPI (новое поле в `Config` или новый `Event`), то есть новые тесты на то, откуда ядро берёт физические пиксели. Именно поэтому вариант отклонён.

## Проверенные факты API (утверждение -> путь к файлу-источнику)

Все пути относительно `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`.

**tiny-skia 0.12.0 — формат пикселей**

- `Pixmap` — «A container that owns premultiplied RGBA pixels. The data is not aligned, therefore width == stride» -> `tiny-skia-0.12.0/src/pixmap.rs:26-33`
- `BYTES_PER_PIXEL: usize = 4` -> `tiny-skia-0.12.0/src/pixmap.rs:24`
- `Pixmap::data(&self) -> &[u8]`, doc «Byteorder: RGBA» -> `tiny-skia-0.12.0/src/pixmap.rs:227-232`
- `Pixmap::take(self) -> Vec<u8>`, doc «Byteorder: RGBA» -> `tiny-skia-0.12.0/src/pixmap.rs:259-264`
- `Pixmap::take_demultiplied(self)` существует и **демультиплицирует** (использовать нельзя) -> `tiny-skia-0.12.0/src/pixmap.rs:266-280`
- `PremultipliedColorU8([u8; 4])`, doc «Byteorder: RGBA (relevant for bytemuck casts)», `red() = self.0[0]`, `green() = self.0[1]`, `blue() = self.0[2]`, `alpha() = self.0[3]` -> `tiny-skia-0.12.0/src/color.rs:95-150`
- `Pixmap::new` инициализирует прозрачным чёрным (0,0,0,0) -> `tiny-skia-0.12.0/src/pixmap.rs:38, 50-53`
- stride без padding: `min_row_bytes = width * 4`, `data_len = (h-1)*row_bytes + w*4` -> `tiny-skia-0.12.0/src/pixmap.rs:593-612`

**tiny-skia 0.12.0 — рисование**

- `Pixmap::fill_path(&mut self, path: &Path, paint: &Paint, fill_rule: FillRule, transform: Transform, mask: Option<&Mask>)` -> `tiny-skia-0.12.0/src/painter.rs:125-135` (реализация `PixmapMut::fill_path` -> `:217`)
- `Pixmap::fill_rect(..., mask: Option<&Mask>)` -> `tiny-skia-0.12.0/src/painter.rs:112-120` (реализация `:185`)
- `Pixmap::apply_mask(&mut self, mask: &Mask)` -> `tiny-skia-0.12.0/src/painter.rs:171`
- `enum FillRule { Winding (#[default]), EvenOdd }` -> `tiny-skia-0.12.0/src/painter.rs:21-29`
- `Paint { shader, blend_mode, anti_alias, colorspace, force_hq_pipeline }`, `Default`: `SolidColor(Color::BLACK)`, `anti_alias = true`, `ColorSpace::Linear` (= без gamma-expansion, `expand_channel` возвращает x как есть — совпадает с тем, как блендит DWM) -> `tiny-skia-0.12.0/src/painter.rs:31-87`, `tiny-skia-0.12.0/src/color.rs:479-495`
- `Paint::set_color_rgba8(r, g, b, a)` -> `tiny-skia-0.12.0/src/painter.rs:98`
- `Color::from_rgba8(r, g, b, a) -> Color` -> `tiny-skia-0.12.0/src/color.rs:255-262`
- `Mask::new(w, h) -> Option<Mask>` (нули), `Mask::from_vec(data, size)` (len == w*h), `Mask::data_mut() -> &mut [u8]`, `Mask::take() -> Vec<u8>`, `Mask::clear()`, семантика «0 блокирует, 255 пропускает, промежуточное — AA» -> `tiny-skia-0.12.0/src/mask.rs:32-44, 48-54, 98-105, 131-133, 136-137, 375`
- `Mask::fill_path(&mut self, path, fill_rule, anti_alias, transform)` (альтернатива ручному покрытию) -> `tiny-skia-0.12.0/src/mask.rs:259-265`
- `MaskType::{Alpha, Luminance}`, `Mask::from_pixmap` -> `tiny-skia-0.12.0/src/mask.rs:21-30, 57-93`
- Публичный реэкспорт `Color, FillRule, Paint, Mask, MaskType, Pixmap, PixmapRef, Path, PathBuilder, Rect, Transform` -> `tiny-skia-0.12.0/src/lib.rs:59-70`
- `PathBuilder`: `move_to:131`, `line_to:157`, `quad_to:168`, `cubic_to:223`, `close:246`, `push_rect:288`, `push_oval:299`, `finish() -> Option<Path>:411`, `from_rect(rect) -> Path:71`, `from_circle:97`, `from_oval:106` -> `tiny-skia-path-0.12.0/src/path_builder.rs`
- **Готового rounded-rect в tiny-skia 0.12 НЕТ**: `grep -rn "round_rect" tiny-skia-path-0.12.0/src/` -> exit 1 (совпадений ноль) ⇒ скругление строится вручную через `cubic_to`/`quad_to`
- `Rect::from_xywh(f32,f32,f32,f32) -> Option<Rect>`, `Rect::from_ltrb` -> `tiny-skia-path-0.12.0/src/rect.rs:258, 235`
- Фичи: `default = ["std", "simd", "png-format"]`, `png-format = ["std", "dep:png"]`, `std = ["tiny-skia-path/std"]`, `simd = []` -> `tiny-skia-0.12.0/Cargo.toml:35-48`
- `compile_error!("You have to activate either the `std` or the `no-std-float` feature.")` ⇒ `std` обязателен -> `tiny-skia-0.12.0/src/lib.rs:29-30`
- `simd` включает SSE2/NEON/simd128 пути (на x86_64 sse2 — baseline, stable-компилятор достаточен) -> `tiny-skia-0.12.0/src/wide/f32x4_t.rs:16-32`
- Лицензия BSD-3-Clause -> `tiny-skia-0.12.0/Cargo.toml:32`

**ab_glyph 0.2.32**

- `FontRef::try_from_slice(data: &'font [u8]) -> Result<Self, InvalidFont>` -> `ab_glyph-0.2.32/src/ttfp.rs:58`; `FontRef` реализует `Debug` -> `:38`
- `FontVec::try_from_vec(Vec<u8>)` -> `ab_glyph-0.2.32/src/ttfp.rs:122`; `impl_font!(FontRef<'_>)`, `impl_font!(FontVec)` -> `:369-370`
- Трейт `Font`: `units_per_em:49`, `pt_to_px_scale:73-78`, `ascent_unscaled:83`, `descent_unscaled:88`, `height_unscaled:94`, `glyph_id(c) -> GlyphId:128`, `h_advance_unscaled:136`, `h_side_bearing_unscaled:144`, `kern_unscaled:163`, `outline(id) -> Option<Outline>:166`, `glyph_bounds:245`, `outline_glyph(glyph) -> Option<OutlinedGlyph>:259-266`, `as_scaled<S: Into<PxScale>>:283-291` -> `ab_glyph-0.2.32/src/font.rs`
- ASCII-диаграмма ascent/descent/h_advance/h_side_bearing -> `ab_glyph-0.2.32/src/font.rs:19-38`
- Трейт `ScaleFont`: `scale:81`, `h_scale_factor = scale.x / font.height_unscaled():88`, `scale_factor:99`, `ascent:108`, `descent:114`, `height = scale.y:122`, `glyph_id:134`, `scaled_glyph:155`, `h_advance:162`, `h_side_bearing:169`, `kern:187`, `outline_glyph:224` -> `ab_glyph-0.2.32/src/scale.rs`
- `PxScale { x, y }` + `impl From<f32> for PxScale` (uniform) -> `ab_glyph-0.2.32/src/scale.rs:20-47`; `PxScaleFont { font, scale }` -> `:248`
- `Glyph { id: GlyphId, scale: PxScale, position: Point }`, `position` = левая точка на baseline -> `ab_glyph-0.2.32/src/glyph.rs:53-66`
- `GlyphId(pub u16)`, `with_scale_and_position(scale, position) -> Glyph`, `with_scale` -> `ab_glyph-0.2.32/src/glyph.rs:15, 27-50`
- `OutlinedGlyph::px_bounds() -> Rect` — «Conservative whole number pixel bounding box... exactly large enough to `draw` into», и явное предупреждение «Pixel bounds coordinates should not be used for layout logic» -> `ab_glyph-0.2.32/src/outlined.rs:81-98`
- `OutlinedGlyph::draw<O: FnMut(u32, u32, f32)>(&self, o: O)` — coverage 0.0..=1.0, координаты относительно `px_bounds().min` (внутри: `offset = position - px_bounds.min`) -> `ab_glyph-0.2.32/src/outlined.rs:100-152`
- `Rect { min: Point, max: Point }` + `width()`, `height()` -> `ab_glyph-0.2.32/src/outlined.rs:174-191`
- `point()`/`Point` реэкспортируются из `ab_glyph_rasterizer` -> `ab_glyph-0.2.32/src/lib.rs:52`
- Фичи: `default = ["std", "variable-fonts", "gvar-alloc"]`, `std = ["owned_ttf_parser/default", "ab_glyph_rasterizer/default"]` -> `ab_glyph-0.2.32/Cargo.toml:35-51`
- Лицензия Apache-2.0 -> `ab_glyph-0.2.32/Cargo.toml:32`

**windows 0.62.2 (сигнатуры) + Microsoft Learn (семантика)**

- `UpdateLayeredWindow(hwnd, hdcdst: Option<HDC>, pptdst: Option<*const POINT>, psize: Option<*const SIZE>, hdcsrc: Option<HDC>, pptsrc: Option<*const POINT>, crkey: COLORREF, pblend: Option<*const BLENDFUNCTION>, dwflags: UPDATE_LAYERED_WINDOW_FLAGS) -> windows_core::Result<()>` -> `windows-0.62.2/src/Windows/Win32/UI/WindowsAndMessaging/mod.rs:2439`
- `ULW_ALPHA = 2`, `ULW_COLORKEY = 1`, `ULW_OPAQUE = 4`; `UPDATELAYEREDWINDOWINFO`; `USER_DEFAULT_SCREEN_DPI = 96` -> `windows-0.62.2/src/Windows/Win32/UI/WindowsAndMessaging/mod.rs:6646-6649, 6655-6666, 6676`
- `CreateDIBSection(hdc: Option<HDC>, pbmi: *const BITMAPINFO, usage: DIB_USAGE, ppvbits: *mut *mut c_void, hsection: Option<HANDLE>, offset: u32) -> windows_core::Result<HBITMAP>` -> `windows-0.62.2/src/Windows/Win32/Graphics/Gdi/mod.rs:242`
- `BITMAPINFO { bmiHeader, bmiColors: [RGBQUAD; 1] }` (+ `Default` через `zeroed`) -> `windows-0.62.2/src/Windows/Win32/Graphics/Gdi/mod.rs:2319-2327`
- `BITMAPINFOHEADER` — поле `biCompression: u32` (не newtype!) ⇒ писать `BI_RGB.0` -> `windows-0.62.2/src/Windows/Win32/Graphics/Gdi/mod.rs:2330-2342`
- `BI_RGB: BI_COMPRESSION = BI_COMPRESSION(0)`, `DIB_RGB_COLORS: DIB_USAGE = DIB_USAGE(0)` -> `windows-0.62.2/src/Windows/Win32/Graphics/Gdi/mod.rs:2402, 3007`
- `BLENDFUNCTION { BlendOp: u8, BlendFlags: u8, SourceConstantAlpha: u8, AlphaFormat: u8 }`; `AC_SRC_ALPHA: u32 = 1`, `AC_SRC_OVER: u32 = 0` -> `windows-0.62.2/src/Windows/Win32/Graphics/Gdi/mod.rs:2412-2417, 2174-2175`
- `CreateCompatibleDC:207`, `DeleteDC:448`, `DeleteObject:463`, `GdiFlush:759`, `MonitorFromPoint:1474`, `SelectObject:1756` -> `windows-0.62.2/src/Windows/Win32/Graphics/Gdi/mod.rs`
- `GetDpiForMonitor(hmonitor, dpitype, dpix, dpiy) -> Result<()>`, `GetDpiForWindow(hwnd) -> u32` -> `windows-0.62.2/src/Windows/Win32/UI/HiDpi/mod.rs:39, 49`
- Имена фич: `Win32_Foundation:416`, `Win32_Graphics_Gdi:440`, `Win32_UI_HiDpi:688`, `Win32_UI_WindowsAndMessaging:707` -> `windows-0.62.2/Cargo.toml`
- **32bpp BI_RGB layout (ключевая цитата):** «If the **biCompression** member of the **BITMAPINFOHEADER** is BI_RGB, the **bmiColors** member of BITMAPINFO is NULL. Each **DWORD** in the bitmap array represents the relative intensities of blue, green, and red for a pixel. The value for blue is in the least significant 8 bits, followed by 8 bits each for green and red. The high byte in each **DWORD** is not used.» -> https://learn.microsoft.com/en-us/previous-versions/dd183376(v=vs.85) (таблица `biBitCount`, строка 32). На little-endian младший байт — байт 0 ⇒ порядок в памяти **B, G, R, X**.
- Тот же порядок подтверждает `RGBQUAD { BYTE rgbBlue; BYTE rgbGreen; BYTE rgbRed; BYTE rgbReserved; }` -> https://learn.microsoft.com/en-us/windows/win32/api/wingdi/ns-wingdi-rgbquad
- **Premultiplied обязателен:** «AC_SRC_ALPHA — This flag is set when the bitmap has an Alpha channel (that is, per-pixel alpha). Note that the APIs use premultiplied alpha, which means that the red, green and blue channel values in the bitmap must be premultiplied with the alpha channel value.» и «When the **AlphaFormat** member is AC_SRC_ALPHA, the source bitmap must be 32 bpp.» -> https://learn.microsoft.com/en-us/windows/win32/api/wingdi/ns-wingdi-blendfunction
- `ULW_ALPHA` = «Use *pblend* as the blend function»; «The source DC should contain the surface that defines the visible contents of the layered window... you can select a bitmap into a device context obtained by calling CreateCompatibleDC»; «UpdateLayeredWindow always updates the entire window» -> https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-updatelayeredwindow
- **Top-down DIB:** «If **biHeight** is negative, the bitmap is a top-down DIB and its origin is the upper-left corner. If **biHeight** is negative, indicating a top-down DIB, **biCompression** must be either BI_RGB or BI_BITFIELDS.» -> https://learn.microsoft.com/en-us/previous-versions/dd183376(v=vs.85)
- **stride 32bpp без padding:** «For uncompressed RGB formats, the minimum stride is always the image width in bytes, rounded up to the nearest DWORD... `stride = ((((biWidth * biBitCount) + 31) & ~31) >> 3)`» ⇒ при `biBitCount = 32` это ровно `biWidth * 4` -> https://learn.microsoft.com/en-us/windows/win32/api/wingdi/ns-wingdi-bitmapinfoheader (раздел «Calculating Surface Stride»)
- **Синхронизация прямой записи в биты DIB:** «You need to guarantee that the GDI subsystem has completed any drawing to a bitmap created by CreateDIBSection before you draw to the bitmap yourself... Do this by calling the GdiFlush function.» -> https://learn.microsoft.com/en-us/windows/win32/api/wingdi/nf-wingdi-createdibsection (мы в DIB только пишем CPU и ничего не рисуем через GDI, но `GdiFlush()` перед `UpdateLayeredWindow` — дешёвая страховка; функция есть: `Gdi/mod.rs:759`)

**Локальный репозиторий**

- `BadgeImage`/`rgba_premul`/`Placement` не упоминаются в `switcher-core` — только `crates/switcher-platform/src/events.rs:47,50,55` и `crates/switcher-platform/src/ports.rs:5,31` ⇒ переименование поля не может сломать тесты ядра.
- Текущие зависимости (`tiny-skia = "0.12"`, `ab_glyph = "0.2"` с default-фичами; `switcher-windows` только с `Win32_Foundation` + `Win32_UI_WindowsAndMessaging`; в `switcher-app` нет `thiserror`) -> `D:\disk.w\Projects\evk-soft\lang-switcher\.claude\worktrees\m1-plan-adrs\Cargo.toml`, `crates\switcher-app\Cargo.toml`, `crates\switcher-windows\Cargo.toml`
- Лейбл всегда ASCII-uppercase, fallback `"??"` -> `crates\switcher-core\src\content.rs:69-79`
- `Effect::ShowBadge { content, anchor }` и 6 тестов с литералами `ShowBadge` -> `crates\switcher-core\src\engine.rs:46-49, 364-443, 519-533`

## UNVERIFIED / открытые вопросы

- **Метрики бейджа (26/40/9/7/16 dip) — не проверенные факты, а дизайн-предложение.** Их надо один раз подтвердить визуально на 100 % / 150 % / 200 % и после этого зафиксировать как константы (визуальный smoke-чеклист по скилу `platform-api-work`, автоматике недоступно).
- **Конкретный шрифт.** Что Inter распространяется под SIL OFL 1.1 — я знаю по памяти, а не из вендоренных исходников; **перед коммитом артефакта нужно скачать актуальный релиз и прочитать вложенный `OFL.txt`**. Альтернативы того же класса: Noto Sans, Fira Sans (обе OFL 1.1), Roboto (Apache-2.0). Решение стоит зафиксировать ADR-ом (`adr` скил), потому что это новая вшитая зависимость с лицензионными обязательствами.
- **Побайтовая детерминированность `render_badge` между сборками** (разные `target-feature`, `simd` on/off, разные версии `ab_glyph_rasterizer`) не проверена. Поэтому unit-тесты должны утверждать **инварианты** (размер, alpha угла == 0, центральный пиксель == `[bg.b, bg.g, bg.r, 255]`, `b<=a && g<=a && r<=a` для всех пикселей, наличие fg-пикселей), а **не** golden-байты.
- **`tiny_skia::Pixmap::fill_path` тихо выходит** (`log::warn!` + `return`) если bounds пути «nearly zero» по ширине или высоте (`tiny-skia-0.12.0/src/painter.rs:229-233`). При каких именно минимальных физических размерах это срабатывает — не проверял; нужен тест на нижней границе (dpi = 96, `BadgeStyle::Color`).
- **Округление размеров при нецелых DPI** (кастомное масштабирование Windows, напр. dpi = 110). Не проверено, что `round()` не даёт «дрожащую» на 1 px высоту при переходе между мониторами; возможно, потребуется квантование dpi в ключе до ближайших 25 % — тогда ключ станет `dpi_bucket: u32`. Решать после первого прогона на mixed-DPI.
- **Реальное множество возвращаемых `GetDpiForMonitor` значений** (произвольное или всегда кратное 24?) — по документации не установлено; поэтому `MAX_ENTRIES` держит потолок вместо предположения о «5 стандартных масштабах».
- **Нужен ли `GdiFlush()`** при сценарии «только CPU-запись в биты + `UpdateLayeredWindow`» — документация требует его для смешивания GDI-рисования и прямого доступа; для нашего чистого случая обязательность не подтверждена. Ставить, пока не доказано обратное.
- **`FontRef: Send`/`Sync`** не проверял — в предложенном дизайне не требуется (`BadgeCache` живёт только на потоке главного цикла). Если в M2 рендер понадобится на другом потоке, это придётся проверить отдельно.
- **`OverlayScaleChanged` пока никем не порождается** (`switcher-windows` — пустой stub). Кто и когда его пошлёт (`WM_DPICHANGED` у layered-окна без активации приходит не всегда; вероятно, придётся сравнивать `dpi_for` при каждом `MoveBadge`) — открытый вопрос реализации адаптера, к рендеру и кэшу отношения не имеет, но влияет на то, как часто кэш пополняется в follow-режиме на mixed-DPI.