# Dependencies

Record kept for every dependency: version, licence,
origin, maintenance status and known advisories. A dependency is added to a
manifest only when first used, at the version recorded here; changing a
version updates this file in the same commit. Checked 2026-10-15
(milestones H0.2 and H0.3).

## Toolchains

| Tool | Pinned version | Where it is pinned | Platforms |
|---|---|---|---|
| Rust | 1.99.0 (rustfmt, clippy) | `rust-toolchain.toml`, `rust-version` in `Cargo.toml` | all (installed with rustup) |
| Edition | 2024, resolver 3 | `Cargo.toml` | all |
| CMake | ≥ 3.28 | `CMakeLists.txt` (`cmake_minimum_required`) | all |
| C++ | C++17 | `CMakeLists.txt` | Apple Clang, GCC/Clang, MSVC 2022 |
| ccache | 4.14.2 (optional, build time only) | Homebrew `ccache`; `scripts/package-macos.sh` uses it when installed | macOS (Homebrew); GPL-3.0-or-later, https://ccache.dev, not deprecated in Homebrew; known advisories: not verified |
| rsync | system `/usr/bin/rsync` (openrsync, protocol 29) | macOS base system; `scripts/package-macos.sh` copies sources with it | macOS; known advisories: not verified |
| Qt | API minimum **6.8**; builds with 6.8.2 (Debian 13) and **6.12.0** (macOS, Windows) | `find_package(Qt6 6.8 …)` in `CMakeLists.txt`, installers | see below |

### Qt (confirmed at the H0.2 review, 2026-10-15)

| Platform | Version | Source | Integrity |
|---|---|---|---|
| Debian 13 amd64 | 6.8.2 (`qt6-base-dev 6.8.2+dfsg-9+deb13u2`) | Debian packages | apt signatures; security updates from Debian |
| macOS | 6.12.0 | Homebrew `qt` | Homebrew bottle checksums |
| Windows 11 x64 | 6.10.3 (`win64_msvc2022_64`) | Qt online repository through aqtinstall 3.3.0 | aqtinstall verifies the hashes published by the Qt repository |

The Windows MSI (`scripts/package-msi.ps1`, 2026-10-20) uses 6.10.3 because
the Qt repository did not publish the `Updates.xml` metadata that aqtinstall
3.3.0 needs for 6.11.x or 6.12.0 on Windows at that date. Both are above the
6.8 floor. `scripts/development/Install-Windows.ps1` still defaults to 6.12.0
and must be aligned if 6.12.0 becomes reachable.

- Modules used: Core, Gui, Widgets, **Network** (QLocalServer, H1.2; in
  `qt6-base-dev` and the aqtinstall base) and **Core5Compat** (Scintilla's Qt
  layer uses `QTextCodec`; Debian `qt6-5compat-dev`, aqtinstall module
  `qt5compat`, included in Homebrew `qt`). Added in H0.3.
  **LinguistTools** (build-time `lupdate`/`lrelease`, H1.6b): part of Qt
  Tools; Homebrew `qt`, Debian `qt6-tools-dev` + `qt6-l10n-tools`, the
  aqtinstall base on Windows (not verified). Nothing new is linked; the
  `.qm` files are embedded resources.
  **PrintSupport** (H4.1a, approved 2026-10-17: print dialog and
  `QPrinter`): part of Qt Base (Homebrew `qt`, Debian `qt6-base-dev`, the
  aqtinstall base); no new third party. PDF export uses `QPdfWriter` from
  Gui. Packaging with the new module not verified yet.
- Licence: LGPL-3.0, dynamic linking only (DESIGN §11).
- Reason: Debian 13 ships the 6.8 LTS line and receives security fixes from
  Debian. Open-source 6.8 builds from Qt stop at 6.8.3 (no further public
  patches), so macOS and Windows use the current release, 6.12.0. The API
  floor stays at 6.8; building on Debian catches any use of newer APIs.
- Residual risk: two Qt versions are in use; behaviour differences are
  caught only by running on each platform (cross-OS validation).

### Libraries bundled in the macOS DMG (H1.6c)

