#include "mainwindow.h"

#include "fernyeditor.h"
#include "filebrowser.h"
#include "findbar.h"
#include "messagewindow.h"
#include "preferences.h"

#include <QAction>
#include <QActionGroup>
#include <QApplication>
#include <QBuffer>
#include <QByteArray>
#include <QCheckBox>
#include <QClipboard>
#include <QCloseEvent>
#include <QCoreApplication>
#include <QDateTime>
#include <QDesktopServices>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDir>
#include <QDockWidget>
#include <QEvent>
#include <QFileDialog>
#include <QFileInfo>
#include <QFormLayout>
#include <QGuiApplication>
#include <QHBoxLayout>
#include <QHash>
#include <QHeaderView>
#include <QIcon>
#include <QInputDialog>
#include <QKeySequence>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QMenu>
#include <QMenuBar>
#include <QMessageBox>
#include <QMouseEvent>
#include <QPageSize>
#include <QPainter>
#include <QPdfWriter>
#include <QPixmap>
#include <QPrintDialog>
#include <QPrinter>
#include <QProcess>
#include <QPushButton>
#include <QSplitter>
#include <QStatusBar>
#include <QTabBar>
#include <QTabWidget>
#include <QTextBrowser>
#include <QTimer>
#include <QToolBar>
#include <QToolButton>
#include <QToolTip>
#include <QTreeWidget>
#include <QUrl>
#include <QVBoxLayout>

#include <algorithm>

#include <Scintilla.h>

namespace ferny {

namespace {

// Indicators: 8-10 diagnostics, 11 marked occurrences (FernyEditor),
// 12 misspelled words (H4.2c).
constexpr uptr_t kSpellIndicator = 12;

// Titles come from the core as English source strings (the core never
// produces translated prose); the UI translates them in this context.
QString translatedTitle(const QString &source) {
    const QByteArray utf8 = source.toUtf8();
    return QCoreApplication::translate("ferny::commands", utf8.constData());
}

QString eolName(EolMode mode) {
    switch (mode) {
        case EolMode::CrLf:
            return QStringLiteral("CRLF");
        case EolMode::Cr:
            return QStringLiteral("CR");
        case EolMode::Lf:
            return QStringLiteral("LF");
    }
    return QStringLiteral("LF");
}

}  // namespace

MainWindow::MainWindow(FernyCore &core, QWidget *parent)
    : QMainWindow(parent),
      m_core(core),
      m_tabs(new QTabWidget(this)),
      m_findBar(new FindBar(this)),
      m_results(new QTreeWidget(this)),
      m_position(new QLabel(this)),
      m_selection(new QLabel(this)),
      m_indentation(new QLabel(this)),
      m_encoding(new QLabel(this)),
      m_eol(new QLabel(this)),
      m_filetype(new QLabel(this)),
      m_documentState(new QLabel(this)),
      m_trust(new QLabel(this)) {
    m_scheme = m_core.settingValue(QStringLiteral("editor.color_scheme"));
    m_showInvisible =
        m_core.settingValue(QStringLiteral("editor.show_invisible")) == QStringLiteral("true");
    m_autoClose =
        m_core.settingValue(QStringLiteral("editor.auto_close")) == QStringLiteral("true");
    m_tabs->setDocumentMode(true);
    m_tabs->setTabsClosable(true);
    m_tabs->setMovable(true);
    m_tabs->tabBar()->installEventFilter(this);
    connect(m_tabs, &QTabWidget::tabCloseRequested, this, [this](int index) { closeTab(index); });
    connect(m_tabs, &QTabWidget::currentChanged, this, [this]() {
        updateTitle();
        updateStatus();
        refreshDocuments();
        emitPluginEvent(QStringLiteral("document.activated"), currentEditor());
        refreshPreview();
        if (m_spellTimer) {
            m_spellTimer->start();
        }
        refreshToolbarActions();
    });
    connect(m_tabs->tabBar(), &QTabBar::tabMoved, this, [this]() { refreshDocuments(); });
    // Tabs with the find bar under them.
    auto *central = new QWidget(this);
    auto *layout = new QVBoxLayout(central);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);
    // Restricted-mode banner (R1), shown after the first blocked action.
    m_banner = new QWidget(central);
    auto *bannerLayout = new QHBoxLayout(m_banner);
    bannerLayout->setContentsMargins(8, 4, 8, 4);
    auto *bannerText =
        new QLabel(tr("Restricted mode: this folder is not trusted, so Make, project commands and "
                      "programs from the folder are blocked."),
                   m_banner);
    bannerText->setWordWrap(true);
    auto *bannerTrust = new QPushButton(tr("Trust Folder…"), m_banner);
    auto *bannerClose = new QToolButton(m_banner);
    bannerClose->setText(QStringLiteral("×"));
    bannerClose->setAutoRaise(true);
    bannerClose->setToolTip(tr("Close"));
    bannerLayout->addWidget(bannerText, 1);
    bannerLayout->addWidget(bannerTrust);
    bannerLayout->addWidget(bannerClose);
    m_banner->setAutoFillBackground(true);
    m_banner->setBackgroundRole(QPalette::ToolTipBase);
    m_banner->setForegroundRole(QPalette::ToolTipText);
    m_banner->hide();
    connect(bannerTrust, &QPushButton::clicked, this, &MainWindow::trustFolder);
    connect(bannerClose, &QToolButton::clicked, m_banner, &QWidget::hide);
    layout->addWidget(m_banner);
    m_splitter = new QSplitter(central);
    m_splitter->addWidget(m_tabs);
    layout->addWidget(m_splitter, 1);
    layout->addWidget(m_findBar);
    setCentralWidget(central);
    connect(m_findBar, &FindBar::findNext, this, [this]() { findInEditor(true); });
    connect(m_findBar, &FindBar::findPrevious, this, [this]() { findInEditor(false); });
    connect(m_findBar, &FindBar::replaceOne, this, &MainWindow::replaceOne);
    connect(m_findBar, &FindBar::replaceAll, this, &MainWindow::replaceAllInEditor);
    connect(m_findBar, &FindBar::closed, this, [this]() {
        if (FernyEditor *editor = currentEditor()) {
            editor->setFocus();
        }
    });

    // Find in files results, a tab of the message window.
    m_results->setColumnCount(3);
    m_results->setHeaderLabels({tr("File"), tr("Line"), tr("Text")});
    m_results->setRootIsDecorated(false);
    m_results->header()->setStretchLastSection(true);
    connect(m_results, &QTreeWidget::itemActivated, this, &MainWindow::openFileHit);
    connect(&m_core, &FernyCore::findInFilesFinished, this, &MainWindow::showFindInFilesResults);
    resize(1100, 760);
    // Status bar fields, as in Geany: position, selection, indentation,
    // encoding, line ending, filetype, state.
    for (QLabel *field : {m_position, m_selection, m_indentation, m_encoding, m_eol, m_filetype,
                          m_documentState, m_trust}) {
        field->setTextFormat(Qt::PlainText);
        statusBar()->addPermanentWidget(field);
    }
    createDocks();
    createMenus();
    createToolbar();
    addTab(m_core.newDocument());
}

void MainWindow::createDocks() {
    // Sidebar: Documents and Files.
    m_sidebarDock = new QDockWidget(tr("Sidebar"), this);
    m_sidebarDock->setObjectName(QStringLiteral("sidebarDock"));
    m_sidebarDock->setFeatures(QDockWidget::DockWidgetMovable | QDockWidget::DockWidgetClosable);
    m_sidebar = new QTabWidget(m_sidebarDock);
    m_sidebar->setDocumentMode(true);
    m_documents = new QListWidget(m_sidebar);
    m_documents->setUniformItemSizes(true);
    m_sidebar->addTab(m_documents, tr("Documents"));
    m_files = new FileBrowser(m_core, m_sidebar);
    m_files->setShowHidden(m_core.settingValue(QStringLiteral("files.browser_show_hidden")) ==
                           QStringLiteral("true"));
    m_files->setRoot(QString());
    m_sidebar->addTab(m_files, tr("Files"));
    // Symbols (H2.2): grouped by kind, rebuilt after edits settle.
    m_symbols = new QTreeWidget(m_sidebar);
    m_symbols->setHeaderHidden(true);
    m_symbols->setUniformRowHeights(true);
    m_sidebar->insertTab(1, m_symbols, tr("Symbols"));
    // Spell checking (H4.2c): half a second after typing.
    m_spellTimer = new QTimer(this);
    m_spellTimer->setSingleShot(true);
    m_spellTimer->setInterval(500);
    connect(m_spellTimer, &QTimer::timeout, this, &MainWindow::refreshSpelling);
    // Save Actions (H4.2a): autosave once a minute when enabled.
    auto *autosave = new QTimer(this);
    connect(autosave, &QTimer::timeout, this, &MainWindow::autosaveNow);
    autosave->start(60 * 1000);
    m_symbolsTimer = new QTimer(this);
    m_symbolsTimer->setSingleShot(true);
    m_symbolsTimer->setInterval(800);
    connect(m_symbolsTimer, &QTimer::timeout, this, &MainWindow::refreshSymbols);
    connect(m_symbolsTimer, &QTimer::timeout, this, [this]() {
        if (const FernyEditor *editor = currentEditor()) {
            m_core.lspChange(editor->documentId(), editor->utf8Text());
        }
    });
    // Language servers (H3.1a).
    connect(&m_core, &FernyCore::lspDiagnosticsChanged, this, &MainWindow::applyDiagnostics);
    connect(&m_core, &FernyCore::lspAnswer, this, &MainWindow::onLspAnswer);
    connect(&m_core, &FernyCore::lspServerStopped, this, [this](const QString &name, bool crashed) {
        if (crashed) {
            showMessage(tr("The language server %1 stopped.").arg(m_core.displayName(name)));
        }
    });
    connect(m_symbols, &QTreeWidget::itemActivated, this, [this](QTreeWidgetItem *item) {
        const quint32 line = item->data(0, Qt::UserRole).toUInt();
        if (line > 0) {
            jumpTo(documentId(), line, 0);
        }
    });
    m_sidebarDock->setWidget(m_sidebar);
    addDockWidget(Qt::LeftDockWidgetArea, m_sidebarDock);
    resizeDocks({m_sidebarDock}, {220}, Qt::Horizontal);
    connect(m_documents, &QListWidget::currentRowChanged, this, [this](int row) {
        const quint64 id = row >= 0 ? m_documents->item(row)->data(Qt::UserRole).toULongLong() : 0;
        for (int index = 0; index < m_tabs->count(); ++index) {
            if (id != 0 && editorAt(index)->documentId() == id) {
                m_tabs->setCurrentIndex(index);
            }
        }
    });
    // Git status of the shown folder (H4.2d): trusted folders only.
    connect(m_files, &FileBrowser::rootChanged, this,
            [this](const QString &root) { m_core.gitStatus(root); });
    connect(&m_core, &FernyCore::gitStatusReady, this,
            [this](const QString &, const QStringList &entries) {
                QHash<QString, QString> status;
                for (const QString &entry : entries) {
                    status.insert(entry.mid(2), entry.left(1));
                }
                m_files->setGitStatus(status);
            });
    connect(&m_core, &FernyCore::gitBlameReady, this,
            [this](const QString &path, const QStringList &lines) {
                if (lines.isEmpty()) {
                    statusBar()->showMessage(tr("Git has no history for this file."), 5000);
                    return;
                }
                m_results->clear();
                QList<QTreeWidgetItem *> items;
                for (const QString &line : lines) {
                    items.append(new QTreeWidgetItem({path, line.section(QLatin1Char('\t'), 0, 0),
                                                      line.section(QLatin1Char('\t'), 1)}));
                }
                m_results->addTopLevelItems(items);
                m_messageDock->show();
                m_messageWindow->showResults(
                    tr("Blame of %1").arg(m_core.displayName(QFileInfo(path).fileName())));
            });
    connect(m_files, &FileBrowser::fileActivated, this,
            [this](const QString &path) { openFile(path); });
    connect(m_files, &FileBrowser::currentFolderRequested, this, [this]() {
        const QString path = m_core.documentPath(documentId());
        m_files->setRoot(path.isEmpty() ? QString() : QFileInfo(path).absolutePath());
    });

    // Message window: Status, Messages, Find in Files, Scribble.
    m_messageDock = new QDockWidget(tr("Message Window"), this);
    m_messageDock->setObjectName(QStringLiteral("messageDock"));
    m_messageDock->setFeatures(QDockWidget::DockWidgetMovable | QDockWidget::DockWidgetClosable);
    m_messageWindow = new MessageWindow(m_results, m_core.maxScribbleBytes(), m_messageDock);
    m_messageDock->setWidget(m_messageWindow);
    addDockWidget(Qt::BottomDockWidgetArea, m_messageDock);
    resizeDocks({m_messageDock}, {160}, Qt::Vertical);
    connect(statusBar(), &QStatusBar::messageChanged, m_messageWindow,
            &MessageWindow::appendStatus);

    // Markdown preview (H4.2b): no scripts, no images, links opened only
    // when clicked and only for safe schemes.
    m_previewDock = new QDockWidget(tr("Markdown Preview"), this);
    m_previewDock->setObjectName(QStringLiteral("previewDock"));
    m_previewDock->setFeatures(QDockWidget::DockWidgetMovable | QDockWidget::DockWidgetClosable);
    m_preview = new QTextBrowser(m_previewDock);
    m_preview->setOpenLinks(false);
    m_preview->setOpenExternalLinks(false);
    connect(m_preview, &QTextBrowser::anchorClicked, this, [](const QUrl &url) {
        const QString scheme = url.scheme().toLower();
        if (scheme == QStringLiteral("http") || scheme == QStringLiteral("https") ||
            scheme == QStringLiteral("mailto")) {
            QDesktopServices::openUrl(url);
        }
    });
    m_previewDock->setWidget(m_preview);
    addDockWidget(Qt::RightDockWidgetArea, m_previewDock);
    m_previewDock->hide();
    m_previewTimer = new QTimer(this);
    m_previewTimer->setSingleShot(true);
    m_previewTimer->setInterval(300);
    connect(m_previewTimer, &QTimer::timeout, this, &MainWindow::refreshPreview);
    // Only when shown: hiding also happens while the window is destroyed.
    connect(m_previewDock, &QDockWidget::visibilityChanged, this, [this](bool visible) {
        if (visible) {
            refreshPreview();
        }
    });
}

void MainWindow::refreshPreview() {
    if (!m_previewDock || m_previewDock->isHidden()) {
        return;
    }
    const FernyEditor *editor = currentEditor();
    if (!editor || editor->filetypeId() != QStringLiteral("markdown")) {
        m_preview->setPlainText(tr("The preview shows Markdown documents."));
        return;
    }
    const QString html = m_core.markdownHtml(editor->utf8Text());
    if (html.isEmpty() && editor->length() > 0) {
        m_preview->setPlainText(tr("This document is too large to preview."));
    } else {
        m_preview->setHtml(html);
    }
}

QString MainWindow::previewText() const {
    return m_previewDock->isHidden() ? QString() : m_preview->toPlainText();
}

void MainWindow::showMessage(const QString &text) {
    m_messageWindow->appendMessage(text);
    statusBar()->showMessage(text);
}

void MainWindow::refreshDocuments() {
    if (!m_documents) {
        return;
    }
    const QSignalBlocker blocker(m_documents);
    m_documents->clear();
    for (int index = 0; index < m_tabs->count(); ++index) {
        const FernyEditor *editor = editorAt(index);
        const QString path = m_core.documentPath(editor->documentId());
        const QString name =
            path.isEmpty() ? tr("Untitled") : m_core.displayName(QFileInfo(path).fileName());
        auto *item = new QListWidgetItem(editor->isModified() ? QStringLiteral("*") + name : name);
        item->setToolTip(path.isEmpty() ? QString() : m_core.displayName(path));
        item->setData(Qt::UserRole, QVariant::fromValue(editor->documentId()));
        m_documents->addItem(item);
    }
    m_documents->setCurrentRow(m_tabs->currentIndex());
}

void MainWindow::refreshSymbols() {
    if (!m_symbols) {
        return;
    }
    // The language server first (H4.2a); tree-sitter without one.
    FernyEditor *editor = currentEditor();
    m_symbolsToken = editor ? m_core.lspRequest(editor->documentId(), 8, editor->utf8Text(), 0) : 0;
    if (m_symbolsToken == 0) {
        showTreeSitterSymbols();
    }
}

void MainWindow::addSymbol(QHash<QString, QTreeWidgetItem *> &groups, const QString &kind,
                           const QString &name, int line) {
    QTreeWidgetItem *group = groups.value(kind);
    if (!group) {
        group = new QTreeWidgetItem(
            m_symbols, {QCoreApplication::translate("ferny::symbols", kind.toUtf8().constData())});
        group->setData(0, Qt::UserRole + 1, kind);
        group->setExpanded(true);
        groups.insert(kind, group);
    }
    auto *item = new QTreeWidgetItem(group, {name});
    item->setData(0, Qt::UserRole, line);
    item->setToolTip(0, tr("Line %1").arg(line));
}

void MainWindow::showTreeSitterSymbols() {
    m_symbols->clear();
    m_symbolsSource = QStringLiteral("tree-sitter");
    const FernyEditor *editor = currentEditor();
    const int count = editor ? m_core.extractSymbols(editor->filetypeId(), editor->utf8Text()) : -1;
    QHash<QString, QTreeWidgetItem *> groups;
    for (int i = 0; i < count; ++i) {
        addSymbol(groups, m_core.symbolKind(i), m_core.symbolName(i),
                  static_cast<int>(m_core.symbolLine(i)));
    }
}

QStringList MainWindow::symbolEntries() const {
    QStringList entries;
    for (int g = 0; g < m_symbols->topLevelItemCount(); ++g) {
        const QTreeWidgetItem *group = m_symbols->topLevelItem(g);
        for (int i = 0; i < group->childCount(); ++i) {
            entries.append(QStringLiteral("%1:%2:%3")
                               .arg(group->data(0, Qt::UserRole + 1).toString(),
                                    group->child(i)->text(0),
                                    group->child(i)->data(0, Qt::UserRole).toString()));
        }
    }
    return entries;
}

QStringList MainWindow::documentNames() const {
    QStringList names;
    for (int row = 0; row < m_documents->count(); ++row) {
        names.append(m_documents->item(row)->text());
    }
    return names;
}

QStringList MainWindow::statusFields() const {
    QStringList fields;
    for (const QLabel *field : {m_position, m_selection, m_indentation, m_encoding, m_eol,
                                m_filetype, m_documentState, m_trust}) {
        fields.append(field->text());
    }
    return fields;
}

// --- Menus and commands ----------------------------------------------------

