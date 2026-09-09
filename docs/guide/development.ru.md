# Разработка

[English](development.en.md) · [Установка](installation.ru.md) · [Переводы](translations.ru.md)

## Окружение

- Rust stable, MSVC toolchain. MSRV — **1.88** (`rust-version` в корневом `Cargo.toml`,
  проверяется отдельной джобой CI). Причина именно этой границы —
  [ADR-0023](../architecture/adr/0023-msrv-1-88-for-time-advisory.md).
- Целевая тройка выпуска: `x86_64-pc-windows-msvc`. Сборка под `*-windows-gnu` собирается,
  но не эквивалентна: манифест Per-Monitor-V2 и ресурс версии встраиваются только для MSVC.
- Python 3.11+ — для генератора списка лицензий (`tomllib`).
- Inno Setup 6.3+ — только для сборки установщика. Он предустановлен на GitHub-раннерах,
  поэтому локально нужен не всегда.

## Сборка и проверки

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings   # должен быть чистым
cargo fmt --all -- --check
```

Все три гейта обязательны перед коммитом (скил `quality-gates`) и продублированы в
[CI](../../.github/workflows/ci.yml): гейты на Windows, отдельная проверка «ядро
собирается и тестируется вне Windows» и джоба MSRV.

Запуск: `cargo run --release -p switcher-app`.

## Структура

| Крейт | Роль |
| --- | --- |
| `switcher-core` | домен: автомат раскладки, выбор якоря, модель конфига. Без OS-зависимостей, `#![forbid(unsafe_code)]` |
| `switcher-platform` | порты (traits) и плоские типы событий |
| `switcher-windows` | адаптеры Win32/COM — единственное место, где разрешён `unsafe` |
| `switcher-app` | оболочка: трей, главный цикл, растеризация бейджа, локализация, звук, wiring |

Правила проекта — в [CLAUDE.md](../../CLAUDE.md), решения — в
[ADR](../architecture/adr/), общая картина — в
[обзоре архитектуры](../architecture/overview.md).

## Локализация

Добавление языка интерфейса описано в [переводах](translations.ru.md). Коротко: новый
`.ftl` плюс одна строка в `SUPPORTED`.

## Ручные проверки

Автоматизировать хуки ОС нельзя, поэтому для них есть чеклисты и скрипты:

- [smoke-чеклист M1](../smoke/m1-windows.md) — ожидания и то, что ещё не проверено;
- `.\scripts\manual-check.ps1` — пошаговая проверка из PowerShell 7;
- `.\scripts\start-fallback-check.ps1` — короткая проверка резервного чтения раскладки;
- `cargo run -p switcher-windows --example overlay_smoke` — прототип оверлея: клики, DPI,
  переход между мониторами, фиксированный угол.

## Сборка артефактов выпуска

```powershell
pwsh -File packaging/build-release.ps1
```

Скрипт собирает release-бинарник под явную тройку, генерирует
`THIRD-PARTY-LICENSES.md`, упаковывает portable-архив, компилирует установщик и считает
SHA-256. Результат — в `target/packaging`. Без Inno Setup: `-SkipInstaller`.

Тот же скрипт вызывает
[release-workflow](../../.github/workflows/release.yml), поэтому локальный и выпускной
артефакты собираются одинаково.

### Проверка лицензий

```powershell
python packaging/collect-licenses.py --output target/packaging/THIRD-PARTY-LICENSES.md
```

Скрипт берёт зависимости из `cargo tree --edges normal` для Windows-тройки, то есть
только то, что действительно попадает в поставляемый бинарник. Ненулевой код возврата
означает, что какой-то крейт не удалось разобрать; выпускать в таком виде нельзя.

## Иконка

```sh
cargo run -p switcher-app --example make_icon
```

Пересобирает `crates/switcher-app/assets/icons/lang-switcher.ico` из того же
растеризатора, что рисует бейдж. Результат коммитится; сборка эту команду не запускает.

## Выпуск

1. Три гейта и MSRV зелёные; ручные проверки выполнены и записаны.
2. Обновить версию в `workspace.package.version` и запись в
   [CHANGELOG](../../CHANGELOG.md).
3. Слить в `main` через PR.
4. Поставить тег `vX.Y.Z...` и запушить его — release-workflow соберёт артефакты и
   создаст **черновик** выпуска.
5. Скачать артефакты из черновика, сверить SHA-256, проверить установку.
6. Опубликовать выпуск.

Alpha и beta публикуются как prerelease.
