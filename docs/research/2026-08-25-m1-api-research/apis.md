# API прикладных крейтов: tray-icon, muda, rodio, directories, tracing-appender, манифест

> Отчёт исследования от 2026-08-25. Каждое утверждение об API сверено по вендоренным исходникам
> в `~/.cargo/registry/src/index.crates.io-*/<крейт>-<версия>/` (context7 в той сессии был недоступен)
> и по learn.microsoft.com для семантики Win32; источник указан рядом с фактом.
> На основе этих отчётов приняты ADR-0005…ADR-0010. **При расхождении отчёта и ADR побеждает ADR** —
> отчёт фиксирует, что было известно на момент решения, и не переписывается вслед за ним.

## Ответ на вопрос

Проверено по исходникам в реестре. Главные расхождения с «памятью о старых версиях»:

| Пункт | Догадка «по памяти» | Реальность в этой версии |
|---|---|---|
| rodio | `OutputStream` + `OutputStreamBuilder::open_default_stream()` + `Sink` | **Таких имён нет вообще.** `DeviceSinkBuilder::open_default_sink() -> MixerDeviceSink`, `Player::connect_new(&Mixer)`; `SampleRate = NonZero<u32>`, `ChannelCount = NonZero<u16>` |
| tray-icon | «просто держи `TrayIcon` живым» | `TrayIcon` = `Rc<RefCell<…>>` → **`!Send`**; требует Win32-цикла сообщений на своём потоке; `set_icon/set_tooltip/set_menu` шлют `SendMessageW` в собственный HWND |
| tray-icon `Icon::from_rgba` | premultiplied RGBA (как для оверлея) | **straight (non-premultiplied) RGBA** — иначе бейдж в трее будет тёмным по краям |
| muda на Windows | нужен `Menu::init_for_hwnd` | Для трея **не нужен**: `init_for_hwnd` — это меню-бар окна. tray-icon сам вызывает `attach_menu_subclass_for_hwnd` |
| `set_event_handler` | можно вызвать когда угодно, несколько раз | `OnceCell` — **работает только один раз и только до первого события**; иначе молча игнорируется |
| directories на Windows | `qualifier` участвует в пути | `qualifier` **игнорируется**; путь = `organization\application`, плюс подкаталог `config`/`data` |
| embed-manifest | «уже подключён» | **Не в `Cargo.lock`, не в реестре, `build.rs` в репо отсутствует** — заявление подтверждено |

Ключевой архитектурный вывод: **ни один из этих крейтов не требует изменений в `switcher-platform` или в эффектах `switcher-core`**. Ломающихся тестов ядра — ноль. Всё, что нужно, — новый выделенный tray-поток в `switcher-app` и правки `Cargo.toml`.

---

## Рекомендованное решение (точно и проверяемо)

### 1. tray-icon 0.24.1

**Требование по потоку (документировано самим крейтом):** `src/lib.rs:17` — «On Windows and Linux, an event loop must be running on the thread, on Windows, a win32 event loop. It doesn't need to be the main thread but you have to create the tray icon on the same thread as the event loop.»

Подтверждение на уровне типов: `pub struct TrayIcon { id: TrayIconId, tray: Rc<RefCell<platform_impl::TrayIcon>> }` (`src/lib.rs:343-347`) → `!Send`, `!Sync`. `Drop` вызывает `remove_tray_icon` + `DestroyWindow` (`src/platform_impl/windows/mod.rs:297-309`) — обязан выполниться на потоке-создателе. `set_icon`, `set_menu`, `set_tooltip`, `set_visible`, `set_show_menu_on_*` внутри делают **`SendMessageW`** в свой же HWND (`mod.rs:172, 194, 226, 239, 249, 273`) — вызов с непрокачивающего потока заблокируется до того, как tray-поток дойдёт до `GetMessageW`.

⇒ **Трей живёт в собственном потоке с собственным `GetMessageW`-циклом** — ровно по правилу проекта «каждый OS-хук владеет своим потоком с циклом сообщений».

Сигнатуры (все — `src/lib.rs`):

```rust
// TrayIconBuilder
pub fn new() -> Self;                                          // :234
pub fn with_id<I: Into<TrayIconId>>(self, id: I) -> Self;       // :242
pub fn with_menu(self, menu: Box<dyn menu::ContextMenu>) -> Self; // :252
pub fn with_icon(self, icon: Icon) -> Self;                     // :263
pub fn with_tooltip<S: AsRef<str>>(self, s: S) -> Self;         // :273
pub fn with_menu_on_left_click(self, enable: bool) -> Self;     // :313
pub fn with_menu_on_right_click(self, enable: bool) -> Self;    // :323
pub fn build(self) -> tray_icon::Result<TrayIcon>;              // :335

// TrayIcon (обновления «потом»)
pub fn set_icon(&self, icon: Option<Icon>) -> Result<()>;       // :387
pub fn set_tooltip<S: AsRef<str>>(&self, tooltip: Option<S>) -> Result<()>; // :405
pub fn set_menu(&self, menu: Option<Box<dyn menu::ContextMenu>>); // :396
pub fn set_visible(&self, visible: bool) -> Result<()>;         // :424
pub fn show_menu(&self);                                        // :505
pub fn rect(&self) -> Option<Rect>;                             // :515
#[cfg(windows)] pub fn window_handle(&self) -> windows_sys::Win32::Foundation::HWND; // :523

// Icon
pub fn from_rgba(rgba: Vec<u8>, width: u32, height: u32) -> Result<Self, BadIcon>; // src/icon.rs:135
```

**Формат пикселей `Icon::from_rgba` — straight alpha, НЕ premultiplied.** `src/platform_impl/windows/icon.rs:26-54`: строится AND-маска из `pixel.a.wrapping_sub(u8::MAX)`, каждый пиксель переворачивается в BGRA (`convert_to_bgra`, там же :21-24), затем `CreateIcon(null, w, h, 1, 32, and_mask, rgba)`. То есть в приложении будут **два разных растеризатора**: premultiplied ARGB для `UpdateLayeredWindow` (`BadgeImage.rgba_premul`) и straight RGBA 16×16 (или `GetSystemMetrics(SM_CXSMICON)`) для трея.

**Доставка событий.** Два глобальных статика, не привязанных к потоку (`src/lib.rs:650-699`):

```rust
pub type TrayIconEventReceiver = crossbeam_channel::Receiver<TrayIconEvent>; // :650
static TRAY_CHANNEL: Lazy<(Sender<TrayIconEvent>, TrayIconEventReceiver)> = Lazy::new(unbounded); // :653
static TRAY_EVENT_HANDLER: OnceCell<Option<TrayIconEventHandler>> = OnceCell::new();               // :654

pub fn receiver<'a>() -> &'a TrayIconEventReceiver;                                    // :674
pub fn set_event_handler<F: Fn(TrayIconEvent) + Send + Sync + 'static>(f: Option<F>);  // :684
```

**Ловушка, которую надо зафиксировать в плане:** `send` делает `TRAY_EVENT_HANDLER.get_or_init(|| None)` (`:694`), а `set_event_handler` — `let _ = OnceCell::set(...)` (`:686`). Значит: если хоть одно событие уйдёт до установки хендлера, `OnceCell` навсегда зафиксируется в `None` и хендлер будет молча проигнорирован. То же в muda (`muda/src/lib.rs:515-520`). ⇒ **оба `set_event_handler` вызываются самым первым делом в `main`, до создания трея.**

Хорошая новость: `crossbeam-channel` в `Cargo.lock` один — `0.5.15` (`Cargo.lock:278-280`), тот же, что в workspace. Так что альтернатива без хендлеров — `crossbeam_channel::Select` прямо по `TrayIconEvent::receiver()` / `MenuEvent::receiver()` — тоже валидна.

