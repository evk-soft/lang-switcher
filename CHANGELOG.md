# Changelog / История изменений

Формат основан на [Keep a Changelog](https://keepachangelog.com/ru/1.1.0/), версии — по
[семантическому версионированию](https://semver.org/lang/ru/).
Each entry is given in Russian and English.

## [0.1.0-alpha.1] — 2026-09-09

Первый публичный выпуск. Только Windows 11 x64, установка для текущего пользователя,
без подписи кода. First public release: Windows 11 x64 only, per-user install, unsigned.

### Возможности / Features

- Бейдж текущего языка ввода у курсора мыши: появляется на ~1,5 с при смене раскладки или
  виден постоянно в режиме следования. Оверлей поверх всех окон, прозрачен для кликов, не
  крадёт фокус, корректен на мониторах с разным DPI.
  <br>Badge showing the current input language near the mouse cursor: shown for ~1.5 s on a
  layout change, or permanently in follow mode. A click-through, always-on-top overlay that
  never steals focus and handles mixed-DPI monitors.
- Различимые звуковые сигналы для русского и английского, нейтральный для остальных
  языков. Громкость и отключение — в настройках.
  <br>Distinct cues for Russian and English, a neutral one for other languages. Volume and
  an off switch are configurable.
- Значок и меню в трее: режим следования, резервная проверка раскладки, звук, автозапуск,
  язык интерфейса, состояние, выход.
  <br>Tray icon and menu: follow mode, layout fallback check, sound, autostart, interface
  language, status, quit.
- **Мультиязычный интерфейс**: английский, русский, немецкий, испанский, французский,
  упрощённый китайский. Режим «Как в системе» берёт язык интерфейса Windows. Выбор
  применяется сразу и сохраняется.
  <br>**Multilingual interface**: English, Russian, German, Spanish, French, Simplified
  Chinese. "Same as Windows" follows the Windows display language. The choice applies
  immediately and persists.
- Отключаемая резервная проверка раскладки каждые 200 мс для окон, которые не уведомляют
  Windows о смене языка.
  <br>An optional 200 ms layout fallback check for windows that never notify Windows of a
  language change.
- Установщик для текущего пользователя без прав администратора, portable-архив,
  контрольные суммы SHA-256 и список лицензий 87 зависимостей.
  <br>A per-user installer needing no administrator rights, a portable archive, SHA-256
  checksums, and a licence notice covering 87 dependencies.
- Деградация вместо падения: отказ любого адаптера ОС ограничивает одну возможность и
  показывается в меню «Состояние».
  <br>Degrade, never crash: an OS adapter failure limits one capability and is shown under
  "Status".

### Исправлено / Fixed

- Метка бейджа больше не обрезается до двух символов: `fil` показывается как `FIL`, а не
  как `FI`, что читалось бы как финский.
  <br>Badge labels are no longer truncated to two characters: `fil` shows as `FIL`, not
  `FI`, which would read as Finnish.
- `time` обновлён до 0.3.55 (RUSTSEC-2026-0009). Из-за этого MSRV поднят до 1.88.
  <br>`time` updated to 0.3.55 (RUSTSEC-2026-0009), which raised the MSRV to 1.88.

### Известные ограничения / Known limitations

- Только Windows 11 x64. macOS и Linux не поставляются.
  <br>Windows 11 x64 only. macOS and Linux are not shipped.
- Файлы не подписаны: SmartScreen покажет предупреждение. Сверяйте `SHA256SUMS.txt`.
  <br>Files are unsigned; SmartScreen will warn. Verify `SHA256SUMS.txt`.
- Приложение показывает язык ввода, но не переключает раскладку — это делает Windows.
  <br>The application indicates the input language; Windows switches the layout.
- Режимы IME не проверялись. Региональные варианты одного языка получают одинаковую метку.
  <br>IME modes are untested. Regional variants of one language share a label.
- Якорь по текстовой каретке не реализован (M2): бейдж привязан к курсору или к
  фиксированному углу экрана.
  <br>Caret anchoring is not implemented (M2): the badge follows the cursor or sits in a
  fixed corner.
- Режим «Как в системе» проверен модульно и на одной машине; на системе с не-английским и
  не-русским языком интерфейса не проверялся.
  <br>"Same as Windows" is unit-tested and verified on one machine; it has not been checked
  on a system whose display language is neither English nor Russian.
- Не проверены: Windows Terminal после последнего исправления звука, полный набор классов
  окон и запуск с повышенными правами, реальный автозапуск после установки. Целевой объём
  памяти 5–15 МБ не достигнут: измеренный RSS — 17–21 МиБ.
  <br>Unverified: Windows Terminal after the latest audio fix, the full range of window
  classes and elevated processes, real autostart after installation. The 5–15 MB memory
  target is not met: measured RSS is 17–21 MiB.
- `ttf-parser` 0.25.1 объявлен неподдерживаемым (RUSTSEC-2026-0192) — принятый долг, см.
  [SECURITY.md](SECURITY.md).
  <br>`ttf-parser` 0.25.1 is flagged unmaintained (RUSTSEC-2026-0192) — accepted debt, see
  [SECURITY.md](SECURITY.md).

[0.1.0-alpha.1]: https://github.com/evk-soft/lang-switcher/releases/tag/v0.1.0-alpha.1
