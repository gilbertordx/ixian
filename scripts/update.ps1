# Rebuilds Sietch from this folder and refreshes the Desktop icon so both stay in sync.
# Works even while Sietch is open: Windows lets a running .exe be renamed, not overwritten.
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root
if (Test-Path (Join-Path $root ".git")) { git pull --ff-only }
$exe = Join-Path $HOME ".cargo\bin\sietch.exe"
if (Test-Path $exe) {
    Remove-Item "$exe.old" -Force -ErrorAction SilentlyContinue
    Rename-Item $exe "sietch.exe.old" -ErrorAction SilentlyContinue
}
cargo install --path . --locked --force
if ($LASTEXITCODE -ne 0) {
    if (Test-Path "$exe.old") { Move-Item "$exe.old" $exe -Force }
    throw "build failed - the previous version was put back"
}
& powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $root "shortcut\create-desktop-shortcut.ps1")
Write-Host "`nUpdated. Quit Sietch (q) and open it again to use the new version." -ForegroundColor Green
