# Ferny — Roadmap

Each phase ends with a usable, released build on macOS, Linux and Windows.
Security controls per phase come from [THREAT-MODEL.md](THREAT-MODEL.md)
(§6, acceptance criteria): a phase is not done until its controls are tested.
See [DESIGN.md](DESIGN.md) for architecture and
[FEATURE-PARITY.md](FEATURE-PARITY.md) for the full Geany feature checklist.
The executable plan (milestones, tasks, reviews, versions) is kept by the maintainer.

## Phase 0 — Foundations

- CMake + Corrosion + cxx-qt build producing one app on the three platforms.
- Vendored Scintilla + Lexilla; a `FernyEditor` widget showing a file.
- Cargo workspace with empty crates and the bridge wired end-to-end.
- Validation scripts for the three OS (`scripts/check.sh`,
  `scripts/check.ps1`), see [RELEASE-GATE.md](RELEASE-GATE.md): `clippy`,
  `rustfmt`, tests, UI smoke test, `cargo deny`, `cargo audit`, gitleaks,
  Semgrep; `deny.toml` and `rust-toolchain.toml` committed.
- `unsafe` confined to `ferny-bridge`.
- Command registry skeleton (every action is a command).

## Phase 1 — MVP: editor and files

- Tabs: open, new, save, save as, save all, close, close others, reopen closed.
- Encodings (detection + conversion), BOM, EOL (LF/CRLF/CR), read-only.
- External change detection and reload; file deletion handling.
- Recent files, sessions (restore files, cursor positions, window layout).
- Single instance + CLI `file:line:col`.
- Find / replace (plain, regex, whole word, case, in selection), incremental
  search bar, find in files with results panel, go to line.
- Highlighting for all Geany filetypes via Lexilla; filetype detection.
- Colour schemes (imported from Geany), fonts, zoom.
- Editing: auto-indent, auto-close brackets/quotes, brace matching, comment
  toggle, duplicate/move/delete line, case conversion, multi-cursor,
  rectangular selection, indent guides, whitespace/EOL display, long-line
  marker, word wrap, bookmarks/markers, trailing whitespace stripping.
- Settings in TOML + preferences dialog; keybindings editor.
- Geany importer (settings, keybindings, filetypes, colour schemes).
- Status bar, sidebar (Documents, Files), message panel (Status, Messages,
  Scribble).
- UI in English and Spanish.
- Security (T6–T11, T13): atomic safe-save, per-user IPC socket, bounded
  handling of huge files and invalid encodings, visible bidi/invisible
  characters, logs without file contents, fuzz targets for encodings and
  the Geany importer.
- Packaging: dmg, AppImage, MSI.

## Phase 2 — Symbols, build and projects

- **Workspace trust** (T1–T3) first: projects and build commands depend on it.
- tree-sitter for symbols and folding in C, C++, Rust, Python and
  JavaScript first (plan v3); more languages later (Go, Java, Lua, shell,
  HTML, CSS, JSON, YAML, TOML, Markdown, TypeScript). Lexilla keeps
  highlighting.
- Symbols sidebar, go to symbol, go to definition (local), code folding from
  tree-sitter, word/symbol completion, navigation history.
- Build menu with per-filetype and per-project commands (argv, no shell),
  Compiler panel, error regex parsing and jump to error, ANSI escapes
  neutralised.
- Projects (`.ferny/project.toml`), project sessions, Geany `.geany` import.
- Templates and snippets.
- Command palette.

## Phase 3 — LSP and Lua plugins

- LSP client: completion, signature help, hover, go to definition/references,
  diagnostics, rename, formatting, document/workspace symbols.
- Lua 5.4 plugin host, API 0.x (frozen at Ferny 1.0), sandbox and permissions (T4, T5, T17),
  plugin manager UI showing permissions at install.
- LSP only in trusted workspaces, executables resolved outside the project
  (T2, T16).
- Extension points (completion, goto, symbols, diagnostics, formatters).
- Bundled plugins ported from the Geany core plugins: file browser extras,
  HTML chars, save actions, export (HTML/LaTeX), class builder.
- Done at v0.4.0: everything above except LSP document/workspace
  symbols, the goto, symbols and diagnostics extension points, and the
  file browser extras, save actions and export ports (moved to phase 4);
  the plugin network API is deferred.

## Phase 4 — Parity and polish

- Printing / PDF export, split view.
- Ports of popular geany-plugins: Git integration, Markdown preview, spell
  check, tree browser, add-ons (mark word, enclose, …).
- Accessibility pass, performance budget checks in the
  validation script.
- Flatpak, MSIX, universal macOS build (notarised once platform
  certificates exist, R4).
- Release gate (T14, T15): minisign-signed checksums, SBOM, publication
  on the maintainer's Git server, clean-VM install tests, platform code signing before 1.0, full
  `security-audit` of the release candidate.

## Proposed follow-up (not approved for implementation)

- The fixed, built-in toolbar shipped at H5.1 with a curated command set and
  Lucide icons. Optional toolbar customization and a navigable, descriptor-
  driven Preferences dialog are analyzed in
  the Geany integration plan.
  This proposal does not authorize implementation; exact Geany 2.1 preference
  mappings and maintainer UX decisions remain pending.
- Missing Geany Preferences options, in five proposed milestones.
  Proposal; no implementation authorised; decisions D1–D6 pending.

## After 1.0

Each needs its own threat model before any code (plan v3, approved
2026-10-16): embedded terminal, debugger via DAP, workbench.
