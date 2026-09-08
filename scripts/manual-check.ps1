#requires -Version 7.0
<#
Guided acceptance with isolated data. Run from an interactive PowerShell terminal.
PrepareOnly validates binaries and writes the test config without starting processes.
The app remains running until the user closes it through its tray menu.
#>
[CmdletBinding()]
param([switch]$PrepareOnly)

$ErrorActionPreference = 'Stop'
$projectDir = Split-Path -Parent $PSScriptRoot
$appExe = Join-Path $projectDir 'target/release/lang-switcher.exe'
$audioExe = Join-Path $projectDir 'target/release/examples/audio_smoke.exe'
$driverExe = Join-Path $projectDir 'target/release/examples/layout_driver.exe'
$probeExe = Join-Path $projectDir 'target/release/examples/layout_probe.exe'
foreach ($executable in @($appExe, $audioExe, $driverExe, $probeExe)) {
    if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
        throw "Не найдена сборка: $executable"
    }
}
if (Get-Process -Name lang-switcher -ErrorAction SilentlyContinue) {
    throw 'Сначала закройте прежнюю копию lang-switcher через меню трея → Выход.'
}

$checkDir = Join-Path $projectDir ('target/manual-guided-' + (Get-Date -Format 'yyyyMMdd-HHmmssfff'))
New-Item -ItemType Directory -Path $checkDir | Out-Null
$resultsFile = Join-Path $checkDir 'results.txt'
@'
log_level = "debug"
[badge]
mode = "follow"
anchor = "cursor"
[sound]
enabled = true
volume = 0.4
'@ | Set-Content -LiteralPath (Join-Path $checkDir 'config.toml') -Encoding ascii

function Write-Stage([string]$Message) {
    $entry = '{0:o} {1}' -f [DateTimeOffset]::Now, $Message
    Add-Content -LiteralPath $resultsFile -Value $entry -Encoding utf8
    Write-Host $Message
}

function Read-Observation([string]$Prompt) {
    Write-Stage $Prompt
    $answer = Read-Host
    Write-Stage "Ответ: $answer"
}

function Assert-AppRunning {
    $appProcess.Refresh()
    if ($appProcess.HasExited) {
        Write-Stage "Приложение уже завершилось, exit=$($appProcess.ExitCode). Этап не засчитан."
        throw "Приложение не работает. Журнал: $checkDir"
    }
    Write-Stage "Приложение работает, PID=$($appProcess.Id)."
}

function Start-LayoutProbe([string]$Name) {
    $stopFile = Join-Path $checkDir "$Name.stop"
    $process = Start-Process -FilePath $probeExe `
        -ArgumentList @('600', '--sample', '--stop-file', ('"' + $stopFile + '"')) `
        -WorkingDirectory $projectDir -WindowStyle Hidden -PassThru `
        -RedirectStandardOutput (Join-Path $checkDir "$Name-probe.txt") `
        -RedirectStandardError (Join-Path $checkDir "$Name-probe-error.txt")
    Write-Stage "PROBE BEGIN $Name PID=$($process.Id)"
    return [pscustomobject]@{ Process = $process; StopFile = $stopFile; Name = $Name }
}

function Stop-LayoutProbe($Probe) {
    $Probe.Process.Refresh()
    if ($Probe.Process.HasExited) {
        Write-Stage "Наблюдатель $($Probe.Name) завершился раньше конца этапа. Его данные могут быть неполными."
    } else {
        # Only this diagnostic's unique sentinel is written; no process-wide mutation.
        Set-Content -LiteralPath $Probe.StopFile -Value 'stop' -Encoding ascii
        if (-not $Probe.Process.WaitForExit(10000)) {
            $Probe.Process.Kill()
            if (-not $Probe.Process.WaitForExit(5000)) {
                throw "Не удалось завершить диагностический наблюдатель $($Probe.Name)."
            }
            Write-Stage "Наблюдатель $($Probe.Name) превысил срок остановки; его данные неполны."
        }
    }
    Write-Stage "PROBE END $($Probe.Name) exit=$($Probe.Process.ExitCode)"
    $Probe.Process.Dispose()
}

Write-Stage "Папка результатов: $checkDir"
if ($PrepareOnly) { return }

