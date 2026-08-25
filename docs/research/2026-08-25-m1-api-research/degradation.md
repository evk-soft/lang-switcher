# Контракт деградации, супервизия и взведение фолбэка

> Отчёт исследования от 2026-08-25. Каждое утверждение об API сверено по вендоренным исходникам
> в `~/.cargo/registry/src/index.crates.io-*/<крейт>-<версия>/` (context7 в той сессии был недоступен)
> и по learn.microsoft.com для семантики Win32; источник указан рядом с фактом.
> На основе этих отчётов приняты ADR-0005…ADR-0010. **При расхождении отчёта и ADR побеждает ADR** —
> отчёт фиксирует, что было известно на момент решения, и не переписывается вслед за ним.

## Ответ на вопрос

Провёл. Резюме решения, прежде чем детали:

**Деградация НЕ идёт через ядро.** Ядру сообщается ровно один бит и ровно про один случай — автозапуск, потому что `cfg.autostart` в конфиге это не «пожелание пользователя», а **зеркало значения в реестре**. Всё остальное (умер источник раскладки, нет звукового устройства, не создался оверлей) — учёт возможностей для пользователя; у ядра из этого факта нет ни одного решения, которое оно могло бы принять, поэтому оно про него не знает.

Ключевая линия, которую я провожу и на которой держится вся минимальность: **конфиг хранит намерение пользователя; карта возможностей хранит реальность ОС.** `sound.enabled = true` при мёртвом звуке — корректный конфиг (пользователь хочет звук, ОС не может), ядро продолжает эмитить `Effect::PlaySound`, приложение делает no-op и пишет в трей причину. Единственное поле, где намерение и реальность совпадают по определению, — `autostart`. Отсюда: одна новая ветка в `Engine`, один новый `Event`, один новый `Effect`, **один сломанный тест ядра**.

**Фолбэк-поллинг и Raw Input не требуют портов вообще.** Взведение Raw Input уже полностью описано существующим `Effect::SetPointerTracking(bool)` → `PointerTracker::set_active(bool)`. Взведение 500 мс поллинга — целиком внутри адаптера `LayoutMonitor`, решение принимается на уже существующей подписке `EVENT_SYSTEM_FOREGROUND` (то есть решение «опрашивать ли» само не стоит ни одного опроса). Наблюдаемость обоих — через **уже существующие типы**: `LayoutSource::ForegroundPoll` не должен появляться в логе при обычном Win32-окне; `PlatformEvent::PointerMoved` не должен появляться при скрытом бейдже.

И одна важная поправка к формулировке вопроса: **`OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` — неправильный пробник elevated-окна.** Microsoft Learn прямо документирует это право как намеренно ослабленное подмножество, доступное даже к protected processes («The **PROCESS_QUERY_LIMITED_INFORMATION** right was introduced to provide access to a subset of the information available through **PROCESS_QUERY_INFORMATION**»), и в списке прав, запрещённых к protected process, `PROCESS_QUERY_LIMITED_INFORMATION` отсутствует, тогда как `PROCESS_QUERY_INFORMATION` — присутствует. Пробовать надо `PROCESS_QUERY_INFORMATION` (0x0400).

---

## Рекомендованное решение (точно и проверяемо)

### 1. Контракт деградации: `Capability` + один вариант `PlatformEvent`, путь только на уровне приложения

Новые типы в `switcher-platform` (плоские данные, ни одной OS-специфики, ни одного хэндла):

`D:\disk.w\Projects\evk-soft\lang-switcher\.claude\worktrees\m1-plan-adrs\crates\switcher-platform\src\events.rs`

```rust
/// A user-visible capability that can be lost at runtime without killing the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Capability {
    LayoutShellHook,
    LayoutForegroundHook,
    LayoutTsf,
    Pointer,
    Caret,
    Overlay,
    Sound,
    Autostart,
}

impl Capability {
    /// Stable key for log fields and tests. Never localized, never parsed for OS specifics.
    pub const fn key(self) -> &'static str {
        match self {
            Self::LayoutShellHook => "layout.shell_hook",
            Self::LayoutForegroundHook => "layout.foreground_hook",
            Self::LayoutTsf => "layout.tsf",
            Self::Pointer => "pointer",
            Self::Caret => "caret",
            Self::Overlay => "overlay",
            Self::Sound => "sound",
            Self::Autostart => "autostart",
        }
    }

    /// The three redundant layout sources: the tray aggregates them into one line.
    pub const fn is_layout_source(self) -> bool {
        matches!(
            self,
            Self::LayoutShellHook | Self::LayoutForegroundHook | Self::LayoutTsf
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityState {
    Ok,
    /// Works, but with a caveat worth telling the user about.
    Degraded,
    /// Unavailable for the rest of this process' lifetime.
    Off,
}

/// One capability's health. Adapter-authored, app-consumed. The core never sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityReport {
    pub capability: Capability,
    pub state: CapabilityState,
    /// Stable machine key, e.g. "restart_budget_exhausted", "registry_write_denied".
    pub code: &'static str,
    /// One line for tray/settings. English; the app localizes by `code`, never by parsing this.
    pub detail: String,
}
```

и **один** новый вариант события:

```rust
pub enum PlatformEvent {
    LayoutChanged { .. },
    PointerMoved { .. },
    OverlayScaleChanged { .. },
+   /// An adapter lost or regained a capability. Consumed by the app shell ONLY —
+   /// the core is never told, because it has no decision to make from it.
+   CapabilityChanged(CapabilityReport),
}
```

