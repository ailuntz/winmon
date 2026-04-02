[CmdletBinding(PositionalBinding = $false)]
param(
  [switch]$Release,
  [string]$TargetDir,
  [Parameter(Position = 0, ValueFromRemainingArguments = $true)]
  [string[]]$AppArgs
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path $PSScriptRoot -Parent
$vsRoot = "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Tools\MSVC"
$sdkRoot = "C:\Program Files (x86)\Windows Kits\10"

function Find-CargoBin {
  $cmd = Get-Command cargo.exe -ErrorAction SilentlyContinue
  if ($cmd) {
    return Split-Path $cmd.Source -Parent
  }

  $candidates = @()

  if ($env:USERPROFILE) {
    $candidates += (Join-Path $env:USERPROFILE ".cargo\bin")
  }

  if ($env:HOME) {
    $candidates += (Join-Path $env:HOME ".cargo\bin")
  }

  $candidates += Get-ChildItem "C:\Users" -Directory -ErrorAction SilentlyContinue |
    ForEach-Object { Join-Path $_.FullName ".cargo\bin" }

  foreach ($bin in ($candidates | Where-Object { $_ } | Select-Object -Unique)) {
    if (Test-Path (Join-Path $bin "cargo.exe")) {
      return $bin
    }
  }

  throw "cargo.exe not found. Please install Rust toolchain first."
}

$cargoBin = Find-CargoBin

if (!(Test-Path $vsRoot)) {
  throw "MSVC tools not found: $vsRoot"
}

if (!(Test-Path (Join-Path $sdkRoot "Lib"))) {
  throw "Windows SDK not found: $sdkRoot"
}

$vcDir = Get-ChildItem $vsRoot -Directory | Sort-Object Name -Descending | Select-Object -First 1
$sdkVer = Get-ChildItem (Join-Path $sdkRoot "Lib") -Directory | Sort-Object Name -Descending | Select-Object -First 1

if ($null -eq $vcDir) {
  throw "No usable MSVC version directory found"
}

if ($null -eq $sdkVer) {
  throw "No usable Windows SDK version directory found"
}

$vcPath = $vcDir.FullName
$sdkLib = Join-Path (Join-Path $sdkRoot "Lib") $sdkVer.Name
$sdkInc = Join-Path (Join-Path $sdkRoot "Include") $sdkVer.Name

$env:PATH = "$($vcPath)\bin\Hostx64\x64;$cargoBin;$env:PATH"
$env:LIB = "$($vcPath)\lib\x64;$sdkLib\um\x64;$sdkLib\ucrt\x64"
$env:INCLUDE = "$($vcPath)\include;$sdkInc\ucrt;$sdkInc\shared;$sdkInc\um;$sdkInc\winrt;$sdkInc\cppwinrt"

Set-Location $repoRoot

$cargoArgs = @("run")
if ($Release) {
  $cargoArgs += "--release"
}

if ($TargetDir) {
  $cargoArgs += @("--target-dir", $TargetDir)
}

if ($AppArgs.Count -gt 0) {
  $cargoArgs += "--"
  $cargoArgs += $AppArgs
}

& (Join-Path $cargoBin "cargo.exe") @cargoArgs
