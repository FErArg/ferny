<#
.SYNOPSIS
  Install Ferny development tools on Windows 11 (x64) with WinGet.

.DESCRIPTION
  Installs: Git for Windows (provides the bash used by the Git hooks),
  gitleaks, rustup + stable MSVC toolchain (rustfmt, clippy), Visual Studio
  2022 Build Tools (C++ workload), CMake, Ninja, LLVM (clang-format),
  PowerShell 7, Python, cargo-deny, cargo-audit and Qt 6.12.0 for MSVC 2022
  (through aqtinstall 3.3.0, which verifies the hashes published by the Qt
  repository) in C:\Qt. Versions: docs/DEPENDENCIES.md. Fuzzing is not
  supported on Windows.

  Uses WinGet only (official packages). Already installed packages are
  skipped. Every winget and cargo exit code is checked; the script ends with
  exit code 1 if anything failed, and reports the version of each tool.

  Compatible with Windows PowerShell 5.1 and PowerShell 7. Some installers
  (Visual Studio Build Tools, LLVM) ask for administrator confirmation.

.PARAMETER DryRun
  Print what would be done, change nothing.

.PARAMETER WithSemgrep
  Also install Python and Semgrep (optional on Windows: SAST already runs on
  macOS and Linux).

.PARAMETER WithReleaseTools
  Also install minisign and syft (release builds).

.PARAMETER UpdateUserPath
  Persist the tool directories (cargo, CMake, LLVM) in the user PATH.

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File docs\scripts\Install-DevTools.ps1
.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File docs\scripts\Install-DevTools.ps1 -DryRun
#>
[CmdletBinding()]
param(
  [switch]$DryRun,
  [switch]$WithSemgrep,
  [switch]$WithReleaseTools,
  [switch]$UpdateUserPath,
  [string]$QtVersion = '6.12.0',
  [string]$QtArch = 'win64_msvc2022_64',
  [string]$QtRoot = 'C:\Qt',
  [string]$AqtVersion = '3.3.0'
)

$ErrorActionPreference = 'Continue'
Set-StrictMode -Version 2.0
$script:failures = 0

function Say([string]$msg) { Write-Host ''; Write-Host "==> $msg" }
function Note([string]$msg) { Write-Host "    $msg" }

# WinGet exit codes that mean "nothing to do".
$okCodes = @(0, -1978335189, -1978335135)  # success, no applicable upgrade, already installed

function Invoke-Step([string]$exe, [string[]]$arguments, [int[]]$accept = @(0)) {
  $line = "$exe " + ($arguments -join ' ')
  if ($DryRun) { Note "[dry-run] $line"; return $true }
  Note "> $line"
  # Out-Host keeps the tool's output out of the function's return value.
  & $exe @arguments | Out-Host
  if ($accept -contains $LASTEXITCODE) { return $true }
  Write-Host "    !! failed (exit $LASTEXITCODE): $line" -ForegroundColor Red
  $script:failures++
  return $false
}

function Test-WingetInstalled([string]$id) {
  & winget list --id $id --exact --accept-source-agreements *> $null
  return ($LASTEXITCODE -eq 0)
}

function Install-WingetPackage([string]$id, [string[]]$extra = @()) {
  if (Test-WingetInstalled $id) { Note "ok  $id (already installed)"; return }
  $wingetArgs = @('install', '--id', $id, '--exact', '--silent',
    '--accept-package-agreements', '--accept-source-agreements') + $extra
  if (-not (Invoke-Step 'winget' $wingetArgs $okCodes)) {
    Note "   if the id is unknown, look it up with: winget search $id"
  }
}

function Update-SessionPath {
  $machine = [Environment]::GetEnvironmentVariable('Path', 'Machine')
  $user = [Environment]::GetEnvironmentVariable('Path', 'User')
  $env:Path = "$machine;$user"
  foreach ($dir in $script:toolDirs) {
    if ((Test-Path $dir) -and (($env:Path -split ';') -notcontains $dir)) { $env:Path = "$dir;$env:Path" }
  }
}

$qtDir = Join-Path $QtRoot (Join-Path $QtVersion ($QtArch -replace '^win64_', ''))
$script:toolDirs = @(
  (Join-Path $env:USERPROFILE '.cargo\bin'),
  (Join-Path $qtDir 'bin'),
  (Join-Path $env:ProgramFiles 'CMake\bin'),
  (Join-Path $env:ProgramFiles 'LLVM\bin')
)

# --- Checks ---------------------------------------------------------------
if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
  Write-Error 'WinGet is required (App Installer from the Microsoft Store).'
  exit 1
}
if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64') {
  Note "warning: tested on x64; found $($env:PROCESSOR_ARCHITECTURE)"
}
Say ("Platform: Windows {0}, {1}{2}" -f [Environment]::OSVersion.Version, $env:PROCESSOR_ARCHITECTURE, $(if ($DryRun) { ', dry run' } else { '' }))

# --- WinGet packages ------------------------------------------------------
Say 'WinGet packages'
Install-WingetPackage 'Git.Git'
Install-WingetPackage 'Gitleaks.Gitleaks'
Install-WingetPackage 'Kitware.CMake'
Install-WingetPackage 'Ninja-build.Ninja'
Install-WingetPackage 'LLVM.LLVM'
Install-WingetPackage 'Microsoft.PowerShell'
Install-WingetPackage 'Microsoft.VisualStudio.2022.BuildTools' @('--override',
  '--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended')
Install-WingetPackage 'Rustlang.Rustup'
Install-WingetPackage 'Python.Python.3.12'
if ($WithReleaseTools) {
  Install-WingetPackage 'jedisct1.minisign'
  Install-WingetPackage 'Anchore.Syft'
}

