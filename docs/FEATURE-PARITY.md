# Geany → Ferny feature parity

Reference: Geany 2.1 (manual, wiki, `src/` and `plugins/` of
github.com/geany/geany). Phase numbers refer to [ROADMAP.md](ROADMAP.md).

| Area | Geany feature | Ferny approach | Phase |
|---|---|---|---|
| Editor | Scintilla editing, multi-selection, rectangular selection | Scintilla (same component) | 1 |
| Editor | Syntax highlighting, ~90 filetypes | Lexilla, then tree-sitter per language | 1 / 2 |
| Editor | Code folding | Lexilla folding, then tree-sitter folds | 1 / 2 |
| Editor | Auto-indent, auto-close, brace matching, indentation detection, Tab/Backspace indent, smart Home, trailing spaces on Enter, tab width, quote auto-close | Core + Scintilla; options added in v0.6.10 (`editor.auto_indent`, `editor.detect_indent`, `editor.tab_indents`, `editor.backspace_unindents`, `editor.smart_home`, `editor.newline_strips_trailing`, `editor.tab_width`, `editor.auto_close_quotes`; plan M2) | 1 / done (M2) |
| Editor | Indent guides, whitespace, EOL, long-line marker, wrap | Whitespace and long-line marker done; word wrap, indent guides and a separate EOL display added in v0.6.9 (`editor.word_wrap`, `editor.indent_guides`, `editor.show_eol`, View menu toggles; plan M1) | 1 / done (M1) |
| Editor | Markers / bookmarks, line operations, case conversion | Commands | 1 |
| Editor | Comment / uncomment / toggle, per filetype | Filetype comment tokens | 1 |
| Editor | Word and symbol autocompletion, calltips | tree-sitter + LSP + plugin providers | 2 / 3 |
| Editor | Snippets, templates | `snippets.toml`, `templates/` | 2 |
| Editor | Zoom, fonts, colour schemes | TOML schemes, Geany import: fonts and schemes done; zoom added in v0.6.9 (View menu zoom in, out and normal size; per-session state, not a preference; plan M1) | 1 / done (M1) |
| Symbols | Tag manager (ctags), symbols sidebar | tree-sitter queries | 2 |
| Symbols | Go to definition/declaration, global tag files | tree-sitter + LSP | 2 / 3 |
| Symbols | Plugin extension API (Geany 2.1) | Extension points with priorities | 3 |
| Files | Tabs, sessions, recent files | Core + Qt | 1 |
| Files | Encodings, BOM, EOL, file change detection | `encoding_rs`, `notify` | 1 |
| Files | Single instance, `file:line:col` | Per-user `QLocalServer`, restricted permissions | 1 |
| Search | Find/replace, regex, find in files, go to line | `regex`, `grep-searcher` | 1 |
| Build | Compile/Build/Make/Run, per filetype and project | `ferny-build`, argv only, workspace trust required | 2 |
| Build | Error regex, jump to error | Compiler panel | 2 |
| Projects | `.geany` project file, project session | `.ferny/project.toml`, importer | 2 |
| UI | Sidebar (Symbols, Documents), message window | Qt docks/panels | 1 / 2 |
| UI | Scribble | Message panel tab | 1 |
| UI | Embedded terminal (VTE) | `alacritty_terminal` widget (TBD) | 4 |
| UI | Toolbar editor, keybinding editor, prefs dialog | Fixed built-in Qt toolbar with Lucide icons (H5.1); navigable Preferences in categories General/Editor/Files/Templates/Security over the typed descriptors (H5.2); toolbar customization remains proposed; keybindings remain separately editable; remaining Geany Preferences options being mapped | 1 / H5.2 / proposed |
| UI | Fullscreen, split window (plugin) | Built in: split view done at v0.4.2 | 1 / 2 |
| Config | Keyfile configs (system + user) | TOML layers + Geany importer | 1 |
| Other | Printing | Scintilla `FORMATRANGE` / Qt PDF: print, PDF, HTML and LaTeX export done at v0.4.1 | 4 |
| Other | i18n (gettext) | Qt Linguist, en + es first | 1 |
| Plugins | C API, proxy plugins (Lua, Python) | Lua 5.4 API 1.x, sandbox + permissions | 3 |
| Tools | Send selection to custom command | argv command, opt-in, confirmation on first use | 2 |
| Plugins | Core plugins: classbuilder, export, filebrowser, htmlchars, saveactions, splitwindow | Bundled Lua plugins / built in: htmlchars and classbuilder (C++) done at v0.4.0; export built in at v0.4.1 (File menu); splitwindow built in at v0.4.2 (View menu); saveactions (autosave, backup copies) built in at v0.4.3; filebrowser is the Files sidebar | 3 / 4 |
| Plugins | geany-plugins (Git, LSP, Markdown, Spell, Debugger, …) | Built in (LSP) or ported: Markdown preview built in at v0.4.4, spell check at v0.4.5, Git status and blame (read-only) at v0.4.6 | 3 / 4 |

## Qt removal inventory (phase 0a, 2026-10-20)

Scope: what the user sees today and where the Qt code sits. "Core" means the
logic already lives in a Qt-free Rust crate, so only the view would change.
Test column: ctest names in `ui/CMakeLists.txt` (smoke tests run
`ferny --smoke-test` with `QT_QPA_PLATFORM=offscreen`; about 160 entries).

