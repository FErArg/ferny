# Architecture — Ferny

Summary for orientation. Full design: [docs/DESIGN.md](docs/DESIGN.md).
Threats and controls: [docs/THREAT-MODEL.md](docs/THREAT-MODEL.md).

## Purpose

Lightweight, native, cross-platform programmer's editor inspired by Geany.

## Components

| Component | Language | Responsibility | Trust level |
|---|---|---|---|
| `ui/` | C++17 / Qt6 | Windows, tabs, panels, dialogs, `FernyEditor` (Scintilla) | Trusted code, renders untrusted content |
| `crates/ferny-bridge` | Rust (cxx-qt) | Rust ↔ Qt boundary; the only place with `unsafe`/FFI | Trusted, security-critical |
| `crates/ferny-core` | Rust | Documents, rope mirror, sessions, encodings, safe-save, IPC, workspace trust, command registry | Trusted |
| `crates/ferny-config` | Rust | Settings layers, Geany importer | Trusted code parsing untrusted input |
| `crates/ferny-syntax` | Rust | Filetypes, Lexilla mapping, tree-sitter | Trusted code parsing untrusted input |
| `crates/ferny-search` | Rust | Find/replace, find in files | Trusted |
| `crates/ferny-build` | Rust | Spawning build commands, error parsing | Security-critical (process execution) |
| `crates/ferny-lsp` | Rust | Language server client | Security-critical (process execution) |
| `crates/ferny-plugin` | Rust + Lua 5.4 | Plugin host, sandbox, permissions | Security-critical (untrusted code) |
| `third_party/` | C/C++, CMake | Scintilla, Lexilla, Corrosion, cxx-qt-cmake (pinned) | Third-party, pinned |

## Trust boundaries

- **B1** opened file content → core/UI
- **B2** project directory (`.ferny/`, `.editorconfig`, `.geany`, modelines) → core, gated by workspace trust
- **B3** Geany configuration → importer
- **B4** network downloads (none until signed updates/grammars exist)
- **B5** other local processes → single-instance IPC
- **B6** Lua plugins ↔ core
- **B7** core → child processes (build tools, language servers)
- **B8** source → release artefacts (build machines, dependencies, signing)

## Main data flows

Implemented in H0.3 (skeleton): CLI path → `FernyCore::loadDocument`
(bridge) → `ferny_core::document::read_text_file` (regular files only,
256 MiB bound, UTF-8 only) → UTF-8 bytes or a status code → `FernyEditor`
(read-only) with a Lexilla lexer chosen by extension.

Documents (H1.1): `ferny_core::document::Documents` keeps, per id, the
path, encoding, BOM, line ending, file stamp and read-only state; the
editor keeps the text. Open: regular file only, at most 256 MiB → BOM /
UTF-8 / chardetng on 64 KiB → decode (malformed → read-only) → EOL;
above 32 MiB highlighting is off. Save: encode (unmappable → refused) →
stamp check → temp file created exclusively next to the target (symlinks
written through to their target) → fsync → rename → directory fsync;
hard-linked or foreign-owned files are written in place. External
changes are checked on window activation and before saving.

Tabs, sessions, single instance (H1.2): one `FernyEditor` per tab, each
bound to a core document id; an already open file (canonical path) is
focused instead of reopened. Arguments (`file:line[:col]`, `+line`) are
parsed by `ferny_core::location`. A second invocation sends an
`ferny-open 1` request (length-prefixed, ≤ 64 KiB, ≤ 64 absolute paths)
over `QLocalServer` in a private 0700 runtime directory; the core
validates it before any file is opened. `session.toml` (open files,
cursors, active tab, geometry, 10 recent files) is written 0600 on quit
and restored when Ferny starts without files.
IPC decimal line/column fields accept leading zeros and normalize to canonical
decimal spelling on re-encode; the fuzz round-trip property compares decoded
locations rather than requiring byte preservation.

Filetypes and colour schemes (H1.3a): `tools/geany-import` converts the
vendored Geany 2.1.0 data into `data/filetypes/*.toml` (lexer, patterns,
interpreters, keywords, properties, styles) and `data/colorschemes/*.toml`;
`ferny-syntax` embeds them at build time, detects the filetype (longest
matching pattern, then shebang) and resolves styles against the chosen
scheme with Geany's semantics. The UI applies them to Scintilla; the
scheme is saved in the session.