void MainWindow::createMenus() {
    // Menus are data in the core's command registry; every action invokes
    // its command through the core. The recent-files submenu is dynamic and
    // built by the UI after "file.open".
    connect(&m_core, &FernyCore::commandRequested, this, &MainWindow::runUiCommand);
    const QStringList menuIds = m_core.menuIds();
    for (const QString &menuId : menuIds) {
        QMenu *menu = menuBar()->addMenu(translatedTitle(m_core.menuTitle(menuId)));
        const QStringList items = m_core.menuItems(menuId);
        for (const QString &commandId : items) {
            if (commandId.isEmpty()) {
                menu->addSeparator();
                continue;
            }
            QAction *action = menu->addAction(translatedTitle(m_core.commandTitle(commandId)));
            action->setObjectName(commandId);
            QList<QKeySequence> shortcuts;
            const QStringList keys = m_core.commandShortcuts(commandId);
            for (const QString &key : keys) {
                shortcuts.append(QKeySequence::fromString(key, QKeySequence::PortableText));
            }
            action->setShortcuts(shortcuts);
            // On macOS these roles move the entries to the application menu.
            if (commandId == QStringLiteral("app.quit")) {
                action->setMenuRole(QAction::QuitRole);
            } else if (commandId == QStringLiteral("help.about")) {
                action->setMenuRole(QAction::AboutRole);
            } else {
                action->setMenuRole(QAction::NoRole);
            }
            if (commandId == QStringLiteral("view.word_wrap") ||
                commandId == QStringLiteral("view.indent_guides")) {
                action->setCheckable(true);
                const QString key = commandId == QStringLiteral("view.word_wrap")
                                        ? QStringLiteral("editor.word_wrap")
                                        : QStringLiteral("editor.indent_guides");
                action->setChecked(m_core.settingValue(key) == QStringLiteral("true"));
            } else if (commandId == QStringLiteral("view.show_invisible") ||
                       commandId == QStringLiteral("view.auto_close")) {
                action->setCheckable(true);
                action->setChecked(commandId == QStringLiteral("view.show_invisible")
                                       ? m_showInvisible
                                       : m_autoClose);
            } else if (commandId == QStringLiteral("view.sidebar") ||
                       commandId == QStringLiteral("view.message_window")) {
                // Checked state follows the dock (closed with its button,
                // restored from the session).
                QDockWidget *dock =
                    commandId == QStringLiteral("view.sidebar") ? m_sidebarDock : m_messageDock;
                action->setCheckable(true);
                action->setChecked(true);
                connect(dock, &QDockWidget::visibilityChanged, action,
                        [action, dock]() { action->setChecked(!dock->isHidden()); });
            } else if (commandId == QStringLiteral("view.spell_check")) {
                action->setCheckable(true);
                action->setChecked(m_spellCheck);
            } else if (commandId == QStringLiteral("view.markdown_preview")) {
                action->setCheckable(true);
                connect(
                    m_previewDock, &QDockWidget::visibilityChanged, action,
                    [action, dock = m_previewDock]() { action->setChecked(!dock->isHidden()); });
            } else if (commandId == QStringLiteral("app.preferences")) {
                action->setMenuRole(QAction::PreferencesRole);
            }
            connect(action, &QAction::triggered, this, [this, commandId]() {
                if (!m_core.invokeCommand(commandId)) {
                    statusBar()->showMessage(tr("This command is not available."));
                }
            });
            m_commandIds.append(commandId);
            if (commandId == QStringLiteral("file.open")) {
                m_recentMenu = menu->addMenu(tr("Open &Recent"));
                connect(m_recentMenu, &QMenu::aboutToShow, this, &MainWindow::rebuildRecentMenu);
            }
        }
        // Dynamic submenus built by the UI from the core's catalog.
        if (menuId == QStringLiteral("view")) {
            menu->addSeparator();
            m_schemeMenu = menu->addMenu(tr("&Color Scheme"));
            auto *group = new QActionGroup(m_schemeMenu);
            const QStringList schemes = m_core.colorSchemeIds();
            for (const QString &id : schemes) {
                QAction *action = m_schemeMenu->addAction(m_core.colorSchemeName(id));
                action->setCheckable(true);
                action->setData(id);
                group->addAction(action);
                connect(action, &QAction::triggered, this, [this, id]() { setColorScheme(id); });
            }
        } else if (menuId == QStringLiteral("document")) {
            m_filetypeMenu = menu->addMenu(tr("Set &Filetype"));
            QAction *plain = m_filetypeMenu->addAction(tr("Plain Text"));
            connect(plain, &QAction::triggered, this,
                    [this]() { setFiletype(currentEditor(), QString()); });
            m_filetypeMenu->addSeparator();
            const QStringList filetypes = m_core.filetypeIds();
            for (const QString &id : filetypes) {
                QAction *action = m_filetypeMenu->addAction(m_core.filetypeName(id));
                connect(action, &QAction::triggered, this,
                        [this, id]() { setFiletype(currentEditor(), id); });
            }
        }
    }
}

bool MainWindow::runCommand(const QString &commandId) {
    if (commandId.startsWith(QStringLiteral("plugin."))) {
        if (m_core.pluginCommandTitle(commandId).isEmpty()) {
            return false;
        }
        runPluginCommand(commandId);
        return true;
    }
    return m_core.invokeCommand(commandId);
}

void MainWindow::createToolbar() {
    m_toolbar = addToolBar(tr("Main Toolbar"));
    m_toolbar->setObjectName(QStringLiteral("mainToolbar"));
    m_toolbar->setMovable(false);
    m_toolbar->setFloatable(false);
    m_toolbar->setToolButtonStyle(Qt::ToolButtonIconOnly);
    m_toolbar->setIconSize(QSize(20, 20));
    m_toolbar->setAccessibleName(tr("Main Toolbar"));

    // These common, built-in actions are deliberately a fixed starter set.
    // Each must also be present in the core's command registry; unknown or
    // unavailable commands fail closed and are not exposed in the toolbar.
    const QList<QPair<QString, QString>> defaults = {
        // File group
        {QStringLiteral("file.new"), QStringLiteral(":/lucide/file-plus.svg")},
        {QStringLiteral("file.open"), QStringLiteral(":/lucide/folder-open.svg")},
        {QStringLiteral("file.save"), QStringLiteral(":/lucide/save.svg")},
        {QStringLiteral("file.save_all"), QStringLiteral(":/lucide/copy.svg")},
        {},
        {QStringLiteral("file.print"), QStringLiteral(":/lucide/printer.svg")},
        {},
        // Edit group
        {QStringLiteral("edit.undo"), QStringLiteral(":/lucide/undo-2.svg")},
        {QStringLiteral("edit.redo"), QStringLiteral(":/lucide/redo-2.svg")},
        {QStringLiteral("edit.cut"), QStringLiteral(":/lucide/scissors.svg")},
        {QStringLiteral("edit.copy"), QStringLiteral(":/lucide/copy.svg")},
        {QStringLiteral("edit.paste"), QStringLiteral(":/lucide/clipboard.svg")},
        {},
        // Search group
        {QStringLiteral("search.find"), QStringLiteral(":/lucide/search.svg")},
        {QStringLiteral("search.find_next"), QStringLiteral(":/lucide/chevron-down.svg")},
        {QStringLiteral("search.find_previous"), QStringLiteral(":/lucide/chevron-up.svg")},
        {},
        // Build group
        {QStringLiteral("build.compile"), QStringLiteral(":/lucide/code.svg")},
        {QStringLiteral("build.build"), QStringLiteral(":/lucide/hammer.svg")},
        {QStringLiteral("build.execute"), QStringLiteral(":/lucide/play.svg")},
        {QStringLiteral("build.stop"), QStringLiteral(":/lucide/square.svg")},
        {},
        // Navigation group
        {QStringLiteral("view.navigate_back"), QStringLiteral(":/lucide/arrow-left.svg")},
        {QStringLiteral("view.navigate_forward"), QStringLiteral(":/lucide/arrow-right.svg")},
    };
    for (const auto &pair : defaults) {
        const QString commandId = pair.first;
        const QString iconPath = pair.second;
        if (commandId.isEmpty()) {
            m_toolbar->addSeparator();
            continue;
        }
        const QString title = m_core.commandTitle(commandId);
        if (title.isEmpty()) {
            continue;
        }
        QAction *action = m_toolbar->addAction(QIcon(iconPath), QString());
        action->setObjectName(commandId);
        action->setToolTip(translatedTitle(title).remove(QLatin1Char('&')));
        action->setStatusTip(translatedTitle(title).remove(QLatin1Char('&')));
        action->setData(commandId);
        connect(action, &QAction::triggered, this, [this, commandId]() {
            if (!m_core.invokeCommand(commandId)) {
                statusBar()->showMessage(tr("This command is not available."));
            }
        });
        m_toolbarCommandIds.append(commandId);
    }
}

void MainWindow::refreshToolbarActions() {
    if (!m_toolbar) {
        return;
    }
    for (QAction *action : m_toolbar->actions()) {
        const QString commandId = action->data().toString();
        if (commandId == QStringLiteral("edit.undo") || commandId == QStringLiteral("edit.redo")) {
            const FernyEditor *editor = currentEditor();
            action->setEnabled(editor != nullptr && (commandId == QStringLiteral("edit.undo")
                                                         ? editor->send(SCI_CANUNDO) != 0
                                                         : editor->send(SCI_CANREDO) != 0));
        }
    }
}

bool MainWindow::toolbarIconOnly() const {
    return m_toolbar && m_toolbar->toolButtonStyle() == Qt::ToolButtonIconOnly;
}

bool MainWindow::toolbarIconsAvailable() const {
    if (!m_toolbar || m_toolbar->actions().isEmpty()) {
        return false;
    }
    return std::all_of(
        m_toolbar->actions().cbegin(), m_toolbar->actions().cend(),
        [](const QAction *action) { return action->isSeparator() || !action->icon().isNull(); });
}

void MainWindow::applyEditingOptions(FernyEditor *editor) {
    editor->setAutoClose(m_autoClose);
    editor->setControlCharacterLabels(m_showInvisible ? m_core.controlCharacterLabels()
                                                      : QStringList());
    editor->applyDisplay(displayOptions());
    editor->setEditingOptions(editingOptions());
}

EditingOptions MainWindow::editingOptions() const {
    const auto flag = [this](const char *key, bool fallback) {
        const QString value = m_core.settingValue(QString::fromLatin1(key));
        return value.isEmpty() ? fallback : value == QStringLiteral("true");
    };
    EditingOptions options;
    options.autoIndent = flag("editor.auto_indent", true);
    options.tabIndents = flag("editor.tab_indents", true);
    options.backspaceUnindents = flag("editor.backspace_unindents", false);
    options.smartHome = flag("editor.smart_home", true);
    options.newlineStripsTrailing = flag("editor.newline_strips_trailing", false);
    options.autoCloseQuotes = flag("editor.auto_close_quotes", false);
    return options;
}

DisplayOptions MainWindow::displayOptions() const {
    const auto flag = [this](const char *key, bool fallback) {
        const QString value = m_core.settingValue(QString::fromLatin1(key));
        return value.isEmpty() ? fallback : value == QStringLiteral("true");
    };
    const QString virtualSpace = m_core.settingValue(QStringLiteral("editor.virtual_space"));
    const QString history = m_core.settingValue(QStringLiteral("editor.change_history"));
    DisplayOptions options;
    options.wordWrap = flag("editor.word_wrap", false);
    options.indentGuides = flag("editor.indent_guides", false);
    options.showEol = flag("editor.show_eol", false);
    options.markersMargin = flag("editor.markers_margin", true);
    options.folding = flag("editor.folding", true);
    options.scrollPastEnd = flag("editor.scroll_past_end", false);
    options.caretMarginLines =
        m_core.settingValue(QStringLiteral("editor.caret_margin_lines")).toInt();
    options.longLineBackground = m_core.settingValue(QStringLiteral("editor.long_line_style")) ==
                                 QStringLiteral("background");
    options.virtualSpace = virtualSpace == QStringLiteral("rectangular") ? VirtualSpace::Rectangular
                           : virtualSpace == QStringLiteral("everywhere") ? VirtualSpace::Always
                                                                          : VirtualSpace::Off;
    options.changeHistory = history == QStringLiteral("margin")      ? ChangeHistory::Margin
                            : history == QStringLiteral("indicator") ? ChangeHistory::Indicator
                                                                     : ChangeHistory::Off;
    return options;
}

void MainWindow::toggleDisplaySetting(const QString &key, const QString &commandId) {
    const bool on = m_core.settingValue(key) != QStringLiteral("true");
    m_core.setSetting(key, on ? QStringLiteral("true") : QStringLiteral("false"));
    if (QAction *action = findChild<QAction *>(commandId)) {
        action->setChecked(on);
    }
    for (int index = 0; index < m_tabs->count(); ++index) {
        applyEditingOptions(editorAt(index));
    }
    if (m_splitEditor) {
        applyEditingOptions(m_splitEditor);
    }
    saveSettings();
}

void MainWindow::saveSettings() {
    if (!m_settingsPath.isEmpty() && !m_core.saveSettings(m_settingsPath)) {
        statusBar()->showMessage(tr("The settings could not be saved."));
    }
}

void MainWindow::setConfigDir(const QString &dir) {
    m_configDir = dir;
    m_spellCheck = m_core.settingValue(QStringLiteral("spell.enabled")) == QStringLiteral("true");
    if (m_spellCheck) {
        loadSpeller();
    }
}

void MainWindow::loadSpeller() {
    // The user's dictionaries first, then the system's read-only ones.
    QStringList folders;
    if (!m_configDir.isEmpty()) {
        folders << m_configDir + QStringLiteral("/dictionaries");
    }
    folders << QStringLiteral("/usr/share/hunspell") << QStringLiteral("/usr/share/myspell")
            << QDir::homePath() + QStringLiteral("/Library/Spelling");
    const QString language = m_core.settingValue(QStringLiteral("spell.language"));
    const QString personal = m_configDir.isEmpty()
                                 ? QString()
                                 : m_configDir + QStringLiteral("/dictionaries/personal.txt");
    if (m_core.spellLoad(language, folders, personal) != 0) {
        statusBar()->showMessage(
            tr("No usable spelling dictionary for %1.").arg(m_core.displayName(language)), 5000);
    }
}

QByteArray MainWindow::spellText(FernyEditor *editor) const {
    // Plain text and Markdown whole; code only in comments and strings
    // (Lexilla's style tags). Other bytes become spaces: offsets stay.
    QByteArray text = editor->utf8Text();
    const QString filetype = editor->filetypeId();
    if (filetype.isEmpty() || filetype == QStringLiteral("markdown")) {
        return text;
    }
    editor->send(SCI_COLOURISE, 0, -1);
    QHash<int, bool> checkable;
    for (qsizetype i = 0; i < text.size(); ++i) {
        const auto style = static_cast<int>(editor->send(SCI_GETSTYLEAT, static_cast<uptr_t>(i)));
        auto it = checkable.find(style);
        if (it == checkable.end()) {
            const auto s = static_cast<uptr_t>(style);
            QByteArray tags(static_cast<qsizetype>(editor->send(SCI_TAGSOFSTYLE, s, 0)) + 1, '\0');
            editor->send(SCI_TAGSOFSTYLE, s, reinterpret_cast<sptr_t>(tags.data()));
            it = checkable.insert(style, tags.contains("comment") || tags.contains("string"));
        }
        if (!it.value()) {
            text[i] = ' ';
        }
    }
    return text;
}

void MainWindow::refreshSpelling() {
    FernyEditor *editor = currentEditor();
    if (!editor) {
        return;
    }
    editor->send(SCI_SETINDICATORCURRENT, kSpellIndicator);
    editor->send(SCI_INDICATORCLEARRANGE, 0, editor->length());
    m_misspelled = 0;
    if (!m_spellCheck) {
        return;
    }
    const QStringList ranges = m_core.spellMisspelled(spellText(editor));
    for (const QString &range : ranges) {
        const qint64 start = range.section(QLatin1Char(' '), 0, 0).toLongLong();
        const qint64 end = range.section(QLatin1Char(' '), 1, 1).toLongLong();
        editor->send(SCI_INDICATORFILLRANGE, static_cast<uptr_t>(start), end - start);
    }
    m_misspelled = static_cast<int>(ranges.size());
}

int MainWindow::spellingCount() {
    refreshSpelling();
    return m_misspelled;
}

void MainWindow::showEditorMenu(FernyEditor *editor, const QPoint &pos) {
    QMenu menu(this);
    const auto at = editor->send(SCI_POSITIONFROMPOINTCLOSE, static_cast<uptr_t>(pos.x()), pos.y());
    if (m_spellCheck && at >= 0 &&
        editor->send(SCI_INDICATORVALUEAT, kSpellIndicator, static_cast<sptr_t>(at)) != 0) {
        const sptr_t start = editor->send(SCI_INDICATORSTART, kSpellIndicator, at);
        const sptr_t end = editor->send(SCI_INDICATOREND, kSpellIndicator, at);
        const QString word = editor->textRange(start, end);
        const QStringList suggestions = m_core.spellSuggest(word);
        for (const QString &suggestion : suggestions) {
            connect(menu.addAction(suggestion), &QAction::triggered, this,
                    [this, editor, start, end, suggestion]() {
                        const QByteArray utf8 = suggestion.toUtf8();
                        editor->send(SCI_SETTARGETRANGE, static_cast<uptr_t>(start), end);
                        editor->send(SCI_REPLACETARGET, static_cast<uptr_t>(utf8.size()),
                                     reinterpret_cast<sptr_t>(utf8.constData()));
                        refreshSpelling();
                    });
        }
        if (suggestions.isEmpty()) {
            menu.addAction(tr("(no suggestions)"))->setEnabled(false);
        }
        connect(menu.addAction(tr("Add to Dictionary")), &QAction::triggered, this, [this, word]() {
            if (!m_core.spellAdd(word)) {
                statusBar()->showMessage(tr("This word cannot be added."), 3000);
            }
            refreshSpelling();
        });
        menu.addSeparator();
    }
    // Scintilla's own menu is off: the same editing commands, translated.
    for (const QString &id :
         {QStringLiteral("edit.undo"), QStringLiteral("edit.redo"), QString(),
          QStringLiteral("edit.cut"), QStringLiteral("edit.copy"), QStringLiteral("edit.paste"),
          QString(), QStringLiteral("edit.select_all")}) {
        if (id.isEmpty()) {
            menu.addSeparator();
            continue;
        }
        QString title = translatedTitle(m_core.commandTitle(id));
        connect(menu.addAction(title), &QAction::triggered, this, [this, id]() { runCommand(id); });
    }
    menu.exec(editor->mapToGlobal(pos));
}

void MainWindow::applySettings() {
    m_scheme = m_core.settingValue(QStringLiteral("editor.color_scheme"));
    setShowInvisible(m_core.settingValue(QStringLiteral("editor.show_invisible")) ==
                     QStringLiteral("true"));
    setAutoClose(m_core.settingValue(QStringLiteral("editor.auto_close")) ==
                 QStringLiteral("true"));
    setColorScheme(m_scheme);
    for (int index = 0; index < m_tabs->count(); ++index) {
        applyEditingOptions(editorAt(index));
    }
    if (m_splitEditor) {
        applyEditingOptions(m_splitEditor);
    }
    m_files->setShowHidden(m_core.settingValue(QStringLiteral("files.browser_show_hidden")) ==
                           QStringLiteral("true"));
    // Preferences may have changed the dictionary or turned checking on.
    m_spellCheck = m_core.settingValue(QStringLiteral("spell.enabled")) == QStringLiteral("true");
    if (m_spellCheck) {
        loadSpeller();
    }
    refreshSpelling();
}

void MainWindow::showPreferences() {
    PreferencesDialog dialog(m_core, this);
    connect(&dialog, &PreferencesDialog::editShortcutsRequested, this, &MainWindow::showShortcuts);
    if (dialog.exec() != QDialog::Accepted) {
        return;
    }
    if (!dialog.apply().isEmpty()) {
        statusBar()->showMessage(tr("Some values were not valid and were not changed."));
    }
    saveSettings();
    applySettings();
    updateStatus();  // a folder may have been revoked
    refreshToolbarActions();
}

void MainWindow::trustFolder() {
    const QString path = m_core.documentPath(documentId());
    QString folder = m_smokeTrustFolder;
    if (folder.isEmpty()) {
        if (path.isEmpty()) {
            showMessage(tr("Save the document first: only folders can be trusted."));
            return;
        }
        // The document's folder and its parents; the root is never offered.
        QStringList choices;
        for (QDir dir(QFileInfo(path).absolutePath()); !dir.isRoot(); dir.cdUp()) {
            choices.append(QDir::toNativeSeparators(dir.absolutePath()));
        }
        if (choices.isEmpty()) {
            showMessage(tr("The root folder cannot be trusted."));
            return;
        }
        bool ok = false;
        folder = QInputDialog::getItem(
            this, tr("Trust Folder"),
            tr("Trust this folder and everything in it? Trusted folders may run their build "
               "commands and tools."),
            choices, 0, false, &ok);
        if (!ok) {
            return;
        }
        if (QFileInfo(folder).canonicalFilePath() ==
                QFileInfo(QDir::homePath()).canonicalFilePath() &&
            QMessageBox::warning(this, tr("Trust Folder"),
                                 tr("Trusting your home folder trusts everything in it, including "
                                    "downloads. Trust it anyway?"),
                                 QMessageBox::Yes | QMessageBox::Cancel,
                                 QMessageBox::Cancel) != QMessageBox::Yes) {
            return;
        }
    }
    switch (m_core.trustFolder(folder)) {
        case TrustStatus::Trusted:
            statusBar()->showMessage(tr("Folder trusted."));
            m_banner->hide();
            for (int index = 0; index < m_tabs->count(); ++index) {
                lspSync(editorAt(index));
            }
            break;
        case TrustStatus::Root:
            showMessage(tr("The root folder cannot be trusted."));
            break;
        case TrustStatus::Full:
            showMessage(tr("Too many trusted folders; revoke some in Preferences."));
            break;
        case TrustStatus::NotAFolder:
        case TrustStatus::Failed:
        default:
            showMessage(tr("The folder could not be trusted."));
            break;
    }
    updateStatus();
}