| # | Feature | Qt used (file:line) | Core in Rust | Tests (ctest) | Migration note |
|---|---|---|---|---|---|
| 1 | Menu bar, actions, shortcuts | QMenu, QMenuBar, QAction (`mainwindow.cpp:459-535`) | Menu model and shortcuts (`ferny-core/command.rs`) | many | Native macOS menu bar needed |
| 2 | Recent files, colour scheme, filetype submenus | QMenu `aboutToShow` (`mainwindow.cpp:516-534`) | Yes (`session.rs`, filetypes) | none | Dynamic submenus; add a test before phase 1 |
| 3 | Toolbar (H5.1 fixed set) | QToolBar, QAction (`mainwindow.cpp:562-615`) | Command set | `ui_smoke_toolbar_commands`, `_dispatch` | Icons are Lucide SVG, reusable |
| 4 | Sidebar: symbols, documents, file browser | QDockWidget, QTreeWidget (`mainwindow.cpp:199`, `filebrowser.cpp`) | `browse.rs`, symbols (`ferny-syntax`) | none direct | Needs a docking layout; add a test |
| 5 | Message window | QDockWidget (`messagewindow.cpp`, `mainwindow.cpp:296`) | Build output (`ferny-build`) | indirect (`smoke: message` lines in plugin tests) | Needs a direct test |
| 6 | Markdown preview | QDockWidget, QTextBrowser (`mainwindow.cpp:308-317`) | `markdown.rs` (pulldown-cmark) | `ui_markdown_preview`, `_other` | Widget needs a Rust renderer or a webview (security review first) |
| 7 | Editor (Scintilla) | ScintillaEditBase (`fernyeditor.h/.cpp`) | Document model (`document.rs`) | many | Main phase 0b question |
| 8 | Find, replace, find bar | QLineEdit, QCheckBox (`findbar.cpp`) | `search.rs` | `ui_compare_replace_all`, find smoke tests | View only |
| 9 | Preferences dialog (H5.2) | QDialog, QTreeWidget (`preferences.cpp`, `mainwindow.cpp:807`) | `ferny-config` descriptors | `ui_settings_*`, preferences round trip | Descriptor-driven, reusable |
| 10 | Plugin manager and permission prompts | QDialog (`mainwindow.cpp:2896-2916, 2999`) | `ferny-plugin` | `ui_plugins_*` (37 entries) | Permission prompt keeps its wording and default |
| 11 | Git integration (read-only) | Menu and panel in `mainwindow.cpp` | `git.rs` | `ui_git_*` (9 entries) | View only |
| 12 | Build and run (external process) | QProcess, merged channels (`mainwindow.cpp:1347-1439`) | `ferny-build` (argv only) | `ui_build_trust_clean` and build tests | Replace QProcess with a Rust runner, same argv rule |
| 13 | Workspace trust prompts | QMessageBox (`mainwindow.cpp`) | `trust.rs` | `ui_trust_clean` | Dialog only |
| 14 | Print | QPrintDialog, QPrinter (`mainwindow.cpp:1920-1922`) | Page layout not in Rust | none | Phase 5; add a test before phase 1 |
| 15 | Export HTML, TeX, PDF | QFileDialog, QPdfWriter (`mainwindow.cpp:1926-1940`, `:3489`) | `export.rs` (HTML, TeX) | `ui_export_html/tex/pdf_content` (8 entries) | PDF writer is Qt-only: phase 5 |
| 16 | Single instance and file forwarding | QLocalServer, QLocalSocket (`singleinstance.cpp`) | `ipc.rs` (protocol) | `ui_ipc_forward` | Transport is Qt-only; socket permissions stay as they are |
| 17 | Open files from macOS Finder | QFileOpenEvent (`main.cpp:417-424`) | n/a | none (macOS only) | Phase 5: objc2 wrapper |
| 18 | Command line options | QCommandLineParser (`main.cpp:94`) | n/a | smoke options (`--smoke-*`) | Replace with a Rust parser |
| 19 | Translations (es) | QTranslator, `ferny_es.ts` (`main.cpp:57-58`) | `core_strings.cpp` (145 lines) | none | Choose a Rust catalogue format (phase 2) |
| 20 | Open URL, reveal in file manager | QDesktopServices (`mainwindow.cpp:317, 3085`) | n/a | none | Platform open APIs |
| 21 | Save actions, autosave, backups | Core, with QTimer | `backup.rs`, `fsio.rs` | `ui_save_actions_*` | QTimer replaced by a Rust timer |
| 22 | EditorConfig, project setup, Geany import | Core, dialogs only | `editorconfig.rs`, `ferny-config` | `ui_editorconfig_*`, `ui_project_setup*`, `ui_geany_import` | View only |
| 23 | LSP (diagnostics, navigation) | Core, shown in the editor | `ferny-lsp` | `ui_lsp_*` (20 entries) | View only |
| 24 | Spell check | Core, shown in the editor | `spell.rs` | `ui_spell_text`, `_code`, `_off`, `_toggle` | View only |
| 25 | Smoke test harness | `QT_QPA_PLATFORM=offscreen` (`ui/CMakeLists.txt`) | n/a | all `ui_*` | Replaced in phase 1 by a headless harness |

### Summary

- Logic already in Rust for most features: rows 1, 2, 4, 6, 8 to 13, 15, 16,
  18, 21 to 24 have their behaviour in Qt-free crates; only the view changes.
- Qt-only logic with no Rust counterpart yet: print layout (14), PDF writer
  (15), single-instance transport (16), command-line parser (18), translation
  loader (19), URL and file-manager opening (20), macOS open events (17).
- Test gaps before phase 1: print (14), recent and submenus (2), sidebar (4),
  message window (5). Without tests these cannot prove parity.
- Estimate check: the 60–90 person-week range in the plan stands. The cost
  sits in the view (rows 1–10), the editor (7), and phase 5 (14, 15, 17, 20).