`macdeployqt` copies the libraries Homebrew's Qt links against into
`Ferny.app/Contents/Frameworks`: QtCore, QtGui, QtWidgets, QtNetwork,
QtCore5Compat, QtDBus, and libb2, brotli, OpenSSL (libcrypto, libssl),
dbus, double-conversion, freetype, glib (with libintl), graphite2,
harfbuzz, ICU, md4c, pcre2, libpng, zstd. They are not direct
dependencies of Ferny; their versions are those installed by Homebrew on
the build machine and are listed in each DMG's `COMPONENTS.txt`.
Licences: permissive or LGPL (glib, dbus: LGPL/AFL-GPL); licence texts for
each library are not yet included in the DMG (not verified, H4.3).

## Vendored C/C++ (`third_party/`)

| Component | Version | Licence | Origin | Archive SHA-256 | Cross-check |
|---|---|---|---|---|---|
| Scintilla (incl. `qt/ScintillaEdit`) | 5.6.7 | HPND-style (`third_party/scintilla/License.txt`) | https://www.scintilla.org/scintilla567.tgz | `19e4476706750056f86ca70974810e4a5b08e73477e13f7d9fae78243606f6a7` | Identical to the SourceForge release file |
| Lexilla | 5.5.4 | HPND-style (`third_party/lexilla/License.txt`) | https://www.scintilla.org/lexilla554.tgz | `fcd9ef74236a12359d1039827c93dbf827aa476c46f9fd6eef60bb96db1af2f6` | Same tree as GitHub tag `ScintillaOrg/lexilla` `rel-5-5-4` (only VCS files differ) |

| Corrosion | 0.6.1 | MIT (`third_party/corrosion/LICENSE`) | https://github.com/corrosion-rs/corrosion tag `v0.6.1` (commit `1499b14e4906a2890f5cee1547c8848db261753d`) | — (git tag, no archive) | Tag commit confirmed with `git ls-remote`; `cmake/`, top-level files only (tests and docs not vendored) |
| Geany data (subset; templates and snippets added in H2.5b, same tag, existing files verified identical) | 2.1.0 | GPL-2.0-or-later; colour schemes per their headers (GPL/LGPL-2.0-or-later, MIT, BSD, public domain) | https://github.com/geany/geany tag `2.1.0` (commit `cb5ff3cf0b63971010e60ea2feedd1a0181c1ace`) | — (git tag) | Tag commit confirmed with `git ls-remote`; maintainer chose the version (2026-10-15); converted to `data/` by `tools/geany-import` |
| cxx-qt-cmake | 0.10.0 | MIT OR Apache-2.0 (`third_party/cxx-qt-cmake/LICENSES/`) | https://github.com/KDAB/cxx-qt-cmake tag `0.10.0` (commit `64e63f560737b3b7f85ef8267c5918e4b8176d5a`) | — (git tag, no archive) | Tag commit confirmed with `git ls-remote`; same release as the cxx-qt crates (0.10.0); `.github/` not vendored |

- Corrosion and cxx-qt-cmake are CMake modules (build-time code, run by
  CMake). They are vendored so that configuring never downloads anything:
  `CxxQt.cmake` would otherwise fetch Corrosion v0.5.2 from GitHub at
  configure time; Ferny sets `FETCHCONTENT_SOURCE_DIR_CORROSION` to the
  vendored copy and `FETCHCONTENT_FULLY_DISCONNECTED=ON`. Neither tag is
  signed (no signature to verify); trust rests on the tag commit from the
  official repository. Added in H0.3.
- Maintenance: Scintilla and Lexilla are actively maintained by Neil Hodgson, regular releases.
- Advisories: no CVE database covers them directly; follow the release
  notes (`doc/ScintillaHistory.html`, `doc/LexillaHistory.html`) on each
  update.
- Integrity in the repository: `third_party/scintilla.sha256` and
  `third_party/lexilla.sha256` list every file; the validation gate checks
  them. Update procedure in `third_party/README.md`.

## Rust crates

Selected in H0.2. Each is added to a manifest in the milestone that first
needs it; the "In use" column says which already are. Versions are the
latest stable on 2026-10-15.

