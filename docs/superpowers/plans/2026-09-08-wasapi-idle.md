# WASAPI без работы в простое: план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** устранить непрерывную генерацию тишины, сохранив звуковые подсказки M1.
**Architecture:** чистый синтез PCM в app, WASAPI STA-owner в windows; существующий SoundPlayer.
**Tech Stack:** Rust 1.87+, windows-rs 0.62.2, crossbeam; rodio/cpal удаляются.
**Spec:** [ADR-0016](../../architecture/adr/0016-demand-driven-wasapi-cues.md).

## Ограничения

Работа в основной папке без worktree. Unsafe только в windows с SAFETY.
В простое нет таймеров аудио; формат PCM16 mono44100, тон90мс.
Конфиг enabled не меняется при отказе. Все native-ресурсы освобождает STA-владелец.
Проверки: fmt, Clippy -D warnings, workspace tests; независимое read-only ревью.

## Задача 1: звуковой адаптер

- [x] В `app/src/sound.rs` добавить падающие тесты PCM: размер, огибающая, частота, volume.
- [x] В `windows/src/audio` добавить падающие тесты последнего запроса, stop,
  буферизации и drain без устройства.
- [x] Реализовать PCM и native-owner по ADR; подключить вместо rodio в startup.
- [x] Native smoke до/после сигналов и остановка с живым sender; повтор CPU/RSS.
- [x] Независимое ревью, исправления, полные гейты.
- [ ] Отказ/возврат физического устройства и прослушивание: ручная матрица M1.

Hover-таймер трея исправляется отдельным изменением с отдельной регрессией.

## Продолжение аудита 2026-09-08

- [x] Патч tray-icon по ADR-0017; native RED/GREEN по WM_TIMER и кликам.
- [x] Исправления независимого ревью: первый STA, fatal shutdown, исходный код ошибки.
- [x] Найдено резервирование логгера на 128 000 строк; ограничено 1024.
- [x] Повторный замер CPU/RSS после уменьшения очереди логгера.
- [x] Финальные гейты и фиксация результатов в [отчёте](../../research/2026-09-08-idle-fixes.md).

Ручной прогон 07:40 UTC выполнил 20 смен RU/EN в отдельном активном окне,
но приложение их не обнаружило. Это FAIL приёмки доставки, причина пока
не установлена. Последующая парная проверка Shell-получателей подтвердила работу
обеих подписок, но fixture не получила foreground для повторения смен.
[Доказательства и следующие проверки](../../research/2026-09-08-layout-delivery.md).
Для повторения есть `windows/examples/layout_driver.rs` и `shell_probe.rs`.
