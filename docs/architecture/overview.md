# Архитектура lang-switcher

Обновлено: 2026-07-03. Решения зафиксированы в [ADR](adr/); продуктовые требования — в [дизайн-спеке](../superpowers/specs/2026-07-03-lang-switcher-design.md).

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
| `LayoutMonitor` | подписка → поток событий `LayoutChanged { lang_tag, source }`; без поллинга |
| `PointerTracker` | события `PointerMoved { x, y, monitor }`; активен **только** пока бейдж видим |
| `CaretLocator` | best-effort `Option<Rect>` позиции каретки; всегда может вернуть `None` |
| `OverlayWindow` | `show(badge, at) / move_to(at) / hide()`; click-through, topmost, без фокуса |
| `Autostart` | `enable() / disable() / is_enabled()` |
| `SoundPlayer` | `play(cue)`; сэмплы предзагружены |

Адаптеры отдают события в каналы (crossbeam), плоскими данными — никаких хэндлов и колбэков через границу потока.

## Поток событий

```
[поток хуков ОС]                [главный поток]
LayoutMonitor ──┐
PointerTracker ─┼─→ channel ─→ ядро (switcher-core):        ─→ OverlayWindow.show/move
CaretLocator ───┘               дедупликация, выбор языка,   ─→ SoundPlayer.play
                                якорь: каретка > курсор >    ─→ tray icon update
                                фикс. угол; таймер скрытия
```

Каждый хук ОС живёт в своём потоке со своим циклом сообщений (Win32 message-only окно / CFRunLoop / X event loop); COM (TSF, UIA) — в STA-потоке. Ядро — чистая, полностью тестируемая функция `(state, event) -> (state, effects)`.

## Техника по ОС

### Windows (M1) — подробности в ADR-0004

- **Раскладка:** скрытое message-only окно; `RegisterShellHookWindow` → `HSHELL_LANGUAGE`; `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` → перечитать `GetKeyboardLayout(поток переднего окна)` (раскладка на Windows — per-thread!); TSF `ITfActiveLanguageProfileNotifySink` для IME. Фолбэк-поллинг 500 мс взводится **только** пока передний план — elevated/UWP/консоль.
- **Оверлей:** сырое layered-окно `WS_EX_LAYERED|WS_EX_TRANSPARENT|WS_EX_NOACTIVATE|WS_EX_TOOLWINDOW|WS_EX_TOPMOST`; бейджи пререндерены в ARGB на каждый язык и DPI-масштаб; вывод через `UpdateLayeredWindow` только при смене; перемещение `SetWindowPos(SWP_NOACTIVATE|SWP_NOSIZE|SWP_NOZORDER)`.
- **Курсор:** Raw Input (`RegisterRawInputDevices`, `RIDEV_INPUTSINK`) — не `WH_MOUSE_LL` (система молча снимает медленные хуки). Взводится только пока бейдж видим; коалесценция до частоты кадров.
- **Каретка (M2):** `SetWinEventHook(EVENT_OBJECT_LOCATIONCHANGE)` + `OBJID_CARET` → `GetGUIThreadInfo.rcCaret` → фолбэк UIA `TextPattern2::GetCaretRange`; при провале — автоматически якорь «курсор».
- **DPI:** манифест Per-Monitor-V2, обработка `WM_DPICHANGED`, пересчёт при пересечении мониторов.

### macOS (M3)

Non-activating `NSPanel` (`ignoresMouseEvents`, level `.statusBar`, `canJoinAllSpaces+fullScreenAuxiliary`); `kTISNotifySelectedKeyboardInputSourceChanged` в потоке с CFRunLoop; перечитывать источник при `NSWorkspace.didActivateApplicationNotification` (per-app раскладки); автозапуск `SMAppService`; каретка — AX API за opt-in разрешением Accessibility.

### Linux (M4)

X11 — полноценно: `XkbSelectEventDetails(XkbStateNotify)`, override-redirect ARGB-окно + пустой input-region XShape, XInput2 для курсора. Wayland — честная деградация: фиксированный бейдж через layer-shell там, где он есть (не GNOME), события раскладки по D-Bus (KWin `org.kde.KeyboardLayouts`, IBus `GlobalEngineChanged`, fcitx5 `Controller1`), на GNOME — только трей.

## Конфигурация

TOML в платформенном каталоге конфигов (`directories`): режим бейджа (transient/follow), якорь (auto/cursor/fixed), вид (текст RU/EN | флаг | цвет), длительность показа, звук вкл/выкл + выбор сэмплов, автозапуск. Ядро валидирует и мигрирует версии конфига.

## Тестирование

- `switcher-core` — только unit-тесты, test-first; state machine покрывается таблично (событие × состояние).
- Адаптеры — за трейтами; потребители тестируются на моках.
- Нетестируемое автоматикой (хуки, оверлей) — ручной smoke-чеклист из скила `platform-api-work`.

## Дорожная карта

- **M1 — Windows MVP:** трей + события раскладки (3 источника) + транзиентный бейдж у курсора + звук + автозапуск + конфиг.
- **M2 — полировка Windows:** якорь-каретка, окно настроек (egui), режим постоянного следования, инсталлятор (cargo-wix/Inno, ~2–4 МБ), winget.
- **M3 — macOS.**
- **M4 — Linux:** X11 полноценно, Wayland — деградированный режим.

Riskiest-first: первым прототипируется связка «layered click-through оверлей, следующий за курсором на mixed-DPI» + «три источника событий раскладки» — это решает главный технический риск проекта.