| Crate | Version | Licence | MSRV | Origin | Used for | First use |
|---|---|---|---|---|---|---|
| cxx-qt, cxx-qt-lib, cxx-qt-build | 0.10.0 | MIT OR Apache-2.0 | 1.85 | crates.io, github.com/KDAB/cxx-qt | Rust ↔ Qt bridge | **In use (H0.3)**, `ferny_bridge`; cxx-qt-lib without default features |
| cxx | 1.0.205 | MIT OR Apache-2.0 | 1.88 | crates.io, github.com/dtolnay/cxx | FFI used by the cxx-qt bridge | **In use (H0.3)** |
| ropey | 1.6.1 (stable; 2.0 is beta) | MIT OR Apache-2.0 | 1.65 | crates.io, github.com/cessen/ropey | Rope mirror of buffers | H1.1 |
| encoding_rs | 0.8.42 | (Apache-2.0 OR MIT) AND BSD-3-Clause | 1.88 | crates.io, github.com/hsivonen/encoding_rs | Encodings | **In use (H1.1)**, `ferny-core` |
| chardetng | 1.0.0 (default features off) | Apache-2.0 OR MIT | 1.40 | crates.io, github.com/hsivonen/chardetng (Mozilla, Firefox's detector) | Encoding detection | **In use (H1.1)**, `ferny-core`; maintainer decision 2026-10-15. Has two small `unsafe` SIMD blocks (x86 only) |
| serde | 1.0.229 (`derive`) | MIT OR Apache-2.0 | 1.56 | crates.io | Settings (de)serialisation | **In use (H1.2)**, `ferny-core` session, `ferny-syntax` (H1.3a); brought forward from H1.5 (maintainer decision) |
| toml | 1.1.8 | MIT OR Apache-2.0 | 1.85 | crates.io, github.com/toml-rs/toml | TOML settings | **In use (H1.2)**, `session.toml`; `ferny-config` (H1.5a) |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | 1.77 | crates.io | Typed errors | **In use (H0.3)**, `ferny-core` |
| tracing | 0.1.44 | MIT | 1.65 | crates.io, github.com/tokio-rs/tracing | Local logging | Not used: the local log (H3.2) is a few lines in `ferny-core::log` |
| regex | 1.13.1 | MIT OR Apache-2.0 | 1.65 | crates.io, github.com/rust-lang/regex | Search (linear-time engine) | **In use (H1.3a)** by `tools/geany-import`; in `ferny-core` (`.editorconfig` globs) since H1.3c and search since H1.4 |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | 1.56 | crates.io, github.com/serde-rs/json | JSON-RPC with language servers | **In use (H3.1a)**, `ferny-lsp`, `ferny-bridge`; approved 2026-10-16; was already in `Cargo.lock` as a build dependency, no new crates; `cargo audit`: no advisories |
| tree-sitter | 0.27.1 | MIT | 1.90 | crates.io, github.com/tree-sitter/tree-sitter (official) | Parsing for symbols | **In use (H2.2)**, `ferny-syntax`; C runtime compiled with `cc`; brings `tree-sitter-language` 0.1.x and `streaming-iterator` 0.1.9 (MIT OR Apache-2.0) |
| tree-sitter-c / -cpp / -rust / -python / -javascript | 0.24.2 / 0.23.4 / 0.24.2 / 0.25.0 / 0.25.0 | MIT | — | crates.io, github.com/tree-sitter/tree-sitter-{c,cpp,rust,python,javascript} (official organisation, maintained) | Grammars and their `tags.scm` for the Symbols sidebar | **In use (H2.2)**, approved 2026-10-16; generated C parsers compiled with `cc`; one fuzz target (`symbols`) covers all five |
| mlua (`lua54`, `vendored`) | 0.12.2 | MIT | 1.88 | crates.io, github.com/mlua-rs/mlua | Lua 5.4 plugin host | **In use (H3.2)**, `ferny-plugin` (and the `lua_patterns` fuzz target since the phase 3 audit, as a reference); approved 2026-10-16; features `lua54` + `vendored`: Lua 5.4.9 compiled with `cc` from `lua-src` 551.0.2 (MIT); brings 16 crates (autocfg, bitflags, bstr, either, lock_api, lua-src, luajit-src, mlua-sys, num-traits, parking_lot, parking_lot_core, pkg-config, redox_syscall, rustc-hash, smallvec and mlua itself), all MIT and/or Apache-2.0; `cargo deny` and `cargo audit`: clean |
| pulldown-cmark (`html` only) | 0.13.4 | MIT | 1.71.1 | crates.io, github.com/raphlinus/pulldown-cmark (maintained) | Markdown preview | **In use (H4.2b)**, `ferny-core`; approved 2026-10-17; brings `pulldown-cmark-escape` 0.11.0 (MIT) and `unicase` 2.10.0 (MIT OR Apache-2.0); `cargo deny` and `cargo audit`: clean |
| spellbook | 0.4.2 | MPL-2.0 (compatible with GPL-2.0-or-later as a secondary licence) | 1.70 | crates.io, github.com/helix-editor/spellbook (maintained by the Helix editor project) | Spell checking with Hunspell dictionaries | **In use (H4.2c)**, `ferny-core`; approved 2026-10-17; no new transitive crates (`foldhash`, `hashbrown` already present); `cargo deny` and `cargo audit`: clean; no dictionary bundled |

Evidence (2026-10-15, throwaway project with all the crates above, 171
crates in the resolved graph): `cargo audit` → **no advisories**;
`cargo deny check` with `deny.toml` → advisories, bans, licences and
sources ok; warnings only for duplicate versions (`syn`, `thiserror`,
`unicode-width`, `codespan-reporting`, `image`), to be reduced when the
crates are introduced (cxx-qt-lib's `full` feature pulls `image`; Ferny will
enable only the features it needs).

Evidence for the crates in use (2026-10-15, H0.3): `Cargo.lock` has 72
crates; `cargo audit` → no advisories; `cargo deny check` → ok, with
duplicate-version warnings for `syn`, `thiserror`/`thiserror-impl` (1.x
comes from cxx-qt-gen through the `clang-format` crate), `unicode-width`
and `codespan-reporting`. Note: cxx-qt-gen formats the generated C++ with
the `clang-format` binary when it is on the `PATH` (build time only).

Evidence for H1.1 (2026-10-15): `Cargo.lock` has 78 crates (new:
encoding_rs, chardetng and their dependencies cfg-if, memchr, scopeguard,
simdutf8, multiversion_no_op — all MIT/Apache-2.0 or Unlicense/MIT);
`cargo audit` → no advisories; `cargo deny check` → ok, same
duplicate-version warnings as H0.3.

Fuzzing (not shipped, own workspace in `fuzz/`, `fuzz/Cargo.lock`, 26
crates): libfuzzer-sys 0.4.13, (MIT OR Apache-2.0) AND NCSA, crates.io,
github.com/rust-fuzz/libfuzzer; builds libFuzzer with `cc` 1.8.0. Runs on
the nightly toolchain (`cargo +nightly fuzz`), macOS/Linux only.

Evidence for H2.2 (2026-10-16): `Cargo.lock` has 97 crates (new:
tree-sitter, the five grammars, tree-sitter-language, streaming-iterator);
`cargo audit` → no advisories; `cargo deny check` → advisories, bans,
licences and sources ok.

Evidence for H1.2 (2026-10-15): `Cargo.lock` has 84 crates (new: serde,
serde_core, serde_derive, toml and its parser/writer crates, winnow —
MIT/Apache-2.0); `cargo audit` and `cargo deny` results in the H1.2
report.

## Development tools

Installed by `docs/scripts/install-dev-tools.sh` and
`docs/scripts/Install-DevTools.ps1`. Not shipped with Ferny. Versions
observed on the development Mac on 2026-10-15:

| Tool | Version | Licence | Purpose |
|---|---|---|---|
| gitleaks | 8.30.1 | MIT | Secret scanning |
| cargo-deny | 0.20.2 | MIT OR Apache-2.0 | Licences, sources, advisories, bans |
| cargo-audit | 0.22.2 | MIT OR Apache-2.0 | RustSec advisories |
| cargo-fuzz | 0.13.2 | MIT OR Apache-2.0 | Fuzzing (macOS/Linux) |
| shellcheck | 0.11.0 | GPL-3.0 | Shell script lint |
| semgrep | 1.180.0 | LGPL-2.1 | SAST |
| clang-format | 23.1.3 | Apache-2.0 WITH LLVM-exception | C++ formatting |
| cmake | 4.4.4 | BSD-3-Clause | Build |
| ninja | 1.13.2 | Apache-2.0 | Build |
| aqtinstall (Windows) | 3.3.0 | MIT | Qt installation |
| Python (Windows, for aqtinstall) | 3.12.10 (WinGet `Python.Python.3.12`) | PSF-2.0 | Runs aqtinstall; not verified beyond the WinGet hash check |
| WiX Toolset (Windows) | 3.14 (`C:\Program Files (x86)\WiX Toolset v3.14`) | Microsoft Reciprocal License (not verified) | Builds the MSI (`packaging/windows/Ferny.wxs`, `scripts/package-msi.ps1`); origin and known vulnerabilities not verified |