void MainWindow::importGeany() {
    if (m_settingsPath.isEmpty()) {
        return;
    }
    const bool smoke = !m_smokeImportDirectory.isEmpty();
#ifdef Q_OS_WIN
    const QString start = QDir(qEnvironmentVariable("APPDATA")).filePath(QStringLiteral("geany"));
#else
    const QString start = QDir::home().filePath(QStringLiteral(".config/geany"));
#endif
    const QString dir = smoke ? m_smokeImportDirectory
                              : QFileDialog::getExistingDirectory(
                                    this, tr("Geany Configuration Folder"),
                                    QFileInfo::exists(start) ? start : QDir::homePath());
    if (dir.isEmpty()) {
        return;
    }
    const GeanyImportSummary summary = m_core.previewGeanyImport(dir);
    if (summary.settings + summary.keybindings + summary.files == 0 && !smoke) {
        QMessageBox::information(this, tr("Import Geany Settings"),
                                 tr("No Geany settings were found in that folder."));
        return;
    }
    // Preview first: nothing is written until the user agrees (T11).
    QMessageBox box(this);
    box.setWindowTitle(tr("Import Geany Settings"));
    box.setText(tr("Import %1 settings, %2 keyboard shortcuts and %3 filetype or colour "
                   "scheme files? Build commands are never imported.")
                    .arg(summary.settings)
                    .arg(summary.keybindings)
                    .arg(summary.files));
    if (summary.not_imported > 0) {
        box.setInformativeText(
            tr("%n item(s) cannot be imported (see details).", nullptr, summary.not_imported));
        box.setDetailedText(m_core.geanyImportNotImported().join(QLatin1Char('\n')));
    }
    box.setStandardButtons(QMessageBox::Yes | QMessageBox::Cancel);
    box.setDefaultButton(QMessageBox::Yes);
    if (!smoke && box.exec() != QMessageBox::Yes) {
        return;
    }
    const QStringList rejected = m_core.applyGeanyImport(QFileInfo(m_settingsPath).absolutePath());
    m_core.loadTemplates(QFileInfo(m_settingsPath).absolutePath());  // imported snippets
    const bool saved = m_core.saveSettings(m_settingsPath) &&
                       (m_keybindingsPath.isEmpty() || m_core.saveKeybindings(m_keybindingsPath));
    if (!saved) {
        statusBar()->showMessage(tr("The settings could not be saved."));
    }
    m_lastImportReport = QStringLiteral(
                             "settings=%1 keybindings=%2 files=%3 not_imported=%4 "
                             "rejected=%5 saved=%6")
                             .arg(summary.settings)
                             .arg(summary.keybindings)
                             .arg(summary.files)
                             .arg(summary.not_imported)
                             .arg(rejected.size())
                             .arg(saved ? QStringLiteral("yes") : QStringLiteral("no"));
    for (const QString &item : m_core.geanyImportNotImported()) {
        m_messageWindow->appendMessage(tr("Not imported from Geany: %1").arg(item));
    }
    applySettings();
    refreshShortcuts();
    for (int index = 0; index < m_tabs->count(); ++index) {
        applyHighlighting(editorAt(index));
    }
    statusBar()->showMessage(rejected.isEmpty()
                                 ? tr("Geany settings imported.")
                                 : tr("Geany settings imported; %n value(s) were not valid.",
                                      nullptr, static_cast<int>(rejected.size())));
}

void MainWindow::refreshShortcuts() {
    const auto actions = findChildren<QAction *>();
    for (QAction *action : actions) {
        if (!m_commandIds.contains(action->objectName())) {
            continue;
        }
        QList<QKeySequence> shortcuts;
        const QStringList keys = m_core.commandShortcuts(action->objectName());
        for (const QString &key : keys) {
            shortcuts.append(QKeySequence::fromString(key, QKeySequence::PortableText));
        }
        action->setShortcuts(shortcuts);
    }
    for (QAction *action : m_toolbar ? m_toolbar->actions() : QList<QAction *>()) {
        const QString commandId = action->data().toString();
        const QStringList keys = m_core.commandShortcuts(commandId);
        action->setShortcut(keys.isEmpty() ? QKeySequence()
                                           : QKeySequence::fromString(keys.constFirst(),
                                                                      QKeySequence::PortableText));
    }
}

void MainWindow::showShortcuts() {
    ShortcutsDialog dialog(m_core, this);
    if (dialog.exec() == QDialog::Accepted) {
        if (!m_keybindingsPath.isEmpty() && !m_core.saveKeybindings(m_keybindingsPath)) {
            statusBar()->showMessage(tr("The keyboard shortcuts could not be saved."));
        }
    } else {
        // Cancelled: back to what is on disk.
        m_core.loadKeybindings(m_keybindingsPath);
    }
    refreshShortcuts();
}

void MainWindow::setShowInvisible(bool show) {
    m_showInvisible = show;
    m_core.setSetting(QStringLiteral("editor.show_invisible"),
                      show ? QStringLiteral("true") : QStringLiteral("false"));
    if (QAction *action = findChild<QAction *>(QStringLiteral("view.show_invisible"))) {
        action->setChecked(show);
    }
    for (int index = 0; index < m_tabs->count(); ++index) {
        applyEditingOptions(editorAt(index));
    }
    if (m_splitEditor) {
        applyEditingOptions(m_splitEditor);
    }
}

void MainWindow::setAutoClose(bool enabled) {
    m_autoClose = enabled;
    m_core.setSetting(QStringLiteral("editor.auto_close"),
                      enabled ? QStringLiteral("true") : QStringLiteral("false"));
    if (QAction *action = findChild<QAction *>(QStringLiteral("view.auto_close"))) {
        action->setChecked(enabled);
    }
    for (int index = 0; index < m_tabs->count(); ++index) {
        applyEditingOptions(editorAt(index));
    }
    if (m_splitEditor) {
        applyEditingOptions(m_splitEditor);
    }
}

void MainWindow::splitView(Qt::Orientation orientation) {
    if (!m_splitEditor) {
        m_splitEditor = new FernyEditor(m_splitter);
        m_splitter->addWidget(m_splitEditor);
        showInSplit(currentEditor());
    }
    m_splitter->setOrientation(orientation);
    const int half =
        (orientation == Qt::Horizontal ? m_splitter->width() : m_splitter->height()) / 2;
    m_splitter->setSizes({half, half});
}

void MainWindow::showInSplit(FernyEditor *source) {
    if (!m_splitEditor || !source) {
        return;
    }
    // One shared document: same text, undo and save point; own cursor.
    m_splitEditor->send(SCI_SETDOCPOINTER, 0, source->send(SCI_GETDOCPOINTER));
    m_splitEditor->setDocumentId(source->documentId());
    m_splitEditor->setFiletypeId(source->filetypeId());
    applyEditingOptions(m_splitEditor);
    applyHighlighting(m_splitEditor);
}

QString MainWindow::splitSummary() const {
    if (!m_splitEditor) {
        return QStringLiteral("none");
    }
    const FernyEditor *editor = currentEditor();
    const bool shared =
        editor && editor->send(SCI_GETDOCPOINTER) == m_splitEditor->send(SCI_GETDOCPOINTER);
    return QStringLiteral("%1 %2 shared=%3")
        .arg(m_splitter->orientation() == Qt::Horizontal ? QStringLiteral("side")
                                                         : QStringLiteral("stacked"),
             QFileInfo(m_core.documentPath(m_splitEditor->documentId())).fileName(),
             shared ? QStringLiteral("yes") : QStringLiteral("no"));
}

namespace {

// Replaces the whole text as one undoable change, keeping the cursor.
void replaceWholeText(FernyEditor *editor, const QByteArray &text) {
    const quint32 line = editor->cursorLine();
    const quint32 column = editor->cursorColumn();
    editor->send(SCI_BEGINUNDOACTION);
    editor->send(SCI_TARGETWHOLEDOCUMENT);
    editor->send(SCI_REPLACETARGET, static_cast<uptr_t>(text.size()),
                 reinterpret_cast<sptr_t>(text.constData()));
    editor->send(SCI_ENDUNDOACTION);
    editor->gotoPosition(line, column);
}

}  // namespace

void MainWindow::applySaveTransforms(FernyEditor *editor) {
    // .editorconfig trim_trailing_whitespace / insert_final_newline, applied
    // in the editor as one undoable change so the buffer matches the disk.
    QByteArray changed;
    if (editor->send(SCI_GETREADONLY) ||
        !m_core.applySaveTransforms(editor->documentId(), editor->utf8Text(), changed)) {
        return;
    }
    replaceWholeText(editor, changed);
}

void MainWindow::runBeforeSave(FernyEditor *editor) {
    // Plugins' document.before_save (H3.3c): their text replaces the
    // buffer as one undo action; on failure the document saves unchanged.
    if (editor->isReadOnly() || !m_core.pluginHasProvider(QStringLiteral("before_save"), {})) {
        return;
    }
    const QStringList text =
        askPluginProvider(QStringLiteral("before_save"), editor,
                          static_cast<int>(editor->send(SCI_GETCURRENTPOS)), QString());
    if (!text.isEmpty() && text.first().toUtf8() != editor->utf8Text()) {
        replaceWholeText(editor, text.first().toUtf8());
    }
}

QStringList MainWindow::askPluginProvider(const QString &kind, FernyEditor *editor, int pos,
                                          const QString &word) {
    const auto upos = static_cast<uptr_t>(pos);
    const int line = static_cast<int>(editor->send(SCI_LINEFROMPOSITION, upos)) + 1;
    const int column = static_cast<int>(editor->send(SCI_GETCOLUMN, upos)) + 1;
    const QStringList actions =
        m_core.pluginProvide(kind, m_core.documentPath(editor->documentId()), editor->filetypeId(),
                             line, column, QString::fromUtf8(editor->utf8Text()), word);
    QStringList answers;
    for (qsizetype i = 0; i + 1 < actions.size(); i += 2) {
        if (actions.at(i) == QStringLiteral("item") || actions.at(i) == QStringLiteral("text")) {
            answers.append(actions.at(i + 1));
        }
    }
    applyPluginActions(actions, nullptr);
    return answers;
}

void MainWindow::toggleComment(FernyEditor *editor) {
    if (!editor || editor->send(SCI_GETREADONLY)) {
        return;
    }
    const QString filetype = editor->filetypeId();
    const bool done = editor->transformSelectedLines([&](const QString &text, QString &out) {
        return m_core.toggleComment(filetype, text, out);
    });
    if (!done) {
        statusBar()->showMessage(tr("This filetype has no comment characters."), 3000);
    }
}

void MainWindow::openFindBar(bool replace) {
    FernyEditor *editor = currentEditor();
    QString seed;
    if (editor && !editor->send(SCI_GETSELECTIONEMPTY)) {
        const sptr_t length = editor->send(SCI_GETSELTEXT, 0, 0);
        QByteArray selected(static_cast<qsizetype>(length) + 1, '\0');
        editor->send(SCI_GETSELTEXT, 0, reinterpret_cast<sptr_t>(selected.data()));
        selected.truncate(static_cast<qsizetype>(length));
        seed = QString::fromUtf8(selected);
    }
    m_findBar->open(replace, seed);
}

bool MainWindow::reportSearch(SearchStatus status) {
    QString message;
    switch (status) {
        case SearchStatus::Found:
            break;
        case SearchStatus::NotFound:
            message = tr("Not found");
            break;
        case SearchStatus::InvalidPattern:
            message = tr("Invalid pattern");
            break;
        case SearchStatus::BadLength:
            message = tr("Enter a pattern");
            break;
    }
    m_findBar->setMessage(message);
    statusBar()->showMessage(message, 3000);
    return status == SearchStatus::Found;
}

bool MainWindow::findInEditor(bool forward) {
    FernyEditor *editor = currentEditor();
    if (!editor || m_findBar->pattern().isEmpty()) {
        openFindBar(false);
        return false;
    }
    const sptr_t from = editor->send(forward ? SCI_GETSELECTIONEND : SCI_GETSELECTIONSTART);
    std::int64_t start = 0;
    std::int64_t end = 0;
    const SearchStatus status = m_core.findInText(editor->utf8Text(), m_findBar->pattern(),
                                                  m_findBar->flags(), from, forward, start, end);
    if (!reportSearch(status)) {
        return false;
    }
    editor->send(SCI_SETSEL, static_cast<uptr_t>(start), end);
    editor->send(SCI_SCROLLCARET);
    return true;
}

void MainWindow::replaceOne() {
    FernyEditor *editor = currentEditor();
    if (!editor || editor->send(SCI_GETREADONLY)) {
        return;
    }
    const sptr_t start = editor->send(SCI_GETSELECTIONSTART);
    const sptr_t end = editor->send(SCI_GETSELECTIONEND);
    QString replacement;
    const SearchStatus status =
        m_core.replacementFor(editor->utf8Text(), m_findBar->pattern(), m_findBar->flags(), start,
                              end, m_findBar->replacement(), replacement);
    if (status == SearchStatus::Found) {
        const QByteArray utf8 = replacement.toUtf8();
        editor->send(SCI_REPLACESEL, 0, reinterpret_cast<sptr_t>(utf8.constData()));
    } else if (status != SearchStatus::NotFound) {
        reportSearch(status);
        return;
    }
    findInEditor(true);
}

void MainWindow::replaceAllInEditor() {
    FernyEditor *editor = currentEditor();
    if (!editor || editor->send(SCI_GETREADONLY)) {
        return;
    }
    QByteArray replaced;
    qint32 count = 0;
    const SearchStatus status =
        m_core.replaceAll(editor->utf8Text(), m_findBar->pattern(), m_findBar->flags(),
                          m_findBar->replacement(), replaced, count);
    if (!reportSearch(status)) {
        return;
    }
    // One undoable change; the caret stays where it was.
    const quint32 line = editor->cursorLine();
    const quint32 column = editor->cursorColumn();
    editor->send(SCI_BEGINUNDOACTION);
    editor->send(SCI_TARGETWHOLEDOCUMENT);
    editor->send(SCI_REPLACETARGET, static_cast<uptr_t>(replaced.size()),
                 reinterpret_cast<sptr_t>(replaced.constData()));
    editor->send(SCI_ENDUNDOACTION);
    editor->gotoPosition(line, column);
    m_findBar->setMessage(tr("%n replaced", nullptr, count));
}

void MainWindow::showFindInFilesDialog() {
    QDialog dialog(this);
    dialog.setWindowTitle(tr("Find in Files"));
    auto *pattern = new QLineEdit(m_findBar->pattern(), &dialog);
    const QString path = m_core.documentPath(documentId());
    // Inside a project: its base folder and file patterns (H2.5a).
    const QString base = m_core.projectBase();
    auto *directory = new QLineEdit(!base.isEmpty()  ? base
                                    : path.isEmpty() ? QDir::currentPath()
                                                     : QFileInfo(path).absolutePath(),
                                    &dialog);
    auto *files = new QLineEdit(m_core.projectPatterns().join(QLatin1Char(' ')), &dialog);
    files->setPlaceholderText(tr("All files (e.g. *.c *.h)"));
    auto *matchCase = new QCheckBox(tr("Match case"), &dialog);
    auto *wholeWord = new QCheckBox(tr("Whole word"), &dialog);
    auto *regex = new QCheckBox(tr("Regular expression"), &dialog);
    const quint32 flags = m_findBar->flags();
    regex->setChecked((flags & 1u) != 0);
    matchCase->setChecked((flags & 2u) != 0);
    wholeWord->setChecked((flags & 4u) != 0);
    auto *form = new QFormLayout(&dialog);
    form->addRow(tr("Search for:"), pattern);
    form->addRow(tr("Directory:"), directory);
    form->addRow(tr("Files:"), files);
    form->addRow(matchCase);
    form->addRow(wholeWord);
    form->addRow(regex);
    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, &dialog);
    form->addRow(buttons);
    connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
    if (dialog.exec() != QDialog::Accepted) {
        return;
    }
    const quint32 chosen = (regex->isChecked() ? 1u : 0u) | (matchCase->isChecked() ? 2u : 0u) |
                           (wholeWord->isChecked() ? 4u : 0u);
    if (!startFindInFiles(directory->text(), pattern->text(), chosen, files->text())) {
        statusBar()->showMessage(tr("Invalid search pattern."));
    }
}

bool MainWindow::startFindInFiles(const QString &directory, const QString &pattern, quint32 flags,
                                  const QString &filePatterns) {
    const SearchStatus status = m_core.startFindInFiles(directory, pattern, flags, filePatterns);
    if (status != SearchStatus::Found) {
        return false;
    }
    m_results->clear();
    m_messageDock->show();
    m_messageWindow->showResults(tr("Searching…"));
    return true;
}

void MainWindow::showFindInFilesResults(int hits, bool truncated, bool cancelled, int searched,
                                        int skipped) {
    m_results->clear();
    QList<QTreeWidgetItem *> items;
    for (int i = 0; i < hits; ++i) {
        const CursorPos cursor = m_core.fileHitCursor(i);
        auto *item = new QTreeWidgetItem(
            {m_core.fileHitPath(i), QString::number(cursor.line), m_core.fileHitText(i).trimmed()});
        item->setData(0, Qt::UserRole, cursor.column);
        items.append(item);
    }
    m_results->addTopLevelItems(items);
    m_results->resizeColumnToContents(1);
    QString title =
        tr("%n result(s) in %1 file(s), %2 skipped", nullptr, hits).arg(searched).arg(skipped);
    if (truncated) {
        title += tr(" (limit reached)");
    }
    if (cancelled) {
        title += tr(" (cancelled)");
    }
    m_messageWindow->showResults(title);
}

void MainWindow::openFileHit(QTreeWidgetItem *item) {
    if (item) {
        openFile(item->text(0), QString(), item->text(1).toUInt(),
                 item->data(0, Qt::UserRole).toUInt());
    }
}

void MainWindow::gotoLine() {
    FernyEditor *editor = currentEditor();
    if (!editor) {
        return;
    }
    const int lines = static_cast<int>(editor->send(SCI_GETLINECOUNT));
    bool ok = false;
    const int line = QInputDialog::getInt(this, tr("Go to Line"), tr("Line:"),
                                          static_cast<int>(editor->cursorLine()), 1, lines, 1, &ok);
    if (ok) {
        jumpTo(editor->documentId(), static_cast<quint32>(line), 1);
    }
}

// --- Build (H2.4) ---------------------------------------------------------------

namespace {

// Output kept in the Compiler tab (maintainer decision, H2.4).
constexpr qint64 kMaxBuildOutputBytes = 4 * 1024 * 1024;
constexpr int kMaxBuildOutputLines = 10000;
// Item data of Compiler lines that point to a file.
constexpr int kFileRole = Qt::UserRole;
constexpr int kLineRole = Qt::UserRole + 1;
constexpr int kColumnRole = Qt::UserRole + 2;

QString quotedArgument(const QString &argument) {
    return argument.isEmpty() || argument.contains(QLatin1Char(' ')) ||
                   argument.contains(QLatin1Char('"'))
               ? QLatin1Char('"') +
                     QString(argument).replace(QLatin1Char('"'), QStringLiteral("\\\"")) +
                     QLatin1Char('"')
               : argument;
}

}  // namespace

bool MainWindow::buildRunning() const {
    return m_process && m_process->state() != QProcess::NotRunning;
}

QStringList MainWindow::compilerLines() const {
    QStringList lines;
    const QListWidget *list = m_messageWindow->compiler();
    for (int row = 0; row < list->count(); ++row) {
        lines.append(list->item(row)->text());
    }
    return lines;
}

