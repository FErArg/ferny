# ui

C++17 / Qt6 Widgets application: main window, tabs, sidebar, message panel, dialogs, FernyEditor (ScintillaEdit).

H0.3 skeleton (`src/`):

- `main.cpp`: command line (`[file]`, `--smoke-test`, `--help`, `--version`).
- `mainwindow.*`: one `FernyEditor`; loads through `ferny::FernyCore` and
  maps each `LoadStatus` to a translatable message; builds the menus from
  the core's command registry and runs UI commands (`file.open`,
  `app.quit`, `help.about`) on `commandRequested`.
- `fernyeditor.*`: typed wrapper over `ScintillaEditBase` (not the
  generated `ScintillaEdit`, which needs a Python generation step).
- Icon and logo: `imagenes/` PNGs embedded as Qt resources; window icon
  set in `main.cpp`; menus File → Quit and Help → About Ferny (large
  logo, version, licence), both from the command registry.
- Documents (H1.1): File → New, Open…, Save, Save As…, Reload, Reload
  With Encoding…; Document → Set Encoding…; unsaved-changes prompt on
  new/open/quit; external changes checked on window activation and
  before saving; status bar shows encoding, BOM, EOL and read-only.
- Git, read-only (H4.2d): status column in the Files tree, Document →
  Git Blame; `smoke: git` line.
- Spell checking (H4.2c): underlined words, editor context menu with
  suggestions, View → Check Spelling; `smoke: spelling`.
- Markdown preview (H4.2b): View → Markdown Preview dock
  (`QTextBrowser`, links opened only when clicked); `smoke: preview`.
- Save Actions, symbols, add-ons (H4.2a): autosave timer, backup folder,
  Symbols sidebar from the server, Go to Workspace Symbol, Mark
  Occurrences, enclose; `--smoke-select-all`, `--smoke-autosave`,
  `--smoke-delay`, `smoke: symbols-source`.
- Split view (H4.1b): View → Split Side by Side / Top and Bottom /
  Shows Current Document / Unsplit; `smoke: split` line.
- Printing and export (H4.1a): File → Print… (Ctrl+P), Export as
  PDF/HTML/LaTeX…; `--smoke-export <path>`.
- Plugin manager (H3.4): Tools → Plugins… lists bundled and user
  plugins with their status; enable, permissions, ask again before first
  use, open the plugins folder; changes apply at the next start.
- Plugin extension points (H3.3c): completion merged into Complete Word,
  hover and Format Document asked first, `before_save` on Save, shortcuts
  from `keybindings.toml` on the Tools entries.
- Plugin permissions (H3.3b): first-use confirmation dialog (then the
  command runs once more), clipboard actions, results of plugin
  processes through `pluginActions`.
- Plugin API (H3.3a): Tools menu with the plugins' commands, document
  events, plugin edits as one undo action; `smoke: message` line.
- Plugins (H3.2): permission dialog for new or changed plugins,
  failures in Messages, `--safe-mode`; `--smoke-grant-plugins` and the
  `smoke: plugins` line for the tests.
- Rename and formatting (H3.1c): Search → Rename Symbol… (F2) with a
  preview, Document → Format Document; changes unsaved and undoable.
- Server features (H3.1b): completion, hover, calltips, definition, Find
  Usages; `--smoke-wait`, `--smoke-hover`.
- Language servers (H3.1a): document sync, diagnostic squiggles and
  tooltips, counts in the status bar; `--smoke-lsp-wait`.
- Templates and snippets (H2.5b): File → New from Template…, Tab
  expands snippets; `--smoke-type` accepts `\t`.
- Projects (H2.5a): Project menu, project session, indentation and Find
  in Files defaults; `--smoke-input` drives the folder and file pickers.
- Build (H2.4): Build menu, Compiler tab (clickable error lines),
  restricted-mode banner, Set Build Commands…; `--smoke-build`.
- Navigation (H2.3): Go to Symbol/Definition, Navigate Back/Forward,
  Complete Word (Scintilla's list), Command Palette; `--smoke-input`.
- Symbols (H2.2): Symbols tab in the sidebar (grouped by kind, click
  to go to the line).
- Workspace trust (H2.1): Document → Trust Folder…, status-bar
  Restricted/Trusted, Preferences → Trusted Folders (Revoke);
  `--smoke-trust`.
- Translations (H1.6b): `translations/ferny_es.ts` (update with
  `cmake --build build --target update_translations`; core strings first
  with `FERNY_UPDATE_CORE_STRINGS=1 cargo test -p ferny-config --test
  i18n`), embedded `.qm`, `--language system|en|es`.
- UI shell (H1.6a): sidebar dock with Documents and Files
  (`filebrowser.*`), message window dock (`messagewindow.*`: Status,
  Messages, Find in Files, Scribble), status bar with line/column,
  selection, indentation, encoding, line ending, filetype, read-only and
  modified; View → Show Sidebar / Show Message Window; `--smoke-browse`,
  `--smoke-scribble`.
- Tabs, recent files, reopen closed tab, next/previous tab, middle-click
  close, session save/restore (H1.2); `singleinstance.*`: forwarding to
  the running instance over `QLocalServer`.
- Geany import (H1.5b): File → Import Geany Settings… with a preview and
  the list of what is not imported; `--smoke-import-geany` for tests.
- Settings (H1.5a): `preferences.*` — Preferences dialog generated from
  the core's descriptors, Keyboard Shortcuts dialog with conflict
  warnings; `--config-dir` and `--smoke-shortcut` for tests.
- Search (H1.4): `findbar.*` (find/replace bar), results for find
  in files (a message window tab since H1.6a), go to line; `--smoke-find`, `--smoke-replace`,
  `--smoke-flags`, `--smoke-find-in-files` for tests.
- Project settings (H1.3c): `.editorconfig` indentation and save
  transforms (applied as one undoable change before saving); filetype
  detection with Geany's modeline.
- Editing (H1.3b): Edit menu, auto-indent, auto-close (`()`, `{}`, `[]`,
  on by default), brace matching, markers margin, folding margin,
  multiple carets (Ctrl/Cmd+click, Alt+drag), View → Show Invisible
  Characters (on by default) and Auto-Close Brackets; `--smoke-command`
  and `--smoke-type` for tests.
- Highlighting (H1.3a): filetype detected by the core (Geany 2.1.0 data),
  styles resolved by the core, applied by `FernyEditor::applyHighlighting`;
  View → Color Scheme, Document → Set Filetype; `--color-scheme <id>`.

Built with `-Wall -Wextra -Wpedantic -Wshadow -Wnon-virtual-dtor` (`/W4`
on MSVC); `FERNY_WARNINGS_AS_ERRORS=ON` turns them into errors. Style:
`.clang-format`.
