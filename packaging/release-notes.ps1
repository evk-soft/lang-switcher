#Requires -Version 7.0
<#
.SYNOPSIS
    Builds the release description from CHANGELOG.md and the produced checksums.

.DESCRIPTION
    The release page and the changelog must not be allowed to disagree, so the description
    is extracted from CHANGELOG.md rather than written twice. The download warning and the
    real checksums are appended, because those belong on the page where people click.

.PARAMETER Version
    Version whose changelog section to extract, e.g. 0.1.0-alpha.1.

.PARAMETER Output
    File to write. The release workflow passes it to `gh release create --notes-file`.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Version,
    [Parameter(Mandatory)][string]$Output,
    [string]$StageDir = 'target/packaging'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $PSScriptRoot
Push-Location $Root
try {
    $changelog = Get-Content 'CHANGELOG.md'
    $start = -1
    $end = $changelog.Count
    for ($i = 0; $i -lt $changelog.Count; $i++) {
        if ($changelog[$i] -match "^##\s*\[$([regex]::Escape($Version))\]") {
            $start = $i + 1
            continue
        }
        if ($start -ge 0 -and $changelog[$i] -match '^##\s*\[') {
            $end = $i
            break
        }
    }
    if ($start -lt 0) {
        throw "CHANGELOG.md has no section for $Version"
    }
    $body = ($changelog[$start..($end - 1)] -join "`n").Trim()

    $sums = Get-Content "$StageDir/SHA256SUMS.txt" -Raw

    $notes = @"
$body

## Скачивание / Downloads

`lang-switcher-$Version-windows-x64-setup.exe` — установщик, права администратора не
нужны. `...-portable.zip` — версия без установки.

Файлы **не подписаны сертификатом**, поэтому Windows покажет предупреждение SmartScreen
(«Подробнее» → «Выполнить в любом случае»). Перед запуском сверьте контрольную сумму — это
единственная проверка целостности, которая есть у неподписанного файла.

The files are **not code-signed**, so Windows SmartScreen will warn ("More info" → "Run
anyway"). Verify the checksum before running anything; for an unsigned file it is the only
integrity check available.

``````powershell
Get-FileHash .\lang-switcher-$Version-windows-x64-setup.exe -Algorithm SHA256
``````

### SHA-256

``````text
$($sums.Trim())
``````

## Документация / Documentation

- [Установка](https://github.com/evk-soft/lang-switcher/blob/v$Version/docs/guide/installation.ru.md) · [Installation](https://github.com/evk-soft/lang-switcher/blob/v$Version/docs/guide/installation.en.md)
- [Настройки](https://github.com/evk-soft/lang-switcher/blob/v$Version/docs/guide/settings.ru.md) · [Settings](https://github.com/evk-soft/lang-switcher/blob/v$Version/docs/guide/settings.en.md)
- [SECURITY.md](https://github.com/evk-soft/lang-switcher/blob/v$Version/SECURITY.md)

Лицензии всех 87 зависимостей — в приложенном `THIRD-PARTY-LICENSES.md`.
Licences for all 87 dependencies are in the attached `THIRD-PARTY-LICENSES.md`.
"@

    $directory = Split-Path -Parent $Output
    if ($directory -and -not (Test-Path $directory)) {
        New-Item -ItemType Directory -Path $directory -Force | Out-Null
    }
    [System.IO.File]::WriteAllText((Join-Path $Root $Output), $notes)
    Write-Host "wrote $Output ($($notes.Length) characters)"
}
finally {
    Pop-Location
}