Editing (H1.3b): Scintilla does the editing; `FernyEditor` adds
auto-indent, auto-close of `()`, `{}`, `[]`, brace matching, markers,
folding and multiple selections. Comment toggling and the bidi/invisible
character tables are pure functions in `ferny_core::editing`.

Project settings (H1.3c): `ferny_core::editorconfig` resolves the
allowlisted properties for a document from `.editorconfig` files up to
`root = true`; `charset` picks the decoding on open, indentation applies in
the editor, `trim_trailing_whitespace` and `insert_final_newline` are
applied in the editor before saving. Filetype detection follows Geany:
shebang, `-*- name -*-` in the first two lines, then the file name.

Search (H1.4): `ferny_core::search` compiles every query (plain text is
escaped) for the linear-time `regex` engine; the UI sends the document's
UTF-8 text and receives byte ranges or replaced text. Find in files runs
on a background thread started by the bridge and reports back on the Qt
thread (`findInFilesFinished`); a generation counter drops stale results.

Settings (H1.5a): `ferny-config` declares every setting once
(`settings::DESCRIPTORS`: key, type, range, default, title); layers are
built-in defaults, system file and user `settings.toml`; the Preferences
dialog is generated from the descriptors through the bridge.
`keybindings.toml` overrides the registry's default keys. Precedence for
indentation and save transforms: `.editorconfig` > filetype > settings.
The colour scheme moved from `session.toml` to `settings.toml`.

Main toolbar (H5.1): Qt renders a fixed set of built-in command IDs;
`MainWindow` resolves each title from `FernyCore` and invokes through the same
command registry as menus. No project-defined actions or persisted command
strings are accepted. Undo/redo availability is synchronized with the active
Scintilla editor. The fixed toolbar uses embedded Lucide SVG resources; Help →
About is a dialog with fixed, validated links: HTTPS project, author and icon
attribution links, plus the `mailto:` contact address (v0.6.13).
Preferences (H1.5a, redesigned in H5.2) remain descriptor-driven: the dialog
presents every descriptor exactly once in the categories General, Editor,
Files, Templates and Security, links to the separate shortcuts editor and
exposes trusted-folder revocation, which takes effect immediately.
Optional toolbar customization is not approved and remains pending.

Geany import and user catalog (H1.5b): `ferny_config::geany` builds an
`ImportPlan` from a Geany directory (settings, shortcuts, user filedefs
and colour schemes); the UI shows counts and everything not imported, and
only then writes `settings.toml`, `keybindings.toml`,
`filetypes/<id>.toml` and `colorschemes/<id>.toml`.
`ferny_syntax::Catalog::with_user_dir` loads those on top of the built-in
catalog; the bridge keeps the resulting catalog.

UI shell (H1.6a): a sidebar dock (Documents, Files) and a message window
dock (Status, Messages, Find in Files, Scribble); their layout and the
Scribble are saved in `session.toml`. The Files sidebar lists folders
lazily through `ferny_core::browse` (bounded, links never entered,
deceptive characters in names escaped by `display_name`).

Translations (H1.6b): the UI's strings and the core's English titles
(listed in the generated `ui/src/core_strings.cpp`) are translated with
Qt Linguist; `ferny_es.qm` is embedded under `:/i18n` and installed at
start for `ui.language` (system / en / es) or `--language`. Nothing is
loaded from user or project folders.

Release pipeline (H4.3): `tools/ferny-sbom` writes the CycloneDX SBOM;
`scripts/package-macos.sh` adds Developer ID signing, notarisation and
minisign when their environment variables are set (ad-hoc otherwise).

Packaging (H1.6c): `scripts/package-macos.sh` builds `Ferny.app`
(CMake `FERNY_MACOS_BUNDLE`) from `git archive` of a ref, through a cache in
`~/Library/Caches/ferny-package` (source copy and build tree, see
`packaging/README.md`), bundles Qt with
`macdeployqt` (three plugins only, `qt.conf` inside the bundle), signs it
ad-hoc and wraps it in a DMG with licences and a component list.

