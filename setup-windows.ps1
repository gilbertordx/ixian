# One-shot setup for Windows: installs what's missing, gets your projects, builds Launchpad,
# and puts a Launchpad icon on your Desktop. Safe to run again.
$ErrorActionPreference = "Stop"
$here = $PSScriptRoot
function Step($t) { Write-Host "`n=== $t ===" -ForegroundColor Cyan }
function Have($c) { [bool](Get-Command $c -ErrorAction SilentlyContinue) }
function RefreshPath {
    $env:Path = [Environment]::GetEnvironmentVariable("Path","Machine") + ";" +
                [Environment]::GetEnvironmentVariable("Path","User") + ";" + "$HOME\.cargo\bin"
}
function WingetInstall($id, $extra) {
    if (-not (Have winget)) { throw "winget not found. Update 'App Installer' from the Microsoft Store, then run this again." }
    $a = @("install","-e","--id",$id,"--accept-source-agreements","--accept-package-agreements")
    if ($extra) { $a += $extra }
    & winget @a
}

Step "1/6  Git"
RefreshPath
if (-not (Have git)) { WingetInstall "Git.Git"; RefreshPath }
if (-not (Have git)) { throw "Git did not install. Install it from https://git-scm.com then run this again." }
git --version

Step "2/6  C++ build tools (Rust needs a linker on Windows)"
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$hasVC = (Test-Path $vswhere) -and (& $vswhere -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath)
if ($hasVC) { Write-Host "already installed" }
else {
    Write-Host "installing (this is big and can take 10+ minutes; accept the admin prompt)..."
    WingetInstall "Microsoft.VisualStudio.2022.BuildTools" @("--override","--quiet --wait --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended")
}

Step "3/6  Rust"
RefreshPath
if (-not (Have cargo)) { WingetInstall "Rustlang.Rustup"; RefreshPath }
if (-not (Have rustup)) { throw "Rust did not install. Install it from https://rustup.rs then run this again." }
rustup default stable
rustup target add wasm32-unknown-unknown
cargo --version

Step "4/6  Your projects (cloned to $HOME\projects)"
$projects = Join-Path $HOME "projects"
New-Item -ItemType Directory -Force $projects | Out-Null
foreach ($name in "tbjp-oracle","logbook","rhcp-chatbot") {
    $dest = Join-Path $projects $name
    if (Test-Path $dest) { Write-Host "$name already there - updating"; git -C $dest pull --ff-only; continue }

    git clone "https://github.com/gilbertordx/$name" $dest
    if ($LASTEXITCODE -ne 0) { Write-Host "could not clone $name (private repo? sign in to GitHub when asked) - continuing" -ForegroundColor Yellow }
}

Step "5/6  Build and install Launchpad (a few minutes)"
Push-Location $here
cargo install --path . --locked --force
Pop-Location
RefreshPath
if (-not (Have launchpad)) { throw "launchpad did not install - scroll up for the error." }

Step "6/6  Desktop icon"
& powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $here "shortcut\create-desktop-shortcut.ps1")

Step "Check"
launchpad --check

Write-Host "`nDone. Double-click Launchpad on your Desktop, or type: launchpad" -ForegroundColor Green
Write-Host "Optional, for Logbook's web UI:  cargo install trunk --locked   (long build)"
