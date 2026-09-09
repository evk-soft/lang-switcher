#Requires -Version 7.0
<#
.SYNOPSIS
    Installs the built installer, checks the result, then uninstalls and checks the cleanup.

.DESCRIPTION
    Runs against the artifacts packaging/build-release.ps1 produced. Everything here is
    headless, so it also runs on a CI runner: nothing needs a visible desktop.

    What it proves:
      * the installer completes silently and without an elevation prompt;
      * the installed executable is the one that was built, by version resource;
      * the installed executable actually starts and parses its arguments;
      * the recorded checksums match the files that would be uploaded;
      * uninstalling removes the program directory, the Start menu shortcut and the
        autostart registry value, while leaving user settings alone.

.PARAMETER StageDir
    Where the artifacts are. Defaults to target/packaging.
#>
[CmdletBinding()]
param(
    [string]$StageDir = 'target/packaging'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $PSScriptRoot
$InstallDir = Join-Path $env:LOCALAPPDATA 'Programs\lang-switcher'
$RunKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$RunValue = 'lang-switcher'
$Shortcut = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\lang-switcher.lnk'
$SettingsDir = Join-Path $env:APPDATA 'evk-soft\lang-switcher'

$failures = @()
function Check {
    param([string]$Description, [scriptblock]$Condition)
    if (& $Condition) {
        Write-Host "  ok   $Description" -ForegroundColor Green
    }
    else {
        Write-Host "  FAIL $Description" -ForegroundColor Red
        $script:failures += $Description
    }
}

Push-Location $Root
try {
    $setup = Get-ChildItem $StageDir -Filter '*-setup.exe' -File | Select-Object -First 1
    if (-not $setup) { throw "no installer found in $StageDir" }
    Write-Host "Verifying $($setup.Name)" -ForegroundColor Cyan

    Write-Host "`n[checksums]"
    # The sums are only useful if they describe the files that will actually be uploaded.
    foreach ($line in Get-Content "$StageDir/SHA256SUMS.txt") {
        if ($line -notmatch '^([0-9a-f]{64})\s+(.+)$') { throw "malformed sum line: $line" }
        $expected, $name = $Matches[1], $Matches[2]
        $actual = (Get-FileHash "$StageDir/$name" -Algorithm SHA256).Hash.ToLower()
        Check "$name matches its recorded SHA-256" { $actual -eq $expected }
    }

    if (Test-Path $InstallDir) {
        throw "$InstallDir already exists; refusing to test over an existing installation"
    }
    # A pre-existing settings directory must survive; note whether there was one.
    $hadSettings = Test-Path $SettingsDir

    Write-Host "`n[install]"
    $process = Start-Process -FilePath $setup.FullName `
        -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/LOG=target\packaging\install.log' `
        -Wait -PassThru
    Check "the installer exits with 0 (got $($process.ExitCode))" { $process.ExitCode -eq 0 }

    $installed = Join-Path $InstallDir 'lang-switcher.exe'
    Check 'the executable is installed under %LOCALAPPDATA%\Programs' { Test-Path $installed }
    Check 'the licence notice is installed' {
        Test-Path (Join-Path $InstallDir 'THIRD-PARTY-LICENSES.md')
    }
    Check 'a Start menu shortcut exists' { Test-Path $Shortcut }
    Check 'installing did not create an autostart entry' {
        -not (Test-Path $RunKey) -or
        $null -eq (Get-ItemProperty $RunKey -Name $RunValue -ErrorAction SilentlyContinue)
    }

    if (Test-Path $installed) {
        $built = (Get-Item "$StageDir/../x86_64-pc-windows-msvc/release/lang-switcher.exe").VersionInfo.ProductVersion
        $found = (Get-Item $installed).VersionInfo.ProductVersion
        Check "the installed binary is the built one ($found)" { $found -eq $built }

        # Deliberately an invalid argument: the process must start, parse it, and exit
        # non-zero. This exercises the real binary without needing a visible desktop, which
        # is what a tray icon would require.
        $run = Start-Process -FilePath $installed -ArgumentList '--not-an-option' -Wait -PassThru
        Check "the installed binary starts and rejects a bad argument (exit $($run.ExitCode))" {
            $run.ExitCode -ne 0
        }
    }

    Write-Host "`n[uninstall]"
    # Autostart is a tray option, so a fresh install has no Run value. Write one by hand to
    # prove that uninstalling removes whatever the application may have left behind.
    # The key itself may not exist at all on a fresh profile — a CI runner is exactly that
    # — and the application would create it too, through RegCreateKeyExW.
    if (-not (Test-Path $RunKey)) {
        New-Item -Path $RunKey -Force | Out-Null
    }
    New-ItemProperty -Path $RunKey -Name $RunValue -Value "`"$installed`"" `
        -PropertyType String -Force | Out-Null
    Check 'the autostart value under test was written' {
        $null -ne (Get-ItemProperty $RunKey -Name $RunValue -ErrorAction SilentlyContinue)
    }

    $uninstaller = Get-ChildItem $InstallDir -Filter 'unins*.exe' -File | Select-Object -First 1
    Check 'an uninstaller was registered' { $null -ne $uninstaller }
    if ($uninstaller) {
        $process = Start-Process -FilePath $uninstaller.FullName `
            -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART' -Wait -PassThru
        Check "the uninstaller exits with 0 (got $($process.ExitCode))" { $process.ExitCode -eq 0 }
        # Inno's uninstaller detaches a helper to delete its own directory.
        $deadline = (Get-Date).AddSeconds(30)
        while ((Test-Path $InstallDir) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 500 }
    }

    Check 'the program directory is gone' { -not (Test-Path $InstallDir) }
    Check 'the Start menu shortcut is gone' { -not (Test-Path $Shortcut) }
    Check 'the autostart value is gone' {
        -not (Test-Path $RunKey) -or
        $null -eq (Get-ItemProperty $RunKey -Name $RunValue -ErrorAction SilentlyContinue)
    }
    if ($hadSettings) {
        Check 'user settings were left alone' { Test-Path $SettingsDir }
    }

    if ($failures) {
        Write-Host "`n$($failures.Count) check(s) failed:" -ForegroundColor Red
        $failures | ForEach-Object { Write-Host "  - $_" -ForegroundColor Red }
        exit 1
    }
    Write-Host "`nAll installer checks passed." -ForegroundColor Green
}
finally {
    Pop-Location
}
