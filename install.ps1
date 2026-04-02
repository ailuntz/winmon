param(
  [string]$Repo = "ailuntz/winmon",
  [switch]$KeepTemp,
  [switch]$DryRun
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$baseUrl = "https://github.com/$Repo/releases/latest/download"
$zipUrl = "$baseUrl/winmon-windows-x64.zip"
$hashUrl = "$baseUrl/winmon-windows-x64.zip.sha256"
$tempRoot = Join-Path $env:TEMP ("winmon-install-" + [guid]::NewGuid().ToString("N"))
$zipPath = Join-Path $tempRoot "winmon-windows-x64.zip"
$hashPath = Join-Path $tempRoot "winmon-windows-x64.zip.sha256"
$extractDir = Join-Path $tempRoot "payload"

function Invoke-WinmonRequest {
  param(
    [Parameter(Mandatory = $true)]
    [string]$Uri,
    [string]$OutFile
  )

  $params = @{ Uri = $Uri }
  if ($PSVersionTable.PSVersion.Major -lt 6) {
    $params.UseBasicParsing = $true
  }
  if ($OutFile) {
    $params.OutFile = $OutFile
  }

  Invoke-WebRequest @params
}

if ($DryRun) {
  Write-Output "zip:  $zipUrl"
  Write-Output "hash: $hashUrl"
  exit 0
}

New-Item -ItemType Directory -Force $tempRoot | Out-Null

try {
  Invoke-WinmonRequest -Uri $zipUrl -OutFile $zipPath | Out-Null
  Invoke-WinmonRequest -Uri $hashUrl -OutFile $hashPath | Out-Null
  $hashText = (Get-Content -Path $hashPath -Raw).Trim()
  $expectedHash = ($hashText -split '\s+')[0].ToLower()
  $actualHash = (Get-FileHash -Algorithm SHA256 $zipPath).Hash.ToLower()
  if ($expectedHash -ne $actualHash) {
    throw "sha256 mismatch: expected $expectedHash got $actualHash"
  }

  Expand-Archive -Path $zipPath -DestinationPath $extractDir -Force

  $exe = Join-Path $extractDir "winmon.exe"
  if (!(Test-Path $exe)) {
    throw "winmon.exe not found in package"
  }

  & $exe bootstrap

  $stableDir = Join-Path $env:APPDATA "winmon"
  $stableExe = Join-Path $stableDir "winmon.exe"

  Write-Output "installed: $stableDir"
  if (Test-Path $stableExe) {
    Write-Output "run now: $stableExe"
  }
  Write-Output "open a new terminal and run: winmon"
}
finally {
  if (!$KeepTemp) {
    Remove-Item $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
  }
}
