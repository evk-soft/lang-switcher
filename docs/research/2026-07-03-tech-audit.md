# Аудит технологий для lang-switcher (июнь–июль 2026)

Проведён 2026-07-03: 8 параллельных исследований (фреймворки, нативные API трёх ОС, техники оверлея, существующие аналоги) + 3 состязательных «судьи» с разными критериями. Итоговое решение — [ADR-0001](../architecture/adr/0001-pure-native-rust.md).

## Критерии

Постоянно висящая в памяти утилита: бейдж раскладки у курсора/каретки поверх всех окон (click-through), звук при переключении, трей, автозапуск, ~0% CPU в простое, минимум RAM, событийная модель (без поллинга). Windows 11 сначала, macOS/Linux потом.

## Главный вывод аудита

**Ни один фреймворк не решает сложную часть этого приложения.** Определение смены раскладки, отслеживание каретки и click-through оверлей — это рукописный нативный код во **всех** шести вариантах (плагинов/модулей для этого нет нигде). Фреймворки конкурируют только за «оболочку» (трей, настройки, окно) — а оболочка здесь тривиальна. Поэтому платить RAM-налог webview/JVM/Chromium ради оболочки — категориальная ошибка для утилиты, чей смысл — незаметность.

## Сводная таблица (состояние на июнь–июль 2026)

| Кандидат | Версия | RAM в простое | Дистрибутив | Fit |
|---|---|---|---|---|
| **Чистый Rust (без webview)** | стабильные крейты | ~5–15 МБ | ~1–4 МБ | ★ выбран |
| Tauri | 2.11.5 (01.07.2026), v3 не вышел | ~50–80 МБ (webview) | ~2.5–10 МБ | 6.5/10 |
| Avalonia + .NET | Avalonia 12.0.5, .NET 10 LTS | ~40–100 МБ | ~15–40 МБ (AOT) | 6.5/10 |
| Flutter desktop | 3.44 (май 2026) | ~38–150 МБ + GPU-цикл | ~30 МБ | 4.5/10 |
| Electron | 43.0 (02.07.2026) | ~150–250 МБ (3+ процесса) | ~80–150 МБ | 4/10 |
| Compose Multiplatform | 1.11.1 (июнь 2026) | ~108–146 МБ (JVM+Skia) | десятки МБ | 4/10 |

## Ключевые факты по кандидатам

