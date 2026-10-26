#pragma once

#include <QHash>
#include <QList>
#include <QMainWindow>
#include <QPoint>
#include <QSet>
#include <QString>
#include <QStringList>

#include "ferny_bridge/src/core_object.cxxqt.h"

class QCloseEvent;
class QDockWidget;
class QPagedPaintDevice;
class QSplitter;
class QTextBrowser;
class QTreeWidget;
class QTreeWidgetItem;
class QEvent;
class QLabel;
class QListWidget;
class QMenu;
class QProcess;
class QTabWidget;
class QToolBar;
class QTimer;

namespace ferny {

class FernyEditor;
struct DisplayOptions;
struct EditingOptions;
class FileBrowser;
class FindBar;
class MessageWindow;

// The editor window: one tab per document. The core owns what is known
// about each document's file; each tab's editor owns the text.
class MainWindow : public QMainWindow {
    Q_OBJECT

public:
    explicit MainWindow(FernyCore &core, QWidget *parent = nullptr);

    // Opens `path` (encoding empty = detect) in a tab, or focuses the tab
    // that already shows it, and moves the cursor to line/column (1-based,
    // 0 = keep). On failure a translated message is shown.
    LoadStatus openFile(const QString &path, const QString &encoding = QString(), quint32 line = 0,
                        quint32 column = 0);

    // Opens the core's pending files (arguments, forwarded request or
    // session) and clears them. Returns how many failed.
    int openPending();

    // Loads the session file (recent files, geometry, colour scheme).
    // False if there is none or it is disabled.
    bool loadSession();

    // Opens the files of the loaded session and selects its active tab.
    void restoreSessionFiles();

    // The find/replace bar (tests set its fields directly).
    FindBar *findBar() const { return m_findBar; }

    // Sidebar and message window (H1.6a).
    FileBrowser *fileBrowser() const { return m_files; }
    MessageWindow *messageWindow() const { return m_messageWindow; }
    // Names in the Documents sidebar, in order.
    QStringList documentNames() const;
    // Symbols sidebar as "kind:name:line", in order (smoke tests).
    QStringList symbolEntries() const;
    // "lsp" or "tree-sitter": where the Symbols sidebar came from (smoke).
    QString symbolsSource() const { return m_symbolsSource; }
    // Texts of the status-bar fields, in order (smoke tests).
    QStringList statusFields() const;

    // Smoke tests: File → Import Geany Settings… reads `dir` without
    // dialogs. Returns the summary line of the last import.
    void setSmokeImportDirectory(const QString &dir) { m_smokeImportDirectory = dir; }
    // Smoke tests: list pickers (palette, Go to Symbol) choose the first
    // entry containing `text` without showing a dialog.
    void setSmokeInput(const QString &text) {
        m_smokeInput = text;
        m_smoke = true;
    }
    // Whether a build command is running (H2.4).
    bool buildRunning() const;
    // Language servers (H3.1a): the last lspOpen status of the current
    // document (-1 none) and its diagnostics as "severity:line" entries.
    int lspStatus() const;
    QStringList diagnosticEntries() const;
    // Smoke tests (H3.1b): hover at the cursor, and the last hover text.
    void requestHoverAtCursor();
    QString lastHover() const { return m_lastHover; }
    int usageCount() const;
    // Texts of the Compiler tab (smoke tests).
    QStringList compilerLines() const;
    // Folder of the user's configuration (project sessions live there).
    // Also loads the spelling dictionary when spell checking is on (H4.2c).
    void setConfigDir(const QString &dir);
    // Misspelled words in the current document, checked now (smoke tests).
    int spellingCount();
    // Projects (H2.5a). Opens the project in `folder` (closing the open
    // tabs first unless `atStart`); false if cancelled or invalid.
    bool openProject(const QString &folder, bool atStart = false);
    // Plugins (H3.2): finds them, asks about new ones, starts the enabled
    // ones; `grantAll` (smoke tests) enables them with every permission.
    // `language` picks the plugins' translations (H3.3a).
    void startPlugins(bool safeMode, bool grantAll, const QString &language);
    // "id=status" for every plugin found (smoke tests).
    QStringList pluginSummary() const;
    // "none" or "side|stacked <file> shared=yes|no" (smoke tests).
    QString splitSummary() const;
    // Where the user's build.toml is saved; empty = never.
    void setBuildPath(const QString &path) { m_buildPath = path; }
    // Smoke tests: Document → Trust Folder… trusts `dir` without dialogs.
    void setSmokeTrustFolder(const QString &dir) { m_smokeTrustFolder = dir; }
    QString lastImportReport() const { return m_lastImportReport; }