Минимальный скетч (tray-поток):

```rust
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIconBuilder, TrayIconEvent};

// ---- в main(), ДО спавна tray-потока и до создания трея ----
let tx_m = tx.clone();
MenuEvent::set_event_handler(Some(move |e: MenuEvent| { let _ = tx_m.send(AppEvent::Menu(e.id)); }));
let tx_t = tx.clone();
TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| { let _ = tx_t.send(AppEvent::Tray(e)); }));

// ---- на tray-потоке ----
let follow = CheckMenuItem::with_id("mode.follow", "Следовать за курсором", true, false, None);
let sound  = CheckMenuItem::with_id("sound", "Звук", true, true, None);
let quit   = MenuItem::with_id("quit", "Выход", true, None);
let menu = Menu::with_items(&[&follow, &sound, &PredefinedMenuItem::separator(), &quit])?;
let tray = TrayIconBuilder::new()
    .with_menu(Box::new(menu))                       // Box<dyn ContextMenu>
    .with_tooltip("lang-switcher — EN")
    .with_icon(Icon::from_rgba(straight_rgba, 16, 16)?)  // straight alpha!
    .build()?;
// … затем GetMessageW/TranslateMessage/DispatchMessageW в этом же потоке;
// команды из ядра приходят каналом + PostThreadMessageW для побудки пампа.
tray.set_icon(Some(Icon::from_rgba(ru_rgba, 16, 16)?))?;  // только с этого потока
follow.set_checked(true);
```

`Icon` **можно** собирать на любом потоке и отправлять на tray-поток: `unsafe impl Send for WinIcon {}` (`src/platform_impl/windows/icon.rs:67`); `Sync` не реализован.

Windows-специфика, зафиксированная в самом крейте:
- скрытое окно создаётся с `WS_EX_NOACTIVATE|WS_EX_TRANSPARENT|WS_EX_LAYERED|WS_EX_TOOLWINDOW` (`mod.rs:107-115`) — само по себе безопасно и в taskbar не всплывает;
- перерегистрация иконки при перезапуске explorer уже сделана за нас: `RegisterWindowMessageA("TaskbarCreated")` + `ChangeWindowMessageFilterEx(..., MSGFLT_ALLOW, ...)` (`mod.rs:52-53, 133, 367-377`) — то есть UIPI для elevated-процесса учтён;
- tooltip обрезается до **128 UTF-16 единиц** (`mod.rs:216`, `register_tray_icon` :586);
- `set_title` на Windows — no-op (`mod.rs:269`);
- иконка **не масштабируется** под DPI — размер берётся ровно тот, что передан в `CreateIcon`.

Фичи: на Windows `platform_impl/mod.rs` подключает `windows/mod.rs` без каких-либо feature-гейтов ⇒ **`default-features = false` безопасно и рекомендуется** (default = `["libxdo","gtk"]`, они целятся только в Linux).

### 2. muda 0.19.3

```rust
// src/menu.rs
pub fn Menu::new() -> Self;                                            // :25
pub fn Menu::with_id<I: Into<MenuId>>(id: I) -> Self;                  // :34
pub fn Menu::with_items(items: &[&dyn IsMenuItem]) -> Result<Self>;    // :43
pub fn Menu::append(&self, item: &dyn IsMenuItem) -> Result<()>;       // :71
pub fn Menu::append_items(&self, items: &[&dyn IsMenuItem]) -> Result<()>; // :82
pub unsafe fn Menu::init_for_hwnd(&self, hwnd: isize) -> Result<()>;   // :235  <- меню-БАР окна, трею НЕ нужно

// src/items/normal.rs
pub fn MenuItem::new<S: AsRef<str>>(text: S, enabled: bool, accelerator: Option<Accelerator>) -> Self; // :39
pub fn MenuItem::with_id<I: Into<MenuId>, S: AsRef<str>>(id: I, text: S, enabled: bool,
                                                        accelerator: Option<Accelerator>) -> Self;    // :56
pub fn MenuItem::set_enabled(&self, enabled: bool);   // :97
pub fn MenuItem::id(&self) -> &MenuId;                // :75

// src/items/check.rs
pub fn CheckMenuItem::new<S: AsRef<str>>(text: S, enabled: bool, checked: bool,
                                         accelerator: Option<Accelerator>) -> Self;                  // :45
pub fn CheckMenuItem::with_id<I: Into<MenuId>, S: AsRef<str>>(id: I, text: S, enabled: bool,
                                         checked: bool, accelerator: Option<Accelerator>) -> Self;   // :68
pub fn CheckMenuItem::is_checked(&self) -> bool;      // :132
pub fn CheckMenuItem::set_checked(&self, checked: bool); // :137  (&self, не &mut self)
pub fn CheckMenuItem::set_enabled(&self, enabled: bool); // :111

// src/items/submenu.rs
pub fn Submenu::new<S: AsRef<str>>(text: S, enabled: bool) -> Self;    // :41
pub fn Submenu::with_items<S: AsRef<str>>(text: S, enabled: bool, items: &[&dyn IsMenuItem]) -> Result<Self>; // :67

// src/menu_id.rs
pub struct MenuId(pub String);                      // :6
pub fn MenuId::new<S: AsRef<str>>(id: S) -> Self;   // :10
impl<T: ToString> From<T> for MenuId                // menu_id.rs (через ToString)

// src/lib.rs
pub struct MenuEvent { pub id: MenuId }                       // :481
pub fn MenuEvent::receiver<'a>() -> &'a MenuEventReceiver;    // :505  (= crossbeam_channel::Receiver<MenuEvent>, :487)
pub fn MenuEvent::set_event_handler<F: Fn(MenuEvent) + Send + Sync + 'static>(f: Option<F>); // :515
```

**`init_for_hwnd` для трея НЕ нужен.** `TrayIcon::new` сам делает `menu.attach_menu_subclass_for_hwnd(hwnd)` (`tray-icon/src/platform_impl/windows/mod.rs:142-144`), а `set_menu` — detach/attach (`:183-190`). `WM_COMMAND` обрабатывается в `menu_subclass_proc` (`muda/src/platform_impl/windows/mod.rs:1101, 1124`) и в конце вызывает `MenuEvent::send` (`:1239`). `init_for_hwnd` (`muda/src/menu.rs:235`) вызывает `SetMenu` + `DrawMenuBar` — это меню-бар окна, к трею отношения не имеет.

**Насос сообщений на том же потоке — обязателен.** `TrackPopupMenu` вызывается из `tray_proc` (`tray-icon/.../windows/mod.rs:547-563`) и является модальным; `WM_COMMAND` доставляется в тот же HWND ⇒ обрабатывается **потоком, который качает сообщения этого HWND**, т.е. tray-потоком.

**Все типы muda — `!Send`:** `Menu { id: Rc<MenuId>, inner: Rc<RefCell<platform_impl::Menu>> }` (`src/menu.rs:11-15`), `CheckMenuItem` аналогично (`src/items/check.rs:19-23`). ⇒ меню и его пункты строятся и мутируются **только на tray-потоке**. `set_checked` внутри — прямой `CheckMenuItem(hmenu, id, MF_CHECKED|MF_UNCHECKED)` (`platform_impl/windows/mod.rs:777-790`), без `SendMessageW`.

Акселераторы: `muda/src/lib.rs:20-22` — на Windows не работают без `TranslateAcceleratorW` в цикле сообщений. Для трей-меню акселераторы не нужны ⇒ передаём `None`.

