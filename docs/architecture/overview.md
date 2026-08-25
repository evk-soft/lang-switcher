# Архитектура lang-switcher

Обновлено: 2026-08-25. Решения зафиксированы в [ADR](adr/); продуктовые требования — в [дизайн-спеке](../superpowers/specs/2026-07-03-lang-switcher-design.md).

## Принцип

Ядро приложения не знает ни про одну ОС и ни про один UI-фреймворк. Вся платформенная работа спрятана за портами (traits), у каждой ОС — свой крейт-адаптер. Всё событийно: пока раскладка не меняется и бейдж скрыт, процесс не потребляет ничего (ADR-0003).

## Cargo workspace (ADR-0002)

```
lang-switcher/
├── Cargo.toml                  # workspace, MSRV, lints
└── crates/
    ├── switcher-core/          # домен: state machine раскладки, выбор якоря бейджа,
    │                           # модель конфига (serde+TOML). #![forbid(unsafe_code)], без OS-зависимостей
    ├── switcher-platform/      # порты (traits) + типы событий. Без OS-зависимостей
    ├── switcher-windows/       # адаптеры Win32/COM (windows-rs)          [M1]
    ├── switcher-macos/         # адаптеры AppKit/TIS (objc2)              [M3]
    ├── switcher-linux/         # адаптеры X11/D-Bus (x11rb, zbus)         [M4]
    └── switcher-app/           # оболочка: трей (tray-icon+muda), главный цикл,
                                # egui-окно настроек по требованию, звук (rodio), wiring
```

## Порты (switcher-platform)

| Трейт | Контракт |
|---|---|
| `LayoutMonitor` | `current()` — разовое чтение при старте; поток событий `LayoutChanged { layout, lang, source }` идёт в канал, переданный при конструировании; без поллинга (взводимое исключение — ADR-0003) |
| `PointerTracker` | `set_active(bool)`, `cursor_pos()`; события `PointerMoved { pos }` — **только** пока бейдж видим |
| `CaretLocator` | best-effort `caret_point() -> Option<Point>`; всегда может вернуть `None`. В M1 — `NullCaretLocator` |
| `OverlayWindow` | `show(&BadgeImage, ResolvedAnchor)`, `move_to(ResolvedAnchor)`, `hide()`, `dpi_for(ResolvedAnchor)`; click-through, topmost, без фокуса. **Владеет всей геометрией** (ADR-0005) |
| `Autostart` | `is_enabled()`, `set_enabled(bool)` → `Result<_, PlatformError>`; конфиг — зеркало реестра (ADR-0007) |
| `SoundPlayer` | `play(cue, volume)`; в M1 кью синтезируются, не декодируются (ADR-0008) |

Адаптеры отдают события в каналы (crossbeam), плоскими данными — никаких хэндлов и колбэков через границу потока. Отказ адаптера летит тем же каналом как `CapabilityChanged(CapabilityReport)` и до ядра не доходит (ADR-0007).

**Разделение ответственности за бейдж** (ADR-0005, ADR-0006) — три вопроса, три владельца:

| Вопрос | Владелец | Чем отвечает |
|---|---|---|
| к чему привязан бейдж и что на нём написано | `switcher-core` | `Effect::ShowBadge { content, anchor }` |
| сколько это в пикселях | `switcher-app` | `dpi_for(anchor)` → растеризация → `BadgeImage` (premultiplied **BGRA**) с кэшем по `(content, dpi)` |
| где именно левый-верхний угол на экране | адаптер оверлея | смещение от якоря, масштаб смещения по DPI, выбор монитора для `Fixed`, кламп в рабочую область |

DPI до ядра не доходит вообще: `OverlayScaleChanged { dpi }` потребляет только оболочка.

## Поток событий

