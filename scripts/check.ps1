<#
.SYNOPSIS
  Ferny local validation gate for Windows (docs/RELEASE-GATE.md).

.DESCRIPTION
  Runs every check that applies to the current state of the repository and
  prints a summary block to paste into milestone reports. A check whose tool
  or inputs do not exist yet is reported as SKIP (not verified), never as PASS.
  Fuzzing is not run on Windows (cargo-fuzz needs macOS or Linux).

  Exit codes: 0 all run checks passed (and nothing skipped with -Strict),
              1 at least one check failed, 2 usage error.

  Compatible with Windows PowerShell 5.1 and PowerShell 7.

.PARAMETER Strict
  Treat skipped checks as failures (use once the code exists).

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts\check.ps1
#>
[CmdletBinding()]
param([switch]$Strict)

$ErrorActionPreference = 'Continue'
Set-StrictMode -Version 2.0

# Always validate the repository this script belongs to, whatever the cwd.
$root = (& git -C $PSScriptRoot rev-parse --show-toplevel 2>$null)
if ($LASTEXITCODE -ne 0 -or -not $root) {
  Write-Error 'check.ps1: the script must live inside the Ferny Git repository'
  exit 2
}
Set-Location $root

$results = New-Object System.Collections.Generic.List[object]
$script:failed = 0
$script:skipped = 0

function Add-Result([string]$status, [string]$name, [string]$detail) {
  $results.Add([pscustomobject]@{ Status = $status; Name = $name; Detail = $detail })
}
function Skip-Check([string]$name, [string]$detail = 'not verified') {
  Add-Result 'SKIP' $name $detail
  $script:skipped++
}
function Test-Tool([string]$name) {
  return [bool](Get-Command $name -ErrorAction SilentlyContinue)
}
function Test-CargoSub([string]$sub) {
  & cargo $sub --version *> $null
  return ($LASTEXITCODE -eq 0)
}

# Run a native command; output is shown only on failure.
function Invoke-Check([string]$name, [string]$exe, [string[]]$arguments) {
  Write-Host ('  {0,-26} ' -f $name) -NoNewline
  $out = & $exe @arguments 2>&1 | Out-String
  if ($LASTEXITCODE -eq 0) {
    Write-Host 'ok'
    Add-Result 'PASS' $name ''
  } else {
    Write-Host 'FAILED'
    ($out -split "`r?`n" | Select-Object -Last 40) | ForEach-Object { Write-Host "      $_" }
    Add-Result 'FAIL' $name 'see output above'
    $script:failed++
  }
}

function Test-HasFiles([string[]]$pathspecs) {
  $files = & git ls-files --cached --others --exclude-standard -- @pathspecs
  return [bool]($files | Select-Object -First 1)
}

Write-Host 'Ferny validation gate'
Write-Host ''

# --- Repository hygiene ---------------------------------------------------
Invoke-Check 'git diff --check' 'git' @('diff', '--check', 'HEAD')

# --- Secrets --------------------------------------------------------------
if (Test-Tool 'gitleaks') {
  Invoke-Check 'gitleaks (history)' 'gitleaks' @('git', '--redact', '--no-banner', '.')
  Invoke-Check 'gitleaks (staged)' 'gitleaks' @('git', '--pre-commit', '--staged', '--redact', '--no-banner', '.')
  Invoke-Check 'gitleaks (unstaged)' 'gitleaks' @('git', '--pre-commit', '--redact', '--no-banner', '.')
} else {
  Skip-Check 'gitleaks' 'gitleaks not installed'
}

# --- Vendored code integrity -----------------------------------------------
$manifests = @(Get-ChildItem -Path 'third_party' -Filter '*.sha256' -ErrorAction SilentlyContinue)
if ($manifests.Count -eq 0) {
  Skip-Check 'third_party integrity' 'no vendored manifests yet'
} else {
  Write-Host ('  {0,-26} ' -f 'third_party integrity') -NoNewline
  $bad = New-Object System.Collections.Generic.List[string]
  foreach ($m in $manifests) {
    foreach ($line in Get-Content $m.FullName) {
      if ($line -notmatch '^([0-9a-f]{64})  (.+)$') { $bad.Add("malformed line in $($m.Name)"); continue }
      $path = Join-Path 'third_party' $Matches[2]
      if (-not (Test-Path -LiteralPath $path)) { $bad.Add("missing: $path"); continue }
      $actual = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
      if ($actual -ne $Matches[1]) { $bad.Add("modified: $path") }
    }
  }
  if ($bad.Count -eq 0) { Write-Host 'ok'; Add-Result 'PASS' 'third_party integrity' '' }
  else {
    Write-Host 'FAILED'
    $bad | Select-Object -First 40 | ForEach-Object { Write-Host "      $_" }
    Add-Result 'FAIL' 'third_party integrity' 'see output above'
    $script:failed++
  }
}

