# Ferny — Design Document

Status: v0.3 · 2026-10-15 · **approved by the maintainer** (design review
R1–R5)

Ferny is a lightweight, fast, cross-platform programmer's editor inspired by
[Geany](https://www.geany.org/). It aims for feature parity with Geany while
modernising the stack: a Rust core, a native Qt6 UI, Scintilla as the editing
component, tree-sitter + LSP for code intelligence and Lua 5.4 for plugins.

## 1. Goals and non-goals

### Goals

- **Geany spirit**: starts instantly, small memory footprint, no mandatory
  project system, works on a single file as well as on a tree.
- **Native on macOS, Linux and Windows**: native dialogs, global menu bar on
  macOS, HiDPI, platform keyboard conventions (Cmd vs Ctrl), accessibility.
- **Fast**: cold start < 200 ms, typing latency < 5 ms, 100 MB files open
  without freezing the UI (heavy features degrade gracefully above a threshold).
- **Safe**: memory-safe core (Rust), sandboxed plugins, no network access
  unless the user enables a feature that needs it.
- **Stable plugin API**: plugins written against API `1.x` keep working across
  every `1.x` release, independently of UI toolkit changes.
- **Easy migration from Geany**: importer for configuration, keybindings,
  filetypes, colour schemes, snippets, templates and projects.

### Non-goals

- Binary or source compatibility with Geany's C plugin API.
- Becoming a full IDE (no visual designers, no bundled toolchains).
- Tier-1 support for platforms without a build machine: Linux aarch64,
  Windows ARM and BSD are best effort only (no testing, no guarantees).

## 2. Architecture overview

```
┌──────────────────────────────────────────────────────────────────┐
│ ui/  (C++17, Qt6 Widgets)                                        │
│  MainWindow · Tabs · Sidebar · MessagePanel · Dialogs · Prefs     │
│  FernyEditor = ScintillaEdit (Scintilla Qt platform layer)        │
└───────────────▲───────────────────────────────┬──────────────────┘
                │ signals / view updates        │ user intents, SCN_* deltas
┌───────────────┴───────────────────────────────▼──────────────────┐
│ crates/ferny-bridge  (cxx-qt: QObjects implemented in Rust)       │
└───────────────▲───────────────────────────────┬──────────────────┘
                │                               │
┌───────────────┴───────────────────────────────▼──────────────────┐
│ Rust core                                                        │
│  ferny-core     documents, rope mirror, sessions, encodings, fs   │
│  ferny-config   TOML schema, layering, Geany importer             │
│  ferny-syntax   filetypes, Lexilla mapping, tree-sitter           │
│  ferny-search   find/replace, find in files                       │
│  ferny-build    build commands, process spawning, error parsing   │
│  ferny-lsp      LSP client                       (phase 3)        │
│  ferny-plugin   Lua 5.4 host, sandbox, plugin API (phase 3)       │
└──────────────────────────────────────────────────────────────────┘
```

Rule: **nothing below `ferny-bridge` depends on Qt.** The core is testable
headless and the plugin API talks only to the core, so plugins survive UI
changes.

### 2.1 Why this stack

| Concern | Decision | Reason |
|---|---|---|
| OS integration | Qt6 Widgets | Best native look and behaviour on macOS and Windows, mature on Linux (Wayland + X11). GTK is weaker exactly where Geany is weakest. |
| Editing component | Scintilla (`ScintillaEdit`, Qt platform layer) | Same component as Geany: proven multi-cursor, folding, rectangular selection, fast rendering. Lets us reuse Geany's filetype/style knowledge. |
| Core language | Rust | Memory safety and performance for buffers, parsing, search, process and plugin management. |
| Rust ↔ Qt | `cxx-qt` (KDAB) + Corrosion (CMake) | Maintained, supports Qt6, generates QObjects from Rust. |
| Highlighting | Lexilla baseline + tree-sitter upgrade | Lexilla covers all ~90 Geany filetypes on day one; tree-sitter replaces it per language where a grammar exists (better highlighting, symbols, folding). |
| Code intelligence | tree-sitter (offline) + LSP (semantic) | Symbols and navigation work without external tools; LSP adds completion, diagnostics, references when a server is installed. |
| Plugins | Lua 5.4 via `mlua` | Small, portable, easy to sandbox, familiar to Geany (GeanyLua) users. |
| Config | TOML | Typed, diff-friendly, comments allowed. Geany keyfiles are imported, not used natively. |

We use upstream **Scintilla + Lexilla** directly, not QScintilla (GPLv3-only)
— see §10.

## 3. Text model and threading

- **Scintilla owns the authoritative buffer** for each open document (it
  already handles undo, selections, rendering).
- The core keeps a **rope mirror** (`ropey`) per document, updated from
  `SCN_MODIFIED` insert/delete deltas forwarded by the bridge. The same deltas
  produce tree-sitter `InputEdit`s and LSP `didChange` events, so all
  incremental consumers are fed from one stream.
- Background work (parsing, find in files, symbol extraction, LSP, builds) runs
  on a Tokio runtime + a rayon pool. Results carry the document **revision**
  they were computed for; stale results are dropped.
- Mutations requested by the core or by plugins are sent as commands to the UI
  thread and applied through Scintilla, grouped into a single undo action.

## 4. Syntax and filetypes

- A **filetype** is a TOML file (`data/filetypes/<id>.toml`) with: name, glob
  and shebang/modeline detection, comment tokens, indentation defaults,
  Lexilla lexer + style mapping, optional tree-sitter grammar + queries, build
  commands, error regex, and LSP server hint.
- Initial set: converted from Geany's `data/filedefs/filetypes.*` and
  `filetype_extensions.conf` by the importer (license-compatible, see §10).
- Highlighting path:
  - Lexilla lexer → Scintilla styles (synchronous, existing behaviour).
  - tree-sitter → Scintilla container styling (`SCN_STYLENEEDED`): the core
    runs `highlights.scm` on the changed range and returns style runs.
- Colour schemes: TOML mapping semantic names (`keyword`, `string`,
  `comment.doc`, …) to colours. Geany's ~47 colour schemes are imported. Both
  Lexilla style ids and tree-sitter capture names map onto the same semantic
  names, so one scheme drives both paths.

## 5. Configuration

Layering (later wins):

1. Built-in defaults (compiled in)
2. System: `/etc/ferny/`, `%PROGRAMDATA%\ferny\`, app bundle `Resources/`
3. User: `$XDG_CONFIG_HOME/ferny/`, `~/Library/Application Support/Ferny/`,
   `%APPDATA%\Ferny\`
4. Project: `<project>/.ferny/project.toml`
5. `.editorconfig` (indentation, EOL, charset, trailing whitespace) — optional,
   on by default

Project-level layers come from untrusted directories: in an untrusted
workspace only an allowlist of presentation keys (indentation, EOL, charset,
filetype, wrap) is applied; build commands, LSP settings, executables and
project plugins are ignored until the folder is trusted (see §10).

Per-document indentation (v0.6.10) follows this order, first match wins:
`.editorconfig` → project `.ferny/project.toml` indentation → filetype defaults
→ indentation detected from the file (`editor.detect_indent`, off by default)
→ user settings (`editor.indent_width`, `editor.indent_use_tabs`). Tab width
follows `.editorconfig` first, then `editor.tab_width` (0 = indentation width).
Detection reads at most 500 lines (`ferny-core::editing::detect_indent`).

User directory layout:

```
ferny/
  settings.toml        # editor, UI, files, search, terminal, …
  keybindings.toml
  filetypes/<id>.toml  # overrides, merged key-by-key with built-ins
  colorschemes/<name>.toml
  snippets.toml
  templates/
  plugins/<plugin-id>/
  session.toml         # open files, cursor positions, window geometry
```

Example `settings.toml`:

```toml
[editor]
font = "JetBrains Mono 12"
indent = { type = "spaces", width = 4 }
line_numbers = true
long_line_marker = 100
auto_close = ["(", "[", "{", "\"", "'"]

[files]
default_encoding = "utf-8"
default_eol = "lf"
strip_trailing_whitespace = true
ensure_final_newline = true
reload_on_external_change = "ask"   # ask | always | never

[ui]
color_scheme = "darcula"
sidebar = "left"
```

Settings are defined once in Rust (serde structs with defaults and doc
comments); the preferences dialog, JSON schema (for editor validation) and
reference documentation are generated from those definitions.

### 5.1 Geany importer

`ferny import-geany [--from <dir>]` (also offered on first run) converts:

| Geany | Ferny |
|---|---|
| `geany.conf` | `settings.toml` (+ `session.toml` from the `[files]` section) |
| `keybindings.conf` | `keybindings.toml` (action names mapped 1:1 where they exist) |
| `filetype_extensions.conf`, `filedefs/filetypes.*` | `filetypes/*.toml` |
| `colorschemes/*.conf` | `colorschemes/*.toml` |
| `snippets.conf` | `snippets.toml` |
| `templates/` | `templates/` (placeholders translated) |
| `*.geany` projects | `.ferny/project.toml` |

Unmappable keys are reported, never silently dropped.

## 6. UI

Mirrors Geany's layout, which is its main UX asset:

- Menu bar (native global menu on macOS), configurable toolbar.
- Document tabs with drag & drop, middle-click close, MRU switching.
- **Sidebar**: Symbols, Documents (tree or flat), Files (file browser).
- **Message panel**: Status, Compiler, Messages, Scribble, Terminal.
- Status bar: line/column, selection, encoding, EOL, indentation, filetype,
  read-only, modified.
- Split view, fullscreen, focus mode, zoom.
- Command palette (new): fuzzy search over every registered command — cheap
  to add because all actions go through one command registry.

All user actions are **commands** (`file.save`, `edit.duplicate_line`, …)
registered in the core. Menus, toolbar, keybindings, the palette and Lua
plugins all invoke the same registry.

i18n: UI strings via Qt Linguist (`.ts`), starting with English and Spanish.
The core never produces user-facing prose; it returns typed errors that the UI
translates. Plugins ship their own message catalogs.

## 7. Plugin system (Lua 5.4)

### 7.1 Packaging

```
plugins/hello/
  plugin.toml
  init.lua
  locale/es.toml
```

```toml
# plugin.toml
id = "org.example.hello"
name = "Hello"
version = "0.1.0"
api = "1.0"                     # minimum API version required
description = "Says hello on save"
permissions = ["fs.read.project", "process.spawn"]
```

### 7.2 API shape

```lua
-- init.lua
local ferny = require "ferny"

return {
  activate = function(ctx)
    ctx:on("document.saved", function(doc)
      ferny.ui.status("Saved " .. doc.path)
    end)

    ctx:command {
      id = "hello.say",
      title = ferny.tr "Say hello",
      keys = "Ctrl+Alt+H",
      run = function()
        local doc = ferny.documents.current()
        doc:insert(doc:cursor(), "hello")
      end,
    }
  end,
  deactivate = function() end,
}
```

Modules: `ferny.documents`, `ferny.editor` (selections, cursors, indicators,
markers), `ferny.ui` (status, messages, panels, menus, dialogs, quick-pick),
`ferny.project`, `ferny.build`, `ferny.spawn`, `ferny.fs`, `ferny.settings`,
`ferny.symbols`, `ferny.keys`, `ferny.tr`.

**Events** cover Geany's signal set: `document.new/opened/activated/
before_save/saved/reloaded/closed/filetype_changed`, `editor.changed/
char_added/selection_changed/hover`, `project.opened/before_close/closed/
saved`, `build.started/finished`, `app.startup_complete/before_quit`,
`settings.loaded/saved`.

**Extension points** (equivalent to Geany 2.1's `PluginExtension`, generalised):
completion, signature help/calltips, go to definition/declaration, symbol
providers, hover, diagnostics, formatters, symbol highlighting. Built-in
providers (tree-sitter, LSP) use the same mechanism, with priorities, so a
plugin can override them per filetype.

### 7.3 Runtime and safety

- One Lua state per plugin, executed on a plugin worker thread; the UI never
  waits on Lua except for explicitly synchronous hooks (`before_save`,
  completion), which have a time budget (default 50 ms) after which the hook
  is skipped and the plugin flagged.
- Instruction-count hook to stop runaway scripts; memory limit of 64 MB per
  state (defaults approved in R2).
- Standard library reduced: no `io`, `os.execute`, `loadlib`, `debug`. File
  system, process and network access only through `ferny.*` APIs gated by the
  manifest `permissions`, granted at install; sensitive permissions
  (`process.spawn`, `net`, `fs.read.any`) also ask for confirmation on
  first use. Bundled plugins run in the same sandbox with the same
  permission model (R2).
- Plugins can be disabled at startup with `--safe-mode`.

### 7.4 Compatibility policy

- Semantic versioning of the API (`ferny.api_version`). Within a major
  version only additive changes; deprecated functions keep working for the
  whole major and log a warning.
- API surface is described once in Rust and generates: LuaLS type stubs
  (`ferny.d.lua`) for autocompletion while writing plugins, reference docs and
  a compatibility test suite that runs every bundled plugin in the
  validation script.

## 8. Build system and tooling

- Top-level **CMake** builds the Qt app and drives Cargo through Corrosion.
- Scintilla + Lexilla vendored under `third_party/` at pinned releases.
- No CI server: the maintainer's Git server is a plain Git server. Validation runs locally on
  each OS through `scripts/check.sh` / `scripts/check.ps1` and Git hooks
  ([RELEASE-GATE.md](RELEASE-GATE.md)): `cargo test`, `clippy`, `rustfmt`,
  `cargo deny`, `cargo audit`, gitleaks, Semgrep, short fuzzing runs, CTest
  UI smoke tests. Tier-1 targets (one build machine each):
  macOS universal (arm64 + x86_64, built on the arm64 development Mac),
  Linux x86_64 (Debian 13), Windows x86_64 (Windows 11); Linux and
  Windows are built by the maintainer. Linux aarch64 and Windows ARM are
  best effort until a machine exists to test them.
- Packaging: `.dmg` (signed + notarised once R4 certificates exist),
  AppImage + Flatpak + `.deb`,
  MSIX/MSI, each with SHA-256 checksums, SBOM and provenance. Artefacts are
  stored in `bin/` as `ferny-v{VERSION}-{platform}-{arch}`.

## 9. Geany behaviours worth preserving

- Single instance: second invocation forwards files over a per-user local
  socket (`QLocalServer`, restricted permissions, see §10),
  `ferny file.c:120:5` and `+line` syntax.
- No forced project: everything works per file; projects only add base dir,
  build commands, settings and session.
- Build menu semantics: Compile / Build / Make / Make custom / Run, per
  filetype with project overrides, independent and execute commands, error
  regex → jump to location.
- Navigation history (back/forward across jumps).
- Templates with placeholders (`{filename}`, `{year}`, `{developer}`, …).
- Snippets with tab stops, per filetype.
- Scribble (persistent scratch notes).
- Main toolbar: a fixed, text-labelled set of built-in commands, dispatched
  through the shared command registry. User-configurable action lists are
  deferred until their schema, defaults and validation limits receive review.
  Build and Execute remain governed by the existing workspace-trust checks;
  placing them on the toolbar does not change those checks.

## 10. Security model

Full analysis in [THREAT-MODEL.md](THREAT-MODEL.md); process in
[SECURITY.md](../SECURITY.md). Design-level decisions:

- **Opening a file never executes code.** Highlighting, symbols and
  `.editorconfig`/modelines (allowlisted keys only) are the only things that
  happen on open.
- **Workspace trust** (ships with build commands in phase 2): folders are
  untrusted until the user trusts them; trust is stored per canonical path in
  the user config (`trust.toml`) and can be given to a folder or to a parent
  folder (e.g. `~/src`), covering everything below it. Untrusted workspaces
  run in restricted mode: no project build commands, no language server
  spawn, no project-local plugins, no project-supplied executables. A
  non-blocking banner shows the state, and attempting a blocked action asks
  whether to trust the folder. Single files outside any project may use the
  build commands of their filetype from the user's own configuration, as in
  Geany. Trusted folders are listed and revocable in Preferences → Security
  (R1).
- **Process execution** (`ferny-build`, `ferny-lsp`): program + argv only,
  never a shell string; placeholders (`%f`, `%d`, `%e`) expand per argument;
  executables resolved from user-configured paths or `PATH` without
  project directories; sanitised environment; timeouts and output size
  limits; ANSI escapes neutralised before display.
- **Plugins** (§7.3): per-plugin Lua state, stripped stdlib, CPU and memory
  limits, capabilities through manifest permissions (`fs.read.project`,
  `fs.read.any`, `fs.write.project`, `process.spawn`, `net`, `clipboard`);
  no network by default; local activity log.
- **IPC**: per-user socket in the user's runtime dir (0700 dir / 0600
  socket) or per-user named pipe with restrictive DACL on Windows; accepts
  only bounded, validated "open path[:line[:col]]" requests. If the socket
  belongs to another user, Ferny does not connect and starts a separate
  instance (logged); a stale socket is removed and recreated (R3).
- **Saving**: atomic write (temp file in the same directory, fsync,
  rename), preserves permissions, explicit symlink handling, detects
  external modification before overwriting. Backups and session files are
  created with 0600 in the user data dir.
- **Untrusted content**: size thresholds disable heavy features on huge
  files; linear-time regex only; bounded encoding detection; bidi and
  invisible control characters rendered visibly by default.
- **Network**: none by default. No telemetry or crash upload. Grammars are
  compiled in. No automatic updates: users download each version (R4).
- **Distribution** (R4): releases and the minisign public key are published
  on the maintainer's Git server; checksums are signed with minisign. Platform code signing (Apple
  Developer ID, Authenticode) is deferred until approaching 1.0.
- **Memory safety**: `unsafe`/FFI only in `ferny-bridge` and thin wrappers,
  each justified; C/C++ dependencies pinned and fuzzed through our inputs.
- **Logging**: never file contents, clipboard, environment or tokens.

## 11. Licensing

- Ferny is **GPL-2.0-or-later**, which allows reusing code and data from
  Geany (GPL-2.0-or-later). Attribution is kept in `NOTICE`.
- Qt6 is used under LGPLv3. LGPLv3 is incompatible with GPL-2.0-only but
  compatible with GPL-3.0; since Ferny is "or later", **distributed binaries
  are effectively under GPL-3.0**. Source remains GPL-2.0-or-later.
- Scintilla and Lexilla: HPND-style permissive licence. Not QScintilla
  (GPLv3 or commercial) to avoid an unnecessary restriction.
- tree-sitter and grammars: mostly MIT; each grammar's licence is checked by
  `cargo deny` / a vendoring script.
- Lua 5.4: MIT. `mlua`: MIT.
- Imported Geany colour schemes come from several authors: verify each file's
  licence header before bundling.

## 12. Open questions

1. Embedded terminal: there is no cross-platform VTE equivalent in Qt.
   Candidate: `alacritty_terminal` (Rust) rendered in a Qt widget.
2. Which tree-sitter grammars ship in-binary vs. download on demand? (Any
   download needs signed artefacts — threat T12.)
3. Plugin distribution: plain folders only, or a curated index later?
4. Printing: Scintilla has `SCI_FORMATRANGE`; confirm quality on all three
   platforms vs. exporting to PDF via Qt.