Почему на `PlatformEvent`, а не отдельным каналом: адаптеры уже владеют ровно одним `Sender<PlatformEvent>`; второй канал удваивает проводку ради нуля выгоды, и **порядок важен** — `LayoutChanged`, пришедший *после* `CapabilityChanged(LayoutTsf, Off)`, валиден (другой источник) и не должен переупорядочиваться относительно него.

`PlatformError` получает стабильный код (сейчас это `PlatformError(pub String)` и у него **ноль потребителей** во всём воркспейсе — проверено grep'ом, так что смена формы бесплатна):

`crates\switcher-platform\src\ports.rs`

```rust
#[derive(Debug, Clone, thiserror::Error)]
#[error("{detail}")]
pub struct PlatformError {
    /// Stable machine key authored by the adapter: "registry_write_denied",
    /// "hook_register_failed". Lets the app build a CapabilityReport without
    /// knowing anything about the OS.
    pub code: &'static str,
    pub detail: String,
}

impl PlatformError {
    pub fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self { code, detail: detail.into() }
    }
}
```

Это важная деталь: без `code` приложению пришлось бы придумывать код для «сбой записи в реестр», то есть тащить Win32-знание в шелл. С `code` его авторизует адаптер, а шелл только пробрасывает.

**Потребитель — только `switcher-app`.** Он держит `CapabilityMap` (фиксированный массив по числу вариантов `Capability`), и `CapabilityChanged` **не передаётся** в `Engine::handle`. На каждое изменение:

- лог: `warn!(target: "switcher::capability", cap = r.capability.key(), state = ?r.state, code = r.code, "{}", r.detail)`;
- трей-тултип: `TrayIcon::set_tooltip(Some(..))` — компонуется из `Effect::UpdateTray { label, .. }` (ядро) плюс агрегата карты («⚠ 2 функции недоступны»);
- трей-меню: неактивный `MenuItem` («Состояние: …») через `MenuItem::set_text` + `set_enabled(false)` — там ограничения на длину нет, туда идёт построчный список причин.

**Ограничение тултипа, проверенное в исходниках:** `tray-icon` копирует в `szTip: [u16; 128]` циклом `for i in 0..tip.len().min(128)`, где `tip` — `encode_wide` с добавленным `0`. То есть строка ровно в 128 UTF-16-единиц теряет NUL-терминатор. Держать тултип ≤ 126 UTF-16-единиц текста; кириллица — 1 единица на символ, значит ~120 символов, две строки максимум, остальное — в меню.

### 2. Автозапуск: двухфазный round-trip, повторяющий уже доказанный `QueryAnchor → AnchorResolved`

`crates\switcher-core\src\engine.rs`

```rust
 pub enum Event {
     ...
     SetAutostart(bool),
+    /// Runtime's answer to `Effect::ApplyAutostart` — and the startup reconciliation
+    /// path: `Autostart::is_enabled()` is the OS truth, the config only mirrors it.
+    AutostartApplied { requested: bool, ok: bool },
 }

 pub enum Effect {
     ...
     ApplyAutostart(bool),
     /// Config changed: runtime saves it and re-syncs tray checkmarks.
     PersistConfig,
+    /// Re-sync tray checkmarks from `Engine::config()` WITHOUT writing the file:
+    /// the user already toggled a muda CheckMenuItem that the OS then refused.
+    SyncTrayMenu,
 }
```

Диф ветки (было — строки 131‑134):

```rust
-            Event::SetAutostart(enabled) => {
-                self.cfg.autostart = enabled;
-                vec![Effect::ApplyAutostart(enabled), Effect::PersistConfig]
-            }
+            // `cfg.autostart` mirrors a registry value, so the OS — not the click — decides.
+            // No dedup on the request: the Run key may have drifted (cleaner tool, other
+            // instance, manual edit), so a repeated toggle legitimately re-asserts it.
+            Event::SetAutostart(enabled) => vec![Effect::ApplyAutostart(enabled)],
+            Event::AutostartApplied { requested, ok } => {
+                if !ok {
+                    // Refused: the config keeps the old value, the menu is snapped back,
+                    // the reason travels separately as CapabilityChanged(Autostart, ..).
+                    return vec![Effect::SyncTrayMenu];
+                }
+                if self.cfg.autostart == requested {
+                    return vec![];
+                }
+                self.cfg.autostart = requested;
+                vec![Effect::PersistConfig]
+            }
```

Почему именно так, а не «оптимистично + корректирующее событие»: `PersistConfig` эмитится **в том же батче эффектов**, то есть неверное значение успевает попасть на диск до того, как придёт коррекция. Оптимизм тут технически не спасаем.

Почему в ветке `SetAutostart` нет дедупа, хотя в `on_set_mode` он есть: режим — это радиокнопка, чей единственный источник истины — конфиг; автозапуск — чекбокс, чей источник истины — реестр. Повторный запрос к реестру стоит микросекунды и происходит только по клику.

**Бонус, который этот же `Event` закрывает бесплатно:** сверка на старте. Приложение вызывает `Autostart::is_enabled()`; если результат расходится с `cfg.autostart` — реестр побеждает, и приложение скармливает ядру `Event::AutostartApplied { requested: actual, ok: true }`, что чинит конфиг и чекмарк. Если `is_enabled()` вернул `Err` — `CapabilityChanged(Autostart, Off, ..)` и `CheckMenuItem::set_enabled(false)`.

**Что видит пользователь в трее при отказе:** чекбокс «Автозапуск» отщёлкивается назад (`SyncTrayMenu` → `CheckMenuItem::set_checked(false)`), становится неактивным (`set_enabled(false)`), тултип получает строку «⚠ Автозапуск недоступен», неактивный пункт «Состояние» — строку с `detail` («cannot write HKCU\…\Run: access denied»). Конфиг на диске остаётся `autostart = false`.

### 3. Паника в потоке хука: супервизор внутри `switcher-windows`, ядро не знает

**Владелец — адаптерный крейт, не приложение.** Приложение физически не может перезапустить поток, который не создавало: setup потока на 100% OS-специфичен (регистрация класса, message-only окно, `RegisterShellHookWindow`, `SetWinEventHook`, `CoInitializeEx(COINIT_APARTMENTTHREADED)` + TSF-синк). Универсальный супервизор в шелле потребовал бы `Box<dyn Fn(...) -> ... + Send>` — колбэк через границу потока в порту, что прямо запрещено правилом «flat data only». Поэтому цикл живёт внутри адаптера, а через порт летит только `PlatformEvent::CapabilityChanged`.

Чтобы не копировать цикл трижды — маленький хелпер **внутри `switcher-windows`** (замыкание не пересекает ни один порт):

`crates\switcher-windows\src\supervise.rs` (adapter-internal)

```rust
pub(crate) const BACKOFF_MS: [u64; 3] = [250, 1_000, 4_000];
/// A thread that ran this long without dying is healthy: reset its restart budget.
pub(crate) const HEALTHY_AFTER_MS: u64 = 60_000;

pub(crate) fn spawn_supervised<F>(
    cap: Capability,
    tx: crossbeam_channel::Sender<PlatformEvent>,
    run: F,
) -> std::thread::JoinHandle<()>
where
    F: Fn(&crossbeam_channel::Sender<PlatformEvent>) -> Result<(), PlatformError>
        + Send
        + 'static,
```

Цикл: `std::panic::catch_unwind(AssertUnwindSafe(|| run(&tx)))`; на `Err(payload)` (паника) или `Ok(Err(e))` (штатный сбой) — `warn!` с `cap.key()`, attempt, code; сон `BACKOFF_MS[attempt]`; retry. Если поток прожил ≥ `HEALTHY_AFTER_MS` — `attempt = 0`. После исчерпания трёх попыток — `tx.send(PlatformEvent::CapabilityChanged(CapabilityReport { capability: cap, state: Off, code: "restart_budget_exhausted", detail }))` и выход.

Обоснование расписания: 250/1000/4000 — три перезапуска за ~5,25 с. Транзиент (перезапуск `explorer.exe`, роняющий shell-hook) восстанавливается с первой-второй попытки; поток, умерший трижды за пять секунд, от упорства не оживёт. **Сброс бюджета после 60 с здоровья** — та часть, которую обычно забывают: без него процесс, живущий неделю, накапливает несвязанные сбои и глушит здоровый источник.

**Ядру знать не надо, и это доказуемо:** поведение ядра при мёртвом источнике раскладки *побитово* идентично поведению при тихом источнике — событий нет, решений нет.

**Критический футган, который надо зафиксировать:** в `Cargo.toml` воркспейса **нет секции `[profile.release]`** (проверено), то есть действует дефолтный `panic = "unwind"` и `catch_unwind` работает. Если кто-то когда-нибудь добавит `panic = "abort"` ради размера бинарника — весь супервизор становится мёртвым кодом. Это должно попасть в ADR как явный запрет.

**M1 честно:** `catch_unwind` + 250/1000/4000 + сброс через 60 с + `CapabilityChanged` при исчерпании — для трёх источников раскладки. Оверлей / звук / автозапуск сообщают `Off` только при сбое конструирования (это не потоки с хуками, перезапускать нечего).

**M2 честно отложено:** повторное включение выключенного источника без перезапуска приложения («Повторить» в трее); панель возможностей в egui-настройках; супервизия потока оверлея (его состояние — видимый бейдж — пришлось бы восстанавливать реплеем эффектов ядра, для M1 не стоит); эвристика «источник ни разу не выстрелил» (см. UNVERIFIED про `HSHELL_LANGUAGE`).

### 4. Взведение: 500 мс поллинг и Raw Input

#### Raw Input — порт уже есть, новой поверхности ноль

`Effect::SetPointerTracking(bool)` → `PointerTracker::set_active(bool)`. Ядро уже эмитит это ровно в нужных точках (проверено в `engine.rs`: `SetPointerTracking(true)` в `on_anchor` только при `ResolvedAnchor::Cursor`, `false` — при скрытии и при не-курсорных якорях). Внутри адаптера:

- взвод: `RegisterRawInputDevices(&[RAWINPUTDEVICE { usUsagePage: 0x01, usUsage: 0x02, dwFlags: RIDEV_INPUTSINK, hwndTarget: hwnd }], size_of::<RAWINPUTDEVICE>() as u32)`;
- снятие: тот же TLC с `dwFlags: RIDEV_REMOVE` и **`hwndTarget: HWND(ptr::null_mut())`**.

Два документированных ограничения, оба проверены на Learn: «If **RIDEV_REMOVE** is set and the **hwndTarget** member is not set to **NULL**, then RegisterRawInputDevices function will fail»; и для `RIDEV_INPUTSINK` — «Note that **hwndTarget** must be specified». Перепутать местами — гарантированный отказ снятия подписки, то есть тихое нарушение ADR-0003.

`set_active` вызывается из потока цикла ядра, а `RegisterRawInputDevices` обязана исполниться в потоке-владельце очереди сообщений `hwndTarget` — поэтому реализация постит `WM_APP + 1` в своё message-only окно и делает регистрацию в wndproc.

**Как ревьюер это наблюдает:** при скрытом бейдже и `RUST_LOG=switcher_windows=trace` прогон мыши по всему экрану даёт **ноль** `PlatformEvent::PointerMoved`. Наблюдаемое — уже существующий тип, новая инструментация не нужна.

#### Фолбэк-поллинг — целиком внутри адаптера, поверхности порта ноль

Решение о взводе принимается на **уже имеющейся** подписке `EVENT_SYSTEM_FOREGROUND` (`SetWinEventHook`), то есть «решить, опрашивать ли» само не стоит ни одного опроса. Механика: `SetTimer(hwnd, IDT_FALLBACK, 500, None)` / `KillTimer(hwnd, IDT_FALLBACK)` на message-only окне потока раскладки; в `WM_TIMER` — перечитать `GetKeyboardLayout(GetWindowThreadProcessId(GetForegroundWindow(), None))`, привести LANGID через `LCIDToLocaleName` и отправить `LayoutChanged { source: LayoutSource::ForegroundPoll }`. Дедуп ядра уже глотает повторы (тест `same_layout_from_another_source_is_deduplicated`), так что тик можно эмитить безусловно.

Лестница классификации переднего окна, **fail-safe в сторону корректности** (ADR-0003: «некорректность хуже редких взведённых опросов» — неизвестное классифицируется как «взводить»):

1. `GetWindowThreadProcessId(hwnd, Some(&mut pid))`; `tid == 0` ⇒ окно умерло, состояние не менять.
2. `pid == GetCurrentProcessId()` ⇒ наше собственное окно (трей/оверлей) ⇒ состояние не менять.
3. `GetClassNameW(hwnd, &mut buf)` ⇒ взвод, если класс в списке заведомо слепых (`ConsoleWindowClass`, `PseudoConsoleWindow`, `CASCADIA_HOSTING_WINDOW_CLASS`, `ApplicationFrameWindow`, `Windows.UI.Core.CoreWindow`). **Сами строки классов — не из вендоренных исходников, см. UNVERIFIED.**
4. `OpenProcess(PROCESS_QUERY_INFORMATION, false, pid)`:
   - `Err(e)` и `WIN32_ERROR::from_error(&e) == Some(ERROR_ACCESS_DENIED)` ⇒ взвод, `reason = "opaque_process"`. Именно `PROCESS_QUERY_INFORMATION`, **не** `..._LIMITED_...` — см. поправку в начале.
   - `Ok(h)` ⇒ обернуть в `windows_core::Owned<HANDLE>` (RAII, `impl Free for HANDLE` есть), затем `GetPackageFullName(*h, &mut len, None)`:
     - `APPMODEL_ERROR_NO_PACKAGE` ⇒ обычное desktop-приложение ⇒ **снять** поллинг;
     - `ERROR_INSUFFICIENT_BUFFER` ⇒ упакованное (UWP/MSIX) ⇒ взвод, `reason = "packaged_app"`;
     - что-либо иное ⇒ взвод, `reason = "probe_failed"`.

Почему `GetPackageFullName`, а не `IsImmersiveProcess`: в windows-0.62.2 `IsImmersiveProcess` сгенерирована как `BOOL → Result<()>` через `.ok()`, поэтому `Err` неотличим от «не immersive» без отдельного `GetLastError` — трёхзначный ответ `GetPackageFullName` (`ERROR_SUCCESS` / `ERROR_INSUFFICIENT_BUFFER` / `APPMODEL_ERROR_NO_PACKAGE`) однозначен.

**Как ревьюер наблюдает, что поллинг снят в простое — три слоя, по возрастанию неподделываемости:**

1. **Уровень типов (главный, бесплатный):** `LayoutSource::ForegroundPoll` **не должен появляться** в логе, пока переднее окно — обычное Win32-приложение (Блокнот, Проводник). Появился — логика взвода сломана. Наблюдаемое уже в системе типов.
2. **Лог переходов (INFO), одна строка на взвод/снятие, никогда на тик:**
   `info!(target: "switcher_windows::layout", hwnd = ?h, class = %class, reason = "opaque_process", interval_ms = 500, "fallback poll armed")`
   `info!(target: "switcher_windows::layout", ticks = n, "fallback poll disarmed")`
   Счётчик тиков печатается **только** в строке снятия — простаивающее приложение даёт ноль лог-трафика, а ревьюер видит точный израсходованный бюджет тиков. Сам тик — `trace!`, не выше.
3. **Уровень ОС, независимый от нашего кода:** при скрытом бейдже и фокусе на Блокноте `typeperf "\Thread(lang-switcher/*)\Context Switches/sec"` (или Process Explorer → lang-switcher → Threads) на потоке раскладки должен показывать ~0. Живой `SetTimer` на 500 мс даёт ровно ~2 переключения контекста в секунду на этом потоке; снятый — ноль. Это та проверка, которую наше собственное логирование подделать не может.

Все три — в smoke-чеклист M1 (скил `platform-api-work`).

---

## Рассмотренные альтернативы (по 1‑2 строки каждая, с причиной отклонения)

**Контракт деградации**

- **`Event::CapabilityChanged` + `Effect::ShowCapabilityWarning` в ядре.** Отклонено: ядро получило бы список OS-ных имён возможностей и тесты, проверяющие только проброс. Стоимость без выгоды; нарушает «ядро не знает про ОС» по духу.
- **Отдельный канал `Receiver<CapabilityReport>` рядом с каналом событий.** Отклонено: удваивает проводку, и теряется порядок относительно `LayoutChanged` (событие после «источник Off» валидно и не должно переупорядочиваться).
- **Оставить `PlatformError(String)` и парсить текст в шелле.** Отклонено: парсинг человекочитаемых строк — это OS-знание в шелле плюс поломка при любой правке текста.
- **`Capability` как `&'static str`, без enum.** Отклонено: теряется исчерпывающий `match` при компоновке трея и `CapabilityMap` фиксированного размера.
- **Один агрегат `CapabilityState` вместо трёх источников раскладки.** Отклонено: логу нужна гранулярность («умер именно TSF»), а агрегацию для трея приложение делает само через `Capability::is_layout_source()`.

**Автозапуск**

- **Оптимистично + корректирующее событие при отказе.** Отклонено: `PersistConfig` летит в том же батче, неверное значение успевает на диск. Технически неспасаемо.
- **Убрать `autostart` из конфига совсем, читать реестр по требованию.** Отклонено: конфиг перестаёт быть самодостаточным для переноса/бэкапа, а чтение реестра при каждом открытии меню — лишняя работа на UI-пути.
- **Приложение безусловно ресинхронизирует меню после каждого события из трея, без `Effect::SyncTrayMenu`.** Отклонено: у шелла появляется невидимая обязанность, не выразимая тестом ядра; и комментарий у `PersistConfig` («runtime saves it and re-syncs tray checkmarks») становится ложью.
- **Дедуп в `SetAutostart` (как в `on_set_mode`).** Отклонено: реестр мог разъехаться с конфигом (чистильщик, ручная правка), повторный клик обязан его перезаявить.

**Супервизия**

- **Универсальный супервизор в `switcher-app` с фабрикой-замыканием.** Отклонено: колбэк через границу потока в порту — прямой запрет «flat data only»; плюс шелл не знает OS-специфичного setup потока.
- **Хелпер супервизии в `switcher-platform`.** Отклонено: тогда замыкание-фабрика становится частью портовой поверхности. Хелпер должен жить в адаптерном крейте.
- **Экспоненциальный backoff без потолка + бесконечные попытки.** Отклонено: спека требует «повторные падения **отключают** источник»; бесконечный retry — это тихая деградация без сообщения пользователю.
- **Backoff без сброса бюджета.** Отклонено: процесс, живущий неделю, накопит несвязанные сбои и заглушит здоровый источник.

**Взведение**

- **Порт `LayoutMonitor::set_fallback_poll(bool)`.** Отклонено: приложение не знает и не должно знать, что такое «elevated foreground»; решение принимается там, где приходит `EVENT_SYSTEM_FOREGROUND` — в адаптере.
- **`PROCESS_QUERY_LIMITED_INFORMATION` как пробник elevated.** Отклонено фактически: Learn документирует это право как намеренно ослабленное подмножество, доступное даже к protected processes.
- **`IsImmersiveProcess` для UWP.** Отклонено: в windows-0.62.2 это `BOOL → Result<()>`, `Err` неотличим от «не immersive».
- **`WH_MOUSE_LL` вместо Raw Input.** Уже отклонено в ADR-0004; подтверждаю.
- **Взводить поллинг всегда «на всякий случай».** Отклонено: ровно то, чем продукт не хочет быть (ADR-0003).

---

## Последствия: изменения в портах/событиях/эффектах (точные сигнатуры Rust)

### `crates\switcher-platform\src\events.rs` — добавить

```rust
pub enum Capability { LayoutShellHook, LayoutForegroundHook, LayoutTsf, Pointer, Caret, Overlay, Sound, Autostart }
impl Capability {
    pub const fn key(self) -> &'static str;
    pub const fn is_layout_source(self) -> bool;
}

pub enum CapabilityState { Ok, Degraded, Off }

pub struct CapabilityReport {
    pub capability: Capability,
    pub state: CapabilityState,
    pub code: &'static str,
    pub detail: String,
}
```

и один вариант в существующий `PlatformEvent`:

```rust
CapabilityChanged(CapabilityReport),
```

### `crates\switcher-platform\src\ports.rs` — заменить

```rust
// было: pub struct PlatformError(pub String);
#[derive(Debug, Clone, thiserror::Error)]
#[error("{detail}")]
pub struct PlatformError {
    pub code: &'static str,
    pub detail: String,
}
impl PlatformError {
    pub fn new(code: &'static str, detail: impl Into<String>) -> Self;
}
```

Трейты `LayoutMonitor`, `PointerTracker`, `CaretLocator`, `OverlayWindow`, `Autostart`, `SoundPlayer` — **без изменений**. Это, пожалуй, главный результат: полный контракт деградации не потребовал ни одного нового метода порта.

### `crates\switcher-core\src\engine.rs` — добавить

```rust
// в enum Event
AutostartApplied { requested: bool, ok: bool },

// в enum Effect
SyncTrayMenu,
```

и переписать ветку `Event::SetAutostart` (диф выше, строки 131‑134).

### `crates\switcher-windows\Cargo.toml` — нужны фичи `windows`

Сейчас объявлены только `Win32_Foundation`, `Win32_UI_WindowsAndMessaging`. Для описанного нужны также: `Win32_System_Threading` (`OpenProcess`, `PROCESS_QUERY_INFORMATION`), `Win32_UI_Accessibility` (`SetWinEventHook`), `Win32_UI_Input` (`RegisterRawInputDevices`, `RAWINPUTDEVICE`), `Win32_UI_Input_KeyboardAndMouse` (`GetKeyboardLayout`), `Win32_Storage_Packaging_Appx` (`GetPackageFullName`), `Win32_Globalization` (`LCIDToLocaleName`), `Win32_System_Registry` (автозапуск).

### `crates\switcher-app` — новое

`CapabilityMap` (массив `[CapabilityState; N]` + последний `CapabilityReport`), обработчик `PlatformEvent::CapabilityChanged`, компоновщик тултипа/меню, исполнитель `Effect::SyncTrayMenu`, сверка автозапуска на старте, `tracing_appender::rolling::daily` + `non_blocking` в `ProjectDirs::data_dir()` с уровнем из `cfg.log_level`.

### `Cargo.toml` воркспейса

Ничего не менять, но зафиксировать в ADR: **`panic = "abort"` запрещён**, иначе супервизор мёртв.

---

## Ломающиеся тесты ядра (файл::тест -> что изменить)

**Ровно один сломанный тест.**

`crates\switcher-core\src\engine.rs::tests::set_autostart_applies_and_persists` (строки 591‑600) — сейчас утверждает `vec![ApplyAutostart(true), PersistConfig]` и `e.config().autostart == true` после одного события. Переписать в двухфазный:

```rust
#[test]
fn set_autostart_requests_the_os_before_touching_the_config() {
    let mut e = engine_after_initial();
    let fx = e.handle(Event::SetAutostart(true), 1200);
    assert_eq!(fx, vec![Effect::ApplyAutostart(true)]);
    assert!(!e.config().autostart, "config must not claim what the OS has not confirmed");
    let fx = e.handle(Event::AutostartApplied { requested: true, ok: true }, 1210);
    assert_eq!(fx, vec![Effect::PersistConfig]);
    assert!(e.config().autostart);
}
```

**Ничего больше не ломается, и это проверяемо:**

- `Engine::handle`'s `match event` обязан получить новую ветку — это компиляционное требование, то есть сам предмет изменения, а не поломка теста.
- Ни один тест не делает исчерпывающий `match` по `Effect` (все используют `assert_eq!` по вектору либо `matches!`), поэтому новый вариант `Effect::SyncTrayMenu` не ломает ни одного.
- `crates\switcher-core\src\config.rs::tests::roundtrip_preserves_config` (строка 202) и `::default_config_matches_spec` (строка 175) трогают `cfg.autostart` напрямую, минуя `Engine` — не затронуты.
- `content.rs` — не затронут.
- `PlatformError` имеет **ноль** потребителей вне `ports.rs` (проверено grep'ом по `crates/`), смена формы не ломает ни одного теста.
- `PlatformEvent::CapabilityChanged` не ломает ничего в ядре: `switcher-core` вообще не матчит по `PlatformEvent`, он импортирует из `events` только `LangTag`, `LayoutId`, `LayoutSource`, `Point`. Это сильнейший аргумент в пользу app-only-пути: полный контракт деградации стоит ядру **ноль тестов**.

**Добавить (4 новых):**

```rust
refused_autostart_never_reaches_the_config()      // ok: false -> [SyncTrayMenu], !config().autostart, ни одного PersistConfig
autostart_confirmation_matching_config_is_a_noop() // ok: true, значение то же -> vec![]
startup_reconciliation_adopts_the_os_value()      // AutostartApplied{true,true} без предшествующего SetAutostart -> [PersistConfig]
repeated_toggle_re_asserts_the_registry()         // SetAutostart(true) дважды -> [ApplyAutostart(true)] оба раза
```

Итог: **42 → 45 тестов**, один переписан.

---

## Проверенные факты API (утверждение -> путь к файлу-источнику)

`$REG` = `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f`

| Утверждение | Источник |
|---|---|
| `GetWindowThreadProcessId(hwnd, Option<*mut u32>) -> u32` | `$REG/windows-0.62.2/src/Windows/Win32/UI/WindowsAndMessaging/mod.rs:1184` |
| `OpenProcess(PROCESS_ACCESS_RIGHTS, bool, u32) -> windows_core::Result<HANDLE>`, ошибка берётся из `Error::from_thread` (то есть GetLastError) | `$REG/windows-0.62.2/src/Windows/Win32/System/Threading/mod.rs:1192-1195` |
| `PROCESS_QUERY_LIMITED_INFORMATION = 4096` (0x1000) | `$REG/windows-0.62.2/src/Windows/Win32/System/Threading/mod.rs:2818` |
| `ERROR_ACCESS_DENIED = WIN32_ERROR(5)` | `$REG/windows-0.62.2/src/Windows/Win32/Foundation/mod.rs:1143` |
| `ERROR_INSUFFICIENT_BUFFER = WIN32_ERROR(122)` | `$REG/windows-0.62.2/src/Windows/Win32/Foundation/mod.rs:2813` |
| `APPMODEL_ERROR_NO_PACKAGE = WIN32_ERROR(15700)` | `$REG/windows-0.62.2/src/Windows/Win32/Foundation/mod.rs:125` |
| `WIN32_ERROR::from_error(&windows_core::Error) -> Option<WIN32_ERROR>` — точный способ сопоставить ошибку с `ERROR_ACCESS_DENIED` | `$REG/windows-0.62.2/src/extensions/Win32/Foundation/WIN32_ERROR.rs` (`from_error`, `is_ok`, `ok`) |
| `GetPackageFullName(HANDLE, *mut u32, Option<PWSTR>) -> WIN32_ERROR` (kernel32) | `$REG/windows-0.62.2/src/Windows/Win32/Storage/Packaging/Appx/mod.rs:206-208` |
| `IsImmersiveProcess(HANDLE) -> Result<()>` — сгенерирована как `BOOL … .ok()`, поэтому `Err` ≠ «не immersive» однозначно | `$REG/windows-0.62.2/src/Windows/Win32/System/Threading/mod.rs:1100-1103` |
| `GetClassNameW(hwnd, &mut [u16]) -> i32` | `$REG/windows-0.62.2/src/Windows/Win32/UI/WindowsAndMessaging/mod.rs:795` |
| `RegisterShellHookWindow(HWND) -> BOOL`, `HSHELL_LANGUAGE = 8` | `.../WindowsAndMessaging/mod.rs:1943`, `:4018` |
| `RegisterWindowMessageW` (для регистрации «SHELLHOOK») | `.../WindowsAndMessaging/mod.rs:1956` |
| `SetWinEventHook(u32, u32, Option<HMODULE>, WINEVENTPROC, u32, u32, u32) -> HWINEVENTHOOK`; `EVENT_SYSTEM_FOREGROUND = 3` | `$REG/windows-0.62.2/src/Windows/Win32/UI/Accessibility/mod.rs:222`; `.../WindowsAndMessaging/mod.rs:3467` |
| `GetKeyboardLayout(idthread: u32) -> HKL` | `$REG/windows-0.62.2/src/Windows/Win32/UI/Input/KeyboardAndMouse/mod.rs:68` |
| `LCIDToLocaleName(u32, Option<&mut [u16]>, u32) -> i32` | `$REG/windows-0.62.2/src/Windows/Win32/Globalization/mod.rs:684` |
| `SetTimer(Option<HWND>, usize, u32, TIMERPROC) -> usize`, `KillTimer(Option<HWND>, usize) -> Result<()>`, `WM_TIMER = 275` | `.../WindowsAndMessaging/mod.rs:2238`, `:1365`, `:7128` |
| `WM_APP = 32768` (база для внутренних сообщений взвода) | `.../WindowsAndMessaging/mod.rs:6897` |
| `RegisterRawInputDevices(&[RAWINPUTDEVICE], u32) -> Result<()>`; `RIDEV_INPUTSINK = 256`, `RIDEV_REMOVE = 1`; поля `RAWINPUTDEVICE { usUsagePage, usUsage, dwFlags, hwndTarget }`; `WM_INPUT = 255` | `$REG/windows-0.62.2/src/Windows/Win32/UI/Input/mod.rs:61`, `:254`, `:258`, `:145-150`; `.../WindowsAndMessaging/mod.rs:6990` |
| «If **RIDEV_REMOVE** is set and the **hwndTarget** member is not set to **NULL**, then RegisterRawInputDevices function will fail»; для `RIDEV_INPUTSINK` — «Note that **hwndTarget** must be specified» | Microsoft Learn, `winuser/ns-winuser-rawinputdevice` (Remarks; таблица dwFlags) |
| `PROCESS_QUERY_LIMITED_INFORMATION` — намеренно ослабленное подмножество, доступное к protected process, тогда как `PROCESS_QUERY_INFORMATION` — запрещён к protected process ⇒ LIMITED непригоден как пробник «непрозрачного» процесса | Microsoft Learn, `procthread/process-security-and-access-rights` (таблица прав + раздел «Protected Processes») |
| `RegCreateKeyExW` / `RegSetValueExW` / `RegDeleteValueW` возвращают `WIN32_ERROR` (не `Result`) ⇒ обязателен `.ok()` или ручная проверка | `$REG/windows-0.62.2/src/Windows/Win32/System/Registry/mod.rs:84`, `:626`, `:211` |
| `GetLastError() -> WIN32_ERROR` | `$REG/windows-0.62.2/src/Windows/Win32/Foundation/mod.rs:27` |
| `windows_core::Owned<T: Free>` — RAII для хэндлов, `unsafe fn new`; `impl Free for HANDLE` существует ⇒ `Owned<HANDLE>` закрывает handle-leak в пробнике | `$REG/windows-core-0.62.2/src/handles.rs:16-33` (+ экспорт `windows.rs:12-13`); `$REG/windows-0.62.2/src/Windows/Win32/Foundation/mod.rs:5297` |
| `windows_core::Error::code() -> HRESULT`, `Error::message() -> String` | `$REG/windows-result-0.4.1/src/error.rs:123`, `:132` |
| Имена фич: `Win32_System_Threading`, `Win32_System_Registry`, `Win32_UI_Accessibility`, `Win32_UI_Input`, `Win32_UI_Input_KeyboardAndMouse`, `Win32_Storage_Packaging_Appx`, `Win32_Globalization`, `Win32_Foundation`, `Win32_UI_WindowsAndMessaging` | `$REG/windows-0.62.2/Cargo.toml:650, 629, 682, 689, 692, 552, 418, 416, 707` |
| `TrayIcon::set_tooltip(Option<S>) -> Result<()>`, `set_icon`, `set_menu` | `$REG/tray-icon-0.24.1/src/lib.rs:405, 387, 396` |
| Тултип обрезается в `szTip: [u16; 128]` циклом `for i in 0..tip.len().min(128)`, где `tip` = `encode_wide` с добавленным NUL ⇒ строка ровно в 128 UTF-16-единиц теряет терминатор; держать ≤126 | `$REG/tray-icon-0.24.1/src/platform_impl/windows/mod.rs:205-217`; `.../windows/util.rs:7-11` |
| `MenuItem::set_text` / `set_enabled`, `CheckMenuItem::set_checked` — есть, значит трей может показывать состояние без пересборки меню | `$REG/muda-0.19.3/src/items/normal.rs:87, 97`; `.../items/check.rs:137` |
| `tracing_appender::rolling::daily`, `tracing_appender::non_blocking` | `$REG/tracing-appender-0.2.5/src/rolling.rs:370`; `.../src/lib.rs:195` |
| В `Cargo.toml` воркспейса нет секции `[profile.release]` ⇒ действует `panic = "unwind"`, `catch_unwind` работоспособен | `D:\disk.w\Projects\evk-soft\lang-switcher\.claude\worktrees\m1-plan-adrs\Cargo.toml` |
| `PlatformError` имеет ноль потребителей вне `ports.rs` ⇒ смена формы бесплатна | `grep -rn "PlatformError" crates/` — совпадения только в `crates/switcher-platform/src/ports.rs` |

---

## UNVERIFIED / открытые вопросы

1. **Строки имён классов окон** (`ConsoleWindowClass`, `PseudoConsoleWindow`, `CASCADIA_HOSTING_WINDOW_CLASS`, `ApplicationFrameWindow`, `Windows.UI.Core.CoreWindow`) — **UNVERIFIED**: их нет в вендоренных исходниках, это эмпирическое/общинное знание. Обязательны к подтверждению через Spy++ в smoke-чеклисте M1 до того, как на них будет что-то опираться. Именно поэтому шаг 3 лестницы — не единственный: шаг 4 (`OpenProcess`/`GetPackageFullName`) закрывает случай неверного имени класса.

2. **`HSHELL_LANGUAGE` на Windows 11: wParam/lParam и вообще живость.** Страница `nc-winuser-shellproc` вернула 404 при обеих попытках; семантику (что lParam — это HKL, а wParam — HWND) я **не подтвердил** и от себя утверждать не буду. Хуже: есть сомнение, отдаёт ли современный Windows `HSHELL_LANGUAGE` вообще. Это ровно тот «главный технический риск», который overview.md уже ставит первым спринтом M1 (riskiest-first). **Побочный вывод для контракта деградации:** если источник окажется мёртвым, обнаружить это можно только эвристикой «источник A дал ноль событий, пока источник B дал N» — а это состояние с таймаутом, то есть новое взводимое исключение и новый ADR. **Честно откладываю в M2**; в M1 мёртвый shell-hook просто тихо не даёт событий, а корректность держат `EVENT_SYSTEM_FOREGROUND` + TSF + фолбэк-поллинг.

3. **Работает ли `GetKeyboardLayout(tid)` для потока elevated-процесса из medium-IL процесса** (возвращает 0, свою раскладку, или чужую корректно) — **UNVERIFIED**, и это ключевой вопрос: если возвращает корректную, фолбэк-поллинг для elevated вообще не нужен и остаётся только для UWP/консоли, что сузит исключение ADR-0003. Требует эмпирической проверки в первом спринте (запустить elevated `cmd`, переключить раскладку, сравнить показания). Результат может уменьшить объём исключения — и тогда ADR-0003 надо будет отредактировать в сторону строгости.

4. **Точный порог `HEALTHY_AFTER_MS = 60_000` и расписание `250/1000/4000`** — инженерное суждение, не выведенное из документации. Проверяемо только эмпирически (убить `explorer.exe` и посмотреть, за сколько восстанавливается shell-hook). Числа должны попасть в ADR как константы с обоснованием, чтобы их правили осознанно.

5. **Локализация `detail`.** Спека (строка 43) обещает `ui_language`, но `CapabilityReport.detail` — английская строка от адаптера. В M1 в трей идёт английский `detail` при русских подписях меню — **осознанный компромисс**. Правильное решение (таблица `code -> локализованная строка` в шелле) — M2, вместе с окном настроек.

6. **Спека, строка 47, «показывает причину в настройках/трее» — в M1 выполнена только наполовину**: трей есть, настроек нет (окно настроек — M2 по дорожной карте). Формулировать в отчёте о M1 надо именно так, без округления в свою пользу.

7. **`Effect::SyncTrayMenu` vs комментарий у `PersistConfig`.** Оставляю асимметрию: `PersistConfig` = «сохранить + ресинк», `SyncTrayMenu` = «только ресинк». Альтернатива (сузить `PersistConfig` до «только сохранить» и добавлять `SyncTrayMenu` рядом с каждым его вхождением) ломает ~6 тестов ядра ради косметики. Если ревьюер сочтёт асимметрию неприемлемой — это осознанная развилка, а не недосмотр.