#requires -Version 7.0
[CmdletBinding()]
param([switch]$PrepareOnly)
$ErrorActionPreference = 'Stop'
$projectDir = Split-Path -Parent $PSScriptRoot
$probeExe = Join-Path $projectDir 'target/release/examples/layout_tsf_probe.exe'
if (-not (Test-Path -LiteralPath $probeExe -PathType Leaf)) {
    throw 'Сначала соберите: cargo build --release -p switcher-windows --example layout_tsf_probe'
}
$checkDir = Join-Path $projectDir ('target/layout-tsf-manual-' + (Get-Date -Format yyyyMMdd-HHmmssfff))
New-Item -ItemType Directory -Path $checkDir | Out-Null
$resultFile = Join-Path $checkDir 'results.txt'
$stopFile = Join-Path $checkDir 'stop'
function Write-Stage([string]$Text) {
    $line = "$( [DateTimeOffset]::UtcNow.ToString('o') ) $Text"
    Add-Content -LiteralPath $resultFile -Value $line -Encoding utf8
    Write-Host $Text
}
Write-Stage "Папка результатов: $checkDir"
if ($PrepareOnly) { return }
Write-Host 'Проверка занимает около минуты. Оставьте это окно Terminal активным.'
Write-Host 'В каждом шаге: один раз переключите указанной комбинацией, отпустите клавиши,'
Write-Host 'нажмите физическую клавишу F (русская А), затем Enter. Получится f или а.'
$null = Read-Host 'Нажмите Enter, когда будете готовы'
$probe = Start-Process -FilePath $probeExe -WorkingDirectory $projectDir `
    -ArgumentList @('120', '--stop-file', ('"' + $stopFile + '"')) `
    -WindowStyle Hidden -PassThru `
    -RedirectStandardOutput (Join-Path $checkDir 'probe.txt') `
    -RedirectStandardError (Join-Path $checkDir 'probe-error.txt')
try {
    Write-Stage "PROBE BEGIN PID=$($probe.Id)"
    foreach ($shortcut in @('Win+Space', 'Alt+Shift')) {
        foreach ($index in 1..4) {
            if ($probe.HasExited) { throw 'Диагностика завершилась раньше окончания шагов; результаты неполные.' }
            Write-Stage "STEP BEGIN shortcut=$shortcut index=$index"
            $typed = Read-Host "$shortcut, шаг $index/4 — переключите, нажмите F/А и Enter"
            if ($typed -cnotmatch '^[fFаА]$') {
                Write-Stage "STEP INVALID shortcut=$shortcut index=$index (expected one F/А key)"
                throw 'Ожидалась одна буква f или а. Проверка остановлена; прогон неполный.'
            }
            Write-Stage "STEP RESULT shortcut=$shortcut index=$index typed=$typed"
            # Allow independent 200ms snapshots after the confirmed input character.
            Start-Sleep -Milliseconds 1000
            if ($probe.HasExited) { throw 'Диагностика завершилась раньше окончания шагов; результаты неполные.' }
            Write-Stage "STEP END shortcut=$shortcut index=$index"
        }
    }
    Write-Stage 'Все восемь шагов выполнены.'
} finally {
    # Cooperative stop first; terminate only this retained diagnostic process if COM hangs.
    try {
        New-Item -ItemType File -Path $stopFile -Force | Out-Null
    } catch {
        Write-Warning 'Не удалось записать stop-file; процесс всё равно будет завершён.' -WarningAction Continue
    }
    try {
        if (-not $probe.WaitForExit(5000)) {
            Write-Warning 'PROBE TIMEOUT: остановка зависшего процесса; результаты неполные.' -WarningAction Continue
            $probe.Kill()
            if (-not $probe.WaitForExit(5000)) { throw 'Не удалось завершить диагностический процесс.' }
        }
        $probeExit = $probe.ExitCode
    } finally {
        $probe.Dispose()
    }
    Write-Stage "PROBE END exit=$probeExit"
    if ($probeExit -ne 0) { throw "Диагностический процесс завершился с кодом $probeExit; прогон неполный." }
}
Write-Stage 'Готово. Напишите, что проверка завершена; результаты уже сохранены.'