void MainWindow::runBuild(const QString &command) {
    if (buildRunning()) {
        statusBar()->showMessage(tr("A command is already running; stop it first."), 3000);
        return;
    }
    FernyEditor *editor = currentEditor();
    if (!editor) {
        return;
    }
    // As Geany: the file is saved before it is built or run.
    const quint64 id = editor->documentId();
    if ((editor->isModified() || !m_core.documentInfo(id).has_path) && !save(editor)) {
        return;
    }
    QString program;
    QStringList arguments;
    QString dir;
    const BuildStatus status =
        m_core.prepareBuild(editor->filetypeId(), command, m_core.documentPath(id),
                            editor->cursorLine(), program, arguments, dir);
    switch (status) {
        case BuildStatus::Ready:
            break;
        case BuildStatus::Untrusted:
            m_banner->show();
            showMessage(
                tr("This folder is not trusted: this command would run the folder's "
                   "own content (a Makefile, a project command or a program in it)."));
            if (!m_smoke &&
                QMessageBox::question(this, tr("Folder Not Trusted"),
                                      tr("This command would run content from this folder, "
                                         "which could do anything. Trust the folder?"),
                                      QMessageBox::Yes | QMessageBox::Cancel,
                                      QMessageBox::Cancel) == QMessageBox::Yes) {
                trustFolder();
            }
            return;
        case BuildStatus::NoCommand:
            showMessage(
                tr("This filetype has no such command; set one in Build → Set Build "
                   "Commands…"));
            return;
        case BuildStatus::Invalid:
            showMessage(tr("The command is not valid; fix it in Build → Set Build Commands…"));
            return;
        case BuildStatus::NotFound:
        default:
            showMessage(tr("The program of this command was not found."));
            return;
    }
    QListWidget *list = m_messageWindow->compiler();
    list->clear();
    QStringList shown{quotedArgument(program)};
    for (const QString &argument : arguments) {
        shown.append(quotedArgument(argument));
    }
    // Every run shows exactly what is executed, and where (decision H2.4).
    list->addItem(QStringLiteral("$ ") +
                  m_core.neutraliseOutput(shown.join(QLatin1Char(' ')).toUtf8()));
    list->addItem(tr("in %1").arg(m_core.neutraliseOutput(dir.toUtf8())));
    m_messageDock->show();
    m_messageWindow->showCompiler();
    m_pendingOutput.clear();
    m_outputBytes = 0;
    m_outputTruncated = false;
    m_buildDir = dir;
    if (!m_process) {
        m_process = new QProcess(this);
        m_process->setProcessChannelMode(QProcess::MergedChannels);
#ifdef Q_OS_WIN
        // Build tools are console programs: they run without a window of their own.
        m_process->setCreateProcessArgumentsModifier([](QProcess::CreateProcessArguments *args) {
            constexpr quint32 createNoWindow = 0x0800'0000;  // CREATE_NO_WINDOW (winbase.h)
            args->flags |= createNoWindow;
        });
#endif
        connect(m_process, &QProcess::readyReadStandardOutput, this,
                [this]() { appendBuildOutput(m_process->readAllStandardOutput(), false); });
        connect(m_process, &QProcess::finished, this,
                [this](int exitCode, QProcess::ExitStatus exitStatus) {
                    appendBuildOutput(m_process->readAllStandardOutput(), true);
                    const bool crashed = exitStatus != QProcess::NormalExit;
                    m_messageWindow->compiler()->addItem(
                        crashed ? tr("Stopped.") : tr("Finished with exit code %1.").arg(exitCode));
                    emit buildFinished(crashed ? -1 : exitCode);
                });
        connect(m_process, &QProcess::errorOccurred, this, [this](QProcess::ProcessError error) {
            if (error == QProcess::FailedToStart) {
                m_messageWindow->compiler()->addItem(tr("The program could not be started."));
                emit buildFinished(-1);
            }
        });
        connect(m_messageWindow->compiler(), &QListWidget::itemActivated, this,
                [this](QListWidgetItem *item) {
                    openCompilerLine(m_messageWindow->compiler()->row(item));
                });
    }
    m_process->setWorkingDirectory(dir);
    // Program and arguments only: no shell ever sees the file name (T3).
    // The log keeps the program, never its arguments (they hold names).
    m_core.logWrite(QStringLiteral("build"), QStringLiteral("run ") + program);
    m_process->start(program, arguments, QIODevice::ReadOnly);
}

void MainWindow::appendBuildOutput(const QByteArray &bytes, bool flush) {
    m_outputBytes += bytes.size();
    if (m_outputTruncated) {
        return;  // keep draining the pipe, show nothing more
    }
    m_pendingOutput += bytes;
    // Whole lines only, so escape sequences and characters are not cut.
    const qsizetype end = flush ? m_pendingOutput.size() : m_pendingOutput.lastIndexOf('\n') + 1;
    if (end <= 0) {
        return;
    }
    const QString text = m_core.neutraliseOutput(m_pendingOutput.left(end));
    m_pendingOutput.remove(0, end);
    QListWidget *list = m_messageWindow->compiler();
    const QStringList lines = text.split(QLatin1Char('\n'));
    for (qsizetype i = 0; i < lines.size(); ++i) {
        if (i == lines.size() - 1 && lines.at(i).isEmpty()) {
            break;  // after the final newline
        }
        if (list->count() >= kMaxBuildOutputLines || m_outputBytes > kMaxBuildOutputBytes) {
            m_outputTruncated = true;
            list->addItem(tr("(output truncated: the program keeps running; use Build → Stop)"));
            m_pendingOutput.clear();
            return;
        }
        auto *item = new QListWidgetItem(lines.at(i), list);
        QString file;
        CursorPos cursor{0, 0};
        if (m_core.outputLocation(lines.at(i), file, cursor)) {
            item->setData(kFileRole, file);
            item->setData(kLineRole, cursor.line);
            item->setData(kColumnRole, cursor.column);
            item->setForeground(palette().link());
        }
    }
}

void MainWindow::openCompilerLine(int row) {
    const QListWidgetItem *item = m_messageWindow->compiler()->item(row);
    const QString file = item ? item->data(kFileRole).toString() : QString();
    if (file.isEmpty()) {
        return;
    }
    pushNavigation();
    openFile(QDir(m_buildDir).absoluteFilePath(file), QString(), item->data(kLineRole).toUInt(),
             item->data(kColumnRole).toUInt());
}

void MainWindow::setBuildCommands() {
    const FernyEditor *editor = currentEditor();
    const QString filetype = editor ? editor->filetypeId() : QString();
    if (filetype.isEmpty()) {
        showMessage(tr("Set a filetype for this document first (Document → Set Filetype)."));
        return;
    }
    QDialog dialog(this);
    dialog.setWindowTitle(tr("Build Commands: %1").arg(m_core.filetypeName(filetype)));
    dialog.resize(560, 0);
    auto *form = new QFormLayout(&dialog);
    form->addRow(new QLabel(tr("Placeholders: %f file, %e file without extension, %d folder, "
                               "%l line. There is no shell: use sh -c \"…\" explicitly."),
                            &dialog));
    const QStringList commands{QStringLiteral("compile"), QStringLiteral("build"),
                               QStringLiteral("lint"), QStringLiteral("execute")};
    const QStringList labels{tr("Compile:"), tr("Build:"), tr("Lint:"), tr("Execute:")};
    QList<QLineEdit *> fields;
    for (qsizetype i = 0; i < commands.size(); ++i) {
        auto *field = new QLineEdit(m_core.buildTemplate(filetype, commands.at(i)), &dialog);
        form->addRow(labels.at(i), field);
        fields.append(field);
    }
    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, &dialog);
    form->addRow(buttons);
    connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
    if (dialog.exec() != QDialog::Accepted) {
        return;
    }
    QStringList rejected;
    for (qsizetype i = 0; i < commands.size(); ++i) {
        const QString text = fields.at(i)->text().trimmed();
        if (text != m_core.buildTemplate(filetype, commands.at(i)) &&
            !m_core.setBuildTemplate(filetype, commands.at(i), text)) {
            rejected.append(labels.at(i));
        }
    }
    if (!rejected.isEmpty()) {
        showMessage(tr("Not valid and not changed: %1").arg(rejected.join(QStringLiteral(", "))));
    }
    if (!m_buildPath.isEmpty() && !m_core.saveBuildCommands(m_buildPath)) {
        showMessage(tr("The build commands could not be saved."));
    }
}

// --- Navigation, completion, palette (H2.3) ----------------------------------

void MainWindow::pushNavigation() {
    if (const FernyEditor *editor = currentEditor()) {
        m_back.append({editor->documentId(), editor->cursorLine(), editor->cursorColumn()});
        if (m_back.size() > 100) {
            m_back.removeFirst();
        }
        m_forward.clear();
    }
}

bool MainWindow::jumpTo(quint64 id, quint32 line, quint32 column) {
    for (int index = 0; index < m_tabs->count(); ++index) {
        if (editorAt(index)->documentId() == id) {
            pushNavigation();
            m_tabs->setCurrentIndex(index);
            editorAt(index)->gotoPosition(line, column);
            editorAt(index)->setFocus();
            return true;
        }
    }
    return false;
}

void MainWindow::navigate(bool back) {
    QList<NavPosition> &from = back ? m_back : m_forward;
    QList<NavPosition> &to = back ? m_forward : m_back;
    const FernyEditor *current = currentEditor();
    while (!from.isEmpty() && current) {
        const NavPosition target = from.takeLast();
        for (int index = 0; index < m_tabs->count(); ++index) {
            if (editorAt(index)->documentId() == target.documentId) {
                to.append({current->documentId(), current->cursorLine(), current->cursorColumn()});
                m_tabs->setCurrentIndex(index);
                editorAt(index)->gotoPosition(target.line, target.column);
                editorAt(index)->setFocus();
                return;
            }
        }
        // That document was closed: try the next position.
    }
    statusBar()->showMessage(back ? tr("No earlier position.") : tr("No later position."), 3000);
}

int MainWindow::pickItem(const QString &title, const QStringList &items) {
    if (!m_smokeInput.isEmpty()) {
        for (int i = 0; i < items.size(); ++i) {
            if (items.at(i).contains(m_smokeInput, Qt::CaseInsensitive)) {
                return i;
            }
        }
        return -1;
    }
    QDialog dialog(this);
    dialog.setWindowTitle(title);
    dialog.resize(480, 360);
    auto *filter = new QLineEdit(&dialog);
    filter->setPlaceholderText(tr("Type to filter"));
    auto *list = new QListWidget(&dialog);
    list->addItems(items);
    list->setCurrentRow(0);
    auto *layout = new QVBoxLayout(&dialog);
    layout->addWidget(filter);
    layout->addWidget(list, 1);
    connect(filter, &QLineEdit::textChanged, &dialog, [list](const QString &text) {
        int first = -1;
        for (int i = 0; i < list->count(); ++i) {
            const bool shown = list->item(i)->text().contains(text, Qt::CaseInsensitive);
            list->item(i)->setHidden(!shown);
            if (shown && first < 0) {
                first = i;
            }
        }
        list->setCurrentRow(first);
    });
    connect(filter, &QLineEdit::returnPressed, &dialog, &QDialog::accept);
    connect(list, &QListWidget::itemActivated, &dialog, &QDialog::accept);
    if (dialog.exec() != QDialog::Accepted || !list->currentItem() ||
        list->currentItem()->isHidden()) {
        return -1;
    }
    return list->currentRow();
}

void MainWindow::gotoSymbol() {
    FernyEditor *editor = currentEditor();
    const int count = editor ? m_core.extractSymbols(editor->filetypeId(), editor->utf8Text()) : -1;
    QStringList items;
    QList<quint32> lines;
    for (int i = 0; i < count; ++i) {
        items.append(tr("%1 (line %2)").arg(m_core.symbolName(i)).arg(m_core.symbolLine(i)));
        lines.append(m_core.symbolLine(i));
    }
    if (items.isEmpty()) {
        statusBar()->showMessage(tr("No symbols in this document."), 3000);
        return;
    }
    const int chosen = pickItem(tr("Go to Symbol"), items);
    if (chosen >= 0) {
        jumpTo(editor->documentId(), lines.at(chosen), 0);
    }
}

void MainWindow::gotoDefinition() {
    FernyEditor *editor = currentEditor();
    if (editor) {
        // The language server first; tree-sitter if there is none or it
        // has no answer within 3 s.
        m_definitionToken = m_core.lspRequest(editor->documentId(), 4, editor->utf8Text(),
                                              static_cast<int>(editor->send(SCI_GETCURRENTPOS)));
        if (m_definitionToken != 0) {
            const qint64 token = m_definitionToken;
            QTimer::singleShot(3000, this, [this, token]() {
                if (m_definitionToken == token) {
                    m_definitionToken = 0;
                    gotoDefinitionLocal();
                }
            });
            return;
        }
    }
    gotoDefinitionLocal();
}

void MainWindow::gotoDefinitionLocal() {
    FernyEditor *current = currentEditor();
    const QString word = current ? current->wordAtCursor(false) : QString();
    if (word.isEmpty()) {
        return;
    }
    // The current document first, then the other open ones, in tab order.
    QList<FernyEditor *> editors{current};
    for (int index = 0; index < m_tabs->count(); ++index) {
        if (editorAt(index) != current) {
            editors.append(editorAt(index));
        }
    }
    for (FernyEditor *editor : editors) {
        const int count = m_core.extractSymbols(editor->filetypeId(), editor->utf8Text());
        for (int i = 0; i < count; ++i) {
            if (m_core.symbolName(i) == word) {
                jumpTo(editor->documentId(), m_core.symbolLine(i), 0);
                refreshSymbols();
                return;
            }
        }
    }
    refreshSymbols();
    statusBar()->showMessage(tr("No definition of %1 found.").arg(m_core.displayName(word)), 3000);
}

void MainWindow::completeWord() {
    FernyEditor *editor = currentEditor();
    m_pluginCompletions.clear();
    if (editor && m_completionToken == 0 &&
        m_core.pluginHasProvider(QStringLiteral("completion"), editor->filetypeId())) {
        m_pluginCompletions = askPluginProvider(QStringLiteral("completion"), editor,
                                                static_cast<int>(editor->send(SCI_GETCURRENTPOS)),
                                                editor->wordAtCursor(true));
    }
    if (editor && m_completionToken == 0) {
        // The language server first (H3.1b); the document's words if there
        // is none or no answer within 3 s.
        m_completionToken = m_core.lspRequest(editor->documentId(), 1, editor->utf8Text(),
                                              static_cast<int>(editor->send(SCI_GETCURRENTPOS)));
        if (m_completionToken != 0) {
            m_requestEditor = editor;
            m_requestPosition = static_cast<int>(editor->send(SCI_GETCURRENTPOS));
            m_completionPrefix = editor->wordAtCursor(true);
            const qint64 token = m_completionToken;
            QTimer::singleShot(3000, this, [this, token]() {
                if (m_completionToken == token) {
                    m_completionToken = 0;
                    completeWordFromDocument();
                }
            });
            return;
        }
    }
    completeWordFromDocument();
}

void MainWindow::completeWordFromDocument() {
    FernyEditor *editor = currentEditor();
    const QString prefix = editor ? editor->wordAtCursor(true) : QString();
    QStringList words = m_pluginCompletions;
    if (!prefix.isEmpty()) {
        for (const QString &word : m_core.completeWords(editor->utf8Text(), prefix)) {
            if (!words.contains(word)) {
                words.append(word);
            }
        }
    }
    if (words.isEmpty()) {
        statusBar()->showMessage(tr("No completions."), 3000);
        return;
    }
    editor->showCompletions(words, static_cast<int>(prefix.toUtf8().size()));
}

void MainWindow::showCommandPalette() {
    QStringList items;
    QStringList ids;
    for (const QString &id : m_core.commandIds()) {
        if (id == QStringLiteral("app.command_palette")) {
            continue;
        }
        QString title = translatedTitle(m_core.commandTitle(id));
        title.remove(QLatin1Char('&'));
        const QStringList keys = m_core.commandShortcuts(id);
        if (!keys.isEmpty()) {
            title += QStringLiteral("    ") +
                     QKeySequence::fromString(keys.first(), QKeySequence::PortableText)
                         .toString(QKeySequence::NativeText);
        }
        items.append(title);
        ids.append(id);
    }
    const int chosen = pickItem(tr("Command Palette"), items);
    if (chosen >= 0 && !m_core.invokeCommand(ids.at(chosen))) {
        statusBar()->showMessage(tr("This command is not available."));
    }
}

void MainWindow::setColorScheme(const QString &id) {
    m_scheme = m_core.colorSchemeName(id).isEmpty() ? QStringLiteral("default") : id;
    m_core.setSetting(QStringLiteral("editor.color_scheme"), m_scheme);
    if (m_schemeMenu) {
        const auto actions = m_schemeMenu->actions();
        for (QAction *action : actions) {
            action->setChecked(action->data().toString() == m_scheme);
        }
    }
    for (int index = 0; index < m_tabs->count(); ++index) {
        applyHighlighting(editorAt(index));
    }
    applyHighlighting(m_splitEditor);
}

void MainWindow::setFiletype(FernyEditor *editor, const QString &id) {
    if (editor) {
        editor->setFiletypeId(id);
        applyHighlighting(editor);
    }
}

void MainWindow::detectFiletype(FernyEditor *editor, const QByteArray &utf8) {
    // The first two lines (bounded) carry the shebang and Geany's
    // `-*- name -*-` modeline.
    const QByteArray head = utf8.left(512);
    const qsizetype first = head.indexOf('\n');
    const qsizetype second = first < 0 ? -1 : head.indexOf('\n', first + 1);
    const QString header = QString::fromUtf8(second < 0 ? head : head.left(second));
    const QString path = m_core.documentPath(editor->documentId());
    editor->setFiletypeId(m_core.detectFiletype(path, header));
    applyHighlighting(editor);
}

void MainWindow::applyHighlighting(FernyEditor *editor) {
    if (!editor) {
        return;
    }
    const bool large = m_core.documentInfo(editor->documentId()).large;
    const QString filetype = large ? QString() : editor->filetypeId();
    auto look = [](const StyleSpec &spec) {
        StyleLook style;
        style.style = static_cast<int>(spec.style);
        style.fore = spec.fore;
        style.back = spec.back;
        style.bold = spec.bold;
        style.italic = spec.italic;
        style.fillEol = spec.fill_eol;
        return style;
    };
    auto named = [&](const char *name) -> std::optional<StyleLook> {
        const StyleSpec spec = m_core.namedStyle(m_scheme, QString::fromLatin1(name));
        return spec.found ? std::optional<StyleLook>(look(spec)) : std::nullopt;
    };

    editor->setFontSettings(m_core.settingValue(QStringLiteral("editor.font_family")),
                            m_core.settingValue(QStringLiteral("editor.font_size")).toInt());
    Highlighting h;
    h.lexer = m_core.filetypeLexer(filetype).toUtf8();
    const QStringList keywords = m_core.filetypeKeywords(filetype);
    for (const QString &words : keywords) {
        h.keywords.append(words.toUtf8());
    }
    const QStringList properties = m_core.filetypeProperties(filetype);
    for (qsizetype i = 0; i + 1 < properties.size(); i += 2) {
        h.properties.append({properties.at(i).toUtf8(), properties.at(i + 1).toUtf8()});
    }
    const int count = m_core.prepareStyles(filetype, m_scheme);
    h.defaults = look(m_core.preparedStyle(-1));
    for (int i = 0; i < count; ++i) {
        h.styles.append(look(m_core.preparedStyle(i)));
    }
    h.caret = named("caret");
    h.selection = named("selection");
    h.currentLine = named("current_line");
    h.lineNumbers = named("margin_line_number");
    h.braceGood = named("brace_good");
    h.braceBad = named("brace_bad");
    h.indentGuide = named("indent_guide");
    h.whiteSpace = named("white_space");
    editor->applyHighlighting(h);
    // Filetype defaults, then the project's .editorconfig (H1.3c).
    // .editorconfig, then the filetype, then the user's settings.
    const IndentSpec indent = m_core.filetypeIndent(filetype);
    const bool numbers =
        m_core.settingValue(QStringLiteral("editor.line_numbers")) == QStringLiteral("true");
    editor->send(SCI_SETMARGINWIDTHN, 0, numbers ? 48 : 0);
    const int edge = m_core.settingValue(QStringLiteral("editor.long_line_marker")).toInt();
    editor->send(SCI_SETEDGEMODE, edge > 0 ? EDGE_LINE : EDGE_NONE);
    editor->send(SCI_SETEDGECOLUMN, static_cast<uptr_t>(edge));
    const EditorConfigSpec config = m_core.editorConfig(editor->documentId());
    const int settingWidth = m_core.settingValue(QStringLiteral("editor.indent_width")).toInt();
    const bool settingTabs =
        m_core.settingValue(QStringLiteral("editor.indent_use_tabs")) == QStringLiteral("true");
    // .editorconfig, then the project (H2.5a), then the filetype, then
    // the user's settings.
    const IndentSpec project = m_core.projectIndent(m_core.documentPath(editor->documentId()));
    // Detection (M2) comes after the files and before the user's settings.
    const bool detect =
        m_core.settingValue(QStringLiteral("editor.detect_indent")) == QStringLiteral("true");
    const IndentSpec found = detect ? m_core.detectIndent(editor->utf8Text()) : IndentSpec{-1, -1};
    const int width = config.indent_size > 0 ? config.indent_size
                      : project.width > 0    ? project.width
                      : indent.width > 0     ? indent.width
                      : found.width > 0      ? found.width
                                             : settingWidth;
    const bool tabs = config.indent_style >= 0 ? config.indent_style == 1
                      : project.use_tabs >= 0  ? project.use_tabs == 1
                      : indent.use_tabs >= 0   ? indent.use_tabs == 1
                      : found.use_tabs >= 0    ? found.use_tabs == 1
                                               : settingTabs;
    editor->setIndentation(width, tabs);
    // Tab width: .editorconfig, then the setting (0 = the indentation width).
    const int settingTabWidth = m_core.settingValue(QStringLiteral("editor.tab_width")).toInt();
    const int tabWidth = config.tab_width > 0 ? config.tab_width : settingTabWidth;
    if (tabWidth > 0) {
        editor->send(SCI_SETTABWIDTH, static_cast<uptr_t>(tabWidth));
    }
}

void MainWindow::rebuildRecentMenu() {
    m_recentMenu->clear();
    const QStringList recent = m_core.recentFiles();
    for (const QString &path : recent) {
        QAction *action = m_recentMenu->addAction(QFileInfo(path).fileName());
        action->setToolTip(path);
        action->setStatusTip(path);
        connect(action, &QAction::triggered, this, [this, path]() { openFile(path); });
    }
    m_recentMenu->setEnabled(!recent.isEmpty());
}