Фичи: `common-controls-v6` меняет только About-диалог (`TaskDialogIndirect` вместо `MessageBoxW`, `platform_impl/windows/mod.rs:1337-1360`) — не нужна. **Отдельная зависимость `muda` в `switcher-app` избыточна**: tray-icon реэкспортирует всё через `pub mod menu { pub use muda::*; }` (`tray-icon/src/lib.rs:144-146`), а в lock ровно один `muda 0.19.3` (`Cargo.lock:918-921`).

### 3. rodio 0.22.2

```rust
// src/stream.rs — точка входа
pub fn DeviceSinkBuilder::open_default_sink() -> Result<MixerDeviceSink, DeviceSinkError>; // :233
pub fn DeviceSinkBuilder::from_device(device: cpal::Device) -> Result<DeviceSinkBuilder, DeviceSinkError>; // :198
pub fn DeviceSinkBuilder::open_stream(self) -> Result<MixerDeviceSink, DeviceSinkError>;   // :380
pub fn MixerDeviceSink::mixer(&self) -> &Mixer;      // :67
pub fn MixerDeviceSink::log_on_drop(&mut self, enabled: bool); // :78

// src/mixer.rs
pub struct Mixer(Arc<Inner>);  #[derive(Clone)]      // :44-46
pub fn Mixer::add<T: Source + Send + 'static>(&self, source: T);  // :57

// src/player.rs
pub fn Player::connect_new(mixer: &Mixer) -> Player;              // :73
pub fn Player::append<S: Source + Send + 'static>(&self, source: S) where f32: FromSample<S::Item>; // :104
pub fn Player::volume(&self) -> Float;  pub fn Player::set_volume(&self, value: Float);  // :169, :178

// src/common.rs — типы сэмплов
pub type SampleRate = NonZero<u32>;    // :5
pub type ChannelCount = NonZero<u16>;  // :8
pub type Float = f32;                  // :31   (f64 только под фичей "64bit", :35-36)
pub type Sample = Float;               // :43   => Sample == f32

// src/source/mod.rs — трейт
pub trait Source: Iterator<Item = Sample> {
    fn current_span_len(&self) -> Option<usize>;   // :183
    fn channels(&self) -> ChannelCount;            // :193
    fn sample_rate(&self) -> SampleRate;           // :196
    fn total_duration(&self) -> Option<Duration>;  // :201
}
fn take_duration(self, duration: Duration) -> TakeDuration<Self>; // :260
fn amplify(self, value: Float) -> Amplify<Self>;                  // :292
fn fade_in / fade_out(self, duration: Duration) -> …;             // :441, :450

// src/source/sine.rs
pub fn SineWave::new(freq: f32) -> SineWave;   // 48 kHz, 1 канал, бесконечный
```

Трейты в scope: **`rodio::Source`** (даёт `take_duration`/`amplify`/`fade_*`). `rodio::nz!` (`src/math.rs:156-163`, `#[macro_export]`) — для литералов `NonZero` при собственной реализации `Source`.

Жизненный цикл: `MixerDeviceSink` владеет `cpal::Stream`, а `MixerSource` перемещён внутрь callback'а cpal (`src/stream.rs:498, 517-534`) ⇒ **пока жив `MixerDeviceSink`, живёт микшер**. Дропнули — звук умер. `Mixer` — `Clone`, поэтому его можно клонировать и держать в `SoundPlayer`. `Drop` у `MixerDeviceSink` печатает предупреждение в stderr (`src/stream.rs:83-90`) — гасится `log_on_drop(false)`.

Громкость: **не нужен `Player`** для нашего кейса. `Effect::PlaySound { cue, volume }` мапится в `.amplify(volume)` — `volume: f32` совпадает с `rodio::Float`. `Player::set_volume` пригодится, только если понадобится глобальный master-volume.

```rust
use rodio::source::{SineWave, Source};              // Source обязателен в scope
use rodio::{DeviceSinkBuilder, MixerDeviceSink, Mixer};
use std::time::Duration;

// один раз при старте; держать в поле структуры, иначе звук выключится
let mut sink: MixerDeviceSink = DeviceSinkBuilder::open_default_sink()?;
sink.log_on_drop(false);
let mixer: Mixer = sink.mixer().clone();            // Clone + Send + Sync

// на каждый cue — fire-and-forget, никакого Player
let freq = match cue { SoundCue::Ru => 660.0, SoundCue::En => 880.0, SoundCue::Neutral => 520.0 };
let tone = SineWave::new(freq)
    .take_duration(Duration::from_millis(90))
    .fade_out(Duration::from_millis(40))            // убрать щелчок на обрыве
    .amplify(volume);                               // volume: f32
mixer.add(tone);
```

**Фичи — важно для футпринта.** Сейчас `rodio = "0.22"` тянет default: `playback, recording, flac, mp3, mp4, vorbis, wav, dither` ⇒ в `Cargo.lock` уже сидят symphonia (43 упоминания), `rand`, `rand_distr`, `rtrb` (`Cargo.lock:1376-1389`). Для синтезированного тона нужен **только** `playback = ["dep:cpal"]`. Что `default-features = false, features = ["playback"]` компилируется — подтверждается устройством `decoder::DecoderImpl`: вариант `None(Unreachable, PhantomData<R>)` с комментарием «This variant is here just to satisfy the compiler when there are no decoders enabled» (`src/decoder/mod.rs:120-125`).

**COM-ловушка cpal (важно для правила «COM на STA-потоке»).** `cpal-0.17.3/src/host/wasapi/com.rs` в `thread_local!` вызывает `CoInitializeEx(None, COINIT_APARTMENTTHREADED)` — **STA**, и терпит `RPC_E_CHANGED_MODE`. То есть поток, с которого вызван `open_default_sink()`, будет проинициализирован как STA. Планировать так: звук поднимается на своём потоке (или на main), а TSF получает **свой** STA-поток; MTA-потоков в процессе не заводить. `cpal 0.17.3` использует `windows ">=0.59, <=0.62"` и в lock унифицируется в единственный `windows 0.62.2` (`Cargo.lock:2195-2197`) — дублей нет.

### 4. directories 6.0.0

```rust
pub fn ProjectDirs::from(qualifier: &str, organization: &str, application: &str) -> Option<ProjectDirs>; // src/lib.rs:428
pub fn config_dir(&self) -> &Path;        // :453
pub fn config_local_dir(&self) -> &Path;  // :463
pub fn data_dir(&self) -> &Path;          // :473
pub fn data_local_dir(&self) -> &Path;    // :483
pub fn cache_dir(&self) -> &Path;         // :443
pub fn preference_dir(&self) -> &Path;    // :493  (== config_dir на Windows)
```

Реализация Windows — `src/win.rs:98-99`:

```rust
pub fn project_dirs_from(_qualifier: &str, organization: &str, application: &str) -> Option<ProjectDirs> {
    ProjectDirs::from_path(PathBuf::from_iter(&[organization, application]))
}
```

`qualifier` **игнорируется**. Далее `src/win.rs:67-95`: `project_path = organization\application`, и

| Геттер | Путь для `ProjectDirs::from("", "evk-soft", "lang-switcher")` |
|---|---|
| `config_dir()` | `%APPDATA%\evk-soft\lang-switcher\config` |
| `data_dir()` | `%APPDATA%\evk-soft\lang-switcher\data` |
| `config_local_dir()` | `%LOCALAPPDATA%\evk-soft\lang-switcher\config` |
| `data_local_dir()` | `%LOCALAPPDATA%\evk-soft\lang-switcher\data` |
| `cache_dir()` | `%LOCALAPPDATA%\evk-soft\lang-switcher\cache` |

Обратите внимание на суффикс `config`/`data` — его добавляет сам крейт (`app_data_roaming.join("config")`, `win.rs:75`).