```
[потоки хуков ОС]               [поток ядра]                         [адресаты эффектов]
LayoutMonitor ──┐                                            ─→ OverlayWindow.show/move  (поток оверлея)
PointerTracker ─┼─→ channel ─→ ядро (switcher-core):          ─→ SoundPlayer.play        (на месте)
CaretLocator ───┘               дедупликация, выбор языка,     ─→ TrayCommand ─────────→ трей
                                якорь: каретка > курсор >             (канал + PostThreadMessage → главный поток)
                                фикс. угол; таймер скрытия
```

Эффекты исполняет **поток ядра**: `OverlayWindow`/`SoundPlayer`/`PointerTracker` вызываются с него (порты `Send`, а реализации сами переправляют работу в поток-владелец окна). На главный поток уходит только трей — и только плоскими `TrayCommand`.

Каждый хук ОС живёт в своём потоке со своим циклом сообщений (Win32 message-only окно / CFRunLoop / X event loop); COM (TSF, UIA) — в STA-потоке. Ядро — чистая, полностью тестируемая функция `(state, event) -> (state, effects)`; она живёт на выделенном потоке с `recv_timeout`, где таймаут — дедлайн таймера скрытия.

Главный поток занят треем: `TrayIcon` и пункты меню `muda` — `!Send`, а их сеттеры делают `SendMessageW` в собственное окно, поэтому трогать их можно только с потока, который качает его сообщения (ADR-0009). Ядро общается с треем плоскими `TrayCommand`. Выход — `PostQuitMessage`, а не `process::exit`: иначе не сработает `Drop` и потеряется флаш лога.

## Техника по ОС

### Windows (M1) — подробности в ADR-0004

- **Раскладка:** скрытое message-only окно; `RegisterShellHookWindow` → `HSHELL_LANGUAGE`; `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` → перечитать `GetKeyboardLayout(поток переднего окна)` (раскладка на Windows — per-thread!); TSF `ITfActiveLanguageProfileNotifySink` для IME. Фолбэк-поллинг 500 мс взводится **только** пока передний план — elevated/UWP/консоль; решение принимается на уже пришедшем `EVENT_SYSTEM_FOREGROUND`, лестницей «мёртвое окно → наш процесс → известные слепые классы → `OpenProcess(PROCESS_QUERY_INFORMATION)` → `GetPackageFullName`», при любой неопределённости — взводить (ADR-0007).
- **Оверлей:** сырое layered-окно `WS_EX_LAYERED|WS_EX_TRANSPARENT|WS_EX_NOACTIVATE|WS_EX_TOOLWINDOW|WS_EX_TOPMOST`; бейджи растеризуются оболочкой в **premultiplied BGRA** под конкретный DPI и кэшируются (ADR-0006 — именно BGRA, потому что 32bpp `BI_RGB` DIB на little-endian это `B,G,R,A`); вывод через `UpdateLayeredWindow` только при смене — он меняет позицию, размер и содержимое одним вызовом; перемещение без смены размера — `SetWindowPos(SWP_NOACTIVATE|SWP_NOSIZE|SWP_NOZORDER)`, `SWP_ASYNCWINDOWPOS` запрещён.
- **Курсор:** Raw Input (`RegisterRawInputDevices`, `RIDEV_INPUTSINK`) — не `WH_MOUSE_LL` (система молча снимает медленные хуки). Взводится только пока бейдж видим; коалесценция до частоты кадров. При снятии подписки `hwndTarget` обязан быть NULL, иначе `RIDEV_REMOVE` провалится и подписка тихо останется.
- **Устойчивость:** тело каждого потока хука обёрнуто в `catch_unwind` с backoff 250/1000/4000 мс и сбросом бюджета после 60 с работы; исчерпание бюджета гасит источник и сообщает об этом в трей через `CapabilityChanged`. Поэтому `panic = "abort"` в профилях сборки запрещён (ADR-0007).
- **Каретка (M2):** `SetWinEventHook(EVENT_OBJECT_LOCATIONCHANGE)` + `OBJID_CARET` → `GetGUIThreadInfo.rcCaret` → фолбэк UIA `TextPattern2::GetCaretRange`; при провале — автоматически якорь «курсор».
- **DPI:** манифест Per-Monitor-V2 встраивается рукописным XML через флаги линкера из `build.rs` бинарного крейта, плюс на старте, до создания любого окна, процесс читает свою фактическую awareness и выставляет её `SetProcessDpiAwarenessContext` только если манифест не сработал — с `warn!` именно в этом случае (ADR-0010). Без этого «физические пиксели» — ложь, и геометрия разъезжается на любом масштабе ≠ 100 %. Пересечение мониторов ловится сравнением `image.dpi` с DPI монитора-хозяина, а не доставкой `WM_DPICHANGED` (её для нашего `WS_EX_NOACTIVATE|WS_EX_TOOLWINDOW` окна документация не гарантирует); `WM_DPICHANGED` остаётся для случая «масштаб сменили в Параметрах, пока бейдж стоит на месте».