void MainWindow::runUiCommand(const QString &commandId) {
    FernyEditor *editor = currentEditor();
    if (commandId == QStringLiteral("file.new")) {
        newDocument();
    } else if (commandId == QStringLiteral("file.open")) {
        openWithDialog();
    } else if (commandId == QStringLiteral("file.save")) {
        save(editor);
    } else if (commandId == QStringLiteral("file.save_all")) {
        saveAll();
    } else if (commandId == QStringLiteral("file.save_as")) {
        saveAs(editor);
    } else if (commandId == QStringLiteral("view.split_side")) {
        splitView(Qt::Horizontal);
    } else if (commandId == QStringLiteral("view.split_stacked")) {
        splitView(Qt::Vertical);
    } else if (commandId == QStringLiteral("view.split_current")) {
        showInSplit(editor);
    } else if (commandId == QStringLiteral("view.unsplit")) {
        delete m_splitEditor;
        m_splitEditor = nullptr;
    } else if (commandId == QStringLiteral("file.print") && editor) {
        QPrinter printer(QPrinter::HighResolution);
        printer.setDocName(documentTitle(editor));
        QPrintDialog dialog(&printer, this);
        if (dialog.exec() == QDialog::Accepted) {
            printDocument(editor, &printer, documentTitle(editor));
        }
    } else if (commandId.startsWith(QStringLiteral("file.export_")) && editor) {
        const QString kind = commandId.mid(12);
        const QString filter = kind == QStringLiteral("pdf")    ? tr("PDF files (*.pdf)")
                               : kind == QStringLiteral("html") ? tr("HTML files (*.html *.htm)")
                                                                : tr("LaTeX files (*.tex)");
        const QString suffix =
            kind == QStringLiteral("latex") ? QStringLiteral(".tex") : QLatin1Char('.') + kind;
        const QString path = m_core.documentPath(editor->documentId());
        const QString proposed = path.isEmpty() ? documentTitle(editor) + suffix
                                                : QFileInfo(path).path() + QLatin1Char('/') +
                                                      QFileInfo(path).completeBaseName() + suffix;
        // The dialog asks before replacing a file (T9).
        const QString target = QFileDialog::getSaveFileName(this, tr("Export"), proposed, filter);
        if (!target.isEmpty() && !exportCurrent(kind, target)) {
            showMessage(tr("Could not export to %1.")
                            .arg(m_core.displayName(QFileInfo(target).fileName())));
        }
    } else if (commandId == QStringLiteral("file.reload")) {
        reload(editor, QString());
    } else if (commandId == QStringLiteral("file.reload_with_encoding")) {
        reloadWithEncoding();
    } else if (commandId == QStringLiteral("file.close")) {
        closeTab(m_tabs->currentIndex());
    } else if (commandId == QStringLiteral("file.close_others")) {
        closeAllTabs(editor);
    } else if (commandId == QStringLiteral("file.close_all")) {
        closeAllTabs(nullptr);
    } else if (commandId == QStringLiteral("file.reopen_closed")) {
        reopenClosedTab();
    } else if (commandId == QStringLiteral("edit.undo")) {
        editor->send(SCI_UNDO);
    } else if (commandId == QStringLiteral("edit.redo")) {
        editor->send(SCI_REDO);
    } else if (commandId == QStringLiteral("edit.cut")) {
        editor->send(SCI_CUT);
    } else if (commandId == QStringLiteral("edit.copy")) {
        editor->send(SCI_COPY);
    } else if (commandId == QStringLiteral("edit.paste")) {
        editor->send(SCI_PASTE);
    } else if (commandId == QStringLiteral("edit.select_all")) {
        editor->send(SCI_SELECTALL);
    } else if (commandId == QStringLiteral("edit.duplicate_line")) {
        editor->send(SCI_SELECTIONDUPLICATE);
    } else if (commandId == QStringLiteral("edit.delete_line")) {
        editor->send(SCI_LINEDELETE);
    } else if (commandId == QStringLiteral("edit.move_line_up")) {
        editor->send(SCI_MOVESELECTEDLINESUP);
    } else if (commandId == QStringLiteral("edit.move_line_down")) {
        editor->send(SCI_MOVESELECTEDLINESDOWN);
    } else if (commandId == QStringLiteral("edit.toggle_comment")) {
        toggleComment(editor);
    } else if (commandId == QStringLiteral("edit.toggle_marker")) {
        editor->toggleMarker();
    } else if (commandId == QStringLiteral("edit.next_marker")) {
        editor->gotoMarker(true);
    } else if (commandId == QStringLiteral("edit.previous_marker")) {
        editor->gotoMarker(false);
    } else if (commandId == QStringLiteral("view.show_invisible")) {
        setShowInvisible(!m_showInvisible);
    } else if (commandId == QStringLiteral("view.auto_close")) {
        setAutoClose(!m_autoClose);
    } else if (commandId == QStringLiteral("view.word_wrap")) {
        toggleDisplaySetting(QStringLiteral("editor.word_wrap"), commandId);
    } else if (commandId == QStringLiteral("view.indent_guides")) {
        toggleDisplaySetting(QStringLiteral("editor.indent_guides"), commandId);
    } else if (commandId == QStringLiteral("view.zoom_in")) {
        editor->zoomIn();
    } else if (commandId == QStringLiteral("view.zoom_out")) {
        editor->zoomOut();
    } else if (commandId == QStringLiteral("view.zoom_reset")) {
        editor->resetZoom();
    } else if (commandId == QStringLiteral("search.goto_symbol")) {
        gotoSymbol();
    } else if (commandId == QStringLiteral("search.rename_symbol")) {
        renameSymbol();
    } else if (commandId == QStringLiteral("document.format")) {
        formatDocument();
    } else if (commandId == QStringLiteral("edit.mark_occurrences") && editor) {
        statusBar()->showMessage(tr("%n occurrence(s) marked.", nullptr, editor->markOccurrences()),
                                 5000);
    } else if (commandId == QStringLiteral("search.goto_workspace_symbol") && editor) {
        QString query = m_smokeInput;
        if (!m_smoke) {
            bool ok = false;
            query = QInputDialog::getText(this, tr("Go to Workspace Symbol"), tr("Symbol:"),
                                          QLineEdit::Normal, editor->wordAtCursor(false), &ok);
            if (!ok) {
                return;
            }
        }
        m_workspaceSymbolsToken =
            m_core.lspRequestWith(editor->documentId(), 9, editor->utf8Text(), 0, query.trimmed());
        if (m_workspaceSymbolsToken == 0) {
            statusBar()->showMessage(tr("No language server for this document."), 3000);
        }
    } else if (commandId == QStringLiteral("search.find_usages")) {
        findUsages();
    } else if (commandId == QStringLiteral("search.goto_definition")) {
        gotoDefinition();
    } else if (commandId == QStringLiteral("view.navigate_back")) {
        navigate(true);
    } else if (commandId == QStringLiteral("view.navigate_forward")) {
        navigate(false);
    } else if (commandId == QStringLiteral("edit.complete_word")) {
        completeWord();
    } else if (commandId == QStringLiteral("app.command_palette")) {
        showCommandPalette();
    } else if (commandId.startsWith(QStringLiteral("build.")) &&
               commandId != QStringLiteral("build.stop") &&
               commandId != QStringLiteral("build.set_commands")) {
        runBuild(commandId.mid(6));
    } else if (commandId == QStringLiteral("build.stop")) {
        if (buildRunning()) {
            m_process->kill();
        }
    } else if (commandId == QStringLiteral("build.set_commands")) {
        setBuildCommands();
    } else if (commandId == QStringLiteral("file.new_from_template")) {
        newFromTemplate();
    } else if (commandId == QStringLiteral("project.new")) {
        newProject();
    } else if (commandId == QStringLiteral("project.open")) {
        chooseProject();
    } else if (commandId == QStringLiteral("project.close")) {
        closeProject();
    } else if (commandId == QStringLiteral("project.edit")) {
        if (!m_core.projectRoot().isEmpty()) {
            openFile(QDir(m_core.projectRoot()).filePath(QStringLiteral(".ferny/project.toml")));
        }
    } else if (commandId == QStringLiteral("project.import_geany")) {
        importGeanyProject();
    } else if (commandId == QStringLiteral("document.trust_folder")) {
        trustFolder();
    } else if (commandId == QStringLiteral("file.import_geany")) {
        importGeany();
    } else if (commandId == QStringLiteral("view.sidebar")) {
        m_sidebarDock->setVisible(m_sidebarDock->isHidden());
    } else if (commandId == QStringLiteral("view.message_window")) {
        m_messageDock->setVisible(m_messageDock->isHidden());
    } else if (commandId == QStringLiteral("document.git_blame") && editor) {
        const QString path = m_core.documentPath(editor->documentId());
        if (path.isEmpty()) {
            statusBar()->showMessage(tr("Save the document first."), 3000);
        } else if (!m_core.gitBlame(path)) {
            statusBar()->showMessage(tr("Git needs a trusted folder and the git program."), 5000);
        }
    } else if (commandId == QStringLiteral("view.spell_check")) {
        m_spellCheck = !m_spellCheck;
        if (m_spellCheck) {
            loadSpeller();
        }
        if (QAction *action = findChild<QAction *>(QStringLiteral("view.spell_check"))) {
            action->setChecked(m_spellCheck);
        }
        refreshSpelling();
    } else if (commandId == QStringLiteral("view.markdown_preview")) {
        m_previewDock->setVisible(m_previewDock->isHidden());
        refreshPreview();
    } else if (commandId == QStringLiteral("app.preferences")) {
        showPreferences();
    } else if (commandId == QStringLiteral("app.keybindings")) {
        showShortcuts();
    } else if (commandId == QStringLiteral("search.find")) {
        openFindBar(false);
    } else if (commandId == QStringLiteral("search.replace")) {
        openFindBar(true);
    } else if (commandId == QStringLiteral("search.find_next")) {
        findInEditor(true);
    } else if (commandId == QStringLiteral("search.find_previous")) {
        findInEditor(false);
    } else if (commandId == QStringLiteral("search.replace_all")) {
        replaceAllInEditor();
    } else if (commandId == QStringLiteral("search.find_in_files")) {
        showFindInFilesDialog();
    } else if (commandId == QStringLiteral("search.goto_line")) {
        gotoLine();
    } else if (commandId == QStringLiteral("view.next_tab")) {
        cycleTab(1);
    } else if (commandId == QStringLiteral("view.previous_tab")) {
        cycleTab(-1);
    } else if (commandId == QStringLiteral("document.set_encoding")) {
        setEncoding();
    } else if (commandId == QStringLiteral("app.quit")) {
        close();
    } else if (commandId == QStringLiteral("help.about")) {
        showAbout();
    } else {
        statusBar()->showMessage(tr("This command is not available."));
    }
}

// --- Tabs --------------------------------------------------------------------

FernyEditor *MainWindow::currentEditor() const {
    return qobject_cast<FernyEditor *>(m_tabs->currentWidget());
}

int MainWindow::tabCount() const {
    return m_tabs->count();
}

FernyEditor *MainWindow::editorAt(int index) const {
    return qobject_cast<FernyEditor *>(m_tabs->widget(index));
}

quint64 MainWindow::documentId() const {
    const FernyEditor *editor = currentEditor();
    return editor ? editor->documentId() : 0;
}

FernyEditor *MainWindow::addTab(quint64 documentId) {
    auto *editor = new FernyEditor(m_tabs);
    editor->setDocumentId(documentId);
    connect(editor, &ScintillaEditBase::uriDropped, this, [this](const QString &uri) {
        const QUrl url(uri);
        if (url.isLocalFile()) {
            openFile(url.toLocalFile());
        }
    });
    connect(editor, &ScintillaEditBase::savePointChanged, this, [this, editor](bool) {
        updateTabLabel(editor);
        updateTitle();
        updateStatus();
        if (editor == currentEditor()) {
            refreshToolbarActions();
        }
    });
    connect(editor, &ScintillaEditBase::notifyChange, this, [this, editor]() {
        if (editor == currentEditor()) {
            refreshToolbarActions();
            m_symbolsTimer->start();
            m_previewTimer->start();
            m_spellTimer->start();
        }
    });
    connect(editor, &ScintillaEditBase::updateUi, this, [this, editor](Scintilla::Update) {
        if (editor == currentEditor()) {
            updateCursorStatus();
            refreshToolbarActions();
        }
    });
    applyEditingOptions(editor);
    refreshToolbarActions();
    editor->setTabHandler([this, editor]() { return expandSnippet(editor); });
    // Diagnostic squiggles: errors, warnings, information/hints (BGR).
    const int colours[3] = {0x2020E0, 0x0090F0, 0xD08020};
    for (int i = 0; i < 3; ++i) {
        editor->send(SCI_INDICSETSTYLE, static_cast<uptr_t>(8 + i), INDIC_SQUIGGLE);
        editor->send(SCI_INDICSETFORE, static_cast<uptr_t>(8 + i), colours[i]);
    }
    editor->send(SCI_SETMOUSEDWELLTIME, 500);
    connect(editor, &ScintillaEditBase::dwellStart, this,
            [this, editor](int x, int y) { showDiagnosticAt(editor, x, y); });
    connect(editor, &ScintillaEditBase::dwellEnd, this, []() { QToolTip::hideText(); });
    // Misspelled words (H4.2c) and Ferny's own context menu with suggestions.
    editor->send(SCI_INDICSETSTYLE, kSpellIndicator, INDIC_SQUIGGLE);
    editor->send(SCI_INDICSETFORE, kSpellIndicator, 0x3030D0);
    editor->send(SCI_USEPOPUP, SC_POPUP_NEVER);
    editor->setContextMenuPolicy(Qt::CustomContextMenu);
    connect(editor, &QWidget::customContextMenuRequested, this,
            [this, editor](const QPoint &pos) { showEditorMenu(editor, pos); });
    // Mark word (H4.2a): a double-click marks the word everywhere, the next
    // click clears the marks.
    connect(editor, &ScintillaEditBase::buttonPressed, editor, &FernyEditor::clearMarks);
    connect(editor, &ScintillaEditBase::doubleClick, this, [this, editor]() {
        statusBar()->showMessage(tr("%n occurrence(s) marked.", nullptr, editor->markOccurrences()),
                                 5000);
    });
    // Signature help as Geany's calltips: on ( and , ; ) closes it (H3.1b).
    connect(editor, &ScintillaEditBase::charAdded, this, [this, editor](int ch) {
        if (ch == '(' || ch == ',') {
            requestSignature(editor);
        } else if (ch == ')') {
            editor->send(SCI_CALLTIPCANCEL);
        }
    });
    const int index = m_tabs->addTab(editor, QString());
    applyDocument(editor, QByteArray());
    m_tabs->setCurrentIndex(index);
    return editor;
}

bool MainWindow::isPristine(const FernyEditor *editor) const {
    return editor && !m_core.documentInfo(editor->documentId()).has_path && !editor->isModified() &&
           editor->length() == 0;
}

bool MainWindow::closeTab(int index) {
    FernyEditor *editor = editorAt(index);
    if (!editor) {
        return false;
    }
    m_tabs->setCurrentIndex(index);
    if (!maybeSaveChanges(editor)) {
        return false;
    }
    const QString path = m_core.documentPath(editor->documentId());
    if (!path.isEmpty()) {
        m_core.pushClosedTab(path, CursorPos{editor->cursorLine(), editor->cursorColumn()});
    }
    emitPluginEvent(QStringLiteral("document.closed"), editor);
    const bool shownInSplit = m_splitEditor && m_splitEditor->documentId() == editor->documentId();
    m_core.lspClose(editor->documentId());
    m_diagnostics.remove(editor->documentId());
    m_core.closeDocument(editor->documentId());
    m_tabs->removeTab(index);
    editor->deleteLater();
    if (m_tabs->count() == 0) {
        addTab(m_core.newDocument());
    }
    // The second view leaves a closed document (H4.1b).
    if (shownInSplit) {
        showInSplit(currentEditor());
    }
    return true;
}

void MainWindow::closeAllTabs(FernyEditor *keep) {
    for (int index = m_tabs->count() - 1; index >= 0; --index) {
        if (editorAt(index) != keep && !closeTab(index)) {
            return;  // cancelled by the user
        }
    }
}

void MainWindow::reopenClosedTab() {
    QString path;
    CursorPos cursor{0, 0};
    if (m_core.popClosedTab(path, cursor)) {
        openFile(path, QString(), cursor.line, cursor.column);
    }
}

void MainWindow::cycleTab(int step) {
    const int count = m_tabs->count();
    if (count > 1) {
        m_tabs->setCurrentIndex((m_tabs->currentIndex() + step + count) % count);
    }
}

bool MainWindow::eventFilter(QObject *watched, QEvent *event) {
    // Middle click on a tab closes it, as in Geany.
    if (watched == m_tabs->tabBar() && event->type() == QEvent::MouseButtonRelease) {
        auto *mouse = static_cast<QMouseEvent *>(event);
        if (mouse->button() == Qt::MiddleButton) {
            const int index = m_tabs->tabBar()->tabAt(mouse->position().toPoint());
            if (index >= 0) {
                closeTab(index);
                return true;
            }
        }
    }
    return QMainWindow::eventFilter(watched, event);
}

// --- Opening -------------------------------------------------------------------

void MainWindow::newDocument() {
    addTab(m_core.newDocument());
}

void MainWindow::openWithDialog() {
    const QStringList paths = QFileDialog::getOpenFileNames(this, tr("Open File"));
    for (const QString &path : paths) {
        openFile(path);
    }
}

LoadStatus MainWindow::openFile(const QString &path, const QString &encoding, quint32 line,
                                quint32 column) {
    const quint64 existing = m_core.findDocument(path);
    if (existing != 0) {
        for (int index = 0; index < m_tabs->count(); ++index) {
            if (editorAt(index)->documentId() == existing) {
                m_tabs->setCurrentIndex(index);
                if (line > 0) {
                    editorAt(index)->gotoPosition(line, column);
                }
                return LoadStatus::Ok;
            }
        }
    }
    std::uint64_t id = 0;
    QByteArray contents;
    const LoadStatus status = m_core.openDocument(path, encoding, id, contents);
    if (status != LoadStatus::Ok) {
        showMessage(
            tr("%1: %2").arg(m_core.displayName(QFileInfo(path).fileName()), loadMessage(status)));
        return status;
    }
    FernyEditor *editor = currentEditor();
    if (isPristine(editor)) {
        m_core.closeDocument(editor->documentId());
        editor->setDocumentId(id);
    } else {
        editor = addTab(id);
    }
    applyDocument(editor, contents);
    if (line > 0) {
        editor->gotoPosition(line, column);
    }
    m_core.addRecentFile(m_core.documentPath(id));
    emitPluginEvent(QStringLiteral("document.opened"), editor);
    return status;
}

int MainWindow::openPending() {
    const QStringList paths = m_core.pendingPaths();
    int failures = 0;
    for (int i = 0; i < paths.size(); ++i) {
        const CursorPos cursor = m_core.pendingCursor(i);
        if (openFile(paths.at(i), QString(), cursor.line, cursor.column) != LoadStatus::Ok) {
            ++failures;
        }
    }
    m_core.clearPending();
    return failures;
}

bool MainWindow::loadSession() {
    setColorScheme(m_scheme);
    if (m_sessionPath.isEmpty() || !m_core.loadSession(m_sessionPath)) {
        return false;
    }
    const QByteArray geometry = m_core.sessionGeometry();
    if (!geometry.isEmpty()) {
        restoreGeometry(geometry);
    }
    const QByteArray layout = m_core.sessionLayout();
    if (!layout.isEmpty()) {
        restoreState(layout);
    }
    m_messageWindow->setScribble(m_core.sessionScribble());
    return true;
}

void MainWindow::restoreSessionFiles() {
    m_core.queueSessionFiles();
    openPendingWithActive(m_core.sessionActiveIndex());
}

void MainWindow::openPendingWithActive(int active) {
    const QStringList paths = m_core.pendingPaths();
    openPending();
    // Missing files were skipped, so look the active one up by path.
    if (active >= 0 && active < paths.size()) {
        const quint64 id = m_core.findDocument(paths.at(active));
        for (int index = 0; index < m_tabs->count(); ++index) {
            if (id != 0 && editorAt(index)->documentId() == id) {
                m_tabs->setCurrentIndex(index);
            }
        }
    }
}

int MainWindow::collectSessionFiles() {
    m_core.clearSessionFiles();
    int active = 0;
    int saved = 0;
    for (int index = 0; index < m_tabs->count(); ++index) {
        const FernyEditor *editor = editorAt(index);
        const QString path = m_core.documentPath(editor->documentId());
        if (path.isEmpty()) {
            continue;  // untitled documents are not part of the session
        }
        if (index == m_tabs->currentIndex()) {
            active = saved;
        }
        m_core.addSessionFile(path, CursorPos{editor->cursorLine(), editor->cursorColumn()});
        ++saved;
    }
    return active;
}