**Каталоги не создаются.** `grep -rn "create_dir\|fs::" src/` по всему крейту — пустой вывод. ⇒ `std::fs::create_dir_all(project.config_dir())` перед записью конфига обязателен вручную. Для логов — не обязателен (см. п. 5).

Рекомендация: конфиг — `config_dir()` (roaming, переезжает с профилем), логи — `data_local_dir()` (локально, не гоняем логи по сети).

```rust
let dirs = directories::ProjectDirs::from("", "evk-soft", "lang-switcher")
    .ok_or_else(|| anyhow!("cannot resolve %APPDATA%"))?;
let cfg_path = dirs.config_dir().join("config.toml");
std::fs::create_dir_all(dirs.config_dir())?;      // крейт этого НЕ делает
let log_dir = dirs.data_local_dir().join("logs");
```

### 5. tracing-appender 0.2.5 (+ tracing-subscriber 0.3.23)

```rust
// tracing-appender
pub fn tracing_appender::non_blocking<T: Write + Send + 'static>(writer: T) -> (NonBlocking, WorkerGuard); // src/lib.rs:195
pub fn NonBlockingBuilder::lossy(self, is_lossy: bool) -> Self;             // src/non_blocking.rs:204
pub fn NonBlockingBuilder::buffered_lines_limit(self, n: usize) -> Self;    // :193
pub fn NonBlockingBuilder::finish<T: Write + Send + 'static>(self, writer: T) -> (NonBlocking, WorkerGuard); // :218
#[must_use] pub struct WorkerGuard;                                        // :101-107

pub fn RollingFileAppender::new(rotation, directory, filename_prefix) -> RollingFileAppender; // src/rolling.rs:143  ← ПАНИКУЕТ при ошибке (:156 `.expect`)
pub fn RollingFileAppender::builder() -> Builder;                                            // :185
pub fn Builder::rotation(self, rotation: Rotation) -> Self;                 // src/rolling/builder.rs:84
pub fn Builder::filename_prefix(self, prefix: impl Into<String>) -> Self;   // :128
pub fn Builder::filename_suffix(self, suffix: impl Into<String>) -> Self;   // :180
pub fn Builder::max_log_files(self, n: usize) -> Self;                      // :235
pub fn Builder::build(&self, directory: impl AsRef<Path>) -> Result<RollingFileAppender, InitError>; // :298
pub const Rotation::{MINUTELY, HOURLY, DAILY, WEEKLY, NEVER};              // src/rolling.rs:501-509

// tracing-subscriber (feature "env-filter" уже включена в workspace)
pub fn EnvFilter::builder() -> Builder;                                    // src/filter/env/mod.rs:262
pub fn Builder::with_default_directive(self, d: Directive) -> Self;        // src/filter/env/builder.rs:116
pub fn Builder::with_env_var(self, var: impl ToString) -> Self;            // :132
pub fn Builder::from_env_lossy(&self) -> EnvFilter;                        // :188
pub fn SubscriberBuilder::with_env_filter(self, filter: impl Into<EnvFilter>) -> …; // src/fmt/mod.rs:957
pub fn SubscriberBuilder::with_writer<W2>(self, make_writer: W2) -> …;     // :1057
pub fn SubscriberBuilder::with_ansi(self, ansi: bool) -> …;                // :633
pub fn SubscriberBuilder::init(self);                                      // :518
```

Точная последовательность:

```rust
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::filter::{EnvFilter, LevelFilter};

fn init_logging(log_dir: &std::path::Path, level: &str)
    -> anyhow::Result<tracing_appender::non_blocking::WorkerGuard>
{
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("lang-switcher")
        .filename_suffix("log")          // => lang-switcher.2026-08-25.log
        .max_log_files(7)
        .build(log_dir)?;                // Result, а НЕ паника (в отличие от rolling::daily)
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let filter = EnvFilter::builder()
        .with_default_directive(level.parse::<LevelFilter>().unwrap_or(LevelFilter::INFO).into())
        .with_env_var("LANG_SWITCHER_LOG")
        .from_env_lossy();
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)                // в файл ANSI-коды не нужны
        .with_writer(writer)
        .init();
    Ok(guard)                            // #[must_use]
}
// в main: let _guard = init_logging(...)?;  // держать до самого конца main
```

Правило времени жизни `WorkerGuard`: `non_blocking` пишет асинхронно на выделенном потоке, флаш — по `Drop` гарда (`src/non_blocking.rs:69-99`). ⇒ гард **обязан** жить до конца `main`. Именование `let _guard`, а **не** `let _ = …` (второе дропает сразу и обрезает логи). Тонкость нашей архитектуры: приложение завершается по команде трея из tray-потока — надо не `std::process::exit` (Drop не выполнится), а `PostQuitMessage`/сигнал в main, чтобы `main` вернулся штатно.

Имя файла — `join_date` (`src/rolling.rs:640-652`): `(_, Some(prefix), Some(suffix)) => "{prefix}.{date}.{suffix}"`. Дата — **UTC** (`OffsetDateTime::now_utc()`, `:193`, `:220`). Каталог логов **создаётся сам**: `create_writer` делает `fs::create_dir_all(parent)` (`src/rolling.rs:795`). `Builder::latest_symlink` (`builder.rs:261`) использовать **нельзя**: под капотом `symlink::symlink_file` (`rolling.rs:805`), на Windows это требует привилегий.

### 6. embed-manifest 1.5 — DPI-манифест

**Заявление подтверждено:**
- `grep -n -i "embed-manifest" Cargo.lock` → нет совпадений (крейт объявлен только в `[workspace.dependencies]`, `Cargo.toml:34`, и никем не используется, поэтому не резолвится);
- `ls -d ~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/embed-manifest*` → `NOT PRESENT`;
- `find . -name "build.rs" -not -path "./target/*"` → пусто.

**API `embed-manifest` — UNVERIFIED.** Исходников в реестре нет, по памяти описывать нельзя. При реализации задачи `build.rs` контракт (имена типа `new_manifest(...)`, `embed_manifest(...)`, `DpiAwareness`, гейт `if std::env::var("CARGO_CFG_WINDOWS").is_ok()`) обязателен к сверке с docs.rs/README крейта; ADR писать **после** сверки, а не до.

**Что должно произойти по существу** (независимо от крейта): в PE-образ `lang-switcher.exe` должен быть встроен ресурс типа `RT_MANIFEST` с ID `1`, содержащий (проверено на Microsoft Learn, «Setting the default DPI awareness for a process», 2025-07-14):

```xml
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0"
          xmlns:asmv3="urn:schemas-microsoft-com:asm.v3">
  <asmv3:application>
    <asmv3:windowsSettings>
      <dpiAware    xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
    </asmv3:windowsSettings>
  </asmv3:application>
</assembly>
```

Microsoft прямо пишет: «We recommended that you specify the default process DPI awareness via a manifest setting. While specifying the default via API is supported, it is not recommended», и `Per Monitor V2` через `<dpiAware>` **не поддерживается** — только через `<dpiAwareness>`.

**Альтернатива A — рукописный .manifest + флаги линкера (без нового крейта).** Проверено на Microsoft Learn: `/MANIFEST:EMBED[,ID=resource_id]` встраивает манифест как ресурс `RT_MANIFEST`, «Use a resource_id value of 1 for an executable file» (по умолчанию для не-DLL и так `1`); `/MANIFESTINPUT:filename` задаёт входной файл и «This option requires the /MANIFEST:EMBED option». Из `build.rs`:

```rust
fn main() {
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("lang-switcher.manifest");
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}", manifest.display());
        println!("cargo:rerun-if-changed=lang-switcher.manifest");
    }
}
```

