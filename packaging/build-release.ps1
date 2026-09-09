#Requires -Version 7.0
<#
.SYNOPSIS
    Builds every Windows release artifact for lang-switcher.

.DESCRIPTION
    One script so that a local run and the release workflow produce the same files from
    the same commit: an installer, a portable archive, a third-party licence notice and a
    checksum file. Nothing here reads the network beyond what `cargo build --locked`
    already resolved.

    Requires the MSVC toolchain, Python 3.11 or newer (for tomllib) and Inno Setup 6.3 or
    newer. Inno Setup is preinstalled on GitHub's windows runners.

.PARAMETER Version
    Version to stamp on the artifacts. Defaults to the workspace version in Cargo.toml,
    which is the value the binary itself reports.

.PARAMETER SkipInstaller
    Produce only the portable archive. Useful on a machine without Inno Setup.

.EXAMPLE
    pwsh -File packaging/build-release.ps1
#>
[CmdletBinding()]
param(
    [string]$Version,
    [switch]$SkipInstaller
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $PSScriptRoot
$Target = 'x86_64-pc-windows-msvc'
$BuildDir = "target/$Target/release"
$StageDir = 'target/packaging'
$PortableDir = "$StageDir/portable"

function Invoke-Checked {
    param([string]$Executable, [string[]]$Arguments)
    Write-Host "==> $Executable $($Arguments -join ' ')" -ForegroundColor Cyan
    & $Executable @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Executable exited with $LASTEXITCODE"
    }
}

Push-Location $Root
try {
    if (-not $Version) {
        # The single source of truth: workspace.package.version, which is also what the
        # binary's VERSIONINFO carries.
        $manifest = Get-Content 'Cargo.toml' -Raw
        if ($manifest -notmatch '(?m)^version\s*=\s*"([^"]+)"') {
            throw 'could not read workspace.package.version from Cargo.toml'
        }
        $Version = $Matches[1]
    }
    Write-Host "Building lang-switcher $Version for $Target" -ForegroundColor Green

    # A stale staging directory would silently ship last run's files.
    if (Test-Path $StageDir) { Remove-Item $StageDir -Recurse -Force }
    # The archive holds a single top-level directory, so unpacking it into a Downloads
    # folder cannot scatter eight loose files next to whatever is already there.
    $PortableRoot = "$PortableDir/lang-switcher-$Version"
    New-Item -ItemType Directory -Path $PortableRoot -Force | Out-Null

    Invoke-Checked 'rustup' @('target', 'add', $Target)
    Invoke-Checked 'cargo' @(
        'build', '--release', '--locked', '--target', $Target, '-p', 'switcher-app'
    )

    $exe = "$BuildDir/lang-switcher.exe"
    if (-not (Test-Path $exe)) { throw "build produced no $exe" }
    $stamped = (Get-Item $exe).VersionInfo.ProductVersion
    if ($stamped -ne $Version) {
        throw "the binary reports version '$stamped' but '$Version' was requested"
    }

    Invoke-Checked 'python' @(
        'packaging/collect-licenses.py', '--output', "$StageDir/THIRD-PARTY-LICENSES.md"
    )

    # Portable archive: the executable plus everything a user needs to know what they
    # downloaded. It runs from any directory and stores its settings in AppData, exactly
    # like the installed copy.
    $payload = @(
        $exe,
        'LICENSE-MIT',
        'LICENSE-APACHE',
        "$StageDir/THIRD-PARTY-LICENSES.md",
        'README.md',
        'README.en.md',
        'docs/guide/installation.ru.md',
        'docs/guide/installation.en.md'
    )
    foreach ($file in $payload) {
        if (-not (Test-Path $file)) { throw "missing release payload file: $file" }
        Copy-Item $file -Destination $PortableRoot
    }
    $zip = "$StageDir/lang-switcher-$Version-windows-x64-portable.zip"
    Compress-Archive -Path $PortableRoot -DestinationPath $zip -CompressionLevel Optimal

    if (-not $SkipInstaller) {
        $iscc = Get-Command 'ISCC.exe' -ErrorAction SilentlyContinue
        if (-not $iscc) {
            foreach ($candidate in @(
                    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
                    "$env:ProgramFiles\Inno Setup 6\ISCC.exe")) {
                if (Test-Path $candidate) { $iscc = Get-Item $candidate; break }
            }
        }
        if (-not $iscc) {
            throw 'ISCC.exe not found. Install Inno Setup 6, or pass -SkipInstaller.'
        }
        Invoke-Checked $iscc.Source @(
            "/DAppVersion=$Version",
            "/DBuildDir=$($BuildDir -replace '/', '\')",
            "/DStageDir=$($StageDir -replace '/', '\')",
            'packaging\windows\lang-switcher.iss'
        )
    }

    # Checksums are the only integrity signal an unsigned alpha has, so they are computed
    # from the files that will actually be uploaded, after everything else is done.
    Remove-Item $PortableDir -Recurse -Force
    $artifacts = Get-ChildItem $StageDir -File |
        Where-Object { $_.Extension -in '.exe', '.zip' } |
        Sort-Object Name
    if (-not $artifacts) { throw 'no artifacts were produced' }
    $sums = foreach ($artifact in $artifacts) {
        $hash = (Get-FileHash $artifact.FullName -Algorithm SHA256).Hash.ToLower()
        "$hash  $($artifact.Name)"
    }
    # Two spaces between hash and name, LF endings: the format `sha256sum -c` reads.
    [System.IO.File]::WriteAllText(
        (Join-Path $Root "$StageDir/SHA256SUMS.txt"),
        (($sums -join "`n") + "`n")
    )

    Write-Host "`nArtifacts in $StageDir :" -ForegroundColor Green
    Get-ChildItem $StageDir -File | ForEach-Object {
        '{0,12:N0}  {1}' -f $_.Length, $_.Name
    }
}
finally {
    Pop-Location
}