Read-Observation 'Этап 1. Приложение ещё не запущено. Сейчас прозвучат 20 отдельных сигналов RU/EN с паузами. Нажмите Enter, когда будете готовы слушать.'
Write-Stage 'AUDIO BEGIN'
& $audioExe --audible 2>&1 | Tee-Object -FilePath (Join-Path $checkDir 'audio.txt')
$audioExit = $LASTEXITCODE
Write-Stage "AUDIO END exit=$audioExit"
Read-Observation 'Были слышны сигналы? Различались низкий RU и высокий EN? Напишите кратко и нажмите Enter.'

# Scope the diagnostic filter to the child process. Never change persistent settings.
$previousLog = $env:LANG_SWITCHER_LOG
try {
    $env:LANG_SWITCHER_LOG = 'trace,switcher_windows::pointer=debug'
    $appProcess = Start-Process -FilePath $appExe -WorkingDirectory $projectDir `
        -ArgumentList @('--data-dir', ('"' + $checkDir + '"')) `
        -WindowStyle Hidden -PassThru `
        -RedirectStandardError (Join-Path $checkDir 'stderr.txt')
} finally {
    $env:LANG_SWITCHER_LOG = $previousLog
}
Write-Stage "APP BEGIN PID=$($appProcess.Id), без автоматического завершения. Бейдж должен оставаться рядом с курсором."
Read-Observation 'Этап 2. Видите постоянный небольшой бейдж RU/EN рядом с курсором? Напишите да/нет и нажмите Enter.'
Assert-AppRunning

Read-Observation 'Этап 3. Нажмите Enter для окна с 20 переключениями. Затем нажмите в поле этого окна и оставьте его активным до закрытия (примерно 15 секунд).'
Assert-AppRunning
Write-Stage 'FIXTURE BEGIN'
$fixtureProbe = Start-LayoutProbe 'fixture'
try {
    # This is the explicitly interactive fixture, so its window must be visible.
    $driverProcess = Start-Process -FilePath $driverExe -WorkingDirectory $projectDir `
        -WindowStyle Normal -PassThru `
        -RedirectStandardOutput (Join-Path $checkDir 'layout-driver.txt') `
        -RedirectStandardError (Join-Path $checkDir 'layout-driver-error.txt')
    if (-not $driverProcess.WaitForExit(45000)) {
        # Kill only the hung diagnostic process represented by our retained handle.
        $driverProcess.Kill()
        if (-not $driverProcess.WaitForExit(5000)) {
            throw 'Не удалось завершить зависшее тестовое окно.'
        }
        throw 'Тестовое окно превысило свой срок ожидания. Этап не засчитан.'
    }
    Write-Stage "FIXTURE END exit=$($driverProcess.ExitCode)"
    Assert-AppRunning
    if ($driverProcess.ExitCode -ne 0) {
        Write-Stage 'Тестовое окно не завершило все переключения; причина сохранена в layout-driver-error.txt.'
    }
} finally {
    Stop-LayoutProbe $fixtureProbe
}
Read-Observation 'В тестовом окне: менялся ли бейдж RU/EN и были ли звуки? Напишите результат и нажмите Enter.'

foreach ($application in @('Блокнот', 'Windows Terminal')) {
    Assert-AppRunning
    Write-Stage "MANUAL BEGIN $application"
    $probeName = if ($application -eq 'Блокнот') { 'notepad' } else { 'terminal' }
    $manualProbe = Start-LayoutProbe $probeName
    try {
        Read-Observation "Откройте $application, нажмите в поле ввода. Переключите RU/EN через Win+Space 10 раз с паузами 2 секунды. Затем вернитесь сюда и напишите: бейдж менялся/не менялся; звук был/не был."
        Assert-AppRunning
    } finally {
        Stop-LayoutProbe $manualProbe
    }
    Write-Stage "MANUAL END $application"
}

Read-Observation 'Проверка закончена. Закройте приложение через его меню трея → Выход, затем нажмите Enter.'
if ($appProcess.WaitForExit(5000)) {
    Write-Stage "APP END exit=$($appProcess.ExitCode)"
} else {
    Write-Stage 'Приложение всё ещё работает. Завершите его через меню трея → Выход.'
}
Write-Stage "Готово. Пришлите путь к папке: $checkDir. Ответы и журналы уже сохранены."
