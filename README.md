# Ferny

**A fast, lightweight programmer's editor for macOS, Linux and Windows,
inspired by [Geany](https://www.geany.org/).**

Ferny brings Geany's familiar workflow to a modern codebase: a Rust core,
a native Qt 6 interface, the Scintilla editing component, tree-sitter and
Language Server Protocol (LSP) intelligence, and sandboxed Lua 5.4 plugins.
It is designed to stay quiet by default: opening a file never runs code,
project tools need you to trust the folder first, and nothing is sent over
the network.

> **Status:** v0.6.17, a phase 4 maintenance release. The next milestone is
> the 1.0 release candidate (H4.4). See [Project status](#project-status).

## Contents

- [Highlights](#highlights)
- [Features](#features)
- [Installation](#installation)
- [Building from source](#building-from-source)
- [Configuration](#configuration)
- [Security](#security)
- [Documentation](#documentation)
- [Repository layout](#repository-layout)
- [Contributing](#contributing)
- [Project status](#project-status)
- [License](#license)

## Highlights

- **Geany-style workflow.** Tabs, sessions, a sidebar with Documents, Files
  and Symbols, a message window for build output, and a status bar.
- **Safe file handling.** Atomic saves, encoding detection, BOM and line-ending
  control, external-change detection and bounded handling of very large files.
- **Code intelligence.** tree-sitter symbols and folding, plus LSP diagnostics,
  completion, hover, go to definition, find usages, rename and formatting.
- **Trust before execution.** Build commands, language servers and Make run
  only in folders you have trusted. Plugins run only after you approve their
  permissions.
- **Plugins in Lua 5.4.** A small, versioned API with permissions, a sandbox,
  and 24 bundled plugins ported from Geany's plugins.
- **Geany compatibility.** Imports your Geany settings, keybindings, filetypes,
  colour schemes, project files and templates.
- **No telemetry.** Ferny sends no analytics and no crash reports.

## Features

### Editing

- Scintilla editing with multiple carets, rectangular selection, mark
  occurrences, enclose selection, and bookmarks.
- Auto-indent, indentation detection, auto-close for brackets and quotes,
  brace matching, smart Home, and comment toggling per filetype.
- Line operations (duplicate, move, delete), case conversion and trailing
  whitespace handling.
- Word wrap, indent guides, whitespace and end-of-line display, long-line marker,
  zoom in/out/normal.
- Visible bidirectional and invisible Unicode characters, so spoofed or
  hidden code is easy to notice.
- Word and symbol completion, templates and snippets (Tab after the keyword).
- Spell checking with your Hunspell dictionaries (off by default).
- Autosave and backup copies (off by default).
- Markdown preview and split view.

### Files and sessions

- Tabs, new, open, save, save as, save all, close, reopen closed.
- Encodings with BOM detection, LF / CRLF / CR line endings, read-only mode.
- Atomic saves (temporary file, fsync, rename) that preserve file permissions,
  with external-change checks.
- Recent files, session restore (files, cursor positions, window layout), and
  `file:line:col` arguments on the command line.
- Single instance: opening a second file forwards it to the running window.
- `.editorconfig` support (allowlisted keys) and Geany's `-*- mode -*-` modelines.

### Language support

- About 90 filetypes highlighted through Lexilla, and 46 colour schemes
  (generated from Geany 2.1.0 data).
- tree-sitter symbols and folding for C, C++, Rust, Python and JavaScript.
- LSP client with diagnostics and navigation. Language servers are configured
  per filetype; examples used in testing are clangd, rust-analyzer, pylsp and
  typescript-language-server.
- Go to symbol, Go to Workspace Symbol, Go to Definition, back/forward
  navigation and a command palette.

### Search

- Find and replace, with plain text or regular expressions, whole word, case
  and in-selection options and an incremental search bar.
- Find in files with a results panel.
- Regex searches use a linear-time engine, so pathological patterns cannot
  freeze the editor.
- Go to line.

### Build and projects

- Build menu with Geany's compile, build, make and run commands, per filetype
  and per project. Commands run from an explicit argument list, never through a
  shell.
- Compiler panel with error-pattern parsing and jump to error; ANSI escape
  sequences are neutralised.
- Projects in `.ferny/project.toml`, and import of Geany `.geany` projects.
- Build and Make run only in trusted folders.

### Git (read-only)

- Git status shown in the Files sidebar, and Git Blame, in trusted folders.
- Ferny does not commit, push or change repository state.

### Export and printing

- Print with colours.
- Export to PDF, HTML and LaTeX.

### Plugins

- Lua 5.4 plugins from your configuration folder, each with a `plugin.toml`
  manifest. The API can add commands, react to document events, provide
  completion, hover and formatting, and adjust text before saving.
- Permissions: `config`, `fs.read.project`, `fs.read.any`, `fs.write.project`,
  `process.spawn` and `clipboard`. The first use of a sensitive permission asks
  you again. Plugins have no network permission.
- Plugin manager under **Tools → Plugins…**, with origin, status and permissions
  for each plugin.
- `--safe-mode` starts Ferny without plugins.
- 24 bundled plugins in [`plugins/`](plugins/) include HTML Characters, Class
  Builder, Auto-close, Automark, Line Operations, Overview, Spell Check,
  Pretty Printer, Table Convert and Markdown tools. HTML Characters and Class
  Builder are compiled into the application and stay disabled until you enable
  them in Tools → Plugins….

Plugin authors: see [Writing plugins](docs/PLUGINS.md) and the type stubs in
[`data/plugin-api/ferny.d.lua`](data/plugin-api/ferny.d.lua). The API is
version 0 until Ferny 1.0 freezes it.

### Interface and localisation

- English and Spanish interface. Follows the system language, or set
  `ui.language`.
- Preferences organised in categories (General, Editor, Files, Templates,
  Security), with every setting in one place.
- Fixed main toolbar for the most common commands.

## Installation

Ferny is built from source today. Release packages are produced by the
scripts in [`scripts/`](scripts/) and documented in
[`packaging/README.md`](packaging/README.md):

| Platform | Package | Status |
|---|---|---|
| macOS (arm64) | `.dmg` (ad-hoc signed, not notarised) | Built on the development Mac; installation on a clean Mac not verified |
| Linux (amd64) | `.deb` | Built; desktop installation not verified |
| Windows (x64) | `.msi` (unsigned) | Built; not verified on a clean machine |

Flatpak, MSIX and a universal macOS build are planned for later phases.
Signed and notarised releases need explicit maintainer approval.

## Building from source

You need CMake 3.28 or later, Ninja, a Rust toolchain (pinned in
[`rust-toolchain.toml`](rust-toolchain.toml)) and Qt 6. Full setup instructions
are in [`docs/SETUP.md`](docs/SETUP.md), and the dependencies are listed in
[`docs/DEPENDENCIES.md`](docs/DEPENDENCIES.md).

Build and run in one step:

```sh
scripts/dev.sh [file]                                                  # macOS / Linux
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev.ps1 [file]  # Windows
```

Or step by step:

```sh
cmake -S . -B build/dev -G Ninja -DCMAKE_BUILD_TYPE=Debug
cmake --build build/dev
ctest --test-dir build/dev --output-on-failure
build/dev/ui/ferny path/to/file.rs
```

Install the development tools (preview first with `--dry-run`):

```sh
bash docs/scripts/install-dev-tools.sh --dry-run    # macOS / Debian 13
bash docs/scripts/install-dev-tools.sh
```

```powershell
# Windows 11
powershell -NoProfile -ExecutionPolicy Bypass -File docs\scripts\Install-DevTools.ps1 -DryRun
powershell -NoProfile -ExecutionPolicy Bypass -File docs\scripts\Install-DevTools.ps1
```

## Configuration

- **Settings** are TOML files in layers, and later layers win: built-in
  defaults, system folder, user folder, then the project's
  `.ferny/project.toml`. `.editorconfig` also applies to indentation, line
  endings, charset and trailing whitespace.
- **Untrusted projects** can only change presentation settings (indentation,
  line endings, charset, filetype, wrap). Build commands, language-server
  settings, executables and project plugins are ignored until you trust the folder.
- **Preferences** edits the settings with a description for each option.
- **Keybindings** are stored in `keybindings.toml` and can be changed.
- **Import Geany Settings** reads an existing Geany configuration folder,
  including keybindings, filetypes, colour schemes and file templates.
  **Import Geany Project** does the same for `.geany` project files.
- **Command-line options:**
  - `file[:line[:col]]` opens a file at a position.
  - `--safe-mode` starts without plugins.
  - `--no-session` skips restoring the previous session.

## Security

Ferny treats every file, project folder, `.editorconfig`, modeline, Geany
import, language-server message, plugin and downloaded component as untrusted
input.

- Opening a file never runs code.
- Build commands, Make and language servers require trusting the folder
  (Trust Folder… in the banner or the Document menu; revocable in Preferences).
- Plugins load only from your configuration folder and need your approval.
- Processes are started from an argument list, never from a shell string.
- Saves are atomic (temporary file, fsync, rename).
- The IPC socket for single-instance mode is per user, with restricted
  permissions.
- `unsafe` Rust is allowed only in `ferny-bridge` (`#![deny(unsafe_code)]`
  elsewhere), and each block carries a `// SAFETY:` justification.
- No telemetry or analytics.

Read the [threat model](docs/THREAT-MODEL.md) and the
[security policy](SECURITY.md). To report a vulnerability, follow
[SECURITY.md](SECURITY.md) and do not open a public issue.

## Documentation

| Topic | Document |
|---|---|
| Architecture and design decisions | [docs/DESIGN.md](docs/DESIGN.md), [ARCHITECTURE.md](ARCHITECTURE.md) |
| Technical reference | [docs/TECHNICAL.md](docs/TECHNICAL.md) |
| Threat model | [docs/THREAT-MODEL.md](docs/THREAT-MODEL.md) |
| Security policy | [SECURITY.md](SECURITY.md) |
| Validation and release gate | [docs/RELEASE-GATE.md](docs/RELEASE-GATE.md) |
| Developer setup and dependencies | [docs/SETUP.md](docs/SETUP.md), [docs/DEPENDENCIES.md](docs/DEPENDENCIES.md) |
| Writing plugins | [docs/PLUGINS.md](docs/PLUGINS.md) |
| Screen mockups | [docs/design/index.html](docs/design/index.html) |
| Geany feature parity | [docs/FEATURE-PARITY.md](docs/FEATURE-PARITY.md) |
| Roadmap | [docs/ROADMAP.md](docs/ROADMAP.md) |
| Packaging | [packaging/README.md](packaging/README.md) |

## Repository layout

```
crates/                 Rust core (no Qt dependency, except ferny-bridge)
  ferny-core            documents, sessions, encodings, command registry, file I/O
  ferny-config          settings, TOML layers, Geany importer, projects
  ferny-syntax          filetypes, Lexilla mapping, tree-sitter symbols
  ferny-search          find and replace, find in files
  ferny-build           build commands, argument-vector process runner, error parsing
  ferny-lsp             Language Server Protocol client
  ferny-plugin          Lua 5.4 plugin host, sandbox and permissions
  ferny-bridge          the only crate with unsafe code; bridge to the Qt UI
ui/                     C++17 / Qt 6 application
third_party/            vendored Scintilla, Lexilla, Corrosion, cxx-qt-cmake and
                        Geany 2.1.0 data (with SHA-256 manifests)
tools/                  development tools (geany-import, ferny-sbom)
data/                   filetypes, colour schemes, templates, snippets, plugin API stubs
plugins/                bundled Lua plugins
i18n/                   translations
packaging/              macOS, Windows and Linux packaging files
scripts/                build, package and validation scripts
docs/                   design, threat model, technical and release documentation
test/                   fixtures, security regression tests and script tests
fuzz/                   fuzz targets (separate Cargo workspace)
imagenes/               images and logos
```

## Contributing

Before you start, read [docs/DESIGN.md](docs/DESIGN.md) for the architecture, and
[docs/THREAT-MODEL.md](docs/THREAT-MODEL.md) for the trust boundaries.

Enable the secret-scanning hooks once per clone (requires
[gitleaks](https://github.com/gitleaks/gitleaks)):

```sh
git config core.hooksPath .githooks
```

Run the validation gate before committing (see
[docs/RELEASE-GATE.md](docs/RELEASE-GATE.md)):

```sh
scripts/check.sh                                                       # macOS / Linux
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\check.ps1  # Windows
```

The gate runs formatting, Clippy, tests, `cargo deny`, `cargo audit`,
gitleaks, the C++ build, CTest, Semgrep and `git diff --check`. Checks that
cannot run on your machine are reported as not verified, not as passing.

## Project status

- **Current version:** v0.6.17 (phase 4 maintenance release).
- **Done:** the editor and file handling (phase 1), symbols, build and projects
  (phase 2), the LSP client and Lua plugin host (phase 3), and printing, export,
  split view, Markdown preview, spell check and Git integration (phase 4).
- **Next:** the 1.0 release candidate (H4.4). Remaining 1.0 work includes
  signed and notarised releases, a universal macOS build, Flatpak and MSIX
  packages, and a full security audit of the release candidate.
- **Platform verification:** development and testing happen on macOS (arm64).
  Linux and Windows builds exist but are not yet verified on clean installs.
  Checks not run on a given platform are reported as not verified.
- **Planned change:** the user interface is Qt 6 today. A migration away from
  Qt is planned.
- **Not planned before 1.0:** an embedded terminal, a debugger (DAP) and a
  workbench. Each needs its own threat model first.

The phase-by-phase
scope is in the [roadmap](docs/ROADMAP.md).

## License

Ferny is free software under the GNU General Public License, version 2 or
later (GPL-2.0-or-later). See [LICENSE](LICENSE). Code and data reused from
Geany, and third-party components, keep their attribution in [NOTICE](NOTICE).
