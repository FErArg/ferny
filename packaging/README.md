# packaging

Platform packaging: macos/ (dmg), linux/ (AppImage, Flatpak, deb), windows/ (MSI/MSIX).

## Debian (Linux amd64)

`scripts/package-deb.sh [--ref <tag-or-commit> | --source-tree <dir>] [--out <dir>]` builds
`Ferny-v<VERSION>-linux-x86_64.deb` from a committed ref (default `HEAD`),
offline, in a temporary source/build tree. It requires Debian 13 amd64 with
Qt 6.8 development tools, CMake, Cargo and `dpkg-deb`. The script verifies
package metadata, included paths and the executable version, runs an
offscreen smoke test, refuses to overwrite an existing package, and never
installs the package. The artifact is copied to `bin/` by default.
`--source-tree` is for explicitly requested local validation builds; such
packages are not release builds and must not be represented as built from a
commit or tag. The Release build retains compiler warnings but currently turns
off project-level warnings-as-errors because CXX-Qt generated headers trigger
`-Wshadow` in the current GCC build; `-Wno-error=shadow` is also supplied to
the compiler. The normal full gate still uses the project's strict setting and
reports its independent result; packaging success does not imply the full gate
passed.

## macOS (H1.6c)

`scripts/package-macos.sh [--ref <tag-or-commit>] [--out <dir>]` builds
`bin/ferny-v<VERSION>-macos-arm64.dmg` and its `.sha256` from a committed
ref (default `HEAD`), offline. The ref is exported to a temporary directory;
the sources and the build tree are kept in `~/Library/Caches/ferny-package`,
so a repeated run rebuilds only what changed. Delete that folder to start from
scratch. A second run at the same time is refused (`lock`). The run requires
the Rust toolchain pinned in `rust-toolchain.toml` as the first `rustc` on
`PATH`. If `ccache` is installed, it is used for C and C++ (optional).

- Measured 2026-10-20 (Apple Silicon, 12 cores, up to the DMG step, with
  `hdiutil` stubbed): 79 s cold; 21 s when the sources are unchanged; 21 s
  when one C++ file changed. Before the cache, every run rebuilt everything.

- `macos/Info.plist.in`: bundle metadata (`com.ferarg.ferny`), used when
  CMake is configured with `-DFERNY_MACOS_BUNDLE=ON`.
- `macos/licenses/`: LGPL-3.0 and GPL-3.0 texts (Qt is LGPL-3.0; the
  texts are the FSF's, SHA-256 `a853c2ff…` and `8ceb4b9e…`).
- Only the Cocoa, offscreen and macOS-style Qt plugins are bundled;
  `qt.conf` keeps plugins and Qt's translations inside the bundle.
- The bundle is re-signed ad-hoc (no identity, no notarisation): the
  first open needs System Settings → Privacy & Security → Open Anyway.
- Verification: signature, arm64 only, no library or rpath outside the
  bundle, plugins loaded from the bundle, smoke test in Spanish from the
  mounted DMG. Existing artefacts are never overwritten.
- Tests: `bash test/scripts/package_macos_test.sh`.

## Windows x64 MSI

`scripts/package-msi.ps1 [-Ref <tag-or-commit> | -WorkingTree] [-Out <dir>]`
builds `Ferny-v<VERSION>-windows-x86_64.msi` and its `.sha256` in `bin/`
without installing anything. The default `-Ref HEAD` exports the committed
tree with `git archive` and requires a clean tracked working tree; `-WorkingTree`
packages the current source and is reported as an uncommitted, non-release build.

- Toolchain: Visual Studio 2022 Build Tools (C++), the Qt MSVC kit
  (`C:\Qt\6.10.3\msvc2022_64` by default, `-QtDir` to change it) and WiX
  Toolset v3.14 (`-WixDir`). See `docs/DEPENDENCIES.md`.
- `Ferny.wxs` installs to `Program Files\Ferny` (per machine), creates a Start
  menu shortcut and uses a fixed UpgradeCode. Qt files come from `windeployqt`
  and are harvested by `heat.exe`. No custom actions, no network access.
- Verification: product name, version and `x64` template from the Windows
  Installer API; administrative extraction (`msiexec /a`, no installation) and
  the extracted executable's `--version`; offscreen `--smoke-test` of the
  staged executable. Existing artefacts are never overwritten.
- Not signed. Signing needs explicit approval and an identity that never
  enters the repository.