bool MainWindow::saveSession() {
    const int active = collectSessionFiles();
    // A project keeps its open files in its own session (H2.5a).
    const QString projectSession =
        m_configDir.isEmpty() ? QString() : m_core.projectSessionFile(m_configDir);
    if (!projectSession.isEmpty()) {
        m_core.saveProjectSession(projectSession, active);
    }
    if (m_sessionPath.isEmpty()) {
        return false;
    }
    return m_core.saveSession(m_sessionPath, active, saveGeometry(), saveState(),
                              m_messageWindow->scribble(), m_core.projectRoot());
}

// --- Language servers (H3.1a) ------------------------------------------------------

void MainWindow::lspSync(FernyEditor *editor) {
    const quint64 id = editor->documentId();
    const QString path = m_core.documentPath(id);
    if (path.isEmpty() || editor->filetypeId().isEmpty()) {
        return;
    }
    m_core.lspClose(id);  // a reload or a new filetype starts again
    const int status = m_core.lspOpen(id, path, editor->filetypeId(), editor->utf8Text());
    m_lspStatus.insert(id, status);
    // A missing or failing server is reported once per server.
    const QString server = m_core.lspServerName(editor->filetypeId());
    if (status >= 4 && !m_lspReported.contains(server)) {
        m_lspReported.insert(server);
        showMessage(
            status == 4
                ? tr("Language server %1 not found; install it or set it in "
                     "lsp.toml.")
                      .arg(m_core.displayName(server))
                : tr("Language server %1 could not be started.").arg(m_core.displayName(server)));
    }
}

int MainWindow::lspStatus() const {
    const FernyEditor *editor = currentEditor();
    return editor ? m_lspStatus.value(editor->documentId(), -1) : -1;
}

QStringList MainWindow::diagnosticEntries() const {
    QStringList entries;
    const FernyEditor *editor = currentEditor();
    if (!editor) {
        return entries;
    }
    for (const DiagnosticMark &mark : m_diagnostics.value(editor->documentId())) {
        const auto line = editor->send(SCI_LINEFROMPOSITION, static_cast<uptr_t>(mark.start)) + 1;
        entries.append(QStringLiteral("%1:%2").arg(mark.severity).arg(line));
    }
    return entries;
}

void MainWindow::applyDiagnostics(quint64 documentId) {
    FernyEditor *editor = nullptr;
    for (int index = 0; index < m_tabs->count(); ++index) {
        if (editorAt(index)->documentId() == documentId) {
            editor = editorAt(index);
        }
    }
    if (!editor) {
        return;
    }
    const int count = m_core.lspDiagnostics(documentId, editor->utf8Text());
    QList<DiagnosticMark> marks;
    const auto length = static_cast<sptr_t>(editor->length());
    for (int i = 8; i <= 10; ++i) {
        editor->send(SCI_SETINDICATORCURRENT, static_cast<uptr_t>(i));
        editor->send(SCI_INDICATORCLEARRANGE, 0, length);
    }
    for (int i = 0; i < count; ++i) {
        const CursorPos range = m_core.lspDiagnosticRange(i);
        const int severity = m_core.lspDiagnosticSeverity(i);
        const int start = static_cast<int>(range.line);
        // A zero-width range still marks one character.
        const int end = std::max(static_cast<int>(range.column), start + 1);
        editor->send(SCI_SETINDICATORCURRENT, static_cast<uptr_t>(severity == 1   ? 8
                                                                  : severity == 2 ? 9
                                                                                  : 10));
        editor->send(SCI_INDICATORFILLRANGE, static_cast<uptr_t>(start), end - start);
        marks.append({start, end, severity, m_core.lspDiagnosticMessage(i)});
    }
    m_diagnostics.insert(documentId, marks);
    if (editor == currentEditor()) {
        updateStatus();
    }
}

void MainWindow::showDiagnosticAt(FernyEditor *editor, int x, int y) {
    const auto pos = static_cast<int>(
        editor->send(SCI_POSITIONFROMPOINTCLOSE, static_cast<uptr_t>(x), static_cast<sptr_t>(y)));
    if (pos < 0) {
        return;
    }
    QStringList messages;
    for (const DiagnosticMark &mark : m_diagnostics.value(editor->documentId())) {
        if (pos >= mark.start && pos < mark.end) {
            messages.append(mark.message);
        }
    }
    if (messages.isEmpty() &&
        m_core.pluginHasProvider(QStringLiteral("hover"), editor->filetypeId())) {
        const auto upos = static_cast<uptr_t>(pos);
        const QStringList hover =
            askPluginProvider(QStringLiteral("hover"), editor, pos,
                              editor->textRange(editor->send(SCI_WORDSTARTPOSITION, upos, 1),
                                                editor->send(SCI_WORDENDPOSITION, upos, 1)));
        messages = hover;
    }
    if (messages.isEmpty()) {
        // Nothing to report there: ask the server what is under the mouse.
        m_hoverToken = m_core.lspRequest(editor->documentId(), 2, editor->utf8Text(), pos);
        m_requestEditor = editor;
        m_hoverPoint = editor->mapToGlobal(QPoint(x, y));
        return;
    }
    {
        // Plain text: the server's message is not interpreted as markup.
        QToolTip::showText(editor->mapToGlobal(QPoint(x, y)),
                           messages.join(QLatin1Char('\n'))
                               .toHtmlEscaped()
                               .replace(QLatin1Char('\n'), QStringLiteral("<br>")),
                           editor);
    }
}

void MainWindow::requestHoverAtCursor() {
    if (FernyEditor *editor = currentEditor()) {
        if (m_core.pluginHasProvider(QStringLiteral("hover"), editor->filetypeId())) {
            const QStringList hover = askPluginProvider(
                QStringLiteral("hover"), editor, static_cast<int>(editor->send(SCI_GETCURRENTPOS)),
                editor->wordAtCursor(false));
            if (!hover.isEmpty()) {
                m_lastHover = hover.first();
                return;
            }
        }
        m_requestEditor = editor;
        m_hoverPoint = editor->mapToGlobal(QPoint(0, 0));
        m_hoverToken = m_core.lspRequest(editor->documentId(), 2, editor->utf8Text(),
                                         static_cast<int>(editor->send(SCI_GETCURRENTPOS)));
    }
}

void MainWindow::requestSignature(FernyEditor *editor) {
    m_requestEditor = editor;
    m_requestPosition = static_cast<int>(editor->send(SCI_GETCURRENTPOS));
    m_signatureToken =
        m_core.lspRequest(editor->documentId(), 3, editor->utf8Text(), m_requestPosition);
}

void MainWindow::findUsages() {
    FernyEditor *editor = currentEditor();
    m_usagesToken = editor ? m_core.lspRequest(editor->documentId(), 5, editor->utf8Text(),
                                               static_cast<int>(editor->send(SCI_GETCURRENTPOS)))
                           : 0;
    if (m_usagesToken == 0) {
        statusBar()->showMessage(tr("No language server for this document."), 3000);
    }
}

void MainWindow::renameSymbol() {
    FernyEditor *editor = currentEditor();
    const QString word = editor ? editor->wordAtCursor(false) : QString();
    if (word.isEmpty()) {
        return;
    }
    QString name = m_smokeInput;
    if (!m_smoke) {
        bool ok = false;
        name = QInputDialog::getText(this, tr("Rename Symbol"), tr("New name:"), QLineEdit::Normal,
                                     word, &ok);
        if (!ok) {
            return;
        }
    }
    name = name.trimmed();
    if (name.isEmpty() || name == word) {
        return;
    }
    m_requestEditor = editor;
    m_renameToken = m_core.lspRequestWith(editor->documentId(), 6, editor->utf8Text(),
                                          static_cast<int>(editor->send(SCI_GETCURRENTPOS)), name);
    if (m_renameToken == 0) {
        statusBar()->showMessage(tr("No language server for this document."), 3000);
    }
}

void MainWindow::formatDocument() {
    FernyEditor *editor = currentEditor();
    if (!editor) {
        return;
    }
    // A plugin formatter first (H3.3c): one undo action, unsaved.
    if (!editor->isReadOnly() &&
        m_core.pluginHasProvider(QStringLiteral("format"), editor->filetypeId())) {
        const QStringList text =
            askPluginProvider(QStringLiteral("format"), editor,
                              static_cast<int>(editor->send(SCI_GETCURRENTPOS)), QString());
        if (!text.isEmpty()) {
            if (text.first().toUtf8() != editor->utf8Text()) {
                replaceWholeText(editor, text.first().toUtf8());
            }
            return;
        }
    }
    const int width = editor->indentWidth();
    m_requestEditor = editor;
    m_formatToken =
        m_core.lspRequestWith(editor->documentId(), 7, editor->utf8Text(), 0,
                              QString::number(editor->indentUsesTabs() ? -width : width));
    if (m_formatToken == 0) {
        statusBar()->showMessage(tr("No language server for this document."), 3000);
    }
}

void MainWindow::applyRename() {
    const QString refusal = m_core.lspEditRefusal();
    if (!refusal.isEmpty()) {
        showMessage(tr("The language server's rename was not applied (%1).").arg(refusal));
        return;
    }
    const QStringList files = m_core.lspEditFiles();
    if (files.isEmpty()) {
        statusBar()->showMessage(tr("Nothing to rename here."), 3000);
        return;
    }
    // Preview first; files open in tabs with the changes, not saved.
    int changes = 0;
    QStringList listed;
    for (int i = 0; i < files.size(); ++i) {
        changes += m_core.lspEditCount(i);
        listed.append(tr("%1: %n change(s)", nullptr, m_core.lspEditCount(i))
                          .arg(m_core.displayName(files.at(i))));
    }
    QMessageBox box(this);
    box.setWindowTitle(tr("Rename Symbol"));
    box.setText(tr("Change %1 place(s) in %n file(s)? The files open with the changes, unsaved, "
                   "so you can review them.",
                   nullptr, static_cast<int>(files.size()))
                    .arg(changes));
    box.setDetailedText(listed.join(QLatin1Char('\n')));
    box.setStandardButtons(QMessageBox::Yes | QMessageBox::Cancel);
    if (!m_smoke && box.exec() != QMessageBox::Yes) {
        return;
    }
    FernyEditor *origin = currentEditor();
    for (int i = 0; i < files.size(); ++i) {
        if (openFile(files.at(i)) != LoadStatus::Ok) {
            continue;
        }
        FernyEditor *editor = currentEditor();
        bool ok = false;
        const QByteArray text = m_core.lspApplyEdits(i, editor->utf8Text(), ok);
        if (!ok) {
            showMessage(tr("%1: the changes overlap and were not applied.")
                            .arg(m_core.displayName(files.at(i))));
            continue;
        }
        editor->replaceAllText(text);
    }
    if (origin) {
        m_tabs->setCurrentWidget(origin);
    }
}

int MainWindow::usageCount() const {
    return m_results->topLevelItemCount();
}

void MainWindow::onLspAnswer(qint64 token, int kind) {
    const int count = m_core.lspAnswerCount();
    // An answer for an editor that was closed meanwhile is dropped.
    bool editorOpen = false;
    for (int index = 0; index < m_tabs->count(); ++index) {
        editorOpen = editorOpen || editorAt(index) == m_requestEditor;
    }
    if (kind == 1 && token == m_completionToken) {
        m_completionToken = 0;
        FernyEditor *editor = currentEditor();
        // Only if the cursor did not move while waiting.
        if (!editorOpen || editor != m_requestEditor ||
            editor->send(SCI_GETCURRENTPOS) != m_requestPosition) {
            return;
        }
        // Only what continues the word typed (servers also send others),
        // after the plugins' items.
        QStringList items = m_pluginCompletions;
        for (int i = 0; i < count; ++i) {
            const QString item = m_core.lspAnswerText(i);
            if (item.startsWith(m_completionPrefix) && !items.contains(item)) {
                items.append(item);
            }
        }
        if (items.isEmpty()) {
            completeWordFromDocument();
            return;
        }
        editor->showCompletions(items, static_cast<int>(m_completionPrefix.toUtf8().size()));
    } else if (kind == 2 && token == m_hoverToken) {
        m_hoverToken = 0;
        m_lastHover = count > 0 ? m_core.lspAnswerText(0) : QString();
        if (editorOpen && !m_lastHover.isEmpty()) {
            QToolTip::showText(
                m_hoverPoint,
                m_lastHover.toHtmlEscaped().replace(QLatin1Char('\n'), QStringLiteral("<br>")),
                m_requestEditor);
        }
    } else if (kind == 3 && token == m_signatureToken) {
        m_signatureToken = 0;
        if (!editorOpen || count == 0) {
            return;
        }
        const QByteArray label = m_core.lspAnswerText(0).toUtf8();
        m_requestEditor->sends(SCI_CALLTIPSHOW, static_cast<uptr_t>(m_requestPosition),
                               label.constData());
        const CursorPos parameter = m_core.lspAnswerParameter();
        m_requestEditor->send(SCI_CALLTIPSETHLT, parameter.line, parameter.column);
    } else if (kind == 4 && token == m_definitionToken) {
        m_definitionToken = 0;
        if (count == 0) {
            gotoDefinitionLocal();
            return;
        }
        const QString path = m_core.lspAnswerPath(0);
        pushNavigation();
        if (openFile(path) != LoadStatus::Ok) {
            return;
        }
        FernyEditor *target = currentEditor();
        const int offset = m_core.lspAnswerOffset(0, target->utf8Text());
        target->send(SCI_GOTOPOS, static_cast<uptr_t>(offset));
        target->send(SCI_SCROLLCARET);
        target->setFocus();
    } else if (kind == 6 && token == m_renameToken) {
        m_renameToken = 0;
        applyRename();
    } else if (kind == 7 && token == m_formatToken) {
        m_formatToken = 0;
        if (!editorOpen) {
            return;
        }
        bool ok = false;
        const QByteArray text = m_core.lspApplyEdits(0, m_requestEditor->utf8Text(), ok);
        if (!m_core.lspEditRefusal().isEmpty() || !ok) {
            showMessage(tr("The language server's formatting was not applied."));
        } else if (text != m_requestEditor->utf8Text()) {
            m_requestEditor->replaceAllText(text);
        }
    } else if (kind == 8 && token == m_symbolsToken) {
        m_symbolsToken = 0;
        if (count == 0) {
            showTreeSitterSymbols();
            return;
        }
        m_symbols->clear();
        m_symbolsSource = QStringLiteral("lsp");
        QHash<QString, QTreeWidgetItem *> groups;
        for (int i = 0; i < count; ++i) {
            const QString text = m_core.lspAnswerText(i);
            addSymbol(groups, text.section(QLatin1Char(' '), 0, 0),
                      text.section(QLatin1Char(' '), 1), static_cast<int>(m_core.lspAnswerLine(i)));
        }
    } else if (kind == 9 && token == m_workspaceSymbolsToken) {
        m_workspaceSymbolsToken = 0;
        m_results->clear();
        QList<QTreeWidgetItem *> items;
        for (int i = 0; i < count; ++i) {
            items.append(new QTreeWidgetItem({m_core.lspAnswerPath(i),
                                              QString::number(m_core.lspAnswerLine(i)),
                                              m_core.lspAnswerText(i)}));
        }
        m_results->addTopLevelItems(items);
        m_messageDock->show();
        m_messageWindow->showResults(tr("%n symbol(s)", nullptr, count));
    } else if (kind == 5 && token == m_usagesToken) {
        m_usagesToken = 0;
        m_results->clear();
        QList<QTreeWidgetItem *> items;
        for (int i = 0; i < count; ++i) {
            items.append(new QTreeWidgetItem({m_core.lspAnswerPath(i),
                                              QString::number(m_core.lspAnswerLine(i)),
                                              m_core.lspAnswerText(i)}));
        }
        m_results->addTopLevelItems(items);
        m_messageDock->show();
        m_messageWindow->showResults(tr("%n usage(s)", nullptr, count));
    }
}

// --- Plugins (H3.2) -------------------------------------------------------------------

namespace {

QString permissionText(const QString &permission) {
    if (permission == QStringLiteral("fs.read.project")) {
        return MainWindow::tr("Read the files of the project");
    }
    if (permission == QStringLiteral("fs.read.any")) {
        return MainWindow::tr("Read any file you can read");
    }
    if (permission == QStringLiteral("fs.write.project")) {
        return MainWindow::tr("Write the files of the project");
    }
    if (permission == QStringLiteral("process.spawn")) {
        return MainWindow::tr("Run programs");
    }
    if (permission == QStringLiteral("net")) {
        return MainWindow::tr("Use the network");
    }
    if (permission == QStringLiteral("clipboard")) {
        return MainWindow::tr("Use the clipboard");
    }
    if (permission == QStringLiteral("config")) {
        return MainWindow::tr("Store private settings for this plugin");
    }
    return permission;
}

}  // namespace

void MainWindow::startPlugins(bool safeMode, bool grantAll, const QString &language) {
    if (m_configDir.isEmpty()) {
        return;
    }
    m_pluginIds = m_core.loadPlugins(m_configDir, safeMode);
    for (const QString &id : m_pluginIds) {
        const QStringList asked = m_core.pluginPermissions(id);
        // Smoke tests only: normal startup keeps every plugin disabled until
        // the user decides explicitly.
        if (grantAll && !m_core.pluginDecided(id)) {
            m_core.decidePlugin(id, true, asked);
            for (const QString &permission : asked) {
                m_core.pluginConfirm(id, permission, true);  // sensitive ones only
            }
            continue;
        }
        if (!m_core.pluginNeedsDecision(id)) {
            continue;
        }
        // A smoke run has nobody to answer the dialog: the plugin stays
        // undecided, so it stays disabled, and nothing is written to grants.
        if (m_smoke) {
            continue;
        }
        // First load (or a changed manifest): the user decides (R2).
        QDialog dialog(this);
        dialog.setWindowTitle(tr("New Plugin"));
        auto *layout = new QVBoxLayout(&dialog);
        auto *intro = new QLabel(
            asked.isEmpty()
                ? tr("Enable the plugin %1? It asks for no permission.")
                      .arg(m_core.displayName(m_core.pluginName(id)))
                : tr("Enable the plugin %1? It asks for these permissions; untick those you do "
                     "not want to grant.")
                      .arg(m_core.displayName(m_core.pluginName(id))),
            &dialog);
        intro->setWordWrap(true);
        intro->setTextFormat(Qt::PlainText);
        layout->addWidget(intro);
        QList<QCheckBox *> boxes;
        for (const QString &permission : asked) {
            auto *box = new QCheckBox(permissionText(permission), &dialog);
            box->setChecked(true);
            box->setProperty("permission", permission);
            layout->addWidget(box);
            boxes.append(box);
        }
        auto *buttons = new QDialogButtonBox(&dialog);
        buttons->addButton(tr("Enable"), QDialogButtonBox::AcceptRole);
        buttons->addButton(tr("Keep Disabled"), QDialogButtonBox::RejectRole);
        layout->addWidget(buttons);
        connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
        connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
        const bool enabled = dialog.exec() == QDialog::Accepted;
        QStringList granted;
        for (const QCheckBox *box : boxes) {
            if (box->isChecked()) {
                granted.append(box->property("permission").toString());
            }
        }
        m_core.decidePlugin(id, enabled, granted);
    }
    m_core.startPlugins(language);
    // Plugin processes never outlive Ferny (audit F2).
    connect(qApp, &QCoreApplication::aboutToQuit, this, [this]() { m_core.stopPluginProcesses(); });
    connect(&m_core, &FernyCore::pluginActions, this,
            [this](const QStringList &actions) { applyPluginActions(actions, nullptr); });
    for (const QString &warning : m_core.pluginWarnings()) {
        m_messageWindow->appendMessage(m_core.displayName(warning));
    }
    buildToolsMenu();
    // Once the event loop runs: the files of the command line are open.
    QTimer::singleShot(0, this, [this]() {
        emitPluginEvent(QStringLiteral("app.startup_complete"), currentEditor());
    });
}

