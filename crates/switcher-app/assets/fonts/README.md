# Шрифт бейджа

- Источник: [Inter v4.1](https://github.com/rsms/inter/releases/tag/v4.1), архив `Inter-4.1.zip`, файл `extras/ttf/Inter-SemiBold.ttf`.
- Внутренняя версия TTF: `Version 4.001;git-9221beed3`.
- Лицензия: SIL Open Font License 1.1; [LICENSE.txt](LICENSE.txt) скопирован из архива без изменений и прочитан перед включением шрифта.
- Статический SemiBold: таблица `fvar` отсутствует как в исходнике, так и в результате.
- Сабсет: 37 символов — `A–Z`, `0–9`, `?`. Неизвестные глифы дают цветной квадрат.
- Инструмент: Python fonttools **4.64.0**, установлен в локальный временный venv.

Точная выполненная команда PowerShell из корня репозитория:

```powershell
& .\target\font-tools-env\Scripts\python.exe -m fontTools.subset .\target\font-source\Inter-SemiBold.ttf '--unicodes=U+0030-0039,U+003F,U+0041-005A' '--name-IDs+=13,14' '--output-file=crates/switcher-app/assets/fonts/Inter-SemiBold-subset.ttf'
```

Размер: **15 304 байта**. SHA-256: `90d452fbcb7091351531014de2c3738795911066c96d13db62d6238c8750636d`.
Лицензия и ссылка на неё сохранены также в таблице `name` (ID 13, 14).
При упаковке приложения включать LICENSE.txt в файл сторонних лицензий.
