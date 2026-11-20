# Puts a "Launchpad" shortcut on your Desktop that opens the menu in a terminal window.
$bin = (Get-Command launchpad -ErrorAction SilentlyContinue).Source
if (-not $bin) { Write-Host "launchpad is not installed yet: run  cargo install --path .  first"; exit 1 }
$desktop = [Environment]::GetFolderPath("Desktop")
$lnk = (New-Object -ComObject WScript.Shell).CreateShortcut("$desktop\Launchpad.lnk")
$lnk.TargetPath = $bin
$lnk.WorkingDirectory = $env:USERPROFILE
$lnk.Save()
Write-Host "Created $desktop\Launchpad.lnk"
