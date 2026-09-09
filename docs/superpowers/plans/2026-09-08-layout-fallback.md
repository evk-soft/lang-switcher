# План: настраиваемая резервная проверка раскладки

> Для выполнения: superpowers:executing-plans; независимая часть конфигурации
> и трея поручена существующему агенту. Работа в основной папке, без worktree.

**Цель:** надёжный резервный путь для обычных Win32-окон и его переключение из трея.

**Архитектура:** существующий layout-поток владеет таймером и принимает последний
bool через защищённое состояние и отзываемый wake. Ядро сохраняет предпочтение,
runtime применяет его через порт; нативные объекты остаются на своих потоках.

**Стек:** Rust 1.87+, windows-rs 0.62.2, текущий Cargo workspace.

**Спецификация:** [ADR-0019](../../architecture/adr/0019-configurable-layout-fallback.md).

## 1. Конфигурация и трей

Файлы: `switcher-core/src/config.rs`, `engine.rs`; `switcher-platform/src/ports.rs`;
`switcher-app/src/menu.rs`, `tray.rs`, `startup.rs`, `runtime.rs`, `runtime/tests.rs`.

- [x] RED: старый TOML получает true; явный false переживает roundtrip;
  Event::SetLayoutFallbackEnabled(false) выдаёт применение и PersistConfig,
  повторное значение не выдаёт эффектов. Runtime применяет сохранённый false,
  переключает on/off, сохраняет флаг и показывает ошибку порта.
- [x] Реализовать `Config.layout.fallback_enabled`, порт
  `set_fallback_enabled(&self, bool) -> Result<(), PlatformError>`, соответствующие
  Event/Effect и CheckMenuItem с `Checks.layout_fallback`.
- [x] Runtime при false отбрасывает queued ForegroundPoll, сохраняя приём
  Shell/ForegroundChange/TSF. Проверить без повторного звука и перерисовки.
- [x] Startup вызывает `LayoutHooks::with_fallback(events, saved_bool)`.

## 2. Windows fallback

Файлы: `switcher-windows/src/layout_monitor.rs`, `layout_monitor/classify.rs`,
новый `layout_monitor/control.rs`, `switcher-windows/Cargo.toml`.

- [x] RED: обычный чужой foreground взводит таймер; disabled всегда снимает;
  own/unknown при рабочем hook снимают, при отказе hook enabled оставляет
  heartbeat, но не разрешает читать HKL own/unknown.
- [x] Удалить process/package probes; оставить проверку HWND/TID/PID.
  Интервал — 200 мс; не сбрасывать уже работающий таймер на каждом foreground.
- [x] Добавить mutex-контроллер: сохранять bool, постить wake под тем же lock,
  revoke в Drop владельца до выхода thread; следующий run читает последний bool.
- [x] При ошибке таймера прекратить публикации/перезапустить попытку. Проверить
  уже queued tick после disable и управление при отсутствующих подписках.
- [x] Native-тест arm/disarm/rearm и контроллера; сохранение текущей проверки
  гонки HWND/TID в snapshot. Без активации чужих окон.

## 3. Проверка и результат

- [x] fmt-check → Clippy workspace/all-targets `-D warnings` → tests all-targets;
  затем MSRV 1.87 locked и release приложения/примеров.
- [x] Независимое ревью native/управления/сохранения. Закрыть реальные замечания.
- [x] Измерить новую release-сборку с sound=false и polling on/off; записать
  методику, CPU, RSS, изменения логов и штатное завершение.
- [x] Подготовить короткий контроль полного приложения: действующие галки,
  Win+Space/Alt+Shift с F/А, независимый layout_probe без адресного TSF,
  ordinary Win32 fixture. Непройденные пользователем шаги оставить открытыми.
- [x] Обновить overview/README/smoke и отчёт; коммит без push/merge.

Результат: 179 исполнений тестов, fmt/Clippy/MSRV/release прошли. Ревью закрыто;
ресурсные замеры и ограничения описаны в
[отчёте](../../research/2026-09-08-configurable-fallback.md).
Подготовка ручной проверки завершена; сами пользовательские пункты
[smoke](../../smoke/m1-windows.md) остаются открытыми.
