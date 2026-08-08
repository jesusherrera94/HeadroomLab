# Builds the Windows release archive.
#
#   powershell -ExecutionPolicy Bypass -File packaging\windows\bundle.ps1
#
# Output: target\release\bundle\HeadroomLab-<version>-windows-<arch>.zip
#
# The icon is not packaged here — build.rs embeds AppIcon.ico into the .exe as a
# Win32 resource, so the executable carries its own icon wherever it is copied.

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location $root

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.*)"').Matches[0].Groups[1].Value
$arch = if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "aarch64" } else { "x86_64" }
$target = "windows-$arch"
$out = "target\release\bundle"
$archive = "$out\HeadroomLab-$version-$target.zip"

Write-Host "==> Building HeadroomLab $version for $target"
$env:HL_UPDATE_ENABLED = "1"
cargo build --release
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

Write-Host "==> Packing $archive"
New-Item -ItemType Directory -Force -Path $out | Out-Null
Remove-Item -Force -ErrorAction SilentlyContinue $archive
Compress-Archive -Path "target\release\HeadroomLab.exe" -DestinationPath $archive

Write-Host ""
Write-Host "Done: $archive"
Write-Host "Upload it to the v$version release."
