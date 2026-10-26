<#+
.SYNOPSIS
  Install Ferny development tools for Windows 11 x64.
.DESCRIPTION
  Installs prerequisites used by the existing Windows development workflow.
  The Windows MSI is built by scripts/package-msi.ps1 (see packaging/README.md).
#>
[CmdletBinding()]
param(
  [switch]$DryRun,
  [switch]$WithReleaseTools,
  [switch]$UpdateUserPath,
  [string]$QtVersion = '6.12.0',
  [string]$QtArch = 'win64_msvc2022_64',
  [string]$QtRoot = 'C:\Qt',
  [string]$AqtVersion = '3.3.0'
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2.0

function Invoke-Step([string]$Exe, [string[]]$Arguments) {
  if ($DryRun) { Write-Host ('[dry-run] ' + $Exe + ' ' + ($Arguments -join ' ')); return }
  & $Exe @Arguments
  if ($LASTEXITCODE -ne 0) { throw "$Exe failed with exit code $LASTEXITCODE" }
}
function Install-Winget([string]$Id) {
  if (-not $DryRun) {
    & winget list --id $Id --exact --accept-source-agreements *> $null
    if ($LASTEXITCODE -eq 0) { return }
  }
  Invoke-Step 'winget' @('install', '--id', $Id, '--exact', '--silent', '--accept-package-agreements', '--accept-source-agreements')
}

if (-not (Get-Command winget -ErrorAction SilentlyContinue)) { throw 'WinGet required' }
if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64') { throw 'Windows x64 required' }

@('Git.Git','Gitleaks.Gitleaks','Kitware.CMake','Ninja-build.Ninja','LLVM.LLVM',
  'Microsoft.PowerShell','Microsoft.VisualStudio.2022.BuildTools','Rustlang.Rustup',
  'Python.Python.3.12') | ForEach-Object { Install-Winget $_ }
if ($WithReleaseTools) { @('jedisct1.minisign','Anchore.Syft') | ForEach-Object { Install-Winget $_ } }

$qtDir = Join-Path $QtRoot (Join-Path $QtVersion ($QtArch -replace '^win64_', ''))
if (-not (Test-Path (Join-Path $qtDir 'bin\qmake.exe'))) {
  Invoke-Step 'py' @('-m','pip','install','--user',"aqtinstall==$AqtVersion")
  Invoke-Step 'py' @('-m','aqt','install-qt','windows','desktop',$QtVersion,$QtArch,'-m','qt5compat','--outputdir',$QtRoot)
}
$env:Path = "$env:USERPROFILE\.cargo\bin;$qtDir\bin;$env:Path"
Invoke-Step 'rustup' @('toolchain','install','stable','--profile','minimal','--component','rustfmt,clippy')
Invoke-Step 'rustup' @('default','stable')
Invoke-Step 'cargo' @('install','--locked','cargo-deny')
Invoke-Step 'cargo' @('install','--locked','cargo-audit')
Invoke-Step 'cargo' @('fetch','--locked')

if ($UpdateUserPath -and -not $DryRun) {
  [Environment]::SetEnvironmentVariable('CMAKE_PREFIX_PATH', $qtDir, 'User')
  Write-Host "Set user CMAKE_PREFIX_PATH=$qtDir"
}
Write-Host 'Ready for development: scripts\dev.ps1'
Write-Host 'Windows MSI: scripts\package-msi.ps1 (needs WiX Toolset v3.14 and the Qt MSVC kit).'