**Tauri 2.x** — зрелый и активный (v2 стабилен ~21 месяц, ежемесячные патчи). Всё «оболочечное» первопартийное: трей в ядре, плагины autostart/global-shortcut/positioner/single-instance, прозрачные click-through окна, `cursor_position()`. Но: RAM-пол ~50–80 МБ из-за webview; click-through только на всё окно; открытые баги прозрачных окон на macOS (#14394 артефакты рамки, #13415 потеря прозрачности после DMG); перемещение webview-окна со скоростью курсора — неисследованная зона джиттера. Честный вывод судей: «Tauri экономит только оболочку, а не сложную часть».

**Electron 43** — самая полная API-поверхность для оверлея (`setIgnoreMouseEvents`, зрелый N-API) и максимальная скорость разработки, но 150–250 МБ RAM в простое структурно нарушает требование лёгкости; поддерживаемого модуля событий раскладки нет (atom/keyboard-layout архивирован в 2022) — нативный аддон писать всё равно; 8-недельный цикл Chromium — вечный налог на сопровождение.

**Flutter 3.44** — multi-window так и не долетел до stable (за флагом, @internal, umbrella-issue #142845 открыт); прозрачность окон на Windows — community-workaround (#71735 открыт с 2020); GPU-цикл рендера греет CPU при следовании за курсором.

**Avalonia 12 + .NET 10 LTS** — сильное второе место по совокупности: NativeAOT официально поддержан, P/Invoke через `[LibraryImport]` AOT-безопасен, встроенный TrayIcon, Velopack для обновлений. Но click-through полностью рукописный, каждое окно держит Skia-swapchain, 40–100 МБ RAM — в разы выше нативного пола. MAUI не подходит: Linux официально «not planned».

**Compose Multiplatform 1.11** — desktop явно вторичен для JetBrains; каждый нативный вызов через JNA; 108–146 МБ на hello-world; 1–2 с прогрева JVM при автозапуске.

## Нативные API (одинаковы для любого стека)

- **Windows 11** — всё достижимо стабильными Win32 API: `GetKeyboardLayout(поток_переднего_окна)` + связка `RegisterShellHookWindow` (HSHELL_LANGUAGE) + `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` + TSF `ITfActiveLanguageProfileNotifySink` (IME/TIP). `WM_INPUTLANGCHANGE` приходит только окнам сфокусированного приложения — одного его недостаточно. Каретка: `EVENT_OBJECT_LOCATIONCHANGE`+`OBJID_CARET`, `GetGUIThreadInfo`, фолбэк UIA `TextPattern2::GetCaretRange`. Оверлей: layered-окно `WS_EX_LAYERED|WS_EX_TRANSPARENT|WS_EX_NOACTIVATE|WS_EX_TOOLWINDOW` + `UpdateLayeredWindow` — техника «rock-solid», десятилетиями стабильная. Курсор без поллинга: Raw Input (`RIDEV_INPUTSINK`), включается только пока бейдж видим.
- **macOS** — раскладка тривиальна: `TISCopyCurrentKeyboardInputSource` + распределённое уведомление `kTISNotifySelectedKeyboardInputSourceChanged` (наблюдать в потоке с CFRunLoop). Каретка — только через AX API с разрешением Accessibility.
- **Linux** — X11 полностью решаем (XKB `XkbStateNotify`, override-redirect окно + XShape). **Wayland — жёсткая стена для всех стеков**: нет глобальной позиции курсора, нет стандартного протокола состояния раскладки, layer-shell не поддержан GNOME. Деградация by design: трей + фиксированный бейдж + D-Bus сигналы (KWin `org.kde.KeyboardLayouts`, IBus, fcitx5).

## Аналоги и рыночный пробел

- **Windows:** Punto Switcher почти заморожен (последний релиз 07.2024); Caramba Switcher — про автопереключение, не про индикацию; живые индикаторы у каретки — AutoHotkey-проекты (yakunins/language-indicator v0.78, июнь 2026); запрос такой фичи в PowerToys закрыт без реализации.
- **macOS:** Input Source Pro (Swift, GPLv3, 3.3k звёзд) — золотой стандарт UX: индикатор у каретки, правила per-app. Подтверждает востребованность паттерна «транзиентный бейдж у точки ввода».
- **Linux:** трей-индикаторы живы (gxkb), xneur мёртв с 2016.
- **Пробел:** современного нативного маловесного кроссплатформенного приложения «флаг у курсора/каретки + звук» на все три ОС не существует.

## Вердикты судей

| Линза | Победитель | Ранжирование |
|---|---|---|
| Глубина OS-интеграции | чистый Rust | rust > tauri > avalonia > electron > flutter > compose |
| Footprint и долговечность | чистый Rust | rust > tauri > avalonia > flutter > compose > electron |
| Скорость разработки | electron | electron > avalonia > tauri > flutter > compose > rust |

Судья по скорости разработки сам отметил: Electron «структурно нарушает заявленное требование лёгкости» — то есть выигрывает только там, где требования проекта игнорируются. Две содержательные линзы сошлись на чистом Rust; Tauri — резервный путь, если понадобится богатый UI настроек (архитектура это позволяет: ядро UI-независимо).

## Решение

**Чистый нативный Rust**: Cargo workspace, UI-независимое ядро, порты/адаптеры под ОС, оверлей на сырых платформенных окнах, egui-настройки по требованию. Детали — [ADR-0001](../architecture/adr/0001-pure-native-rust.md)…[ADR-0004](../architecture/adr/0004-overlay-raw-win32.md), архитектура — [overview.md](../architecture/overview.md).

## Избранные источники

- Tauri: github.com/tauri-apps/tauri/releases, v2.tauri.app/plugin/ (autostart, global-shortcut, positioner)
- Electron: electronjs.org/blog/electron-43-0, github.com/electron/electron/issues/33281
- Flutter: github.com/flutter/flutter/issues/142845 (multi-window), issues/71735 (прозрачность на Windows)
- Avalonia: avaloniaui.net/whats-new/12-0, github.com/AvaloniaUI/Avalonia/releases
- Compose: blog.jetbrains.com/kotlin/2026/05/compose-multiplatform-1-11-0/
- Win32: learn.microsoft.com — wm-inputlangchange, getkeyboardlayout, registershellhookwindow, lowlevelmouseproc (рекомендация Raw Input)
- Аналоги: github.com/runjuu/InputSourcePro, github.com/yakunins/language-indicator, caramba-switcher.com