Ограничение (задокументировано Microsoft): полный путь в `/MANIFESTINPUT` не должен превышать `MAX_PATH` (260). Флаги — только MSVC; для `*-windows-gnu` нужен путь через `.rc` + `windres`, что для этого проекта не требуется.

**Альтернатива B — рантайм, `SetProcessDpiAwarenessContext`.** Проверено в `windows-0.62.2/src/Windows/Win32/UI/HiDpi/mod.rs`:

```rust
// :137
pub unsafe fn SetProcessDpiAwarenessContext(value: DPI_AWARENESS_CONTEXT) -> windows_core::Result<()>;
// :240
pub struct DPI_AWARENESS_CONTEXT(pub *mut core::ffi::c_void);
// :252
pub const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: DPI_AWARENESS_CONTEXT = DPI_AWARENESS_CONTEXT(-4i32 as _);
```

Требуется фича `Win32_UI_HiDpi` (`windows-0.62.2/Cargo.toml:688`: `Win32_UI_HiDpi = ["Win32_UI"]`) — её в `crates/switcher-windows/Cargo.toml` сейчас нет. Ограничение из Learn: «Once a window (an HWND) has been created in your process, changing the DPI awareness mode is no longer supported… you must call the corresponding API before any HWNDs have been created». Для нас это жёстко: **вызов должен быть первой строкой `main`, до `TrayIconBuilder::build()` (он создаёт HWND) и до создания оверлея.**

**Рекомендация:** манифест (A или embed-manifest) как основной путь + `SetProcessDpiAwarenessContext` в самом начале `main` как страховка на случай сборки без манифеста (двойная установка безвредна: при уже выставленном из манифеста режиме API вернёт ошибку, которую логируем в `debug!`).

---

## Рассмотренные альтернативы (по 1-2 строки каждая, с причиной отклонения)

- **Трей на главном потоке приложения.** Отклонено: главный поток — это `crossbeam::select` по каналам, а tray-icon требует `GetMessageW`-цикл на потоке-создателе (`tray-icon/src/lib.rs:17`); смешивать канальный и оконный насосы в одном потоке — источник залипаний.
- **Трей за портом `switcher-platform`.** Отклонено: tray-icon/muda сами кроссплатформенны (Windows/macOS/Linux), порт был бы обёрткой над обёрткой; архитектура (overview.md) уже помещает трей в `switcher-app`. Плюс потребовал бы новых типов в `switcher-platform` без выигрыша.
- **`TrayIconEvent::receiver()` / `MenuEvent::receiver()` напрямую в `select!` главного цикла.** Работоспособно (единый `crossbeam-channel 0.5.15`, `Cargo.lock:278`), но оставляет два дополнительных источника; `set_event_handler` сводит всё к одному каналу и одному `AppEvent`. Второй вариант держим как fallback, если `OnceCell` окажется уже занят.
- **`rodio::Player` для громкости вместо `.amplify()`.** Отклонено: `Player` держит очередь и поток ожидания; `Effect::PlaySound { volume }` — разовый cue, `Mixer::add(source.amplify(v))` короче и без состояния.
- **Предзаписанные WAV/OGG-сэмплы вместо синтеза.** Отклонено для M1: тянет `rodio` decoder-фичи (symphonia ≈ десятки крейтов) при бюджете бинарника 1–4 МБ; синтез `SineWave` требует только `playback`.
- **`rolling::daily(dir, prefix)` вместо `Builder`.** Отклонено: паникует при ошибке инициализации (`rolling.rs:156` `.expect("initializing rolling file appender failed")`), а спека требует деградации, не падения. `Builder::build()` возвращает `Result`.
- **`ProjectDirs::from_path`.** Отклонено: сам крейт называет это «strongly discouraged» (`src/lib.rs:398-401`).
- **`Effect::UpdateTray { content: BadgeContent }` вместо `{ label, lang }`.** Отклонено: экономит один пересчёт `BadgeContent::for_lang` (микросекунды), но ломает два теста ядра и втаскивает в эффект цвета, которые трей не использует (иконка трея рисуется своим растеризатором со straight-alpha).
- **`SetProcessDpiAwareness` (shcore) / `SetProcessDPIAware`.** Отклонено: не дают Per-Monitor-V2; Learn называет `SetProcessDpiAwarenessContext` «current recommended API».
- **`default-features` у tray-icon/muda оставить как есть.** Отклонено: на Windows `libxdo`/`gtk` инертны, но `default-features = false` делает зависимость честной и не даст сюрпризов при M4 (там фичи включим отдельной target-секцией).

---

## Последствия: изменения в портах/событиях/эффектах (точные сигнатуры Rust)

**В `switcher-platform` (`events.rs`, `ports.rs`) — изменений НЕТ. В эффектах `switcher-core` (`engine.rs`) — изменений НЕТ.**

Обоснование по каждому месту, где изменение могло бы понадобиться:

| Что нужно оболочке | Существующий контракт | Хватает? |
|---|---|---|
| Обновить иконку/тултип трея | `Effect::UpdateTray { label: String, lang: LangTag }` | Да. `label` → растеризация иконки, `lang` → цвет через `BadgeContent::for_lang(lang, cfg.badge.style, &cfg.badge.colors)`, конфиг доступен через `Engine::config()` (`engine.rs:99`) |
| Синхронизировать галки меню | `Effect::PersistConfig` + `Engine::config()` | Да. Оболочка после `PersistConfig` читает `engine.config()` и вызывает `CheckMenuItem::set_checked` на tray-потоке |
| Проиграть cue | `Effect::PlaySound { cue: SoundCue, volume: f32 }` + `SoundPlayer::play(&self, cue: SoundCue, volume: f32)` | Да. `volume: f32` == `rodio::Float` (`rodio/src/common.rs:31`) |
| Команды из меню в ядро | `Event::SetMode(BadgeMode)`, `Event::SetSoundEnabled(bool)`, `Event::SetAutostart(bool)` | Да. `MenuId` → маппинг в `Event` живёт в `switcher-app` |
| Два формата пикселей бейджа | `BadgeImage { rgba_premul }` — только для `OverlayWindow::show` | Да. Иконка трея не проходит через порт вообще |

Единственные новые сущности — **локальные для `switcher-app`**, порты не трогают:

```rust
// crates/switcher-app/src/event.rs — объединённая шина главного цикла
#[derive(Debug)]
pub enum AppEvent {
    Platform(switcher_platform::events::PlatformEvent),
    /// Клик по пункту трей-меню; id из muda::MenuId.
    Menu(tray_icon::menu::MenuId),
    /// Клики/наведение по самой иконке трея.
    Tray(tray_icon::TrayIconEvent),
    /// Сработал таймер скрытия бейджа.
    HideTimer,
}

// crates/switcher-app/src/tray.rs — команды на tray-поток (только плоские данные)
#[derive(Debug)]
pub enum TrayCommand {
    SetIcon { rgba_straight: Vec<u8>, size: u32 }, // straight alpha, НЕ premultiplied
    SetTooltip(String),
    SyncChecks { follow: bool, sound: bool, autostart: bool },
    Shutdown,
}

/// Владеет TrayIcon/Menu/CheckMenuItem (все !Send) и качает GetMessageW.
/// Возвращает id потока для PostThreadMessageW-побудки.
pub fn spawn_tray_thread(
    initial: TrayInit,
    rx: crossbeam_channel::Receiver<TrayCommand>,
) -> anyhow::Result<TrayHandle>;

// crates/switcher-app/src/sound.rs — реализация существующего порта, без правки порта
pub struct RodioSoundPlayer {
    _sink: rodio::MixerDeviceSink,  // держит поток живым
    mixer: rodio::Mixer,            // Clone + Send + Sync
}
impl switcher_platform::ports::SoundPlayer for RodioSoundPlayer {
    fn play(&self, cue: switcher_platform::ports::SoundCue, volume: f32) { /* Mixer::add(...) */ }
}
```