### macOS (M3)

Non-activating `NSPanel` (`ignoresMouseEvents`, level `.statusBar`, `canJoinAllSpaces+fullScreenAuxiliary`); `kTISNotifySelectedKeyboardInputSourceChanged` в потоке с CFRunLoop; перечитывать источник при `NSWorkspace.didActivateApplicationNotification` (per-app раскладки); автозапуск `SMAppService`; каретка — AX API за opt-in разрешением Accessibility.

### Linux (M4)

X11 — полноценно: `XkbSelectEventDetails(XkbStateNotify)`, override-redirect ARGB-окно + пустой input-region XShape, XInput2 для курсора. Wayland — честная деградация: фиксированный бейдж через layer-shell там, где он есть (не GNOME), события раскладки по D-Bus (KWin `org.kde.KeyboardLayouts`, IBus `GlobalEngineChanged`, fcitx5 `Controller1`), на GNOME — только трей.

## Конфигурация

TOML в платформенном каталоге конфигов (`directories`): режим бейджа (transient/follow), якорь (auto/cursor/fixed), вид (текст RU/EN | флаг | цвет), длительность показа, звук вкл/выкл + громкость (выбор сэмплов — M2: в M1 кью синтезируются), автозапуск, уровень логов, язык интерфейса. Ядро валидирует и мигрирует версии конфига.

## Тестирование

- `switcher-core` — только unit-тесты, test-first; state machine покрывается таблично (событие × состояние).
- Адаптеры — за трейтами; потребители тестируются на моках.
- Внутри адаптеров вся арифметика вынесена в чистые модули без `unsafe` и без вызовов ОС (`overlay::geometry` — размещение и кламп; классификация переднего окна), куда факты ОС приходят аргументами. Они покрываются табличными unit-тестами, поэтому «непроверяемого» кода остаётся только добыча фактов.
- Растеризация бейджа (`switcher-app/render.rs`) — чистая функция; тесты утверждают инварианты (размер по DPI, прозрачность угла, премультиплицированность, наличие пикселей текста), а не golden-байты.
- Нетестируемое автоматикой (хуки, оверлей, трей) — ручной smoke-чеклист `docs/smoke/m1-windows.md` по шаблону из скила `platform-api-work`.

## Дорожная карта

- **M1 — Windows MVP:** трей + события раскладки (3 источника) + транзиентный бейдж у курсора + звук + автозапуск + конфиг.
- **M2 — полировка Windows:** якорь-каретка, окно настроек (egui), режим постоянного следования, инсталлятор (cargo-wix/Inno, ~2–4 МБ), winget.
- **M3 — macOS.**
- **M4 — Linux:** X11 полноценно, Wayland — деградированный режим.

Riskiest-first: первым прототипируется связка «layered click-through оверлей, следующий за курсором на mixed-DPI» + «три источника событий раскладки» — это решает главный технический риск проекта.