void MainWindow::buildToolsMenu() {
    const QStringList ids = m_core.pluginCommandIds();
    // Tools, before Help: the plugin manager, then a submenu per plugin, in
    // plain text (T7).
    m_toolsMenu = new QMenu(tr("&Tools"), this);
    const QList<QAction *> menus = menuBar()->actions();
    menuBar()->insertMenu(menus.isEmpty() ? nullptr : menus.last(), m_toolsMenu);
    QAction *manager = m_toolsMenu->addAction(tr("&Plugins…"));
    manager->setObjectName(QStringLiteral("tools.plugins"));
    manager->setMenuRole(QAction::NoRole);
    connect(manager, &QAction::triggered, this, &MainWindow::showPluginManager);
    if (!ids.isEmpty()) {
        m_toolsMenu->addSeparator();
    }
    QHash<QString, QMenu *> submenus;
    for (const QString &id : ids) {
        const QString pluginId = id.section(QLatin1Char('.'), 1, 1);
        QMenu *&submenu = submenus[pluginId];
        if (!submenu) {
            submenu = m_toolsMenu->addMenu(m_core.displayName(m_core.pluginName(pluginId))
                                               .replace(QLatin1Char('&'), QStringLiteral("&&")));
        }
        QAction *action = submenu->addAction(
            m_core.pluginCommandTitle(id).replace(QLatin1Char('&'), QStringLiteral("&&")));
        action->setObjectName(id);
        // Keys only from the user's keybindings.toml (H3.3c).
        QList<QKeySequence> shortcuts;
        for (const QString &key : m_core.pluginCommandShortcuts(id)) {
            shortcuts.append(QKeySequence::fromString(key, QKeySequence::PortableText));
        }
        action->setShortcuts(shortcuts);
        connect(action, &QAction::triggered, this, [this, id]() { runPluginCommand(id); });
    }
}

void MainWindow::emitPluginEvent(const QString &event, FernyEditor *editor) {
    if (m_pluginIds.isEmpty()) {
        return;
    }
    const QString path = editor ? m_core.documentPath(editor->documentId()) : QString();
    const QString filetype = editor ? editor->filetypeId() : QString();
    applyPluginActions(m_core.pluginEmit(event, editor != nullptr, path, filetype), nullptr);
}

void MainWindow::runPluginCommand(const QString &id) {
    FernyEditor *editor = currentEditor();
    if (!editor) {
        return;
    }
    // Once more if the user just allowed a permission it asked for (H3.3b).
    for (int attempt = 0; attempt < 2; ++attempt) {
        const QStringList actions = m_core.pluginRunCommand(
            id, m_core.documentPath(editor->documentId()), editor->filetypeId(),
            static_cast<int>(editor->cursorLine()), static_cast<int>(editor->cursorColumn()),
            QString::fromUtf8(editor->utf8Text()), editor->selectedText(),
            QGuiApplication::clipboard()->text());
        if (!applyPluginActions(actions, editor)) {
            break;
        }
    }
}

bool MainWindow::applyPluginActions(const QStringList &actions, FernyEditor *editor) {
    QStringList edits;
    QStringList confirms;
    for (qsizetype i = 0; i + 1 < actions.size(); i += 2) {
        const QString &kind = actions.at(i);
        const QString &text = actions.at(i + 1);
        if (kind == QStringLiteral("status")) {
            statusBar()->showMessage(text, 5000);
        } else if (kind == QStringLiteral("message") || kind == QStringLiteral("error")) {
            m_messageWindow->appendMessage(text);
        } else if (kind == QStringLiteral("clipboard")) {
            QGuiApplication::clipboard()->setText(text);
        } else if (kind == QStringLiteral("confirm")) {
            confirms.append(text);
        } else if (editor) {
            edits << kind << text;
        }
    }
    // Edits land on the document the command ran on, never saved (T9).
    if (editor && !edits.isEmpty() && !editor->isReadOnly()) {
        editor->applyPluginEdits(edits);
    }
    // First use of a sensitive permission (R2): ask once, remember.
    bool allowed = false;
    for (const QString &confirm : confirms) {
        const QString pluginId = confirm.section(QLatin1Char(' '), 0, 0);
        const QString permission = confirm.section(QLatin1Char(' '), 1);
        QMessageBox box(
            QMessageBox::Question, tr("Plugin Permission"),
            tr("The plugin %1 wants to use this permission for the first time: %2. "
               "Allow it? Ferny remembers the answer until the plugin's permissions "
               "change.")
                .arg(m_core.displayName(m_core.pluginName(pluginId)), permissionText(permission)),
            QMessageBox::NoButton, this);
        box.setTextFormat(Qt::PlainText);
        QPushButton *allow = box.addButton(tr("Allow"), QMessageBox::AcceptRole);
        box.addButton(tr("Deny"), QMessageBox::RejectRole);
        box.exec();
        const bool yes = box.clickedButton() == allow;
        m_core.pluginConfirm(pluginId, permission, yes);
        allowed = allowed || yes;
    }
    return allowed;
}

void MainWindow::showPluginManager() {
    QDialog dialog(this);
    dialog.setWindowTitle(tr("Plugins"));
    dialog.resize(560, 420);
    auto *layout = new QVBoxLayout(&dialog);
    auto *tree = new QTreeWidget(&dialog);
    tree->setHeaderLabels({tr("Plugin"), tr("Origin"), tr("Status")});
    const auto statusText = [](const QString &status) {
        if (status == QStringLiteral("running")) {
            return tr("Running");
        }
        if (status == QStringLiteral("failed")) {
            return tr("Failed");
        }
        if (status == QStringLiteral("undecided")) {
            return tr("Not decided");
        }
        if (status == QStringLiteral("safe-mode")) {
            return tr("Safe mode");
        }
        return tr("Disabled — enable and restart");
    };
    for (const QString &id : m_pluginIds) {
        auto *item = new QTreeWidgetItem(
            tree,
            {m_core.displayName(m_core.pluginName(id)),
             m_core.pluginOrigin(id) == QStringLiteral("bundled") ? tr("Bundled") : tr("User"),
             statusText(m_core.pluginStatus(id))});
        item->setFlags(item->flags() | Qt::ItemIsUserCheckable);
        item->setCheckState(0, m_core.pluginEnabled(id) ? Qt::Checked : Qt::Unchecked);
        item->setData(0, Qt::UserRole, id);
        const bool decided = m_core.pluginDecided(id);
        const QStringList granted = m_core.pluginGranted(id);
        for (const QString &permission : m_core.pluginPermissions(id)) {
            auto *child = new QTreeWidgetItem(item, {permissionText(permission)});
            child->setFlags(child->flags() | Qt::ItemIsUserCheckable);
            // Not decided yet: ticked, as in the first-load dialog.
            child->setCheckState(
                0, !decided || granted.contains(permission) ? Qt::Checked : Qt::Unchecked);
            child->setData(0, Qt::UserRole, permission);
        }
    }
    for (const QString &id : m_core.pluginQuarantinedIds()) {
        auto *item =
            new QTreeWidgetItem(tree, {id, tr("External"), tr("Excluded — review pending")});
        item->setFlags(item->flags() & ~Qt::ItemIsUserCheckable);
        item->setToolTip(2, tr("This plugin is disabled until provenance, licence, API and "
                               "resource checks pass."));
    }
    tree->expandAll();
    tree->resizeColumnToContents(0);
    layout->addWidget(tree);
    auto *note = new QLabel(tr("Plugins are disabled until you enable them. They run in a "
                               "sandbox and use only the permissions ticked here. Changes apply "
                               "when Ferny restarts."),
                            &dialog);
    note->setWordWrap(true);
    layout->addWidget(note);
    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, &dialog);
    QPushButton *reset =
        buttons->addButton(tr("Ask Again Before First Use"), QDialogButtonBox::ActionRole);
    QPushButton *folder =
        buttons->addButton(tr("Open Plugins Folder"), QDialogButtonBox::ActionRole);
    layout->addWidget(buttons);
    connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
    connect(reset, &QPushButton::clicked, this, [this, tree]() {
        QTreeWidgetItem *item = tree->currentItem();
        while (item && item->parent()) {
            item = item->parent();
        }
        if (item && m_core.pluginResetConfirmations(item->data(0, Qt::UserRole).toString())) {
            statusBar()->showMessage(
                tr("The plugin will ask again before running programs or reading outside the "
                   "project."),
                5000);
        }
    });
    connect(folder, &QPushButton::clicked, this, [this]() {
        const QDir configDir(m_configDir);
        const QString path = configDir.filePath(QStringLiteral("plugins"));
        if (!QDir().mkpath(path)) {
            statusBar()->showMessage(tr("Could not create the plugins folder."), 5000);
            QMessageBox::warning(this, tr("Plugins"), tr("Could not create the plugins folder."));
            return;
        }
        if (!QDesktopServices::openUrl(QUrl::fromLocalFile(path))) {
            statusBar()->showMessage(tr("Could not open the plugins folder."), 5000);
            QMessageBox::warning(this, tr("Plugins"), tr("Could not open the plugins folder."));
            return;
        }
        statusBar()->showMessage(tr("Plugins folder opened."), 5000);
    });
    if (dialog.exec() != QDialog::Accepted) {
        return;
    }
    bool changed = false;
    for (int i = 0; i < tree->topLevelItemCount(); ++i) {
        const QTreeWidgetItem *item = tree->topLevelItem(i);
        const QString id = item->data(0, Qt::UserRole).toString();
        const bool enabled = item->checkState(0) == Qt::Checked;
        QStringList granted;
        for (int c = 0; c < item->childCount(); ++c) {
            if (item->child(c)->checkState(0) == Qt::Checked) {
                granted.append(item->child(c)->data(0, Qt::UserRole).toString());
            }
        }
        // Only what the user changed becomes a decision.
        const bool same = m_core.pluginDecided(id) && enabled == m_core.pluginEnabled(id) &&
                          (!enabled || granted == m_core.pluginGranted(id));
        if (!same) {
            m_core.decidePlugin(id, enabled, granted);
            changed = true;
        }
    }
    if (changed) {
        statusBar()->showMessage(tr("Plugin changes apply when Ferny restarts."), 5000);
    }
}

QStringList MainWindow::pluginSummary() const {
    QStringList summary;
    for (const QString &id : m_pluginIds) {
        summary.append(id + QLatin1Char('=') + m_core.pluginStatus(id));
    }
    return summary;
}

// --- Templates and snippets (H2.5b) --------------------------------------------

void MainWindow::newFromTemplate() {
    const QStringList names = m_core.fileTemplateNames();
    const int chosen = pickItem(tr("New from Template"), names);
    if (chosen < 0) {
        return;
    }
    const QDateTime now = QDateTime::currentDateTime();
    const QStringList dates{now.toString(QStringLiteral("yyyy")),
                            now.toString(QStringLiteral("yyyy-MM-dd")),
                            now.toString(QStringLiteral("yyyy-MM-dd HH:mm:ss"))};
    QString filetype;
    const QString text = m_core.expandFileTemplate(names.at(chosen), dates, filetype);
    newDocument();
    FernyEditor *editor = currentEditor();
    if (!editor) {
        return;
    }
    editor->setUtf8Text(text.toUtf8());
    if (!filetype.isEmpty()) {
        setFiletype(editor, filetype);
    }
    updateTabLabel(editor);
}

bool MainWindow::expandSnippet(FernyEditor *editor) {
    const QString keyword = editor->wordAtCursor(true);
    if (keyword.isEmpty()) {
        return false;
    }
    const int eol = static_cast<int>(editor->send(SCI_GETEOLMODE));
    const QString eolText = eol == SC_EOL_CRLF ? QStringLiteral("\r\n")
                            : eol == SC_EOL_CR ? QStringLiteral("\r")
                                               : QStringLiteral("\n");
    const QString unit = editor->indentUsesTabs()
                             ? QStringLiteral("\t")
                             : QString(editor->indentWidth(), QLatin1Char(' '));
    QString text;
    int cursor = -1;
    if (!m_core.expandSnippet(editor->filetypeId(), keyword,
                              {editor->lineIndentation(), unit, eolText}, text, cursor)) {
        return false;
    }
    editor->replaceBeforeCursor(static_cast<int>(keyword.toUtf8().size()), text, cursor);
    return true;
}

// --- Projects (H2.5a) ------------------------------------------------------------

bool MainWindow::closeEveryTab() {
    for (int index = m_tabs->count() - 1; index >= 0; --index) {
        if (!closeTab(index)) {
            return false;
        }
    }
    return true;
}

bool MainWindow::openProject(const QString &folder, bool atStart) {
    if (!atStart) {
        if (!m_core.projectRoot().isEmpty() && !closeProject()) {
            return false;
        }
        saveSession();  // the global session, before its files are closed
        if (!closeEveryTab()) {
            return false;
        }
    }
    switch (m_core.openProject(folder)) {
        case 0:
            break;
        case 1:
            showMessage(tr("There is no project in that folder (.ferny/project.toml)."));
            return false;
        default:
            showMessage(tr("The project file is not valid."));
            return false;
    }
    for (const QString &warning : m_core.projectWarnings()) {
        m_messageWindow->appendMessage(tr("Project: %1").arg(warning));
    }
    const QString session =
        m_configDir.isEmpty() ? QString() : m_core.projectSessionFile(m_configDir);
    const int active = session.isEmpty() ? -1 : m_core.queueProjectSession(session);
    if (active >= 0) {
        openPendingWithActive(active);
    }
    m_files->setRoot(m_core.projectBase());
    statusBar()->showMessage(tr("Project %1 opened.").arg(m_core.displayName(m_core.projectName())),
                             3000);
    for (int index = 0; index < m_tabs->count(); ++index) {
        applyHighlighting(editorAt(index));
    }
    updateTitle();
    updateStatus();
    return true;
}

bool MainWindow::closeProject() {
    if (m_core.projectRoot().isEmpty()) {
        return true;
    }
    const int active = collectSessionFiles();
    if (!m_configDir.isEmpty()) {
        m_core.saveProjectSession(m_core.projectSessionFile(m_configDir), active);
    }
    if (!closeEveryTab()) {
        return false;
    }
    m_core.closeProject();  // brings the global session's files back
    m_core.queueSessionFiles();
    openPendingWithActive(m_core.sessionActiveIndex());
    updateTitle();
    updateStatus();
    return true;
}

void MainWindow::chooseProject() {
    const QString folder =
        m_smoke ? m_smokeInput
                : QFileDialog::getExistingDirectory(this, tr("Open Project (folder with .ferny)"));
    if (!folder.isEmpty()) {
        openProject(folder);
    }
}

void MainWindow::newProject() {
    const QString folder =
        m_smoke ? m_smokeInput : QFileDialog::getExistingDirectory(this, tr("New Project Folder"));
    if (folder.isEmpty()) {
        return;
    }
    QString name = QFileInfo(folder).fileName();
    if (!m_smoke) {
        bool ok = false;
        name = QInputDialog::getText(this, tr("New Project"), tr("Name:"), QLineEdit::Normal, name,
                                     &ok);
        if (!ok) {
            return;
        }
    }
    switch (m_core.createProject(folder, name)) {
        case 0:
            openProject(folder);
            break;
        case 1:
            showMessage(tr("That folder already has a project; open it instead."));
            break;
        default:
            showMessage(tr("The project could not be created."));
            break;
    }
}

void MainWindow::importGeanyProject() {
    const QString file =
        m_smoke ? m_smokeInput
                : QFileDialog::getOpenFileName(this, tr("Import Geany Project"), QString(),
                                               tr("Geany projects (*.geany)"));
    if (file.isEmpty()) {
        return;
    }
    const QString target = m_core.previewGeanyProject(file);
    if (target.isEmpty()) {
        showMessage(tr("That Geany project cannot be imported."));
        return;
    }
    // Preview first: nothing is written until the user agrees (T11).
    QMessageBox box(this);
    box.setWindowTitle(tr("Import Geany Project"));
    box.setText(tr("Create a Ferny project in %1? Imported build commands run only once the "
                   "folder is trusted.")
                    .arg(m_core.displayName(target)));
    box.setInformativeText(m_core.geanyProjectImported().join(QLatin1Char('\n')));
    const QStringList left = m_core.geanyProjectNotImported();
    if (!left.isEmpty()) {
        box.setDetailedText(tr("Not imported:") + QLatin1Char('\n') + left.join(QLatin1Char('\n')));
    }
    box.setStandardButtons(QMessageBox::Yes | QMessageBox::Cancel);
    if (!m_smoke && box.exec() != QMessageBox::Yes) {
        return;
    }
    if (!m_core.applyGeanyProject()) {
        showMessage(tr("The project could not be written (a project may already exist there)."));
        return;
    }
    for (const QString &item : left) {
        m_messageWindow->appendMessage(tr("Not imported from the Geany project: %1").arg(item));
    }
    openProject(target);
}

void MainWindow::applyDocument(FernyEditor *editor, const QByteArray &utf8) {
    const quint64 id = editor->documentId();
    const DocumentInfo info = m_core.documentInfo(id);
    editor->setReadOnly(false);
    detectFiletype(editor, utf8);
    editor->setUtf8Text(utf8);
    editor->setEolMode(static_cast<int>(info.eol));
    editor->setReadOnly(info.read_only != ReadOnlyState::Writable);

    QString message;
    if (info.read_only == ReadOnlyState::MalformedEncoding) {
        message = tr("Some bytes are not valid %1; the document is read-only. "
                     "Reload it with the right encoding or set an encoding to edit it.")
                      .arg(m_core.documentEncoding(id));
    } else if (info.read_only == ReadOnlyState::Permissions) {
        message = tr("The file is read-only.");
    } else if (info.large) {
        message = tr("Large file: syntax highlighting is off.");
    } else if (const int bidi = m_core.countBidiControls(utf8); bidi > 0) {
        // Trojan Source (T7): the controls are drawn as labels, not applied.
        message =
            tr("Warning: the file contains %n bidirectional control character(s); "
               "they are shown as labels.",
               nullptr, bidi);
    } else if (info.mixed_eol) {
        message = tr("The file mixes line endings; new lines use %1.").arg(eolName(info.eol));
    }
    statusBar()->showMessage(message);
    updateTabLabel(editor);
    updateTitle();
    updateStatus();
    lspSync(editor);
}

// --- Saving and reloading --------------------------------------------------------

bool MainWindow::save(FernyEditor *editor) {
    if (!editor) {
        return false;
    }
    const quint64 id = editor->documentId();
    if (!m_core.documentInfo(id).has_path) {
        return saveAs(editor);
    }
    applySaveTransforms(editor);
    runBeforeSave(editor);
    SaveStatus status = m_core.saveDocument(id, editor->utf8Text(), false);
    if (status == SaveStatus::ModifiedExternally) {
        const auto answer =
            QMessageBox::warning(this, tr("File Changed on Disk"),
                                 tr("Another program changed or deleted %1 since it was opened. "
                                    "Overwrite it with the text in the editor?")
                                     .arg(QFileInfo(m_core.documentPath(id)).fileName()),
                                 QMessageBox::Save | QMessageBox::Cancel, QMessageBox::Cancel);
        if (answer != QMessageBox::Save) {
            return false;
        }
        status = m_core.saveDocument(id, editor->utf8Text(), true);
    }
    return reportSave(editor, status);
}

bool MainWindow::saveAs(FernyEditor *editor) {
    if (!editor) {
        return false;
    }
    const quint64 id = editor->documentId();
    const QString path = QFileDialog::getSaveFileName(this, tr("Save As"), m_core.documentPath(id));
    if (path.isEmpty()) {
        return false;
    }
    const bool saved = reportSave(editor, m_core.saveDocumentAs(id, path, editor->utf8Text()));
    if (saved) {
        const DocumentInfo info = m_core.documentInfo(id);
        editor->setReadOnly(info.read_only != ReadOnlyState::Writable);
        detectFiletype(editor, editor->utf8Text().left(256));
        m_core.addRecentFile(path);
    }
    return saved;
}

bool MainWindow::saveAll() {
    bool allSaved = true;
    for (int i = 0; i < m_tabs->count(); ++i) {
        FernyEditor *editor = editorAt(i);
        if (editor &&
            (editor->isModified() || !m_core.documentInfo(editor->documentId()).has_path)) {
            if (!save(editor)) {
                allSaved = false;
            }
        }
    }
    return allSaved;
}

namespace {

// Scintilla colours are 0xBBGGRR; exports use 0xRRGGBB.
quint32 rgb(sptr_t bgr) {
    const auto c = static_cast<quint32>(bgr);
    return ((c & 0xffU) << 16) | (c & 0xff00U) | ((c >> 16) & 0xffU);
}

}  // namespace

QString MainWindow::documentTitle(FernyEditor *editor) const {
    const QString path = m_core.documentPath(editor->documentId());
    return path.isEmpty() ? tr("untitled") : QFileInfo(path).fileName();
}

