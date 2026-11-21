# Puts a "Sietch" shortcut on your Desktop that opens the menu in a terminal window.
$bin = (Get-Command sietch -ErrorAction SilentlyContinue).Source
if (-not $bin) { Write-Host "sietch is not installed yet: run  cargo install --path .  first"; exit 1 }
$desktop = [Environment]::GetFolderPath("Desktop")
Remove-Item "$desktop\Launchpad.lnk" -ErrorAction SilentlyContinue   # the old name
$lnk = (New-Object -ComObject WScript.Shell).CreateShortcut("$desktop\Sietch.lnk")
$lnk.TargetPath = $bin
$lnk.WorkingDirectory = $env:USERPROFILE
$lnk.Save()
Write-Host "Created $desktop\Sietch.lnk"