Windows packaging (v0.6.14): `scripts/package-msi.ps1` builds
`Ferny-v<version>-windows-x86_64.msi` with `windeployqt` (Qt 6.10.3, MSVC kit)
and WiX Toolset v3.14 (`packaging/windows/Ferny.wxs`). It builds from a
committed ref or, with `-WorkingTree`, from uncommitted files (reported as
non-release). It checks the executable version, runs the staged smoke test,
checks the package properties and extracts the package administratively; it
never installs. The MSI is not signed (Authenticode comes before 1.0). The
installer's licence dialog shows the GPL-2.0-or-later text: the script
converts `LICENSE` to RTF at build time (`WixUILicenseRtf`, v0.6.16).

Windows process model (v0.6.15): `ferny.exe` uses the GUI subsystem
(`qt_add_executable(ferny WIN32 ...)`), so starting it opens no console
window. `main` attaches to the parent console only when stdout is not
redirected, so `--help`, `--version` and `--smoke-test` still print in a
terminal. Child programs (LSP servers, plugin commands, build commands) start
with `CREATE_NO_WINDOW`, so they open no window of their own.

Plugins (H3.2): `ferny-plugin` finds `plugins/<id>/` in the user's
configuration folder, checks the manifest against `plugin-grants.toml`
(the UI asks for new or changed ones) and runs each enabled plugin in its
own sandboxed Lua state through `ferny-bridge`; `--safe-mode` skips them.
Events go to the local log (`ferny_core::log`, `ferny.log`).

Plugin API 0 (H3.3a): calls run on the UI thread within 50 ms. The UI
passes the current document (path, filetype, cursor; text and selection
to commands only) through `pluginEmit` / `pluginRunCommand`; the plugin
returns actions (status, message, insert, replace selection) that the UI
applies, edits as one undo action. Commands live in a Tools menu built
after the plugins start.

Permission-gated APIs (H3.3b): files and the clipboard are served inside
the call; `ferny.spawn` only records a request, which the bridge resolves
(`ferny_build::resolve`), runs on a thread (`ferny_plugin::run_process`,
reduced environment from `ferny_lsp`) and hands back through the Qt
queue to the plugin's `on_exit`, whose actions reach the UI as
`pluginActions`. The trusted project root is given to plugins at every
call. A sensitive permission's first use returns a `confirm` action; the
UI asks and stores the answer with `pluginConfirm`.

Extension points (H3.3c): `pluginProvide(kind, …)` asks the plugins'
completion, hover or format providers for the document's filetype (or
runs the `before_save` handlers in order) before the UI falls back to the
language server or the document's words; completion lists are merged.
Plugin commands take keys only from `keybindings.toml`. The API is
described for plugin authors in `docs/PLUGINS.md` and
`data/plugin-api/ferny.d.lua`.

Git, read-only (H4.2d): `gitStatus`/`gitBlame` run `git` (resolved off
`PATH`, trusted folders only) with the safe arguments of
`ferny_core::git` on a thread and answer with `gitStatusReady` /
`gitBlameReady`; the Files tree shows a status column, blame goes to the
results tab.

Spell checking (H4.2c): the UI blanks everything but comment and string
styles of code (Lexilla style tags), `spellMisspelled` returns the
ranges (`ferny_core::spell`, spellbook) and indicator 12 underlines
them; suggestions come only when the context menu asks.

Markdown preview (H4.2b): `markdownHtml` turns the current document into
filtered HTML (`ferny_core::markdown`, pulldown-cmark) for a
`QTextBrowser` dock that never opens links or loads resources itself.

Save Actions and symbols (H4.2a): the bridge copies the file on disk to
`backups/` (`ferny_core::backup`) before `saveDocument` when enabled; a
UI timer autosaves once a minute. LSP requests 8 (document symbols) and
9 (workspace symbols) answer "<kind> <name>" items for the Symbols
sidebar and the results tab.

Split view (H4.1b): a second `FernyEditor` in a `QSplitter` next to the
tabs shows a tab's Scintilla document through its reference-counted
document pointer; no text is copied.

Printing and export (H4.1a): the UI opens one `QPainter` on the
`QPrinter` or `QPdfWriter` and Scintilla renders each page with it
(`SCI_FORMATRANGEFULL`); PDFs are built in memory and written by
`writeExport` (atomic). HTML and LaTeX come from `ferny_core::export`,
which gets the text, a style per byte and the style colours.