    // Starts a find in files and shows the results panel. False if the
    // pattern is invalid.
    bool startFindInFiles(const QString &directory, const QString &pattern, quint32 flags,
                          const QString &filePatterns);

    // Runs a registered command on the current tab (as the menus do).
    bool runCommand(const QString &commandId);

    // Applies a colour scheme to every tab (unknown ids: the default).
    void setColorScheme(const QString &id);
    QString colorScheme() const { return m_scheme; }

    // Where settings.toml and keybindings.toml are saved; empty = never.
    void setConfigPaths(const QString &settings, const QString &keybindings) {
        m_settingsPath = settings;
        m_keybindingsPath = keybindings;
    }

    // Re-applies the settings to every tab (after Preferences).
    void applySettings();

    // Where the session is saved on quit; empty disables the session.
    void setSessionPath(const QString &path) { m_sessionPath = path; }

    // Writes the session now (open files in tab order, active tab,
    // geometry). False on failure or if the session is disabled.
    bool saveSession();

    // Writes the current document to `path` without asking (smoke tests).
    SaveStatus saveCurrentAs(const QString &path);
    // Exports the current document to `path` as `kind` (`pdf`, `html`,
    // `latex`), without a dialog (H4.1a; smoke tests).
    bool exportCurrent(const QString &kind, const QString &path);
    // The Markdown preview as plain text (smoke tests).
    QString previewText() const;
    // Save Actions (H4.2a): with `files.autosave`, saves the modified
    // documents that have a file; returns how many. Runs every minute.
    int autosaveNow();

    FernyEditor *currentEditor() const;
    int tabCount() const;
    FernyEditor *editorAt(int index) const;
    quint64 documentId() const;

    // Ids of the commands present in the menus, in creation order.
    QStringList menuCommandIds() const { return m_commandIds; }
    // Ids of the commands present in the main toolbar, in creation order.
    QStringList toolbarCommandIds() const { return m_toolbarCommandIds; }
    bool toolbarIconOnly() const;
    bool toolbarIconsAvailable() const;

    // Stable, untranslated names (for logs and smoke tests).
    static const char *statusName(LoadStatus status);
    static const char *statusName(SaveStatus status);

signals:
    // A build command finished (exit code; -1 if it crashed or was stopped).
    void buildFinished(int exitCode);

protected:
    void closeEvent(QCloseEvent *event) override;
    void changeEvent(QEvent *event) override;
    bool eventFilter(QObject *watched, QEvent *event) override;

private:
    void createMenus();
    void createDocks();
    // Appends to the Messages tab (and shows it) and the status bar.
    void showMessage(const QString &text);
    // Rebuilds the Documents sidebar from the tabs.
    void refreshDocuments();
    // Rebuilds the Symbols sidebar for the current document (H2.2).
    void refreshSymbols();

    // Navigation and completion (H2.3).
    struct NavPosition {
        quint64 documentId;
        quint32 line;
        quint32 column;
    };
    // Remembers the cursor before a jump (clears Forward).
    void pushNavigation();
    void navigate(bool back);
    // Moves to `line` of the document `id` (focusing its tab).
    bool jumpTo(quint64 id, quint32 line, quint32 column);
    void gotoSymbol();
    // Build (H2.4): runs `command` (compile, build, lint, make, execute)
    // for the current document, output in the Compiler tab.
    void runBuild(const QString &command);
    void appendBuildOutput(const QByteArray &bytes, bool flush);
    void openCompilerLine(int row);
    void setBuildCommands();
    // Sends `editor`'s document to its language server (H3.1a).
    void lspSync(FernyEditor *editor);
    void applyDiagnostics(quint64 documentId);
    void showDiagnosticAt(FernyEditor *editor, int x, int y);
    // Language-server answers (H3.1b).
    void onLspAnswer(qint64 token, int kind);
    void findUsages();
    // Rename and formatting (H3.1c).
    void renameSymbol();
    void formatDocument();
    void applyRename();
    // The tree-sitter fallback of Go to Definition (H2.3).
    void gotoDefinitionLocal();
    // Word completion from the document only (the fallback, H2.3).
    void completeWordFromDocument();
    void requestSignature(FernyEditor *editor);
    void newProject();
    // Templates and snippets (H2.5b).
    void newFromTemplate();
    bool expandSnippet(FernyEditor *editor);
    void chooseProject();
    bool closeProject();
    void importGeanyProject();
    // Closes every tab (asking to save); false if cancelled.
    bool closeEveryTab();
    // Collects the open files (tab order) into the core's session list;
    // returns the active one's index among them.
    int collectSessionFiles();
    // Opens the core's pending files and selects `active` among them.
    void openPendingWithActive(int active);
    void gotoDefinition();
    void completeWord();
    void showCommandPalette();
    // A filtered list of `items`; returns the chosen index or -1.
    int pickItem(const QString &title, const QStringList &items);
    // Updates the line/column and selection fields.
    void updateCursorStatus();
    void runUiCommand(const QString &commandId);
    void rebuildRecentMenu();
    void createToolbar();
    void refreshToolbarActions();