# --- Shell scripts --------------------------------------------------------
# Vendored scripts in third_party/ are not ours and are never edited.
$shellFiles = @(& git ls-files --cached --others --exclude-standard -- '*.sh' '.githooks/*' ':(exclude)third_party')
if ($shellFiles.Count -eq 0 -or -not $shellFiles[0]) {
  Skip-Check 'shellcheck' 'no shell scripts'
} elseif (Test-Tool 'shellcheck') {
  Invoke-Check 'shellcheck' 'shellcheck' $shellFiles
} else {
  Skip-Check 'shellcheck' 'shellcheck not installed (checked on macOS/Linux)'
}

# --- Rust -----------------------------------------------------------------
$rustChecks = @('cargo fmt', 'cargo clippy', 'cargo test', 'cargo deny', 'cargo audit')
if (-not (Test-Path 'Cargo.toml')) {
  foreach ($c in $rustChecks) { Skip-Check $c 'no Cargo workspace yet' }
} elseif (-not (Test-Tool 'cargo')) {
  foreach ($c in $rustChecks) { Skip-Check $c 'cargo not installed' }
} else {
  Invoke-Check 'cargo fmt' 'cargo' @('fmt', '--all', '--check')
  Invoke-Check 'cargo clippy' 'cargo' @('clippy', '--workspace', '--all-targets', '--locked', '--', '-D', 'warnings')
  Invoke-Check 'cargo test' 'cargo' @('test', '--workspace', '--locked')
  if ((Test-CargoSub 'deny') -and (Test-Path 'deny.toml')) {
    Invoke-Check 'cargo deny' 'cargo' @('deny', 'check')
  } else {
    Skip-Check 'cargo deny' 'cargo-deny or deny.toml missing'
  }
  if ((Test-CargoSub 'audit') -and (Test-Path 'Cargo.lock')) {
    Invoke-Check 'cargo audit' 'cargo' @('audit', '--deny', 'warnings')
  } else {
    Skip-Check 'cargo audit' 'cargo-audit or Cargo.lock missing'
  }
}

# --- C++ / Qt -------------------------------------------------------------
$cppFiles = @(& git ls-files --cached --others --exclude-standard -- 'ui/*.cpp' 'ui/*.h')
if ($cppFiles.Count -gt 0 -and $cppFiles[0]) {
  if (Test-Tool 'clang-format') {
    Invoke-Check 'clang-format' 'clang-format' (@('--dry-run', '--Werror') + $cppFiles)
  } else {
    Skip-Check 'clang-format' 'clang-format not installed'
  }
} else {
  Skip-Check 'clang-format' 'no C++ sources yet'
}

if (Test-Path 'CMakeLists.txt') {
  if (Test-Tool 'cmake') {
    $buildDir = 'build/check'
    $configureOutput = & cmake -S . -B $buildDir -DFERNY_WARNINGS_AS_ERRORS=ON 2>&1 | Out-String
    if ($LASTEXITCODE -eq 0) {
      Write-Host ('  {0,-26} ok' -f 'cmake configure')
      Add-Result 'PASS' 'cmake configure' ''
      $buildOutput = & cmake --build $buildDir --config Debug --parallel 2>&1 | Out-String
      $appCandidates = @(
        (Join-Path $buildDir 'ui/ferny.exe'),
        (Join-Path $buildDir 'ui/Debug/ferny.exe'),
        (Join-Path $buildDir 'ui/ferny')
      )
      $appBuilt = $false
      foreach ($app in $appCandidates) {
        if (Test-Path -LiteralPath $app) { $appBuilt = $true; break }
      }
      if ($LASTEXITCODE -eq 0 -and $appBuilt) {
        Write-Host ('  {0,-26} ok' -f 'cmake build')
        Add-Result 'PASS' 'cmake build' ''
        Invoke-Check 'ctest' 'ctest' @('--test-dir', $buildDir, '-C', 'Debug', '--output-on-failure')
      } else {
        Write-Host ('  {0,-26} FAILED' -f 'cmake build')
        ($buildOutput -split "`r?`n" | Select-Object -Last 40) | ForEach-Object { Write-Host "      $_" }
        Add-Result 'FAIL' 'cmake build' 'see output above'
        $script:failed++
        Skip-Check 'ctest' 'build failed; UI tests were not run'
      }
    } else {
      Write-Host ('  {0,-26} FAILED' -f 'cmake configure')
      ($configureOutput -split "`r?`n" | Select-Object -Last 40) | ForEach-Object { Write-Host "      $_" }
      Add-Result 'FAIL' 'cmake configure' 'see output above'
      $script:failed++
      Skip-Check 'cmake build' 'configure failed'
      Skip-Check 'ctest' 'configure failed; UI tests were not run'
    }
  } else {
    Skip-Check 'cmake build' 'cmake not installed'
  }
} else {
  Skip-Check 'cmake build' 'no CMakeLists.txt yet'
  Skip-Check 'ctest' 'no CMakeLists.txt yet'
}

