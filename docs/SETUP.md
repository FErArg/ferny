# Developer setup

How to prepare a machine to develop and validate Ferny. Tier-1 platforms:
macOS (arm64, builds universal), Debian 13 amd64, Windows 11 x64. Versions
and their reasons: [DEPENDENCIES.md](DEPENDENCIES.md).

## 1. Clone and enable the hooks

```sh
git clone https://github.com/FErArg/ferny.git
cd ferny
git config core.hooksPath .githooks     # gitleaks before every commit and push
```

On Windows, use Git for Windows (it provides the bash that runs the hooks)
and keep the default line-ending settings: `.gitattributes` forces LF for
bash scripts and hooks and keeps vendored files byte for byte.

## 2. Install the tools

Preview first, then install. The scripts skip what is already installed
and print the version of every tool at the end.

### macOS (Homebrew required)

```sh
bash scripts/development/install-macos.sh --dry-run
bash scripts/development/install-macos.sh
```

Homebrew's `rustup` is keg-only: `--update-shell-profile` adds
`/opt/homebrew/opt/rustup/bin` and `~/.cargo/bin` to the `PATH` in
`~/.zprofile`. Open a new terminal afterwards. Qt comes from Homebrew
(`qt`, currently 6.12.0, includes Core5Compat).

### Debian 13 amd64

```sh
bash scripts/development/install-debian.sh --dry-run
bash scripts/development/install-debian.sh
```

Uses `sudo apt-get`. Qt comes from Debian (`qt6-base-dev` 6.8.2 and related
packages, including `qt6-5compat-dev` and `qt6-l10n-tools`). If `gitleaks` or `rustup` are not available in apt, the script
downloads a pinned release and verifies its SHA-256 before installing it.

### Windows 11 x64

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\development\Install-Windows.ps1 -DryRun
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\development\Install-Windows.ps1 -UpdateUserPath
```

Visual Studio Build Tools and LLVM ask for administrator confirmation. Qt
6.12.0 for MSVC 2022, with the `qt5compat` module, is installed in `C:\Qt`
through aqtinstall;
`-UpdateUserPath` sets `CMAKE_PREFIX_PATH` so CMake finds it. Open a new
terminal afterwards.

### Optional flags

| macOS / Debian | Windows | Installs |
|---|---|---|
| `--with-fuzz` | (not supported) | nightly toolchain + cargo-fuzz |
| `--with-release-tools` | `-WithReleaseTools` | minisign, syft |
| — | `-WithSemgrep` | Semgrep (SAST already runs on macOS/Linux) |

## 3. Rust

The toolchain is pinned in `rust-toolchain.toml` (Rust 1.99.0 with rustfmt
and clippy). rustup installs it automatically the first time `cargo` runs
inside the repository; nothing else to do.

## 4. Build and run

Quick loop: `scripts/dev.sh [file]` (macOS/Linux) or `scripts\dev.ps1
[file]` (Windows, from a Developer PowerShell) configures `build/dev` the
first time, builds and starts Ferny. Step by step:

```sh
cmake -S . -B build/dev -G Ninja -DCMAKE_BUILD_TYPE=Debug
cmake --build build/dev
ctest --test-dir build/dev --output-on-failure
build/dev/ui/ferny README.md          # opens one file, read-only
```

Configuring downloads nothing (Corrosion and cxx-qt-cmake are vendored);
the first build lets Cargo fetch the crates pinned in `Cargo.lock`. Cargo
commands on the workspace (`cargo clippy`, `cargo test`) also need Qt,
because the bridge's build script runs `qmake` (on Debian it finds
`qmake6`; set `QMAKE` if it does not). On Windows, configure from a
"Developer PowerShell for VS 2022" so CMake finds MSVC.

## 5. Verify

```sh
bash test/scripts/check_sh_test.sh       # tests of the gate (macOS, Linux)
scripts/check.sh                          # the gate itself
```

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File test\scripts\check_ps1_test.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\check.ps1
```

Expected from milestone H0.3: every check passes (git, gitleaks,
shellcheck, vendored code integrity, cargo fmt/clippy/test/deny/audit,
clang-format, CMake configure/build with warnings as errors, CTest,
Semgrep where installed), plus 60 s of fuzzing per target on macOS/Linux
when the nightly toolchain and cargo-fuzz are installed
(`--with-fuzz`); otherwise fuzzing is reported as not verified.

## Platform notes

- macOS: `brew --prefix qt` is found by CMake automatically.
- Debian 13: Wayland and X11 both work with the distribution's Qt; the
  build only needs the `-dev` packages listed in the installer.
- Windows: build from a "Developer PowerShell for VS 2022" or let CMake
  locate MSVC; `cl.exe` is not on the PATH of a normal terminal.
