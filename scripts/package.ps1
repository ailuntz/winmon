param(
  [string]$Version = "dev",
  [string]$TargetDir = "target",
  [string]$OutDir = "dist"
)

$ErrorActionPreference = "Stop"

$root = Split-Path $PSScriptRoot -Parent
$releaseDir = Join-Path $root $TargetDir
$releaseDir = Join-Path $releaseDir "release"
$exe = Join-Path $releaseDir "winmon.exe"
$repoLicense = Join-Path $root "LICENSE"
$ohmNotice = Join-Path $root "third_party\licenses\OHM-NOTICE.txt"
$mplText = Join-Path $root "third_party\licenses\MPL-2.0.txt"
$macmonNotice = Join-Path $root "third_party\licenses\MACMON-NOTICE.txt"
$macmonLicense = Join-Path $root "third_party\licenses\MACMON-MIT.txt"
$bundleName = "winmon-$Version-windows-x64"
$distDir = Join-Path $root $OutDir
$stageDir = Join-Path $distDir $bundleName
$zipPath = Join-Path $distDir "$bundleName.zip"

if (!(Test-Path $exe)) {
  throw "release binary not found: $exe"
}

if (!(Test-Path $repoLicense)) {
  throw "repo license not found: $repoLicense"
}

if (!(Test-Path $ohmNotice)) {
  throw "OHM notice not found: $ohmNotice"
}

if (!(Test-Path $mplText)) {
  throw "MPL text not found: $mplText"
}

if (!(Test-Path $macmonNotice)) {
  throw "macmon notice not found: $macmonNotice"
}

if (!(Test-Path $macmonLicense)) {
  throw "macmon license not found: $macmonLicense"
}

Remove-Item $stageDir -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item $zipPath -Force -ErrorAction SilentlyContinue

New-Item -ItemType Directory -Force $stageDir | Out-Null
New-Item -ItemType Directory -Force (Join-Path $stageDir "third_party\licenses") | Out-Null

Copy-Item $exe (Join-Path $stageDir "winmon.exe")
Copy-Item $repoLicense (Join-Path $stageDir "LICENSE")
Copy-Item $ohmNotice (Join-Path $stageDir "third_party\licenses\OHM-NOTICE.txt")
Copy-Item $mplText (Join-Path $stageDir "third_party\licenses\MPL-2.0.txt")
Copy-Item $macmonNotice (Join-Path $stageDir "third_party\licenses\MACMON-NOTICE.txt")
Copy-Item $macmonLicense (Join-Path $stageDir "third_party\licenses\MACMON-MIT.txt")

@"
winmon package

Requirements:
- Windows 10/11 x64

Run:
- first run winmon.exe from the extracted zip once
- after first run, winmon.exe and helper runtime are cached in %APPDATA%\winmon
- after that, you can move or delete the extracted folder
- new terminals can run winmon directly from PATH

Notes:
- CPU temperature and P/E CPU sensors depend on the embedded OpenHardwareMonitorLib.dll extracted to %APPDATA%\winmon\third_party\ohm
- some sensors may require administrator privileges on some machines
- upstream OHM package: https://www.nuget.org/packages/OpenHardwareMonitorLib/
- winmon.exe is built with static CRT, no separate VC++ redistributable is required
"@ | Set-Content (Join-Path $stageDir "README.txt") -Encoding ASCII

Compress-Archive -Path (Join-Path $stageDir "*") -DestinationPath $zipPath -Force
Write-Output $zipPath