void MainWindow::printDocument(FernyEditor *editor, QPagedPaintDevice *device,
                               const QString &title) {
    // One painter for the whole job: Scintilla draws with it because the
    // device is already painting (ending a painter would end the job).
    editor->send(SCI_SETPRINTCOLOURMODE, SC_PRINT_COLOURONWHITE);
    editor->send(SCI_SETPRINTWRAPMODE, SC_WRAP_WORD);
    // The long-line marker is an editing aid, not part of the document.
    const sptr_t edge = editor->send(SCI_GETEDGEMODE);
    editor->send(SCI_SETEDGEMODE, EDGE_NONE);
    QPainter painter(device);
    QFont headerFont = painter.font();
    headerFont.setPointSize(9);
    painter.setFont(headerFont);
    const int header = painter.fontMetrics().height() * 2;
    const int width = device->width();
    const int height = device->height() - header;
    const auto length = static_cast<Sci_Position>(editor->length());
    Sci_Position pos = 0;
    int pageNumber = 1;
    do {
        if (pageNumber > 1) {
            device->newPage();
        }
        painter.save();
        painter.setFont(headerFont);
        const QRect line(0, 0, width, header / 2);
        painter.drawText(line, Qt::AlignLeft | Qt::AlignVCenter, title);
        painter.drawText(line, Qt::AlignRight | Qt::AlignVCenter, tr("Page %1").arg(pageNumber));
        painter.drawLine(0, header * 3 / 4, width, header * 3 / 4);
        painter.translate(0, header);
        Sci_RangeToFormatFull range{};
        range.hdc = device;
        range.hdcTarget = device;
        range.rc = {0, 0, width, height};
        range.rcPage = range.rc;
        range.chrg = {pos, length};
        const auto next = static_cast<Sci_Position>(
            editor->send(SCI_FORMATRANGEFULL, 1, reinterpret_cast<sptr_t>(&range)));
        painter.restore();
        if (next <= pos) {
            break;  // nothing fits: never loop
        }
        pos = next;
        ++pageNumber;
    } while (pos < length);
    editor->send(SCI_SETEDGEMODE, static_cast<uptr_t>(edge));
}

bool MainWindow::exportCurrent(const QString &kind, const QString &path) {
    FernyEditor *editor = currentEditor();
    if (!editor) {
        return false;
    }
    const QString title = documentTitle(editor);
    if (kind == QStringLiteral("pdf")) {
        // Into memory, then written atomically like a save (T9).
        QBuffer buffer;
        buffer.open(QIODevice::WriteOnly);
        {
            QPdfWriter writer(&buffer);
            writer.setPageSize(QPageSize(QPageSize::A4));
            writer.setPageMargins(QMarginsF(15, 15, 15, 15), QPageLayout::Millimeter);
            writer.setResolution(300);
            writer.setTitle(title);
            writer.setCreator(QStringLiteral("Ferny"));
            printDocument(editor, &writer, title);
        }
        return m_core.writeExport(path, buffer.data());
    }
    // Styles of every byte and the look of each style used.
    editor->send(SCI_COLOURISE, 0, -1);
    const QByteArray text = editor->utf8Text();
    QByteArray styles(text.size(), '\0');
    QSet<int> used;
    for (qsizetype i = 0; i < text.size(); ++i) {
        const auto style = static_cast<int>(editor->send(SCI_GETSTYLEAT, static_cast<uptr_t>(i)));
        styles[i] = static_cast<char>(style);
        used.insert(style);
    }
    QStringList looks;
    for (const int style : used) {
        const auto s = static_cast<uptr_t>(style);
        looks.append(QStringLiteral("%1 %2 %3 %4 %5")
                         .arg(style)
                         .arg(rgb(editor->send(SCI_STYLEGETFORE, s)))
                         .arg(rgb(editor->send(SCI_STYLEGETBACK, s)))
                         .arg(editor->send(SCI_STYLEGETBOLD, s) ? 1 : 0)
                         .arg(editor->send(SCI_STYLEGETITALIC, s) ? 1 : 0));
    }
    const auto background = static_cast<int>(rgb(editor->send(SCI_STYLEGETBACK, STYLE_DEFAULT)));
    return m_core.exportDocument(kind, path, text, styles, looks, background, title,
                                 static_cast<int>(editor->send(SCI_GETTABWIDTH)));
}

int MainWindow::autosaveNow() {
    if (m_core.settingValue(QStringLiteral("files.autosave")) != QStringLiteral("true")) {
        return 0;
    }
    int saved = 0;
    for (int index = 0; index < m_tabs->count(); ++index) {
        FernyEditor *editor = editorAt(index);
        const quint64 id = editor->documentId();
        // Never untitled or read-only documents; never asks (a file changed
        // on disk waits for the user's own save).
        if (!editor->isModified() || editor->isReadOnly() || !m_core.documentInfo(id).has_path) {
            continue;
        }
        if (m_core.saveDocument(id, editor->utf8Text(), false) == SaveStatus::Ok) {
            reportSave(editor, SaveStatus::Ok);
            ++saved;
        }
    }
    return saved;
}

SaveStatus MainWindow::saveCurrentAs(const QString &path) {
    FernyEditor *editor = currentEditor();
    return m_core.saveDocumentAs(editor->documentId(), path, editor->utf8Text());
}

bool MainWindow::reportSave(FernyEditor *editor, SaveStatus status) {
    if (status != SaveStatus::Ok) {
        const QString path = m_core.documentPath(editor->documentId());
        showMessage(path.isEmpty()
                        ? saveMessage(status)
                        : tr("%1: %2").arg(m_core.displayName(QFileInfo(path).fileName()),
                                           saveMessage(status)));
        return false;
    }
    editor->setSavePoint();
    m_core.lspSave(editor->documentId());
    if (!m_files->root().isEmpty()) {
        m_core.gitStatus(m_files->root());  // marks follow the save (H4.2d)
    }
    statusBar()->showMessage(tr("Saved."), 3000);
    updateTabLabel(editor);
    updateTitle();
    updateStatus();
    emitPluginEvent(QStringLiteral("document.saved"), editor);
    return true;
}

void MainWindow::reload(FernyEditor *editor, const QString &encoding) {
    if (!editor || !m_core.documentInfo(editor->documentId()).has_path) {
        return;
    }
    if (editor->isModified()) {
        const auto answer = QMessageBox::question(
            this, tr("Reload"), tr("Discard the unsaved changes and reload the file?"),
            QMessageBox::Discard | QMessageBox::Cancel, QMessageBox::Cancel);
        if (answer != QMessageBox::Discard) {
            return;
        }
    }
    QByteArray contents;
    const LoadStatus status = m_core.reloadDocument(editor->documentId(), encoding, contents);
    if (status != LoadStatus::Ok) {
        statusBar()->showMessage(loadMessage(status));
        return;
    }
    applyDocument(editor, contents);
}

void MainWindow::reloadWithEncoding() {
    FernyEditor *editor = currentEditor();
    if (!editor || !m_core.documentInfo(editor->documentId()).has_path) {
        return;
    }
    const QString encoding = chooseEncoding(tr("Reload With Encoding"));
    if (!encoding.isEmpty()) {
        reload(editor, encoding);
    }
}

void MainWindow::setEncoding() {
    FernyEditor *editor = currentEditor();
    const QString encoding = chooseEncoding(tr("Set Encoding"));
    if (!editor || encoding.isEmpty()) {
        return;
    }
    if (!m_core.setDocumentEncoding(editor->documentId(), encoding)) {
        statusBar()->showMessage(loadMessage(LoadStatus::UnknownEncoding));
        return;
    }
    const DocumentInfo info = m_core.documentInfo(editor->documentId());
    editor->setReadOnly(info.read_only != ReadOnlyState::Writable);
    statusBar()->showMessage(tr("The file will be saved as %1.").arg(encoding));
    updateStatus();
}

QString MainWindow::chooseEncoding(const QString &title) {
    const QStringList encodings = m_core.supportedEncodings();
    const int current = encodings.indexOf(m_core.documentEncoding(documentId()));
    bool ok = false;
    const QString choice = QInputDialog::getItem(this, title, tr("Encoding:"), encodings,
                                                 current < 0 ? 0 : current, false, &ok);
    return ok ? choice : QString();
}

bool MainWindow::maybeSaveChanges(FernyEditor *editor) {
    if (!editor || !editor->isModified()) {
        return true;
    }
    const QString path = m_core.documentPath(editor->documentId());
    const QString name = path.isEmpty() ? tr("Untitled") : QFileInfo(path).fileName();
    const auto answer = QMessageBox::warning(
        this, tr("Unsaved Changes"), tr("%1 has unsaved changes. Save them?").arg(name),
        QMessageBox::Save | QMessageBox::Discard | QMessageBox::Cancel, QMessageBox::Save);
    if (answer == QMessageBox::Save) {
        return save(editor);
    }
    return answer == QMessageBox::Discard;
}

void MainWindow::closeEvent(QCloseEvent *event) {
    for (int index = 0; index < m_tabs->count(); ++index) {
        if (editorAt(index)->isModified()) {
            m_tabs->setCurrentIndex(index);
            if (!maybeSaveChanges(editorAt(index))) {
                event->ignore();
                return;
            }
        }
    }
    saveSession();
    saveSettings();
    m_core.lspShutdownAll();
    for (int index = 0; index < m_tabs->count(); ++index) {
        m_core.closeDocument(editorAt(index)->documentId());
    }
    event->accept();
}

// --- External changes ------------------------------------------------------------

void MainWindow::changeEvent(QEvent *event) {
    QMainWindow::changeEvent(event);
    if (event->type() == QEvent::ActivationChange && isActiveWindow()) {
        checkExternalChanges();
    }
}

void MainWindow::checkExternalChanges() {
    // A dialog shown here changes the activation again: do not re-enter.
    if (m_checkingExternal) {
        return;
    }
    m_checkingExternal = true;
    for (int index = 0; index < m_tabs->count(); ++index) {
        FernyEditor *editor = editorAt(index);
        const quint64 id = editor->documentId();
        const ExternalStatus state = m_core.externalState(id);
        const QString name = QFileInfo(m_core.documentPath(id)).fileName();
        const QString policy =
            m_core.settingValue(QStringLiteral("files.reload_on_external_change"));
        if (state == ExternalStatus::Modified && policy == QStringLiteral("never")) {
            // Keep the editor's text; the next save asks before overwriting.
            statusBar()->showMessage(tr("%1 was changed by another program.").arg(name));
        } else if (state == ExternalStatus::Modified && policy == QStringLiteral("always") &&
                   !editor->isModified()) {
            QByteArray contents;
            if (m_core.reloadDocument(id, QString(), contents) == LoadStatus::Ok) {
                applyDocument(editor, contents);
            }
        } else if (state == ExternalStatus::Modified) {
            m_tabs->setCurrentIndex(index);
            const auto answer = QMessageBox::question(
                this, tr("File Changed on Disk"),
                editor->isModified()
                    ? tr("Another program changed %1. Reload it and lose your unsaved changes?")
                          .arg(name)
                    : tr("Another program changed %1. Reload it?").arg(name),
                QMessageBox::Yes | QMessageBox::No, QMessageBox::Yes);
            if (answer == QMessageBox::Yes) {
                QByteArray contents;
                const LoadStatus status = m_core.reloadDocument(id, QString(), contents);
                if (status == LoadStatus::Ok) {
                    applyDocument(editor, contents);
                } else {
                    statusBar()->showMessage(loadMessage(status));
                }
            } else {
                // Keep the editor's text; the next save overwrites the file.
                m_core.acceptExternalState(id);
            }
        } else if (state == ExternalStatus::Deleted) {
            statusBar()->showMessage(
                tr("%1 was deleted by another program. Saving creates it again.").arg(name));
        }
    }
    m_checkingExternal = false;
}

// --- Labels ---------------------------------------------------------------------

void MainWindow::updateTabLabel(FernyEditor *editor) {
    const int index = m_tabs->indexOf(editor);
    if (index < 0) {
        return;
    }
    const QString path = m_core.documentPath(editor->documentId());
    const QString name = path.isEmpty() ? tr("Untitled") : QFileInfo(path).fileName();
    m_tabs->setTabText(index, editor->isModified() ? QStringLiteral("*") + name : name);
    m_tabs->setTabToolTip(index, path);
    refreshDocuments();
}

void MainWindow::updateTitle() {
    const FernyEditor *editor = currentEditor();
    if (!editor) {
        setWindowTitle(tr("Ferny"));
        return;
    }
    const QString path = m_core.documentPath(editor->documentId());
    const QString name = path.isEmpty() ? tr("Untitled") : QFileInfo(path).fileName();
    const QString marker = editor->isModified() ? QStringLiteral("*") : QString();
    const QString project = m_core.projectName();
    setWindowTitle(project.isEmpty()
                       ? tr("%1%2 — Ferny").arg(marker, name)
                       : tr("%1%2 — %3 — Ferny").arg(marker, name, m_core.displayName(project)));
}

void MainWindow::updateStatus() {
    const quint64 id = documentId();
    const DocumentInfo info = m_core.documentInfo(id);
    QString encoding = m_core.documentEncoding(id);
    if (info.bom) {
        encoding += tr(" (BOM)");
    }
    m_encoding->setText(encoding);
    m_eol->setText(eolName(info.eol));
    const FernyEditor *editor = currentEditor();
    if (editor) {
        const QString filetype = editor->filetypeId();
        m_filetype->setText(filetype.isEmpty() ? tr("Plain Text") : m_core.filetypeName(filetype));
        m_indentation->setText(editor->indentUsesTabs()
                                   ? tr("Tabs: %1").arg(editor->indentWidth())
                                   : tr("Spaces: %1").arg(editor->indentWidth()));
    }
    QStringList state;
    if (info.read_only != ReadOnlyState::Writable) {
        state.append(tr("Read-only"));
    }
    if (editor && editor->isModified()) {
        state.append(tr("Modified"));
    }
    if (editor) {
        int errors = 0;
        int warnings = 0;
        for (const DiagnosticMark &mark : m_diagnostics.value(editor->documentId())) {
            (mark.severity == 1 ? errors : warnings) += mark.severity <= 2 ? 1 : 0;
        }
        if (errors > 0) {
            state.append(tr("%n error(s)", nullptr, errors));
        }
        if (warnings > 0) {
            state.append(tr("%n warning(s)", nullptr, warnings));
        }
    }
    m_documentState->setText(state.join(QStringLiteral(" · ")));
    m_documentState->setVisible(!state.isEmpty());
    refreshSymbols();
    // Workspace trust (H2.1): the folder of the document decides.
    const QString path = m_core.documentPath(id);
    const bool trusted = !path.isEmpty() && m_core.isTrusted(QFileInfo(path).absolutePath());
    m_trust->setText(trusted ? tr("Trusted") : tr("Restricted"));
    m_trust->setToolTip(trusted ? tr("This folder is trusted.")
                                : tr("Restricted mode: nothing from this folder is run. "
                                     "Document → Trust Folder… to trust it."));
    updateCursorStatus();
}

void MainWindow::updateCursorStatus() {
    const FernyEditor *editor = currentEditor();
    if (!editor) {
        return;
    }
    m_position->setText(tr("Line %1/%2, Col %3")
                            .arg(editor->cursorLine())
                            .arg(editor->lineCount())
                            .arg(editor->cursorColumn()));
    m_selection->setText(tr("Sel %1").arg(editor->selectedCharacters()));
}

void MainWindow::showAbout() {
    QDialog dialog(this);
    dialog.setWindowTitle(tr("About Ferny"));

    auto *layout = new QVBoxLayout(&dialog);
    auto *header = new QHBoxLayout();
    auto *logo = new QLabel(&dialog);
    logo->setPixmap(QPixmap(QStringLiteral(":/ferny-logo-removebg-preview.png"))
                        .scaled(96, 96, Qt::KeepAspectRatio, Qt::SmoothTransformation));
    header->addWidget(logo, 0, Qt::AlignTop);

    auto *title = new QLabel(&dialog);
    title->setTextFormat(Qt::PlainText);
    title->setText(tr("Ferny %1\nQt %2 · Commit %3")
                       .arg(m_core.version(), QString::fromLatin1(qVersion()),
                            QStringLiteral(FERNY_GIT_COMMIT)));
    header->addWidget(title, 1, Qt::AlignVCenter);
    layout->addLayout(header);

    // Links are whitelisted: only the project's own web and mail addresses,
    // plus the Geany and Lucide references, are handed to the desktop.
    auto *aboutText = new QTextBrowser(&dialog);
    aboutText->setOpenLinks(false);
    aboutText->setOpenExternalLinks(false);
    aboutText->setMinimumWidth(460);
    aboutText->setHtml(
        tr("A programmer's editor based on the Geany project.<br>"
           "<a href='https://www.geany.org/'>Geany</a><br><br>"
           "Author: <a href='https://www.FErArg.com/'>www.FErArg.com</a><br>"
           "Contact: <a href='mailto:Info@FErArg.com'>Info@FErArg.com</a><br><br>"
           "Toolbar icons by <a href='https://lucide.dev/'>Lucide</a> "
           "(ISC License).<br><br>"
           "Licensed under the GNU General Public License, version 2 or later."));
    connect(aboutText, &QTextBrowser::anchorClicked, this, [](const QUrl &url) {
        const bool isWeb = url.scheme() == QStringLiteral("https") && url.userInfo().isEmpty() &&
                           url.port(-1) == -1 &&
                           (url.path().isEmpty() || url.path() == QStringLiteral("/")) &&
                           url.query().isEmpty() && url.fragment().isEmpty() &&
                           (url.host() == QStringLiteral("www.geany.org") ||
                            url.host() == QStringLiteral("geany.org") ||
                            url.host() == QStringLiteral("www.ferarg.com") ||
                            url.host() == QStringLiteral("ferarg.com") ||
                            url.host() == QStringLiteral("lucide.dev"));
        const bool isContact =
            url.scheme() == QStringLiteral("mailto") && url.query().isEmpty() &&
            url.path().compare(QStringLiteral("Info@FErArg.com"), Qt::CaseInsensitive) == 0;
        if (isWeb || isContact) {
            QDesktopServices::openUrl(url);
        }
    });
    layout->addWidget(aboutText);

    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Close, &dialog);
    connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
    layout->addWidget(buttons);

    dialog.exec();
}

// --- Messages --------------------------------------------------------------------

const char *MainWindow::statusName(LoadStatus status) {
    switch (status) {
        case LoadStatus::Ok:
            return "ok";
        case LoadStatus::NotFound:
            return "not-found";
        case LoadStatus::PermissionDenied:
            return "permission-denied";
        case LoadStatus::NotAFile:
            return "not-a-file";
        case LoadStatus::TooLarge:
            return "too-large";
        case LoadStatus::Binary:
            return "binary";
        case LoadStatus::UnknownEncoding:
            return "unknown-encoding";
        case LoadStatus::UnknownDocument:
            return "unknown-document";
        case LoadStatus::IoError:
            return "io-error";
    }
    return "unknown";
}

const char *MainWindow::statusName(SaveStatus status) {
    switch (status) {
        case SaveStatus::Ok:
            return "ok";
        case SaveStatus::NoPath:
            return "no-path";
        case SaveStatus::ReadOnly:
            return "read-only";
        case SaveStatus::Unencodable:
            return "unencodable";
        case SaveStatus::ModifiedExternally:
            return "modified-externally";
        case SaveStatus::NotFound:
            return "not-found";
        case SaveStatus::PermissionDenied:
            return "permission-denied";
        case SaveStatus::NotAFile:
            return "not-a-file";
        case SaveStatus::UnknownDocument:
            return "unknown-document";
        case SaveStatus::IoError:
            return "io-error";
    }
    return "unknown";
}

QString MainWindow::loadMessage(LoadStatus status) {
    switch (status) {
        case LoadStatus::Ok:
            return {};
        case LoadStatus::NotFound:
            return tr("The file does not exist.");
        case LoadStatus::PermissionDenied:
            return tr("You do not have permission to read the file.");
        case LoadStatus::NotAFile:
            return tr("Only regular files can be opened.");
        case LoadStatus::TooLarge:
            return tr("The file is too large to open.");
        case LoadStatus::Binary:
            return tr("The file looks binary and was not opened.");
        case LoadStatus::UnknownEncoding:
            return tr("That encoding is not supported.");
        case LoadStatus::UnknownDocument:
        case LoadStatus::IoError:
            return tr("The file could not be read.");
    }
    return tr("The file could not be read.");
}

QString MainWindow::saveMessage(SaveStatus status) {
    switch (status) {
        case SaveStatus::Ok:
            return {};
        case SaveStatus::NoPath:
            return tr("Choose where to save the document.");
        case SaveStatus::ReadOnly:
            return tr("The document is read-only. Use Save As to write a copy.");
        case SaveStatus::Unencodable:
            return tr(
                "The text contains characters the encoding cannot represent. "
                "Set another encoding (for example UTF-8) and save again.");
        case SaveStatus::ModifiedExternally:
            return tr("The file was changed by another program and was not overwritten.");
        case SaveStatus::NotFound:
            return tr("The folder or link target does not exist.");
        case SaveStatus::PermissionDenied:
            return tr("You do not have permission to write the file.");
        case SaveStatus::NotAFile:
            return tr("Only regular files can be written.");
        case SaveStatus::UnknownDocument:
        case SaveStatus::IoError:
            return tr("The file could not be saved.");
    }
    return tr("The file could not be saved.");
}

}  // namespace ferny
