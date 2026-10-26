<#
.SYNOPSIS
  Negative tests for scripts/check.ps1: the gate must fail closed.

.DESCRIPTION
  Runs in a throwaway Git repository with synthetic data only.
  Compatible with Windows PowerShell 5.1 and PowerShell 7.

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File test\scripts\check_ps1_test.ps1
#>
$ErrorActionPreference = 'Continue'
Set-StrictMode -Version 2.0

$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$check = Join-Path $root 'scripts\check.ps1'
$tmp = Join-Path ([IO.Path]::GetTempPath()) ('ferny-check-test-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tmp | Out-Null
$repo = Join-Path $tmp 'repo'
$log = Join-Path $tmp 'out.log'
$script:passed = 0
$script:failed = 0

function New-TestRepo {
  if (Test-Path $repo) { Remove-Item -Recurse -Force $repo }
  New-Item -ItemType Directory -Path (Join-Path $repo 'scripts') | Out-Null
  Copy-Item $check (Join-Path $repo 'scripts\check.ps1')
  & git -C $repo init -q
  & git -C $repo config core.hooksPath NUL
  & git -C $repo config core.autocrlf false
  Set-Content -Path (Join-Path $repo 'README.md') -Value '# test' -Encoding ascii
  & git -C $repo add -A
  & git -C $repo -c user.name=test -c user.email=test@example.invalid commit -qm init
}

function Invoke-Gate([string[]]$extra) {
  $ps = (Get-Process -Id $PID).Path
  & $ps -NoProfile -ExecutionPolicy Bypass -File (Join-Path $repo 'scripts\check.ps1') @extra *> $log
  return $LASTEXITCODE
}

function Assert-Exit([string]$name, [int]$want, [string[]]$extra = @()) {
  $got = Invoke-Gate $extra
  if ($got -eq $want) { Write-Host "ok   $name"; $script:passed++ }
  else {
    Write-Host "FAIL $name (exit $got, expected $want)"
    Get-Content $log | Select-Object -Last 20 | ForEach-Object { Write-Host "     $_" }
    $script:failed++
  }
}

function Assert-Output([string]$name, [string]$pattern) {
  if (Select-String -Path $log -SimpleMatch -Pattern $pattern -Quiet) { Write-Host "ok   $name"; $script:passed++ }
  else { Write-Host "FAIL $name (missing: $pattern)"; $script:failed++ }
}

try {
  # 1. Clean repository passes in normal mode.
  New-TestRepo
  Assert-Exit 'clean repo passes' 0
  Assert-Output 'skipped checks are reported as not verified' 'skipped (not verified)'

  # 2. Strict mode fails while checks are skipped.
  Assert-Exit 'strict mode fails with skipped checks' 1 @('-Strict')

  # 3. Trailing whitespace makes git diff --check fail.
  New-TestRepo
  Add-Content -Path (Join-Path $repo 'README.md') -Value 'trailing space ' -Encoding ascii
  Assert-Exit 'whitespace error fails the gate' 1
  Assert-Output 'whitespace failure is named' 'FAIL  git diff --check'

  # 4. A modified vendored file fails the integrity check.
  New-TestRepo
  $vdir = Join-Path $repo 'third_party\demo'
  New-Item -ItemType Directory -Path $vdir | Out-Null
  [IO.File]::WriteAllText((Join-Path $vdir 'file.txt'), "original`n")
  $h = (Get-FileHash -Algorithm SHA256 (Join-Path $vdir 'file.txt')).Hash.ToLowerInvariant()
  [IO.File]::WriteAllText((Join-Path $repo 'third_party\demo.sha256'), "$h  demo/file.txt`n")
  & git -C $repo add -A
  & git -C $repo -c user.name=test -c user.email=test@example.invalid commit -qm vendor
  Assert-Exit 'intact vendored code passes' 0
  [IO.File]::WriteAllText((Join-Path $vdir 'file.txt'), "tampered`n")
  Assert-Exit 'tampered vendored file fails the gate' 1
  Assert-Output 'integrity failure is named' 'FAIL  third_party integrity'

  # 5-6. Staged and unstaged synthetic secrets make gitleaks fail.
  if (Get-Command gitleaks -ErrorAction SilentlyContinue) {
    $chars = [char[]]'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789'
    $token = 'ghp_' + (-join (1..36 | ForEach-Object { $chars | Get-Random }))
    New-TestRepo
    Set-Content -Path (Join-Path $repo 'config.txt') -Value ('github_token = "' + $token + '"') -Encoding ascii
    & git -C $repo add config.txt
    Assert-Exit 'staged secret fails the gate' 1
    Assert-Output 'staged secret failure is named' 'FAIL  gitleaks (staged)'

    & git -C $repo reset -q config.txt
    Add-Content -Path (Join-Path $repo 'README.md') -Value ('github_token = "' + $token + '"') -Encoding ascii
    Assert-Exit 'unstaged secret fails the gate' 1
    Assert-Output 'unstaged secret failure is named' 'FAIL  gitleaks (unstaged)'

    if (Select-String -Path $log -SimpleMatch -Pattern $token -Quiet) { Write-Host 'FAIL secret is redacted in output'; $script:failed++ }
    else { Write-Host 'ok   secret is redacted in output'; $script:passed++ }
  } else {
    Write-Host 'skip secret tests (gitleaks not installed - not verified)'
  }

  # Configure a synthetic project that fails during build. The gate must
  # report CTest as skipped rather than running tests against stale binaries.
  if ((Get-Command cmake -ErrorAction SilentlyContinue) -and
      (Get-Command ctest -ErrorAction SilentlyContinue)) {
    New-TestRepo
    @'
cmake_minimum_required(VERSION 3.16)
project(FailingBuild LANGUAGES NONE)
enable_testing()
add_custom_target(always_fails ALL
  COMMAND "${CMAKE_COMMAND}" -E false
  VERBATIM)
'@ | Set-Content -Path (Join-Path $repo 'CMakeLists.txt') -Encoding ascii
    & git -C $repo add CMakeLists.txt
    & git -C $repo -c user.name=test -c user.email=test@example.invalid commit -qm cmake-fixture
    Assert-Exit 'failed CMake build blocks CTest and gate' 1
    Assert-Output 'CTest is skipped after build failure' 'SKIP.*ctest'
    Assert-Output 'build failure remains in summary' 'FAIL  cmake build'
  }
} finally {
  Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}

Write-Host ''
Write-Host ("{0} passed, {1} failed" -f $script:passed, $script:failed)
if ($script:failed -eq 0) { exit 0 } else { exit 1 }