    // Tabs.
    FernyEditor *addTab(quint64 documentId);
    bool closeTab(int index);
    void closeAllTabs(FernyEditor *keep);
    void reopenClosedTab();
    void cycleTab(int step);
    // Whether `editor` is an untitled, empty, unmodified document that an
    // opened file may replace.
    bool isPristine(const FernyEditor *editor) const;

    void newDocument();
    void openWithDialog();
    bool save(FernyEditor *editor);
    bool saveAs(FernyEditor *editor);
    bool saveAll();
    void reload(FernyEditor *editor, const QString &encoding);
    void reloadWithEncoding();
    void setEncoding();
    void showAbout();

    // Asks to save unsaved changes of `editor`. False if cancelled.
    bool maybeSaveChanges(FernyEditor *editor);
    // Reacts to changes made to open files by other programs.
    void checkExternalChanges();
    // Shows a document just opened or reloaded in `editor`.
    void applyDocument(FernyEditor *editor, const QByteArray &utf8);
    // Detects the filetype of `editor`'s document from its path and the
    // first line of `utf8`, then highlights it.
    void detectFiletype(FernyEditor *editor, const QByteArray &utf8);
    // Re-applies the filetype and colour scheme of `editor`.
    void applyHighlighting(FernyEditor *editor);
    void setFiletype(FernyEditor *editor, const QString &id);
    // Applies the editing options (invisible characters, auto-close).
    void applyEditingOptions(FernyEditor *editor);
    DisplayOptions displayOptions() const;
    EditingOptions editingOptions() const;
    void toggleDisplaySetting(const QString &key, const QString &commandId);
    void setShowInvisible(bool show);
    void showPreferences();
    void importGeany();
    // Document → Trust Folder… (H2.1): the document's folder or a parent.
    void trustFolder();
    void showShortcuts();
    void refreshShortcuts();
    void saveSettings();
    void setAutoClose(bool enabled);
    void toggleComment(FernyEditor *editor);
    void applySaveTransforms(FernyEditor *editor);

    // Search (H1.4).
    void openFindBar(bool replace);
    bool findInEditor(bool forward);
    void replaceOne();
    void replaceAllInEditor();
    void showFindInFilesDialog();
    void showFindInFilesResults(int hits, bool truncated, bool cancelled, int searched,
                                int skipped);
    void openFileHit(QTreeWidgetItem *item);
    void gotoLine();
    // Reports a search status; true if a match was found.
    bool reportSearch(SearchStatus status);
    void updateTabLabel(FernyEditor *editor);
    void updateTitle();
    void updateStatus();
    bool reportSave(FernyEditor *editor, SaveStatus status);
    // Picks an encoding from the supported list; empty if cancelled.
    QString chooseEncoding(const QString &title);

    static QString loadMessage(LoadStatus status);
    static QString saveMessage(SaveStatus status);

