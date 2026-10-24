# Builds Forma for Windows (x64): a portable .zip and, when Inno Setup 6 is installed, an
# installer. Both land in target\windows\.
#
# Needs Rust (MSVC toolchain) and, for the installer, Inno Setup 6
# (winget install JRSoftware.InnoSetup). libforma must be cloned next to forma-desktop.
param(
    # Skip the installer even when Inno Setup is available.
    [switch]$ZipOnly,
    # Path to ISCC.exe, when Inno Setup is not on PATH or in its default folder.
    [string]$Iscc
)
$ErrorActionPreference = 'Stop'

$here = $PSScriptRoot
$desktop = (Resolve-Path (Join-Path $here '..\..')).Path
$version = (Select-String -Path (Join-Path $desktop 'Cargo.toml') -Pattern '^version = "(.+)"$' |
    Select-Object -First 1).Matches[0].Groups[1].Value
$out = Join-Path $desktop 'target\windows'
$name = "Forma-$version-windows-x64"
$stage = Join-Path $out $name

Write-Host "Building Forma $version..."
cargo build --release --locked --manifest-path (Join-Path $desktop 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item (Join-Path $desktop 'target\release\forma.exe') $stage
Copy-Item (Join-Path $desktop 'LICENSE') (Join-Path $stage 'LICENSE.txt')
Copy-Item (Join-Path $desktop 'assets\fonts\Inter-OFL.txt') $stage
# Templates installed with Forma go in this folder, next to forma.exe.
$templates = Join-Path $desktop 'templates'
if (Test-Path $templates) { Copy-Item -Recurse $templates (Join-Path $stage 'templates') }

$zip = Join-Path $out "$name.zip"
if (Test-Path $zip) { Remove-Item $zip }
Compress-Archive -Path $stage -DestinationPath $zip
Write-Host "Portable: $zip"

if ($ZipOnly) { return }
$iscc = @(
    $Iscc,
    (Get-Command ISCC.exe -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source),
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
if (-not $iscc) {
    Write-Warning 'Inno Setup 6 not found; skipping the installer (winget install JRSoftware.InnoSetup).'
    return
}
& $iscc /Q "/DAppVersion=$version" "/DStageDir=$stage" "/DOutputDir=$out" (Join-Path $here 'forma.iss')
if ($LASTEXITCODE -ne 0) { throw 'Inno Setup failed' }
Write-Host "Installer: $(Join-Path $out "$name-setup.exe")"
