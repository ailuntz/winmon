param(
  [Parameter(Mandatory = $true)]
  [string]$Version,

  [Parameter(Mandatory = $true)]
  [string]$InstallerSha256,

  [string]$Repo = "ailuntz/winmon",
  [string]$PackageIdentifier = "Ailuntz.Winmon",
  [string]$Publisher = "ailuntz",
  [string]$PackageName = "winmon",
  [string]$Moniker = "winmon",
  [string]$ManifestVersion = "1.12.0",
  [string]$OutRoot = "winget/manifests"
)

$ErrorActionPreference = "Stop"

$root = Split-Path $PSScriptRoot -Parent
$parts = $PackageIdentifier.Split(".")
if ($parts.Count -ne 2) {
  throw "PackageIdentifier must be in Publisher.Package form"
}

$publisherName = $parts[0]
$appName = $parts[1]
$firstLetter = $publisherName.Substring(0, 1).ToLower()
$manifestDir = Join-Path $root $OutRoot
$manifestDir = Join-Path $manifestDir $firstLetter
$manifestDir = Join-Path $manifestDir $publisherName
$manifestDir = Join-Path $manifestDir $appName
$manifestDir = Join-Path $manifestDir $Version

$repoUrl = "https://github.com/$Repo"
$installerUrl = "$repoUrl/releases/download/v$Version/winmon-windows-x64.zip"
$licenseUrl = "$repoUrl/blob/main/LICENSE"
$publisherUrl = "https://github.com/$Publisher"
$supportUrl = "$repoUrl/issues"

New-Item -ItemType Directory -Force $manifestDir | Out-Null

$versionYaml = @"
# yaml-language-server: `$schema=https://aka.ms/winget-manifest.version.$ManifestVersion.schema.json

PackageIdentifier: $PackageIdentifier
PackageVersion: $Version
DefaultLocale: en-US
ManifestType: version
ManifestVersion: $ManifestVersion
"@

$localeYaml = @"
# yaml-language-server: `$schema=https://aka.ms/winget-manifest.defaultLocale.$ManifestVersion.schema.json

PackageIdentifier: $PackageIdentifier
PackageVersion: $Version
PackageLocale: en-US
Publisher: $Publisher
PublisherUrl: $publisherUrl
PublisherSupportUrl: $supportUrl
Author: $Publisher
PackageName: $PackageName
PackageUrl: $repoUrl
License: MIT
LicenseUrl: $licenseUrl
ShortDescription: Windows terminal hardware monitor
Description: Terminal hardware monitor for Windows. Starts a TUI by default and also supports pipe and debug modes.
Moniker: $Moniker
Tags:
- monitor
- terminal
- cli
- hardware
ManifestType: defaultLocale
ManifestVersion: $ManifestVersion
"@

$installerYaml = @"
# yaml-language-server: `$schema=https://aka.ms/winget-manifest.installer.$ManifestVersion.schema.json

PackageIdentifier: $PackageIdentifier
PackageVersion: $Version
MinimumOSVersion: 10.0.17763.0
InstallerType: zip
NestedInstallerType: portable
NestedInstallerFiles:
- RelativeFilePath: winmon.exe
  PortableCommandAlias: winmon
Installers:
- Architecture: x64
  InstallerUrl: $installerUrl
  InstallerSha256: $InstallerSha256
ManifestType: installer
ManifestVersion: $ManifestVersion
"@

Set-Content -Path (Join-Path $manifestDir "$PackageIdentifier.yaml") -Value $versionYaml -Encoding utf8
Set-Content -Path (Join-Path $manifestDir "$PackageIdentifier.locale.en-US.yaml") -Value $localeYaml -Encoding utf8
Set-Content -Path (Join-Path $manifestDir "$PackageIdentifier.installer.yaml") -Value $installerYaml -Encoding utf8

Write-Output $manifestDir