    FernyCore &m_core;
    QTabWidget *m_tabs;
    FindBar *m_findBar;
    QTreeWidget *m_results;
    QDockWidget *m_sidebarDock = nullptr;
    QTabWidget *m_sidebar = nullptr;
    QListWidget *m_documents = nullptr;
    FileBrowser *m_files = nullptr;
    QTreeWidget *m_symbols = nullptr;
    QTimer *m_symbolsTimer = nullptr;
    QDockWidget *m_messageDock = nullptr;
    MessageWindow *m_messageWindow = nullptr;
    // Status-bar fields.
    QLabel *m_position;
    QLabel *m_selection;
    QLabel *m_indentation;
    QLabel *m_encoding;
    QLabel *m_eol;
    QLabel *m_filetype;
    QLabel *m_documentState;
    QLabel *m_trust;
    QString m_smokeImportDirectory;
    QString m_smokeTrustFolder;
    QString m_smokeInput;
    bool m_smoke = false;
    QProcess *m_process = nullptr;
    QByteArray m_pendingOutput;
    qint64 m_outputBytes = 0;
    bool m_outputTruncated = false;
    QString m_buildDir;
    QString m_buildPath;
    QString m_configDir;
    QStringList m_pluginIds;
    QMenu *m_toolsMenu = nullptr;
    QToolBar *m_toolbar = nullptr;
    // Plugin API (H3.3a): events, commands and the actions they return.
    void emitPluginEvent(const QString &event, FernyEditor *editor);
    void runPluginCommand(const QString &id);
    // Returns whether the user allowed a permission asked for (H3.3b).
    bool applyPluginActions(const QStringList &actions, FernyEditor *editor);
    void buildToolsMenu();
    // Printing and PDF (H4.1a): pages rendered by Scintilla into pictures.
    void printDocument(FernyEditor *editor, QPagedPaintDevice *device, const QString &title);
    QString documentTitle(FernyEditor *editor) const;
    // Tools → Plugins… (H3.4): enable, grant, reset first-use answers.
    void showPluginManager();
    // Split view (H4.1b): a second view sharing a tab's document.
    void splitView(Qt::Orientation orientation);
    void showInSplit(FernyEditor *source);
    FernyEditor *m_splitEditor = nullptr;
    QSplitter *m_splitter = nullptr;
    // Markdown preview (H4.2b).
    QDockWidget *m_previewDock = nullptr;
    QTextBrowser *m_preview = nullptr;
    QTimer *m_previewTimer = nullptr;
    void refreshPreview();
    // Spell checking (H4.2c).
    bool m_spellCheck = false;
    QTimer *m_spellTimer = nullptr;
    int m_misspelled = 0;
    void loadSpeller();
    void refreshSpelling();
    QByteArray spellText(FernyEditor *editor) const;
    void showEditorMenu(FernyEditor *editor, const QPoint &pos);
    // Plugin providers (H3.3c): asks `kind` at byte `pos` of `editor`;
    // returns the `item` / `text` answers, applies the other actions.
    QStringList askPluginProvider(const QString &kind, FernyEditor *editor, int pos,
                                  const QString &word);
    void runBeforeSave(FernyEditor *editor);
    QWidget *m_banner = nullptr;
    QHash<quint64, int> m_lspStatus;
    QSet<QString> m_lspReported;
    // Diagnostics per document: (start, end, severity, message).
    struct DiagnosticMark {
        int start;
        int end;
        int severity;
        QString message;
    };
    QHash<quint64, QList<DiagnosticMark>> m_diagnostics;
    // The request each feature waits for (token 0: none) and its context.
    qint64 m_completionToken = 0;
    qint64 m_hoverToken = 0;
    qint64 m_signatureToken = 0;
    qint64 m_definitionToken = 0;
    qint64 m_usagesToken = 0;
    // LSP symbols (H4.2a): the sidebar's request and Go to Workspace Symbol.
    qint64 m_symbolsToken = 0;
    QString m_symbolsSource;
    qint64 m_workspaceSymbolsToken = 0;
    void showTreeSitterSymbols();
    void addSymbol(QHash<QString, QTreeWidgetItem *> &groups, const QString &kind,
                   const QString &name, int line);
    qint64 m_renameToken = 0;
    qint64 m_formatToken = 0;
    FernyEditor *m_requestEditor = nullptr;
    int m_requestPosition = 0;
    QString m_completionPrefix;
    // Items from plugin completion providers, merged into the list (H3.3c).
    QStringList m_pluginCompletions;
    QPoint m_hoverPoint;
    QString m_lastHover;
    QList<NavPosition> m_back;
    QList<NavPosition> m_forward;
    QString m_lastImportReport;
    QMenu *m_recentMenu = nullptr;
    QMenu *m_schemeMenu = nullptr;
    QMenu *m_filetypeMenu = nullptr;
    QString m_scheme = QStringLiteral("default");
    // Defaults on (H1.3b); the settings decide from H1.5a.
    bool m_showInvisible = true;
    bool m_autoClose = true;
    QString m_settingsPath;
    QString m_keybindingsPath;
    QString m_sessionPath;
    bool m_checkingExternal = false;
    QStringList m_commandIds;
    QStringList m_toolbarCommandIds;
};

}  // namespace ferny