Правки `Cargo.toml` (это изменения зависимостей ⇒ по правилу 6 нужен ADR «состав зависимостей оболочки»):

```toml
# Cargo.toml [workspace.dependencies]
rodio = { version = "0.22", default-features = false, features = ["playback"] }
tray-icon = { version = "0.24", default-features = false }
# muda — удалить из workspace.dependencies: доступна как tray_icon::menu (tray-icon/src/lib.rs:144)

# crates/switcher-app/Cargo.toml — трей нужен только под Windows на M1
[target.'cfg(windows)'.dependencies]
switcher-windows = { workspace = true }
tray-icon = { workspace = true }

# crates/switcher-windows/Cargo.toml — добавить фичу для DPI-страховки
windows = { workspace = true, features = [
    "Win32_Foundation",
    "Win32_UI_WindowsAndMessaging",
    "Win32_UI_HiDpi",            # SetProcessDpiAwarenessContext, GetDpiForMonitor
] }
```

---

## Ломающиеся тесты ядра (файл::тест -> что изменить)

**При рекомендованном решении — ни одного.** Все 42 теста `switcher-core` остаются как есть: ни один тип из `switcher-platform` и ни один вариант `Effect` не меняется.

Для полноты — что именно сломалось бы, если бы отклонённые альтернативы были приняты:

| Альтернатива | Ломается | Что менять |
|---|---|---|
| `Effect::UpdateTray { content: BadgeContent }` вместо `{ label, lang }` | `crates/switcher-core/src/engine.rs::initial_layout_updates_tray_only` (строки 276-288) | Ожидаемый вектор `vec![Effect::UpdateTray { label: "EN".to_owned(), lang: en() }]` → `vec![Effect::UpdateTray { content: default_content(&en()) }]` |
| то же | `crates/switcher-core/src/engine.rs::layout_change_updates_tray_plays_sound_and_queries_anchor` (строки 289-307) | Первый элемент ожидаемого вектора `Effect::UpdateTray { label: "RU".to_owned(), lang: ru() }` → `Effect::UpdateTray { content: default_content(&ru()) }` |
| то же | `crates/switcher-core/src/engine.rs` (строки 155-161, `on_layout`) | `content.label` сейчас **перемещается** в эффект, а `lang.clone()` — в поле; при переходе на `content` придётся убрать частичное перемещение и клонировать `content` перед `for_lang` в `on_anchor` |
| Добавить `Effect::SyncTrayChecks { follow, sound, autostart }` рядом с `PersistConfig` | `set_sound_enabled_updates_config_and_persists` (582), `set_autostart_applies_and_persists` (592), `set_mode_follow_while_hidden_shows_badge_and_persists` (536), `set_mode_follow_before_any_layout_only_persists` (544), `set_mode_follow_while_visible_cancels_timer` (551), `set_mode_transient_while_visible_arms_timer` (558) | Все шесть сравнивают полный вектор эффектов; в каждый пришлось бы добавить новый вариант. Именно поэтому решение — читать `Engine::config()` в оболочке |
| Добавить `PlatformEvent::TrayCommand{..}` в `switcher-platform` | `crates/switcher-platform/src/events.rs` — компиляция тестов ядра не ломается (`PlatformEvent` в ядре не матчится exhaustively), но нарушается правило «ports = только OS-адаптеры» | Не делать; трей-команды — событие уровня `switcher-app` |

---

## Проверенные факты API (утверждение -> путь к файлу-источнику)

Префикс: `$R = ~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f`

**tray-icon 0.24.1**
- Трей нужно создавать на потоке с Win32-циклом сообщений → `$R/tray-icon-0.24.1/src/lib.rs:17`
- `TrayIcon` содержит `Rc<RefCell<…>>` ⇒ `!Send` → `$R/tray-icon-0.24.1/src/lib.rs:343-347`
- `TrayIcon` reference-counted, иконка удаляется при дропе последнего клона → `$R/tray-icon-0.24.1/src/lib.rs:342`
- `Drop` → `remove_tray_icon` + `DestroyWindow` → `$R/tray-icon-0.24.1/src/platform_impl/windows/mod.rs:297-309`
- `set_icon`/`set_menu`/`set_tooltip`/`set_visible` используют `SendMessageW` в свой HWND → `$R/tray-icon-0.24.1/src/platform_impl/windows/mod.rs:172, 194, 226, 273`
- `Icon::from_rgba(Vec<u8>, u32, u32) -> Result<Self, BadIcon>` → `$R/tray-icon-0.24.1/src/icon.rs:135`
- Требуется **straight** (non-premultiplied) RGBA: `CreateIcon` + AND-маска из `a.wrapping_sub(255)` + swap R↔B → `$R/tray-icon-0.24.1/src/platform_impl/windows/icon.rs:21-24, 26-54`
- `Icon` (через `WinIcon`) — `Send`, но не `Sync` → `$R/tray-icon-0.24.1/src/platform_impl/windows/icon.rs:67`
- `TrayIconEvent::receiver()` = `&crossbeam_channel::Receiver<TrayIconEvent>` → `$R/tray-icon-0.24.1/src/lib.rs:650, 674`
- `set_event_handler` — `OnceCell::set`, и `send` делает `get_or_init(|| None)` ⇒ ставить до первого события → `$R/tray-icon-0.24.1/src/lib.rs:654, 684-699`
- `TrayIconBuilder` API (`with_menu`/`with_icon`/`with_tooltip`/`with_menu_on_left_click`/`build`) → `$R/tray-icon-0.24.1/src/lib.rs:252, 263, 273, 313, 335`
- Автоматическая перерегистрация иконки на `"TaskbarCreated"` + `ChangeWindowMessageFilterEx(MSGFLT_ALLOW)` → `$R/tray-icon-0.24.1/src/platform_impl/windows/mod.rs:52-53, 133, 367-377`
- Скрытое окно: `WS_EX_NOACTIVATE|WS_EX_TRANSPARENT|WS_EX_LAYERED|WS_EX_TOOLWINDOW` → `$R/tray-icon-0.24.1/src/platform_impl/windows/mod.rs:107-115`
- Tooltip обрезается до 128 UTF-16 единиц → `$R/tray-icon-0.24.1/src/platform_impl/windows/mod.rs:216, 586`
- `set_title` на Windows — no-op → `$R/tray-icon-0.24.1/src/platform_impl/windows/mod.rs:269`
- Контекстное меню трея показывается через `SetForegroundWindow` + `TrackPopupMenu` + `PostMessageW(WM_NULL)` → `$R/tray-icon-0.24.1/src/platform_impl/windows/mod.rs:547-563`
- На Windows `platform_impl` не имеет feature-гейтов ⇒ `default-features = false` безопасно → `$R/tray-icon-0.24.1/src/platform_impl/mod.rs:5-7`
- default-фичи `["libxdo","gtk"]` целятся в Linux → `$R/tray-icon-0.24.1/Cargo.toml` (`[features]`)
- `window_handle()` возвращает `windows_sys::…::HWND` = `*mut c_void` → `$R/tray-icon-0.24.1/src/lib.rs:523` + `$R/windows-sys-0.61.2/src/Windows/Win32/Foundation/mod.rs:5274`
- muda реэкспортируется как `tray_icon::menu` → `$R/tray-icon-0.24.1/src/lib.rs:144-146`

