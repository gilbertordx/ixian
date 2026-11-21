# Puts a "Ixian" shortcut on your Desktop that opens the menu in a terminal window.
$bin = (Get-Command ixian -ErrorAction SilentlyContinue).Source
if (-not $bin) { Write-Host "ixian is not installed yet: run  cargo install --path .  first"; exit 1 }
$desktop = [Environment]::GetFolderPath("Desktop")
Remove-Item "$desktop\Launchpad.lnk","$desktop\Sietch.lnk" -ErrorAction SilentlyContinue   # the old names
$lnk = (New-Object -ComObject WScript.Shell).CreateShortcut("$desktop\Ixian.lnk")
$lnk.TargetPath = $bin
$lnk.WorkingDirectory = $env:USERPROFILE
$lnk.Save()
Write-Host "Created $desktop\Ixian.lnk"
