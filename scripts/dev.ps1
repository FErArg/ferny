# Build Ferny (Debug) and run it on Windows. Run from a "Developer PowerShell
# for VS 2022" so CMake finds MSVC. Arguments are passed to the app.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev.ps1 [file]
$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
$build = Join-Path $root 'build\dev'

if (-not (Test-Path (Join-Path $build 'CMakeCache.txt'))) {
  $gen = @()
  if (Get-Command ninja -ErrorAction SilentlyContinue) { $gen = @('-G', 'Ninja') }
  & cmake -S $root -B $build @gen -DCMAKE_BUILD_TYPE=Debug
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
& cmake --build $build --parallel --config Debug
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$exe = Join-Path $build 'ui\ferny.exe'
if (-not (Test-Path $exe)) { $exe = Join-Path $build 'ui\Debug\ferny.exe' }
# The Debug build links the Qt debug DLLs (Qt6Cored.dll, Qt6Widgetsd.dll, ...)
# from the Qt kit that CMake found; they are not copied next to the executable,
# so the kit's bin directory must be on PATH for this run.
$qt6Dir = (Select-String -Path (Join-Path $build 'CMakeCache.txt') -Pattern '^Qt6_DIR:PATH=(.*)$' |
  Select-Object -First 1).Matches.Groups[1].Value
if ($qt6Dir) {
  $env:PATH = (Resolve-Path (Join-Path $qt6Dir '..\..\..\bin')).Path + ';' + $env:PATH
}
& $exe @args
exit $LASTEXITCODE