Bundled plugins (H3.4): `plugins/ferny-*/` are compiled into
`ferny-plugin` (`include_str!`) and listed before the user's plugins;
they never come from disk at run time and start disabled. The plugin
manager (Tools → Plugins…) edits `plugin-grants.toml` through the
bridge; changes apply at the next start. Safe mode lists plugins but
starts none.

Rename and formatting (H3.1c): edit answers are checked in the bridge
(workspace only, no file operations) and applied by `ferny_lsp::apply_edits`;
the UI puts the result in the editor as one undo action and never saves.

Language-server features (H3.1b): the bridge's `lspRequest` sends the
current text and a request, and answers come back as `lspAnswer(token,
kind)`; the UI uses them for completion, hover, calltips, definition and
usages, falling back to word completion and tree-sitter.

Language servers (H3.1a): `ferny-lsp` runs servers (`client::Server`:
argv, reduced environment, reader thread, bounded frames) for documents
of trusted workspaces; the bridge keeps one server per workspace and
server name, forwards document changes and turns diagnostics into byte
ranges for the UI's squiggles.

Templates and snippets (H2.5b): `ferny_config::templates` expands
Geany's templates and snippets (built-in from `data/`, or the user's in
the configuration directory) with plain text substitutions; the UI
offers File → New from Template… and expands a snippet when Tab follows
its keyword.

Projects (H2.5a): `ferny_config::project` reads `.ferny/project.toml`
strictly; the bridge keeps the open project, uses its root as the
workspace for trust, its commands before the filetype's (only when
trusted) and its indentation; the global session stashes its files while
a project is open, whose own files go to `projects/<id>.toml`.

Build (H2.4): `ferny-build` turns a template (Geany's `[build]` data or
the user's `build.toml`) into a program and arguments for the current
file; the bridge's `prepareBuild` refuses Make outside trusted folders
and resolves the program on `PATH` without the project's folder; the UI
runs it with `QProcess` (program + arguments, never a shell) and shows
neutralised output in the Compiler tab.

Navigation (H2.3): Go to Definition looks the word under the cursor up in
the symbols of the current document, then of the other open ones; a
history of 100 positions backs Navigate Back/Forward; Complete Word
offers the document's words (`ferny_core::editing::completions`); the
Command Palette lists every registered command and runs it through
`invokeCommand`, like the menus.

Symbols (H2.2): `ferny_syntax::symbols` parses the current document with
tree-sitter (C, C++, Rust, Python, JavaScript) and runs the grammar's
`tags.scm`; the bridge keeps the list and the Symbols tab shows it.
Highlighting and folding stay with Lexilla.

Workspace trust (H2.1): `ferny_core::trust::TrustStore` keeps the
trusted folders (`trust.toml`, owner-only); a file is trusted when its
canonical folder is under one of them. An invalid `trust.toml` trusts nothing
and is renamed to `trust.toml.invalid-<seconds>` before the next change
(v0.5.10). Build programs from `PATH` exclude the trusted workspace root
(`build::resolve`, v0.5.10). The UI shows Restricted/Trusted in
the status bar; build (H2.4), projects (H2.5) and LSP (H3.1) will ask
`isTrusted` before running anything.

Commands (H0.5): `ferny_core::command::Registry` holds every command (id,
English source title, default keys, core or UI handler) and the menus as
data. The UI builds its menus from it through the bridge (`menuIds`,
`menuItems`, `commandTitle`, `commandShortcuts`) and every action calls
`invokeCommand(id)`: core handlers run in Rust; UI commands come back as
the `commandRequested(id)` signal. Unknown or malformed ids fail closed.

Planned:

1. Open: file bytes → encoding detection (bounded) → Scintilla buffer →
   `SCN_MODIFIED` deltas → rope mirror → tree-sitter / LSP (trusted
   workspace only).
2. Save: Scintilla text → encoding → temp file → fsync → atomic rename.
3. Build: command template (user or trusted project) → argv expansion →
   spawn with sanitised env → output (escapes neutralised) → error regex.
4. Plugin call: Lua → `ferny.*` API → permission check → core command
   queue → UI thread.

## Decisions

Recorded in `docs/DESIGN.md` §2.1 (stack), §10 (security model), §11
(licensing).
