#requires -Version 7.0
<#
Compare the same short PCM through the same production stream, with silence
after or before it. These diagnostic variants do not change product settings.
Every variant also receives the production stream's current release tail.
PrepareOnly writes the WAV controls without opening an audio device.
#>
[CmdletBinding()]
param([switch]$PrepareOnly)

$ErrorActionPreference = 'Stop'
$projectDir = Split-Path -Parent $PSScriptRoot
$audioExe = Join-Path $projectDir 'target/release/examples/audio_compare.exe'
if (-not (Test-Path -LiteralPath $audioExe -PathType Leaf)) {
    throw "Не найдена сборка: $audioExe"
}
$checkDir = Join-Path $projectDir ('target/audio-tail-' + (Get-Date -Format 'yyyyMMdd-HHmmssfff'))
New-Item -ItemType Directory -Path $checkDir | Out-Null
[pscustomobject]@{
    StartedUtc=[DateTime]::UtcNow.ToString('o')
    BinarySha256=(Get-FileHash -LiteralPath $audioExe -Algorithm SHA256).Hash
    ProductionTailApplies=$true
} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $checkDir 'run.json') -Encoding utf8NoBOM
$results = [System.Collections.Generic.List[object]]::new()
$cases = @(
    @{Id='E';Label='1. Обычный короткий сигнал, 90 мс'},
    @{Id='B';Label='2. Тот же сигнал, затем 1 секунда тишины до закрытия потока'},
    @{Id='C';Label='3. Сначала 1 секунда тишины, затем тот же сигнал и ещё 1 секунда тишины'}
)
Write-Host "Папка результата: $checkDir"
if (-not $PrepareOnly) {
    Write-Host 'Громкость не меняется. На время серий остановите музыку/видео и не переключайте приложения.'
    Write-Host 'В каждой серии один одинаковый короткий низкий сигнал. Ответьте отдельно по каждой.'
}
foreach ($case in $cases) {
    $wavPath = Join-Path $checkDir ($case.Id + '.wav')
    if (-not $PrepareOnly) { [void](Read-Host ($case.Label + '. Нажмите Enter для запуска')) }
    $startedUtc = [DateTime]::UtcNow.ToString('o')
    $arguments = @($case.Id, ('"' + $wavPath + '"'))
    if ($PrepareOnly) { $arguments += '--prepare-only' }
    $process = $null
    try {
        $process = Start-Process -FilePath $audioExe -WorkingDirectory $projectDir `
            -ArgumentList $arguments -WindowStyle Hidden -PassThru `
            -RedirectStandardOutput (Join-Path $checkDir ($case.Id + '.txt')) `
            -RedirectStandardError (Join-Path $checkDir ($case.Id + '-stderr.txt'))
        if (-not $process.WaitForExit(15000)) { throw "Серия $($case.Id) превысила срок ожидания. См. журнал." }
        if ($process.ExitCode -ne 0) { throw "Серия $($case.Id) завершилась с ошибкой $($process.ExitCode). См. журнал." }
    } finally {
        if ($null -ne $process) {
            if (-not $process.HasExited) { $process.Kill(); [void]$process.WaitForExit(5000) }
            $process.Dispose()
        }
    }
    $observation = if ($PrepareOnly) { 'not played' } else { Read-Host 'Слышали сигнал? Напишите: да / нет / тихо' }
    $results.Add([pscustomobject]@{Case=$case.Id;Label=$case.Label;StartedUtc=$startedUtc;Observation=$observation})
    $results | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $checkDir 'results.json') -Encoding utf8NoBOM
}
Write-Host "Готово. Результаты: $checkDir"
