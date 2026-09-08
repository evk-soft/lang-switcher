#requires -Version 7.0
<#
Start a bounded full-app check with an independent Win32-only layout observer.
DataDir reuses the saved config so tray preferences can be checked after restart.
PrepareOnly validates inputs and prepares data without launching any processes.
#>
[CmdletBinding()]
param([string]$DataDir, [switch]$PrepareOnly)

$ErrorActionPreference = 'Stop'
$projectDir = Split-Path -Parent $PSScriptRoot
$appExe = Join-Path $projectDir 'target/release/lang-switcher.exe'
$probeExe = Join-Path $projectDir 'target/release/examples/layout_probe.exe'
foreach ($executable in @($appExe, $probeExe)) {
    if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
        throw "Не найдена сборка: $executable"
    }
}
if (Get-Process -Name lang-switcher -ErrorAction SilentlyContinue) {
    throw 'Сначала закройте прежнюю копию lang-switcher через меню трея → Выход.'
}

if ($DataDir) {
    $checkDir = (Resolve-Path -LiteralPath $DataDir).Path
    if (-not (Test-Path -LiteralPath (Join-Path $checkDir 'config.toml') -PathType Leaf)) {
        throw 'В указанной папке нет config.toml предыдущей проверки.'
    }
} else {
    $checkDir = Join-Path $projectDir ('target/fallback-manual-' + (Get-Date -Format 'yyyyMMdd-HHmmssfff'))
    New-Item -ItemType Directory -Path $checkDir | Out-Null
    @'
log_level = "debug"
[layout]
fallback_enabled = true
[badge]
mode = "follow"
anchor = "cursor"
[sound]
enabled = true
volume = 0.4
'@ | Set-Content -LiteralPath (Join-Path $checkDir 'config.toml') -Encoding ascii
}
Write-Host "Папка проверки: $checkDir"
if ($PrepareOnly) { return }

$runId = Get-Date -Format 'yyyyMMdd-HHmmssfff'
$appProcess = $null
$probeProcess = $null
$previousLog = $env:LANG_SWITCHER_LOG
try {
    # The app trace records the delivered source; the independent probe uses no TSF.
    $env:LANG_SWITCHER_LOG = 'trace,switcher_windows::pointer=debug'
    # End in a dot, not a backslash that would escape the closing Windows quote.
    # Join-Path also preserves drive/UNC roots when DataDir names such a directory.
    $appDataArgument = '"' + (Join-Path $checkDir '.') + '"'
    $appProcess = Start-Process -FilePath $appExe -WorkingDirectory $projectDir `
        -ArgumentList @('--data-dir', $appDataArgument, '--run-for', '600') `
        -WindowStyle Hidden -PassThru `
        -RedirectStandardError (Join-Path $checkDir "$runId-app-stderr.txt")
    $probeProcess = Start-Process -FilePath $probeExe -WorkingDirectory $projectDir `
        -ArgumentList @('600', '--sample', '--stop-file', ('"' + (Join-Path $checkDir "$runId-probe.stop") + '"')) `
        -WindowStyle Hidden -PassThru `
        -RedirectStandardOutput (Join-Path $checkDir "$runId-probe.txt") `
        -RedirectStandardError (Join-Path $checkDir "$runId-probe-stderr.txt")
    if ($appProcess.WaitForExit(1000)) { throw "Приложение завершилось при запуске: exit=$($appProcess.ExitCode). См. журналы." }
    if ($probeProcess.HasExited) { throw 'Наблюдатель завершился при запуске. См. журналы.' }
    [pscustomobject]@{
        StartedUtc=[DateTime]::UtcNow.ToString('o');AppPid=$appProcess.Id;ProbePid=$probeProcess.Id
        BinarySha256=(Get-FileHash -LiteralPath $appExe -Algorithm SHA256).Hash
        Config=(Get-Content -LiteralPath (Join-Path $checkDir 'config.toml') -Raw)
    } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $checkDir "$runId-run.json") -Encoding utf8NoBOM
} catch {
    # Retained handles identify only the two processes started by this invocation.
    foreach ($process in @($probeProcess, $appProcess)) {
        if ($null -ne $process -and -not $process.HasExited) {
            $process.Kill()
            [void]$process.WaitForExit(5000)
        }
    }
    throw
} finally {
    $env:LANG_SWITCHER_LOG = $previousLog
    if ($null -ne $appProcess) { $appProcess.Dispose() }
    if ($null -ne $probeProcess) { $probeProcess.Dispose() }
}

Write-Host 'Запущены приложение и наблюдатель. Через 10 минут они завершатся сами.'
Write-Host '1. В tray проверьте галки: «Следовать за курсором», «Звук», «Резервная проверка раскладки».'
Write-Host '2. В Notepad++ и отдельной вкладке Terminal переключите Win+Space 4 раза, затем Alt+Shift 4 раза. Между сменами ждите 2 секунды и нажимайте физическую F: должна печататься f или а. В Terminal не нажимайте Enter.'
Write-Host '3. Снимите и верните галку «Резервная проверка раскладки». После включения вернитесь в редактор и повторите несколько смен. Открытие tray само по себе не должно менять RU/EN или подавать звук.'
Write-Host '4. Снимите галку, выйдите через tray и повторно запустите этот скрипт с тем же -DataDir: галка должна остаться снятой. Затем включите её обратно.'
$quotedDir = "'" + $checkDir.Replace("'", "''") + "'"
Write-Host ('.\scripts\start-fallback-check.ps1 -DataDir ' + $quotedDir)
Write-Host 'После проверки пришлите путь к папке и результат отдельно для Notepad++/Terminal: бейдж, звук, галка после перезапуска.'
Write-Host ('Для досрочной остановки наблюдателя: Set-Content -LiteralPath ' + "'" + (Join-Path $checkDir "$runId-probe.stop").Replace("'", "''") + "' -Value stop")
