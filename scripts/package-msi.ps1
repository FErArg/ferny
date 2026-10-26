# Build the Windows x64 MSI for Ferny without installing it.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\package-msi.ps1 [-Ref <tag-or-commit> | -WorkingTree] [-Out <dir>]
#
# Default: -Ref HEAD, exported with git archive from a clean tracked tree.
# -WorkingTree packages the current working tree (uncommitted; not a release
# build). Requires Visual Studio 2022 Build Tools with the C++ workload, the Qt
# MSVC kit (Qt 6.8 or newer) and WiX Toolset v3. Existing outputs are never
# replaced. The MSI is not signed.
[CmdletBinding()]
param(
  [string]$Ref = 'HEAD',
  [switch]$WorkingTree,
  [string]$Out = 'bin',
  [string]$QtDir = 'C:\Qt\6.10.3\msvc2022_64',
  [string]$WixDir = 'C:\Program Files (x86)\WiX Toolset v3.14\bin'
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2.0

function Fail([string]$Message) { throw "package-msi: $Message" }
function Invoke-Native([string]$Exe, [string[]]$Arguments) {
  & $Exe @Arguments
  if ($LASTEXITCODE -ne 0) { Fail "$Exe failed with exit code $LASTEXITCODE" }
}
# Runs an executable with one argument and returns its stdout and stderr as
# text, without PowerShell turning stderr lines into error records.
# Converts plain ASCII text to an RTF document for the WiX license dialog.
# Backslash and braces are escaped; any other non-ASCII character becomes \uN.
function ConvertTo-LicenseRtf([string]$Text) {
  $rtf = New-Object Text.StringBuilder
  [void]$rtf.Append('{\rtf1\ansi\deff0{\fonttbl{\f0\fmodern Courier New;}}\f0\fs20 ')
  foreach ($char in $Text.ToCharArray()) {
    switch ([int]$char) {
      92 { [void]$rtf.Append('\\') }
      123 { [void]$rtf.Append('\{') }
      125 { [void]$rtf.Append('\}') }
      10 { [void]$rtf.Append('\par' + "`r`n") }
      13 { }
      9 { [void]$rtf.Append('\tab ') }
      default {
        if ([int]$char -gt 126) { [void]$rtf.Append("\u$([int]$char)?") }
        else { [void]$rtf.Append($char) }
      }
    }
  }
  [void]$rtf.Append('}')
  return $rtf.ToString()
}

function Get-NativeOutput([string]$Exe, [string]$Argument) {
  $info = New-Object Diagnostics.ProcessStartInfo
  $info.FileName = $Exe
  $info.Arguments = $Argument
  $info.UseShellExecute = $false
  $info.RedirectStandardOutput = $true
  $info.RedirectStandardError = $true
  $info.CreateNoWindow = $true
  $process = [Diagnostics.Process]::Start($info)
  $stdout = $process.StandardOutput.ReadToEnd()
  $stderr = $process.StandardError.ReadToEnd()
  $process.WaitForExit()
  if ($process.ExitCode -ne 0) { Fail "$Exe $Argument exited with code $($process.ExitCode)" }
  return "$stdout$stderr"
}

$root = Split-Path -Parent $PSScriptRoot
if ($WorkingTree -and $PSBoundParameters.ContainsKey('Ref')) { Fail 'choose either -Ref or -WorkingTree' }

$commit = ''
if ($WorkingTree) {
  $cargoText = Get-Content -Raw -LiteralPath (Join-Path $root 'Cargo.toml')
} else {
  $dirty = & git -C $root status --porcelain --untracked-files=no
  if ($dirty) { Fail 'working tree has tracked changes; build from a clean committed ref or use -WorkingTree' }
  $commit = & git -C $root rev-parse --verify --quiet "$Ref^{commit}"
  if (-not $commit) { Fail "unknown ref: $Ref" }
  $cargoText = (& git -C $root show "${commit}:Cargo.toml") -join "`n"
}
$version = [regex]::Match($cargoText, '(?m)^version = "([0-9][0-9.]*)"\r?$').Groups[1].Value
if (-not $version) { Fail 'no workspace version in Cargo.toml' }

$name = "Ferny-v$version-windows-x86_64.msi"
$outDir = if ([IO.Path]::IsPathRooted($Out)) { $Out } else { Join-Path $root $Out }
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
$msi = Join-Path $outDir $name
if ((Test-Path -LiteralPath $msi) -or (Test-Path -LiteralPath "$msi.sha256")) {
  Fail "$msi exists; earlier artefacts are never overwritten"
}

if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64') { Fail 'Windows x64 only' }
$windeployqt = Join-Path $QtDir 'bin\windeployqt.exe'
$candle = Join-Path $WixDir 'candle.exe'
$light = Join-Path $WixDir 'light.exe'
$heat = Join-Path $WixDir 'heat.exe'
foreach ($tool in @($windeployqt, $candle, $light, $heat, (Join-Path $QtDir 'bin\qmake.exe'))) {
  if (-not (Test-Path -LiteralPath $tool)) { Fail "missing tool: $tool" }
}
foreach ($tool in @('cmake', 'ninja', 'git', 'tar')) {
  if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { Fail "missing tool: $tool" }
}

# MSVC environment for the native build (vcvars64 of the newest VS with C++).
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$vsPath = ''
if (Test-Path -LiteralPath $vswhere) {
  $vsPath = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
}
if (-not $vsPath) { Fail 'Visual Studio with the C++ tools was not found' }
$vcvars = Join-Path $vsPath 'VC\Auxiliary\Build\vcvars64.bat'
$envDump = cmd.exe /d /s /c "`"$vcvars`" >nul && set"
foreach ($line in $envDump) {
  if ($line -match '^([^=]+)=(.*)$') { Set-Item -Path "env:$($Matches[1])" -Value $Matches[2] }
}

$work = Join-Path $env:TEMP ('ferny-msi-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory -Path $work | Out-Null
$succeeded = $false
try {
  if ($WorkingTree) {
    $src = $root
    Write-Host "package-msi: working tree $root (uncommitted source; not a release build), version $version, x64"
  } else {
    $src = Join-Path $work 'src'
    New-Item -ItemType Directory -Path $src | Out-Null
    $archive = Join-Path $work 'src.tar'
    Invoke-Native 'git' @('-C', $root, 'archive', '--format=tar', '-o', $archive, $commit)
    Invoke-Native 'tar' @('-xf', $archive, '-C', $src)
    $short = & git -C $root rev-parse --short $commit
    Write-Host "package-msi: $Ref ($short), version $version, x64"
  }

  # Cargo output stays out of the source tree; target/ is git-ignored anyway.
  $env:CARGO_TARGET_DIR = Join-Path $root 'target\msi-build'
  $build = Join-Path $work 'build'
  Invoke-Native 'cmake' @('-S', $src, '-B', $build, '-G', 'Ninja', '-DCMAKE_BUILD_TYPE=Release',
    "-DCMAKE_PREFIX_PATH=$QtDir", '-DFERNY_WARNINGS_AS_ERRORS=OFF')
  Invoke-Native 'cmake' @('--build', $build, '--parallel', '--target', 'ferny')

  $stage = Join-Path $work 'stage'
  New-Item -ItemType Directory -Path $stage | Out-Null
  Copy-Item -LiteralPath (Join-Path $build 'ui\ferny.exe') -Destination $stage
  Copy-Item -LiteralPath (Join-Path $src 'LICENSE') -Destination (Join-Path $stage 'LICENSE.txt')
  Copy-Item -LiteralPath (Join-Path $src 'NOTICE') -Destination (Join-Path $stage 'NOTICE.txt')
  Invoke-Native $windeployqt @('--release', '--no-translations', '--dir', $stage, (Join-Path $stage 'ferny.exe'))

  $exe = Join-Path $stage 'ferny.exe'
  $versionText = Get-NativeOutput $exe '--version'
  if ($versionText -notmatch "(^|\s)$([regex]::Escape($version))(\s|$)") { Fail "executable version does not match package version: $versionText" }
  # Default platform plugin: windeployqt ships only qwindows here. The test
  # opens a window briefly on the interactive desktop.
  $smokeText = Get-NativeOutput $exe '--smoke-test'
  if ($smokeText -notmatch 'smoke: ok') { Fail 'staged executable smoke test did not report success' }

  # The harvested file list is the MSI payload; Product.wxs holds the rest.
  $harvest = Join-Path $work 'Harvest.wxs'
  Invoke-Native $heat @('dir', $stage, '-cg', 'QtFiles', '-dr', 'INSTALLFOLDER', '-gg', '-srd',
    '-sfrag', '-platform', 'x64', '-var', 'var.StageDir', '-out', $harvest, '-nologo')

  # License dialog: the GPL text from the same source as LICENSE.txt, with a
  # short notice first. Generated here so the text is not copied into the repo.
  $licenseText = [IO.File]::ReadAllText((Join-Path $src 'LICENSE'), [Text.Encoding]::ASCII)
  $notice = 'Ferny is free software, licensed under the GNU General Public License, version 2 or ' +
    "later (GPL-2.0-or-later). Third-party notices are in NOTICE.txt in the installation folder.`n`n"
  $licenseRtf = Join-Path $work 'License.rtf'
  [IO.File]::WriteAllText($licenseRtf, (ConvertTo-LicenseRtf ($notice + $licenseText)), [Text.Encoding]::ASCII)

  $obj = Join-Path $work 'obj'
  New-Item -ItemType Directory -Path $obj | Out-Null
  $product = Join-Path $src 'packaging\windows\Ferny.wxs'
  Invoke-Native $candle @('-nologo', '-arch', 'x64', "-dVersion=$version", "-dStageDir=$stage",
    "-dProjectDir=$src", "-dLicenseRtf=$licenseRtf", '-out', "$obj\", $product, $harvest)
  $msiWork = Join-Path $work $name
  Invoke-Native $light @('-nologo', '-ext', (Join-Path $WixDir 'WixUIExtension.dll'), '-out', $msiWork,
    (Join-Path $obj 'Ferny.wixobj'), (Join-Path $obj 'Harvest.wixobj'))

  # Verify the package metadata with the Windows Installer API (read only).
  $installer = New-Object -ComObject WindowsInstaller.Installer
  $db = $installer.OpenDatabase($msiWork, 0)
  function Get-MsiProperty([string]$Property) {
    $view = $db.OpenView("SELECT Value FROM Property WHERE Property='$Property'")
    [void]$view.Execute()
    $record = $view.Fetch()
    $value = [string]$record.StringData(1)
    [void]$view.Close()
    return $value.Trim()
  }
  $productName = Get-MsiProperty 'ProductName'
  $productVersion = Get-MsiProperty 'ProductVersion'
  $template = $db.SummaryInformation(0).Property(7)
  if ($productName -ne 'Ferny') { Fail "wrong product name: $productName" }
  if ($productVersion -ne $version) { Fail "wrong product version: $productVersion" }
  if ($template -notlike 'x64;*') { Fail "wrong package architecture: $template" }

  # Administrative extraction unpacks the payload without installing anything.
  $admin = Join-Path $work 'admin'
  $extract = Start-Process -FilePath 'msiexec.exe' -Wait -PassThru -ArgumentList @('/a', "`"$msiWork`"", '/qn', "TARGETDIR=`"$admin`"")
  if ($extract.ExitCode -ne 0) { Fail "administrative extraction failed with exit code $($extract.ExitCode)" }
  $packagedExe = Get-ChildItem -LiteralPath $admin -Recurse -Filter 'ferny.exe' | Select-Object -First 1
  if (-not $packagedExe) { Fail 'package lacks ferny.exe' }
  $packagedVersion = Get-NativeOutput $packagedExe.FullName '--version'
  if ($packagedVersion -notmatch "(^|\s)$([regex]::Escape($version))(\s|$)") { Fail "packaged executable reports an unexpected version: $packagedVersion" }

  Copy-Item -LiteralPath $msiWork -Destination $msi
  $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $msi).Hash.ToLower()
  Set-Content -Encoding ascii -LiteralPath "$msi.sha256" -Value "$hash  $name"
  Write-Host "package-msi: wrote $msi"
  Write-Host "$hash  $name"
  $succeeded = $true
} finally {
  # A failed run keeps its staging tree so the failure can be inspected.
  if ($succeeded) {
    Remove-Item -Recurse -Force -LiteralPath $work -ErrorAction SilentlyContinue
  } else {
    Write-Host "package-msi: failed; staging kept for inspection at $work"
  }
}