# --- SAST -----------------------------------------------------------------
if (Test-HasFiles @('*.rs', '*.cpp', '*.h')) {
  if (Test-Tool 'semgrep') {
    # Registry rules are downloaded from semgrep.dev; metrics are disabled.
    Invoke-Check 'semgrep' 'semgrep' @('scan', '--metrics=off', '--error', '--quiet',
      '--config', 'p/rust', '--config', 'p/c',
      '--exclude', 'third_party', '--exclude', 'target', '--exclude', 'build')
  } else {
    Skip-Check 'semgrep' 'semgrep not installed'
  }
} else {
  Skip-Check 'semgrep' 'no Rust or C++ sources yet'
}

Skip-Check 'cargo fuzz' 'not run on Windows (macOS/Linux only)'

# --- Summary --------------------------------------------------------------
function Get-ToolVersion([string]$tool, [string[]]$arguments) {
  if (Test-Tool $tool) {
    $v = & $tool @arguments 2>$null | Select-Object -First 1
    if ($v) { return "$v" }
  }
  return 'not installed'
}

$commit = (& git rev-parse --short HEAD 2>$null)
if (-not $commit) { $commit = 'none' }
$dirty = ''
if (& git status --porcelain) { $dirty = ' (uncommitted changes)' }
$os = Get-CimInstance Win32_OperatingSystem -ErrorAction SilentlyContinue
$osDesc = if ($os) { "$($os.Caption) $($os.Version)" } else { [Environment]::OSVersion.VersionString }
$arch = $env:PROCESSOR_ARCHITECTURE
$total = $results.Count
$passed = $total - $script:failed - $script:skipped

Write-Host ''
Write-Host '----- BEGIN FERNY CHECK SUMMARY -----'
Write-Host ('Date:     {0}' -f (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ'))
Write-Host ('System:   {0}, {1}' -f $osDesc, $arch)
Write-Host ('Branch:   {0}' -f (& git branch --show-current))
Write-Host ('Commit:   {0}{1}' -f $commit, $dirty)
Write-Host ('Mode:     {0}' -f $(if ($Strict) { 'strict' } else { 'normal' }))
Write-Host ''
Write-Host 'Tools:'
Write-Host ('  git        {0}' -f (Get-ToolVersion 'git' @('--version')))
Write-Host ('  gitleaks   {0}' -f (Get-ToolVersion 'gitleaks' @('version')))
Write-Host ('  cargo      {0}' -f (Get-ToolVersion 'cargo' @('--version')))
Write-Host ('  cmake      {0}' -f (Get-ToolVersion 'cmake' @('--version')))
Write-Host ('  semgrep    {0}' -f (Get-ToolVersion 'semgrep' @('--version')))
Write-Host ('  powershell {0}' -f $PSVersionTable.PSVersion)
Write-Host ''
Write-Host 'Results:'
foreach ($r in $results) {
  if ($r.Detail) { Write-Host ('  {0,-4}  {1,-24} ({2})' -f $r.Status, $r.Name, $r.Detail) }
  else { Write-Host ('  {0,-4}  {1}' -f $r.Status, $r.Name) }
}
Write-Host ''
Write-Host ('Totals:   {0} passed, {1} failed, {2} skipped (not verified)' -f $passed, $script:failed, $script:skipped)

$verdict = 'PASS'
if ($script:failed -gt 0) { $verdict = 'FAIL' }
elseif ($Strict -and $script:skipped -gt 0) { $verdict = 'FAIL (strict: skipped checks are not allowed)' }
Write-Host ('Verdict:  {0}' -f $verdict)
Write-Host '----- END FERNY CHECK SUMMARY -----'

if ($verdict -eq 'PASS') { exit 0 } else { exit 1 }