Update-SessionPath

# --- Rust toolchain and cargo tools ---------------------------------------
Say 'Rust stable toolchain (MSVC)'
if (-not $DryRun -and -not (Get-Command rustup -ErrorAction SilentlyContinue)) {
  Write-Host '    !! rustup not found after installation; open a new terminal and re-run' -ForegroundColor Red
  $script:failures++
} else {
  [void](Invoke-Step 'rustup' @('toolchain', 'install', 'stable', '--profile', 'minimal', '--component', 'rustfmt,clippy'))
  [void](Invoke-Step 'rustup' @('default', 'stable'))

  Say 'Cargo tools'
  foreach ($pair in @(@('cargo-deny', $env:CARGO_DENY_VERSION), @('cargo-audit', $env:CARGO_AUDIT_VERSION))) {
    $name = $pair[0]; $ver = $pair[1]
    if (-not $DryRun -and -not $ver -and (Get-Command $name -ErrorAction SilentlyContinue)) {
      Note "ok  $name (already installed)"; continue
    }
    $cargoArgs = @('install', '--locked', $name)
    if ($ver) { $cargoArgs += @('--version', $ver) }
    [void](Invoke-Step 'cargo' $cargoArgs)
  }
}

Say "Qt $QtVersion ($QtArch)"
# Core5Compat (module qt5compat) is required by Scintilla's Qt layer (QTextCodec).
if ((Test-Path (Join-Path $qtDir 'bin\qmake.exe')) -and (Test-Path (Join-Path $qtDir 'lib\cmake\Qt6Core5Compat'))) {
  Note "ok  Qt $QtVersion with Core5Compat (already in $qtDir)"
} elseif (-not $DryRun -and -not (Get-Command py -ErrorAction SilentlyContinue)) {
  Write-Host '    !! Python launcher (py) not found; open a new terminal and re-run' -ForegroundColor Red
  $script:failures++
} else {
  if (Invoke-Step 'py' @('-m', 'pip', 'install', '--user', "aqtinstall==$AqtVersion")) {
    [void](Invoke-Step 'py' @('-m', 'aqt', 'install-qt', 'windows', 'desktop', $QtVersion, $QtArch, '-m', 'qt5compat', '--outputdir', $QtRoot))
  }
}

if ($WithSemgrep) {
  Say 'Semgrep (pipx)'
  if (Get-Command semgrep -ErrorAction SilentlyContinue) { Note 'ok  semgrep (already installed)' }
  else {
    [void](Invoke-Step 'py' @('-m', 'pip', 'install', '--user', 'pipx'))
    [void](Invoke-Step 'py' @('-m', 'pipx', 'install', 'semgrep'))
  }
}

# --- PATH ---------------------------------------------------------------------
if ($UpdateUserPath) {
  Say 'CMAKE_PREFIX_PATH (lets CMake find Qt)'
  if ($DryRun) { Note "[dry-run] set user variable CMAKE_PREFIX_PATH=$qtDir" }
  elseif ([Environment]::GetEnvironmentVariable('CMAKE_PREFIX_PATH', 'User') -eq $qtDir) { Note 'ok  already set' }
  else { [Environment]::SetEnvironmentVariable('CMAKE_PREFIX_PATH', $qtDir, 'User'); Note "set to $qtDir" }

  Say 'User PATH'
  $user = [Environment]::GetEnvironmentVariable('Path', 'User')
  $parts = @($user -split ';' | Where-Object { $_ })
  $added = @()
  foreach ($dir in $script:toolDirs) {
    if ((Test-Path $dir) -and ($parts -notcontains $dir)) { $parts += $dir; $added += $dir }
  }
  if ($added.Count -eq 0) { Note 'ok  nothing to add' }
  elseif ($DryRun) { Note ("[dry-run] add to user PATH: " + ($added -join ', ')) }
  else {
    [Environment]::SetEnvironmentVariable('Path', ($parts -join ';'), 'User')
    Note ("added: " + ($added -join ', ') + ' (open a new terminal)')
  }
}

# --- Report -------------------------------------------------------------------
Say 'Installed versions'
function Show-Tool([string]$name, [string[]]$arguments) {
  $cmd = Get-Command $name -ErrorAction SilentlyContinue
  if ($cmd) {
    $v = & $name @arguments 2>$null | Select-Object -First 1
    Write-Host ('    {0,-13} {1}' -f $name, $v)
  } else {
    Write-Host ('    {0,-13} missing' -f $name)
  }
}
Show-Tool 'git' @('--version')
Show-Tool 'gitleaks' @('version')
Show-Tool 'rustup' @('--version')
Show-Tool 'cargo' @('--version')
Show-Tool 'cargo-deny' @('--version')
Show-Tool 'cargo-audit' @('--version')
Show-Tool 'cmake' @('--version')
Show-Tool 'ninja' @('--version')
Show-Tool 'clang-format' @('--version')
Show-Tool 'pwsh' @('--version')
Show-Tool 'qmake' @('--version')
if ($WithSemgrep) { Show-Tool 'semgrep' @('--version') }
if ($WithReleaseTools) { Show-Tool 'minisign' @('-v'); Show-Tool 'syft' @('version') }
Note 'MSVC (cl.exe) is used from a "Developer PowerShell for VS 2022" or through CMake presets.'

if ($script:failures -gt 0) {
  Say "Finished with $($script:failures) failure(s); see the messages above."
  exit 1
}
Say 'Done. Verify with: powershell -NoProfile -ExecutionPolicy Bypass -File scripts\check.ps1'
exit 0