**muda 0.19.3**
- `Menu`/`CheckMenuItem` содержат `Rc<RefCell<…>>` ⇒ `!Send` → `$R/muda-0.19.3/src/menu.rs:11-15`, `$R/muda-0.19.3/src/items/check.rs:19-23`
- `CheckMenuItem::with_id(id, text, enabled, checked, accelerator)` — именно такой порядок → `$R/muda-0.19.3/src/items/check.rs:68-73`
- `MenuItem::with_id(id, text, enabled, accelerator)` → `$R/muda-0.19.3/src/items/normal.rs:56-61`
- `set_checked(&self, bool)` / `is_checked(&self)` / `set_enabled(&self, bool)` → `$R/muda-0.19.3/src/items/check.rs:137, 132, 111`
- `set_checked` под капотом — `CheckMenuItem(hmenu, id, MF_CHECKED|MF_UNCHECKED)`, без `SendMessageW` → `$R/muda-0.19.3/src/platform_impl/windows/mod.rs:777-790`
- `MenuId(pub String)`, `impl<T: ToString> From<T> for MenuId` → `$R/muda-0.19.3/src/menu_id.rs:6, 10`
- `MenuEvent { pub id: MenuId }`, `receiver()`, `set_event_handler` (тот же `OnceCell`-паттерн) → `$R/muda-0.19.3/src/lib.rs:481, 487, 505, 515-520`
- `init_for_hwnd` = меню-БАР окна (`SetMenu` + `DrawMenuBar`), трею не нужен → `$R/muda-0.19.3/src/menu.rs:235` → `$R/muda-0.19.3/src/platform_impl/windows/mod.rs:332-353`
- Для трея достаточно `attach_menu_subclass_for_hwnd` (вызывается tray-icon автоматически) → `$R/muda-0.19.3/src/platform_impl/windows/mod.rs:377-384` + `$R/tray-icon-0.24.1/src/platform_impl/windows/mod.rs:142-144`
- `WM_COMMAND` → `menu_subclass_proc` → `MenuEvent::send` ⇒ насос сообщений на том же потоке обязателен → `$R/muda-0.19.3/src/platform_impl/windows/mod.rs:1101, 1124-1145, 1239`
- Акселераторы на Windows требуют `TranslateAcceleratorW` в цикле → `$R/muda-0.19.3/src/lib.rs:20-22`
- HMENU создаётся `CreateMenu()`/`CreatePopupMenu()` → `$R/muda-0.19.3/src/platform_impl/windows/mod.rs:157-158`
- `ContextMenu::hpopupmenu(&self) -> isize`, `show_context_menu_for_hwnd(hwnd: isize, position: Option<dpi::Position>) -> bool` (unsafe) → `$R/muda-0.19.3/src/lib.rs:357, 369-373`
- `common-controls-v6` влияет только на About-диалог → `$R/muda-0.19.3/src/platform_impl/windows/mod.rs:1337-1360`
- `muda::Error` содержит `NotInitialized`/`AlreadyInitialized`/`NotAChildOfThisMenu` → `$R/muda-0.19.3/src/error.rs`

**rodio 0.22.2**
- Нет `OutputStream`, `OutputStreamBuilder`, `Sink`; экспортируются `DeviceSinkBuilder`, `MixerDeviceSink`, `Player`, `play` → `$R/rodio-0.22.2/src/lib.rs:215-227`
- `DeviceSinkBuilder::open_default_sink() -> Result<MixerDeviceSink, DeviceSinkError>` → `$R/rodio-0.22.2/src/stream.rs:233`
- `MixerDeviceSink::mixer(&self) -> &Mixer`, `log_on_drop(&mut self, bool)` → `$R/rodio-0.22.2/src/stream.rs:67, 78`
- `MixerSource` перемещён в callback cpal ⇒ пока жив `MixerDeviceSink`, живёт микшер → `$R/rodio-0.22.2/src/stream.rs:498, 517-534`
- `Drop` у `MixerDeviceSink` печатает предупреждение в stderr → `$R/rodio-0.22.2/src/stream.rs:83-90`
- `Mixer(Arc<Inner>)`, `Clone`, `add<T: Source + Send + 'static>(&self, T)` → `$R/rodio-0.22.2/src/mixer.rs:44-46, 57-64`
- `Player::connect_new(&Mixer)`, `append<S>` (с доп. границей `f32: FromSample<S::Item>`), `set_volume(Float)` → `$R/rodio-0.22.2/src/player.rs:73, 104-107, 178`
- `Sample = Float = f32` (без фичи `64bit`); `SampleRate = NonZero<u32>`, `ChannelCount = NonZero<u16>` → `$R/rodio-0.22.2/src/common.rs:5, 8, 31, 43`
- `trait Source: Iterator<Item = Sample>` с 4 обязательными методами → `$R/rodio-0.22.2/src/source/mod.rs:166, 183, 193, 196, 201`
- `take_duration(Duration)`, `amplify(Float)`, `fade_out(Duration)` — методы `Source` → `$R/rodio-0.22.2/src/source/mod.rs:260, 292, 450`
- `SineWave::new(freq: f32)`, фиксированные 48 кГц и 1 канал → `$R/rodio-0.22.2/src/source/sine.rs`
- `nz!` — `#[macro_export]`, доступен как `rodio::nz` → `$R/rodio-0.22.2/src/math.rs:156-163`
- default-фичи включают symphonia/recording/dither → `$R/rodio-0.22.2/Cargo.toml` (`[features] default`)
- сборка без декодеров поддерживается (`DecoderImpl::None(Unreachable, …)` «to satisfy the compiler when there are no decoders enabled») → `$R/rodio-0.22.2/src/decoder/mod.rs:120-125`
- `feature = "crossbeam-channel"` в коде есть, но в `[features]` не объявлен ⇒ используется `std::sync::mpsc` → `$R/rodio-0.22.2/src/mixer.rs:9-12` + `$R/rodio-0.22.2/Cargo.toml`
- cpal инициализирует COM как **STA** (`COINIT_APARTMENTTHREADED`), терпит `RPC_E_CHANGED_MODE` → `$R/cpal-0.17.3/src/host/wasapi/com.rs`
- cpal `Stream` на WASAPI — `Send`+`Sync` → `$R/cpal-0.17.3/src/host/wasapi/stream.rs:40, 49`
- cpal зависит от `windows ">=0.59, <=0.62"`, в lock — единственный `windows 0.62.2` → `$R/cpal-0.17.3/Cargo.toml:226-227` + `Cargo.lock:2195-2197`
- рабочее дерево уже резолвит rodio с default-фичами (symphonia/rand/rtrb) → `Cargo.lock:1376-1389`

**directories 6.0.0**
- `ProjectDirs::from(qualifier, organization, application) -> Option<ProjectDirs>` → `$R/directories-6.0.0/src/lib.rs:428`
- на Windows `qualifier` игнорируется, `project_path = organization\application` → `$R/directories-6.0.0/src/win.rs:98-99`
- `config_dir = %APPDATA%\<path>\config`, `data_dir = %APPDATA%\<path>\data`, `config_local_dir`/`data_local_dir`/`cache_dir` под `%LOCALAPPDATA%` → `$R/directories-6.0.0/src/win.rs:67-90` (+ таблицы в `src/lib.rs:446-495`)
- каталоги не создаются: во всём крейте нет ни `fs::`, ни `create_dir` → `$R/directories-6.0.0/src/` (grep пуст)
- `from_path` объявлен «strongly discouraged» самим крейтом → `$R/directories-6.0.0/src/lib.rs:395-402`

**tracing-appender 0.2.5 / tracing-subscriber 0.3.23**
- `non_blocking<T: Write + Send + 'static>(T) -> (NonBlocking, WorkerGuard)` → `$R/tracing-appender-0.2.5/src/lib.rs:195`
- `WorkerGuard` — `#[must_use]`, флаш по `Drop`, «should be assigned in the main function» → `$R/tracing-appender-0.2.5/src/non_blocking.rs:69-107`
- `rolling::daily` / `RollingFileAppender::new` **паникуют** при ошибке → `$R/tracing-appender-0.2.5/src/rolling.rs:143-156, 370`
- `RollingFileAppender::builder()` + `Builder::build(dir) -> Result<_, InitError>` → `$R/tracing-appender-0.2.5/src/rolling.rs:185` + `$R/tracing-appender-0.2.5/src/rolling/builder.rs:298`
- `Builder`: `rotation`, `filename_prefix`, `filename_suffix`, `max_log_files`, `latest_symlink` → `$R/tracing-appender-0.2.5/src/rolling/builder.rs:84, 128, 180, 235, 261`
- каталог логов создаётся автоматически (`fs::create_dir_all(parent)`) → `$R/tracing-appender-0.2.5/src/rolling.rs:795`
- `latest_symlink` использует `symlink::symlink_file` (на Windows нужны привилегии) → `$R/tracing-appender-0.2.5/src/rolling.rs:805`
- имя файла `{prefix}.{date}.{suffix}`; дата — **UTC** → `$R/tracing-appender-0.2.5/src/rolling.rs:640-652, 193, 220`
- `Rotation::{MINUTELY,HOURLY,DAILY,WEEKLY,NEVER}` → `$R/tracing-appender-0.2.5/src/rolling.rs:501-509`
- `EnvFilter::builder().with_default_directive(..).with_env_var(..).from_env_lossy()` → `$R/tracing-subscriber-0.3.23/src/filter/env/mod.rs:262, 289` + `$R/tracing-subscriber-0.3.23/src/filter/env/builder.rs:116, 132, 188`
- `SubscriberBuilder::with_env_filter` (за фичей `env-filter`), `with_writer`, `with_ansi`, `init` → `$R/tracing-subscriber-0.3.23/src/fmt/mod.rs:955-957, 1057, 633, 518`

**windows 0.62.2 / инфраструктура**
- `SetProcessDpiAwarenessContext(value: DPI_AWARENESS_CONTEXT) -> windows_core::Result<()>` (unsafe) → `$R/windows-0.62.2/src/Windows/Win32/UI/HiDpi/mod.rs:137`
- `DPI_AWARENESS_CONTEXT(pub *mut c_void)`, `DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2 = -4` → `$R/windows-0.62.2/src/Windows/Win32/UI/HiDpi/mod.rs:240, 252`
- фича `Win32_UI_HiDpi = ["Win32_UI"]` → `$R/windows-0.62.2/Cargo.toml:688`
- `GetMessageW`, `TranslateMessage`, `DispatchMessageW`, `PostThreadMessageW`, `PostQuitMessage` — все присутствуют → `$R/windows-0.62.2/src/Windows/Win32/UI/WindowsAndMessaging/mod.rs:997, 2402, 575, 1872, 1862`
- `windows::…::HWND(pub *mut c_void)` vs `windows_sys::…::HWND = *mut c_void` (конверсия — обёртка кортежа) → `$R/windows-0.62.2/src/Windows/Win32/Foundation/mod.rs:5670` + `$R/windows-sys-0.61.2/src/Windows/Win32/Foundation/mod.rs:5274`
- `embed-manifest` отсутствует в `Cargo.lock` и в реестре; `build.rs` в репозитории нет → `Cargo.lock` (grep пуст), `ls -d $R/embed-manifest*` → `NOT PRESENT`, `find . -name build.rs` → пусто
- единственные версии `crossbeam-channel 0.5.15`, `muda 0.19.3`, `dpi 0.1.2` → `Cargo.lock:278-280, 918-921, 348-350`
- `<dpiAwareness>PerMonitorV2</dpiAwareness>`; манифест рекомендован, API — нет; смена режима после создания HWND не поддерживается → Microsoft Learn, «Setting the default DPI awareness for a process»
- `/MANIFEST:EMBED[,ID=n]` встраивает `RT_MANIFEST` (ID=1 для exe); `/MANIFESTINPUT:file` требует `/MANIFEST:EMBED`, путь ≤ `MAX_PATH` → Microsoft Learn, «/MANIFEST», «/MANIFESTINPUT»

---

## UNVERIFIED / открытые вопросы

1. **API `embed-manifest` 1.5 — UNVERIFIED.** Крейта нет в реестре, исходников нет. Все конкретные имена (`new_manifest`, `embed_manifest`, `DpiAwareness::PerMonitorV2`, `link_manifest_file`, …) описывать нельзя. Задача `build.rs` должна начинаться со сверки с docs.rs/README крейта; до этого в плане писать «манифест встраивается — механизм на выбор: embed-manifest (API уточнить) либо `/MANIFEST:EMBED` + `/MANIFESTINPUT`».
2. **Компиляция `rodio` с `default-features = false, features = ["playback"]` — не проверена сборкой** (запуск cargo был запрещён). Устройство `DecoderImpl::None(Unreachable, …)` и `#![cfg_attr(not(feature = "playback"), allow(unused_imports), allow(dead_code), …)]` (`$R/rodio-0.22.2/src/lib.rs:175-182`) говорят, что конфигурация поддерживается разработчиками, но факт компиляции требует `cargo check -p switcher-app` в первой же задаче реализации.
3. **`cargo:rustc-link-arg-bins` для передачи `/MANIFEST:EMBED` — не сверено с Cargo Book** (проверены только сами опции линкера на Microsoft Learn). Нужно подтвердить точное имя директивы (`rustc-link-arg-bins` vs `rustc-link-arg-bin=<name>=…`) по Cargo Book при реализации.
4. **Правильный размер иконки трея.** tray-icon не масштабирует, `CreateIcon` берёт размер как есть. Нужно ли `GetSystemMetrics(SM_CXSMICON)`/`GetDpiForWindow` вместо жёстких 16×16 и как это ведёт себя при смене DPI панели задач — из исходников tray-icon не выводится, требует ручного smoke-теста на 100 %/150 %/200 %.
5. **Стабильность связки «`set_event_handler` до `TrayIconBuilder::build()`»** проверена только по коду (`OnceCell::set` + `get_or_init`), но не в рантайме. Первый прототип должен явно проверить: события меню действительно приходят в наш канал, а не в глобальный `receiver()`. Если `OnceCell` окажется занят (например, из-за порядка инициализации `Lazy`), fallback — `crossbeam_channel::Select` по `MenuEvent::receiver()` / `TrayIconEvent::receiver()`.
6. **`TrackPopupMenu` — модальный цикл на tray-потоке.** Пока открыто меню, tray-поток не обрабатывает наши `TrayCommand`. Насколько это заметно (иконка не обновится на переключение раскладки при открытом меню) — надо решить продуктово; из исходников следует только сам факт модальности.
7. **Interaction «фолбэк-поллинг 500 мс» (ADR-0003) × tray-поток.** Не определено, на каком потоке будет жить взводимый таймер: на tray-потоке через `SetTimer` (переиспользуем готовый насос) или отдельным потоком. Это вопрос ADR при реализации `LayoutMonitor`, не решается из этих исходников.
8. **`switcher-app` не собирается под non-Windows после переноса tray-icon в `[target.'cfg(windows)']`** — модуль `tray.rs` придётся закрыть `#[cfg(windows)]`, а на M3/M4 добавить target-секции с нужными фичами. Не проверено, компилируется ли `tray-icon` под Linux с `default-features = false` (по коду `platform_impl/gtk` требует `libappindicator`, т.е. фичу `gtk`) — при M4 фичи придётся включать явно.