// Ferny entry point: opens the files given on the command line (with
// optional `file:line[:col]` / `+line` positions), forwards them to a running
// instance if there is one, or restores the last session.

#include "fernyeditor.h"
#include "filebrowser.h"
#include "findbar.h"
#include "mainwindow.h"
#include "messagewindow.h"
#include "preferences.h"
#include "singleinstance.h"

#include <QApplication>
#include <QCommandLineParser>
#include <QCoreApplication>
#include <QDir>
#include <QEvent>
#include <QEventLoop>
#include <QFileInfo>
#include <QFileOpenEvent>
#include <QIcon>
#include <QKeyEvent>
#include <QLibraryInfo>
#include <QLocale>
#include <QMenuBar>
#include <QPixmap>
#include <QStandardPaths>
#include <QStatusBar>
#include <QStringList>
#include <QTimer>
#include <QTranslator>

#include <cstdio>
#include <functional>
#include <memory>

#ifdef Q_OS_WIN
#define NOMINMAX
#include <windows.h>
#endif

#include <Scintilla.h>

namespace {

// Bytes inspected by the smoke test to confirm syntax highlighting.
constexpr qint64 kSmokeStyleProbeBytes = 64 * 1024;
// How long the IPC smoke test waits for a forwarded request.
constexpr int kSmokeIpcWaitMs = 10000;

// Translation context "main" for the command line (read by lupdate).
// Smoke-test options are for developers and stay in English.
struct Cli {
    Q_DECLARE_TR_FUNCTIONS(main)
};

// Installs the translations for `language` (system, en or es; anything
// else is treated as system), replacing earlier ones. Ferny's come only
// from the embedded resources (":/i18n"), Qt's only from Qt's own
// installation directory (H1.6b, T7).
QString installTranslations(QApplication &app, const QString &language) {
    static QTranslator ferny;
    static QTranslator qtBase;
    QApplication::removeTranslator(&ferny);
    QApplication::removeTranslator(&qtBase);
    if (language == QStringLiteral("en")) {
        return QStringLiteral("en");  // the source strings are English
    }
    const QLocale locale =
        language == QStringLiteral("es") ? QLocale(QLocale::Spanish) : QLocale::system();
    if (ferny.load(locale, QStringLiteral("ferny"), QStringLiteral("_"),
                   QStringLiteral(":/i18n"))) {
        QApplication::installTranslator(&ferny);
        if (qtBase.load(locale, QStringLiteral("qtbase"), QStringLiteral("_"),
                        QLibraryInfo::path(QLibraryInfo::TranslationsPath))) {
            QApplication::installTranslator(&qtBase);
        }
        Q_UNUSED(app);
        // The language in use, for the plugins' translations (H3.3a).
        return locale.name().section(QLatin1Char('_'), 0, 0);
    }
    Q_UNUSED(app);
    return QStringLiteral("en");
}

#ifdef Q_OS_WIN
// The executable uses the Windows GUI subsystem (ui/CMakeLists.txt), so it has
// no console of its own: started from Explorer it opens no terminal window.
// Command-line output (--help, --version, --smoke-test) goes to the parent
// console when there is one and stdout is not redirected.
void attachParentConsole() {
    const HANDLE out = GetStdHandle(STD_OUTPUT_HANDLE);
    if (out != nullptr && out != INVALID_HANDLE_VALUE) {
        return;  // inherited from a terminal, or redirected to a pipe or file
    }
    if (AttachConsole(ATTACH_PARENT_PROCESS)) {
        std::FILE *console = nullptr;
        freopen_s(&console, "CONOUT$", "w", stdout);
        freopen_s(&console, "CONOUT$", "w", stderr);
    }
}
#endif

}  // namespace

int main(int argc, char *argv[]) {
#ifdef Q_OS_WIN
    attachParentConsole();
#endif
    QApplication app(argc, argv);
    QApplication::setApplicationName(QStringLiteral("Ferny"));
    // Window, taskbar and Dock icon (embedded resource, ui/CMakeLists.txt).
    QApplication::setWindowIcon(QIcon(QStringLiteral(":/ferny-icon-app.png")));

    ferny::FernyCore core;
    QApplication::setApplicationVersion(core.version());
    // The system language for --help; the setting may change it below.
    installTranslations(app, QStringLiteral("system"));

    QCommandLineParser parser;
    parser.setApplicationDescription(Cli::tr("Ferny, a programmer's editor"));
    parser.addHelpOption();
    parser.addVersionOption();
    parser.addPositionalArgument(QStringLiteral("files"),
                                 Cli::tr("Files to open, optionally as file:line[:column] or "
                                         "+line file."),
                                 QStringLiteral("[files...]"));
    const QCommandLineOption standalone(
        QStringLiteral("standalone"),
        Cli::tr("Do not forward to or act as the single running instance."));
    const QCommandLineOption noSession(QStringLiteral("no-session"),
                                       Cli::tr("Neither restore nor save the session."));
    const QCommandLineOption sessionFile(QStringLiteral("session-file"),
                                         Cli::tr("Use <path> as the session file."),
                                         QStringLiteral("path"));
    const QCommandLineOption colorScheme(QStringLiteral("color-scheme"),
                                         Cli::tr("Use colour scheme <id> (overrides the session)."),
                                         QStringLiteral("id"));
    const QCommandLineOption configDir(
        QStringLiteral("config-dir"),
        Cli::tr(
            "Read settings.toml and keybindings.toml from <dir> (used by the automated tests)."),
        QStringLiteral("dir"));
    const QCommandLineOption smokeImportGeany(
        QStringLiteral("smoke-import-geany"),
        QStringLiteral(
            "With --smoke-test and --config-dir: import the Geany configuration in <dir> into "
            "the configuration directory, report and exit."),
        QStringLiteral("dir"));
    const QCommandLineOption smokeShortcut(
        QStringLiteral("smoke-shortcut"),
        QStringLiteral("With --smoke-test: report the shortcuts of command <id>."),
        QStringLiteral("id"));
    const QCommandLineOption smokePrefs(
        QStringLiteral("smoke-prefs"),
        QStringLiteral(
            "With --smoke-test: open Preferences, check one editor per setting, apply the "
            "unchanged values and report."));
    const QCommandLineOption ipcDir(
        QStringLiteral("ipc-dir"),
        Cli::tr("Use <dir> for the single-instance endpoint (used by the automated tests)."),
        QStringLiteral("dir"));
    const QCommandLineOption language(
        QStringLiteral("language"),
        Cli::tr("Interface language: system, en or es (overrides the setting)."),
        QStringLiteral("language"));
    const QCommandLineOption safeMode(QStringLiteral("safe-mode"),
                                      Cli::tr("Start without any plugin."));
    const QCommandLineOption smokeTest(
        QStringLiteral("smoke-test"),
        QStringLiteral(
            "Start, open the files if given, report on stdout and exit (used by the "
            "automated tests). Implies --standalone and --no-session unless --ipc-dir or "
            "--session-file are given."));
    const QCommandLineOption smokeSaveAs(
        QStringLiteral("smoke-save-as"),
        QStringLiteral("With --smoke-test: write the current document to <path>."),
        QStringLiteral("path"));
    const QCommandLineOption smokeDelay(
        QStringLiteral("smoke-delay"),
        QStringLiteral("With --smoke-test: wait <ms> before typing and running commands (a "
                       "language server indexing)."),
        QStringLiteral("ms"));
    const QCommandLineOption smokeAutosave(
        QStringLiteral("smoke-autosave"),
        QStringLiteral("With --smoke-test: run one autosave pass after the commands."));
    const QCommandLineOption smokeSelectAll(
        QStringLiteral("smoke-select-all"),
        QStringLiteral("With --smoke-test: select the whole document before --smoke-type."));
    const QCommandLineOption smokeExport(
        QStringLiteral("smoke-export"),
        QStringLiteral("With --smoke-test: export the current document to <path> (.pdf, .html, "
                       ".tex)."),
        QStringLiteral("path"));
    const QCommandLineOption smokeCommand(
        QStringLiteral("smoke-command"),
        QStringLiteral(
            "With --smoke-test: run command <id> on the current document before reporting "
            "(repeatable)."),
        QStringLiteral("id"));
    const QCommandLineOption smokeType(
        QStringLiteral("smoke-type"),
        QStringLiteral("With --smoke-test: type <text> into the current document as key presses "
                       "(\\n is Enter)."),
        QStringLiteral("text"));
    const QCommandLineOption smokeFind(
        QStringLiteral("smoke-find"),
        QStringLiteral("With --smoke-test: search pattern for find commands."),
        QStringLiteral("pattern"));
    const QCommandLineOption smokeReplace(QStringLiteral("smoke-replace"),
                                          QStringLiteral("With --smoke-test: replacement text."),
                                          QStringLiteral("text"));
    const QCommandLineOption smokeFlags(
        QStringLiteral("smoke-flags"),
        QStringLiteral("With --smoke-test: search flags (1 regex, 2 match case, 4 whole word)."),
        QStringLiteral("flags"));
    const QCommandLineOption smokeFindInFiles(
        QStringLiteral("smoke-find-in-files"),
        QStringLiteral("With --smoke-test: find --smoke-find in <dir>, report the results, exit."),
        QStringLiteral("dir"));
    const QCommandLineOption smokeBrowse(
        QStringLiteral("smoke-browse"),
        QStringLiteral(
            "With --smoke-test: show <dir> in the Files sidebar and report its entries."),
        QStringLiteral("dir"));
    const QCommandLineOption smokeScribble(
        QStringLiteral("smoke-scribble"),
        QStringLiteral(
            "With --smoke-test: put <text> in the Scribble before the session is saved."),
        QStringLiteral("text"));
    const QCommandLineOption smokeInput(
        QStringLiteral("smoke-input"),
        QStringLiteral("With --smoke-test: list pickers choose the first entry containing <text>."),
        QStringLiteral("text"));
    const QCommandLineOption smokeBuild(
        QStringLiteral("smoke-build"),
        QStringLiteral("With --smoke-test: run the build commands <list> (compile,build,lint,make,"
                       "execute) one after the other through the Build menu, report, exit."),
        QStringLiteral("list"));
    const QCommandLineOption smokeLspWait(
        QStringLiteral("smoke-lsp-wait"),
        QStringLiteral("With --smoke-test: wait for the language server's diagnostics of the "
                       "current document, report, exit."));
    const QCommandLineOption smokeWait(
        QStringLiteral("smoke-wait"),
        QStringLiteral("With --smoke-test: after the commands, wait <ms> before reporting "
                       "(answers from language servers)."),
        QStringLiteral("ms"));
    const QCommandLineOption smokeHover(
        QStringLiteral("smoke-hover"),
        QStringLiteral("With --smoke-test: ask the language server about the cursor's symbol "
                       "after the commands, and report the answer."));
    const QCommandLineOption smokeGrantPlugins(
        QStringLiteral("smoke-grant-plugins"),
        QStringLiteral(
            "With --smoke-test: enable new plugins with every permission they ask for."));
    const QCommandLineOption smokeTrust(
        QStringLiteral("smoke-trust"),
        QStringLiteral("With --smoke-test: run Document > Trust Folder... on <dir>, report, exit."),
        QStringLiteral("dir"));
    const QCommandLineOption smokeIpcWait(
        QStringLiteral("smoke-ipc-wait"),
        QStringLiteral(
            "With --smoke-test and --ipc-dir: wait for one forwarded request, report, exit."));
    for (const auto &option :
         {standalone,    noSession,        sessionFile,   colorScheme,       configDir,
          language,      smokeImportGeany, smokeShortcut, smokePrefs,        ipcDir,
          smokeTest,     smokeSaveAs,      smokeExport,   smokeCommand,      smokeType,
          smokeFind,     smokeReplace,     smokeFlags,    smokeFindInFiles,  smokeBrowse,
          smokeScribble, smokeTrust,       smokeInput,    smokeBuild,        smokeLspWait,
          smokeWait,     smokeHover,       safeMode,      smokeGrantPlugins, smokeSelectAll,
          smokeAutosave, smokeDelay,       smokeIpcWait}) {
        parser.addOption(option);
    }
    parser.process(app);

    const bool smoke = parser.isSet(smokeTest);
    core.setPendingFromArguments(parser.positionalArguments(), QDir::currentPath());

    // Single instance (R3): forward the files and exit if one is running.
    ferny::SingleInstance::Role role = ferny::SingleInstance::Role::Standalone;
    ferny::SingleInstance *single = nullptr;
    if (!parser.isSet(standalone) && (!smoke || parser.isSet(ipcDir))) {
        const QString dir =
            parser.isSet(ipcDir)
                ? parser.value(ipcDir)
                : QDir(QStandardPaths::writableLocation(QStandardPaths::RuntimeLocation))
                      .filePath(QStringLiteral("ferny"));
        single = new ferny::SingleInstance(core, dir, &app);
        role = single->start();
        if (role == ferny::SingleInstance::Role::Forwarded) {
            return 0;
        }
    }

    // Settings and keybindings before the window (menus use them). Smoke
    // tests use defaults unless --config-dir is given.
    QString settingsPath;
    QString keybindingsPath;
    QString buildPath;
    if (parser.isSet(configDir) || !smoke) {
        const QDir dir(parser.isSet(configDir)
                           ? parser.value(configDir)
                           : QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation));
        settingsPath = dir.filePath(QStringLiteral("settings.toml"));
        keybindingsPath = dir.filePath(QStringLiteral("keybindings.toml"));
#if defined(Q_OS_WIN)
        const QString system = QDir(qEnvironmentVariable("PROGRAMDATA"))
                                   .filePath(QStringLiteral("ferny/settings.toml"));
#elif defined(Q_OS_MACOS)
        const QString system;  // app bundle Resources: with packaging (H1.6)
#else
        const QString system = QStringLiteral("/etc/ferny/settings.toml");
#endif
        core.loadSettings(parser.isSet(configDir) ? QString() : system, settingsPath);
        core.loadKeybindings(keybindingsPath);
        // User filetypes and colour schemes (H1.5b).
        core.loadUserCatalog(dir.absolutePath());
        // Local log (H3.2): events only, never contents.
        core.logInit(dir.filePath(QStringLiteral("ferny.log")));
        // Backup copies (H4.2a): the user's data folder, never next to files;
        // a test configuration keeps its own.
        core.setBackupDir(parser.isSet(configDir) ? dir.filePath(QStringLiteral("backups"))
                                                  : QDir(QStandardPaths::writableLocation(
                                                             QStandardPaths::AppDataLocation))
                                                        .filePath(QStringLiteral("backups")));
        // Language servers (H3.1a).
        for (const QString &warning : core.loadLsp(dir.filePath(QStringLiteral("lsp.toml")))) {
            std::fprintf(stderr, "ferny: %s\n", warning.toUtf8().constData());
        }
        // Snippets and the user's templates (H2.5b).
        core.loadTemplates(dir.absolutePath());
        // The user's build commands (H2.4).
        for (const QString &warning :
             core.loadBuildCommands(dir.filePath(QStringLiteral("build.toml")))) {
            std::fprintf(stderr, "ferny: %s\n", warning.toUtf8().constData());
        }
        buildPath = dir.filePath(QStringLiteral("build.toml"));
        // Trusted folders (H2.1); an invalid store trusts nothing.
        if (!core.loadTrust(dir.filePath(QStringLiteral("trust.toml")))) {
            std::fprintf(stderr, "ferny: trust.toml is invalid; no folder is trusted\n");
        }
        for (const QString &warning : core.configWarnings()) {
            std::fprintf(stderr, "ferny: %s\n", warning.toUtf8().constData());
        }
    }

    // Interface language: --language, else the setting. In smoke tests
    // "system" means English, so their output does not depend on the
    // machine's language.
    QString uiLanguage = parser.isSet(language) ? parser.value(language)
                                                : core.settingValue(QStringLiteral("ui.language"));
    if (smoke && uiLanguage != QStringLiteral("es")) {
        uiLanguage = QStringLiteral("en");
    }
    const QString activeLanguage = installTranslations(app, uiLanguage);

    ferny::MainWindow window(core);
    // Smoke tests never write the user's files.
    if (!smoke) {
        window.setConfigPaths(settingsPath, keybindingsPath);
        window.setBuildPath(buildPath);
    }
    if (!settingsPath.isEmpty()) {
        window.setConfigDir(QFileInfo(settingsPath).absolutePath());
    }
    if (smoke && parser.isSet(smokeImportGeany) && parser.isSet(configDir)) {
        // Through the File menu command, as a user would (H1.5b, fixed in H1.6a).
        window.setConfigPaths(settingsPath, keybindingsPath);
        window.setSmokeImportDirectory(parser.value(smokeImportGeany));
        window.runCommand(QStringLiteral("file.import_geany"));
        const QByteArray report = window.lastImportReport().toUtf8();
        std::printf("smoke: geany %s\n", report.isEmpty() ? "not run" : report.constData());
        return report.isEmpty() ? 1 : 0;
    }
    if (smoke) {
        window.setSmokeInput(parser.value(smokeInput));
    }
    // Plugins (H3.2): only with a configuration directory.
    if (!smoke || parser.isSet(configDir)) {
        window.startPlugins(parser.isSet(safeMode), smoke && parser.isSet(smokeGrantPlugins),
                            activeLanguage);
    }
    if (smoke && parser.isSet(smokeTrust)) {
        // Through the Document menu command (H2.1).
        window.setSmokeTrustFolder(parser.value(smokeTrust));
        window.runCommand(QStringLiteral("document.trust_folder"));
        const bool trusted = core.isTrusted(parser.value(smokeTrust));
        std::printf("smoke: trust %s folders=%d\n", trusted ? "trusted" : "refused",
                    static_cast<int>(core.trustedFolders().size()));
        return trusted ? 0 : 1;
    }
    if (parser.isSet(sessionFile)) {
        window.setSessionPath(parser.value(sessionFile));
    } else if (!smoke && !parser.isSet(noSession)) {
        window.setSessionPath(
            QDir(QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation))
                .filePath(QStringLiteral("session.toml")));
    }

    // Session settings (recent files, geometry, colour scheme) always;
    // its files only when none were given on the command line.
    window.loadSession();
    if (parser.isSet(colorScheme)) {
        window.setColorScheme(parser.value(colorScheme));
    }
    // The project open at the last quit comes back (H2.5a); its files are
    // its own session. Files given on the command line open as well.
    const bool restore =
        core.settingValue(QStringLiteral("files.restore_session")) == QStringLiteral("true");
    const QStringList files = core.pendingPaths();
    QList<ferny::CursorPos> cursors;
    for (int i = 0; i < files.size(); ++i) {
        cursors.append(core.pendingCursor(i));
    }
    if (restore && !core.sessionProject().isEmpty()) {
        core.clearPending();  // the project's session uses the pending list
        window.openProject(core.sessionProject(), true);
    }
    ferny::LoadStatus status = ferny::LoadStatus::Ok;
    if (!files.isEmpty()) {
        for (int i = 0; i < files.size(); ++i) {
            const ferny::CursorPos cursor = cursors.at(i);
            const ferny::LoadStatus opened =
                window.openFile(files.at(i), QString(), cursor.line, cursor.column);
            if (i == 0) {
                status = opened;
            }
        }
        core.clearPending();
    } else if (restore && core.projectRoot().isEmpty()) {
        window.restoreSessionFiles();
    }

    if (role == ferny::SingleInstance::Role::Primary) {
        QObject::connect(single, &ferny::SingleInstance::openRequested, &window, [&window]() {
            window.openPending();
            window.raise();
            window.activateWindow();
        });
    }

// macOS: handle files opened by double-click (QFileOpenEvent)
#if defined(Q_OS_MACOS)
    class OpenFileFilter : public QObject {
    public:
        explicit OpenFileFilter(ferny::MainWindow *w) : QObject(w), m_window(w) {}
        bool eventFilter(QObject *, QEvent *e) override {
            if (e->type() == QEvent::FileOpen) {
                auto *fe = static_cast<QFileOpenEvent *>(e);
                if (!fe->file().isEmpty()) {
                    m_window->openFile(fe->file());
                    return true;
                }
            }
            return false;
        }

    private:
        ferny::MainWindow *m_window;
    };
    auto *openFileFilter = new OpenFileFilter(&window);
    qApp->installEventFilter(openFileFilter);
#endif

    window.show();

    if (smoke) {
        window.findBar()->setPattern(parser.value(smokeFind));
        window.findBar()->setReplacement(parser.value(smokeReplace));
        window.findBar()->setFlags(parser.value(smokeFlags).toUInt());
    }
    if (smoke && parser.isSet(smokeFindInFiles)) {
        QObject::connect(&core, &ferny::FernyCore::findInFilesFinished, &app,
                         [](int hits, bool truncated, bool, int searched, int skipped) {
                             std::printf("smoke: fif hits=%d truncated=%s searched=%d skipped=%d\n",
                                         hits, truncated ? "yes" : "no", searched, skipped);
                             std::fflush(stdout);
                             QApplication::exit(0);
                         });
        if (!window.startFindInFiles(parser.value(smokeFindInFiles), parser.value(smokeFind),
                                     parser.value(smokeFlags).toUInt(), QString())) {
            std::printf("smoke: fif invalid pattern\n");
            return 1;
        }
        QTimer::singleShot(kSmokeIpcWaitMs, &app, []() {
            std::printf("smoke: fif timeout\n");
            QApplication::exit(1);
        });
    } else if (smoke && parser.isSet(smokeLspWait)) {
        auto report = [&window]() {
            const QByteArray entries = window.diagnosticEntries().join(QLatin1Char('|')).toUtf8();
            std::printf("smoke: lsp status=%d diagnostics=%s\n", window.lspStatus(),
                        entries.isEmpty() ? "none" : entries.constData());
            std::fflush(stdout);
        };
        QObject::connect(&core, &ferny::FernyCore::lspDiagnosticsChanged, &app, [report]() {
            // Let MainWindow apply them first (connected earlier).
            QTimer::singleShot(0, [report]() {
                report();
                QApplication::exit(0);
            });
        });
        QTimer::singleShot(0, &app, [&window, report]() {
            if (window.lspStatus() != 0) {
                report();  // nothing will come: no server for this document
                QApplication::exit(0);
            }
        });
        QTimer::singleShot(kSmokeIpcWaitMs * 3, &app, [report]() {
            report();
            std::printf("smoke: lsp timeout\n");
            QApplication::exit(1);
        });
    } else if (smoke && parser.isSet(smokeBuild)) {
        // Build commands through the menu command, each after the previous
        // one finished (H2.4).
        auto pending = std::make_shared<QStringList>(
            parser.value(smokeBuild).split(QLatin1Char(','), Qt::SkipEmptyParts));
        auto current = std::make_shared<QString>();
        auto finish = [&window]() {
            const QStringList lines = window.compilerLines();
            for (const QString &line : lines) {
                std::printf("smoke: compiler %s\n", line.toUtf8().constData());
            }
            std::fflush(stdout);
            QApplication::exit(0);
        };
        auto next = std::make_shared<std::function<void()>>();
        *next = [&window, pending, current, finish, next]() {
            while (!pending->isEmpty()) {
                *current = pending->takeFirst();
                window.runCommand(QStringLiteral("build.") + *current);
                if (window.buildRunning()) {
                    return;  // continues on buildFinished
                }
                std::printf("smoke: build %s not started\n", current->toUtf8().constData());
            }
            finish();
        };
        QObject::connect(
            &window, &ferny::MainWindow::buildFinished, &app, [current, next](int exitCode) {
                std::printf("smoke: build %s exit=%d\n", current->toUtf8().constData(), exitCode);
                QTimer::singleShot(0, *next);
            });
        QTimer::singleShot(0, &app, [next]() { (*next)(); });
        QTimer::singleShot(kSmokeIpcWaitMs * 6, &app, []() {
            std::printf("smoke: build timeout\n");
            std::fflush(stdout);
            QApplication::exit(1);
        });
    } else if (smoke && parser.isSet(smokeIpcWait)) {
        if (role != ferny::SingleInstance::Role::Primary) {
            std::printf("smoke: ipc not primary\n");
            return 1;
        }
        std::printf("smoke: ipc listening\n");
        std::fflush(stdout);
        QObject::connect(single, &ferny::SingleInstance::openRequested, &app, [&window]() {
            // Runs after the window opened the files (connected earlier).
            const ferny::FernyEditor *editor = window.currentEditor();
            std::printf("smoke: ipc tabs=%d cursor=%u:%u\n", window.tabCount(),
                        editor->cursorLine(), editor->cursorColumn());
            std::fflush(stdout);
            QApplication::exit(0);
        });
        QTimer::singleShot(kSmokeIpcWaitMs, &app, []() {
            std::printf("smoke: ipc timeout\n");
            std::fflush(stdout);
            QApplication::exit(1);
        });
    } else if (smoke) {
        // Report once the event loop runs (window shown, lexing done), then
        // exit. Output names only statuses, encodings and the lexer, never
        // contents or paths.
        const QString saveAsPath = parser.value(smokeSaveAs);
        const QStringList commands = parser.values(smokeCommand);
        const QString typed = parser.value(smokeType);
        QTimer::singleShot(0, &app, [&, saveAsPath, commands, typed]() {
            if (parser.isSet(smokeDelay)) {
                QEventLoop loop;
                QTimer::singleShot(parser.value(smokeDelay).toInt(), &loop, &QEventLoop::quit);
                loop.exec();
            }
            // The Files root first, so its Git status can arrive (H4.2d).
            const bool browseListed = parser.isSet(smokeBrowse) &&
                                      window.fileBrowser()->setRoot(parser.value(smokeBrowse));
            // Real key events, so auto-close and auto-indent run as typed.
            ferny::FernyEditor *target = window.currentEditor();
            if (parser.isSet(smokeSelectAll)) {
                target->send(SCI_SELECTALL);
            }
            const QString keys = QString(typed)
                                     .replace(QStringLiteral("\\n"), QStringLiteral("\n"))
                                     .replace(QStringLiteral("\\t"), QStringLiteral("\t"));
            for (const QChar ch : keys) {
                const bool enter = ch == QLatin1Char('\n');
                const bool tab = ch == QLatin1Char('\t');
                const int key = enter ? Qt::Key_Return
                                : tab ? Qt::Key_Tab
                                      : static_cast<int>(ch.toUpper().unicode());
                const QString text = enter ? QStringLiteral("\r") : QString(ch);
                QKeyEvent press(QEvent::KeyPress, key, Qt::NoModifier, text);
                QApplication::sendEvent(target, &press);
                QKeyEvent release(QEvent::KeyRelease, key, Qt::NoModifier, text);
                QApplication::sendEvent(target, &release);
            }
            for (const QString &command : commands) {
                if (!window.runCommand(command)) {
                    std::printf("smoke: unknown command\n");
                    QApplication::exit(1);
                    return;
                }
            }
            if (parser.isSet(smokeHover)) {
                window.requestHoverAtCursor();
            }
            if (parser.isSet(smokeAutosave)) {
                std::printf("smoke: autosaved %d\n", window.autosaveNow());
            }
            if (parser.isSet(smokeWait)) {
                // Language servers answer asynchronously.
                QEventLoop loop;
                QTimer::singleShot(parser.value(smokeWait).toInt(), &loop, &QEventLoop::quit);
                loop.exec();
            }
            const ferny::FernyEditor *editor = window.currentEditor();
            const long long length = static_cast<long long>(editor->length());
            if (status != ferny::LoadStatus::Ok) {
                std::printf("smoke: failed status=%s length=%lld\n",
                            ferny::MainWindow::statusName(status), length);
                QApplication::exit(1);
                return;
            }
            if (files.isEmpty()) {
                // Embedded resources: application icon and About logo.
                const bool icon = !QApplication::windowIcon().isNull();
                const bool logo =
                    !QPixmap(QStringLiteral(":/ferny-logo-removebg-preview.png")).isNull();
                const QByteArray menuCommands =
                    window.menuCommandIds().join(QLatin1Char(',')).toUtf8();
                const QByteArray toolbarCommands =
                    window.toolbarCommandIds().join(QLatin1Char(',')).toUtf8();
                const bool toolbarIcons = window.toolbarIconOnly();
                const bool toolbarResources = window.toolbarIconsAvailable();
                const QByteArray toolbar =
                    toolbarCommands + " icons=" +
                    (toolbarIcons && toolbarResources ? QByteArray("yes") : QByteArray("no"));
                std::printf("smoke: toolbar=%s\n", toolbar.constData());
                std::printf("smoke: ok icon=%s logo=%s tabs=%d commands=%s\n", icon ? "yes" : "no",
                            logo ? "yes" : "no", window.tabCount(), menuCommands.constData());
            } else {
                const quint64 id = window.documentId();
                const ferny::DocumentInfo info = core.documentInfo(id);
                const QByteArray lexer = editor->lexerName().toUtf8();
                const QByteArray filetype = editor->filetypeId().toUtf8();
                const QByteArray scheme = window.colorScheme().toUtf8();
                const QByteArray encoding = core.documentEncoding(id).toUtf8();
                const bool styled = editor->hasStyledText(kSmokeStyleProbeBytes);
                const char *eol = info.eol == ferny::EolMode::CrLf ? "CRLF"
                                  : info.eol == ferny::EolMode::Cr ? "CR"
                                                                   : "LF";
                std::printf(
                    "smoke: ok lexer=%s styled=%s length=%lld encoding=%s bom=%s eol=%s "
                    "readonly=%s tabs=%d cursor=%u:%u filetype=%s scheme=%s back=%06x bidi=%d "
                    "indent=%d selection=%lld-%lld margin=%lld edge=%lld\n",
                    lexer.isEmpty() ? "none" : lexer.constData(), styled ? "yes" : "no", length,
                    encoding.constData(), info.bom ? "yes" : "no", eol,
                    info.read_only == ferny::ReadOnlyState::Writable ? "no" : "yes",
                    window.tabCount(), editor->cursorLine(), editor->cursorColumn(),
                    filetype.isEmpty() ? "none" : filetype.constData(), scheme.constData(),
                    static_cast<unsigned>(editor->defaultBackground()),
                    core.countBidiControls(editor->utf8Text()),
                    static_cast<int>(editor->send(SCI_GETINDENT)),
                    static_cast<long long>(editor->send(SCI_GETSELECTIONSTART)),
                    static_cast<long long>(editor->send(SCI_GETSELECTIONEND)),
                    static_cast<long long>(editor->send(SCI_GETMARGINWIDTHN, 0)),
                    static_cast<long long>(editor->send(SCI_GETEDGECOLUMN)));
                if (parser.isSet(smokeExport)) {
                    const QString path = parser.value(smokeExport);
                    const QString kind =
                        path.endsWith(QStringLiteral(".pdf"))   ? QStringLiteral("pdf")
                        : path.endsWith(QStringLiteral(".tex")) ? QStringLiteral("latex")
                                                                : QStringLiteral("html");
                    std::printf("smoke: export %s\n",
                                window.exportCurrent(kind, path) ? "ok" : "failed");
                }
                if (!saveAsPath.isEmpty()) {
                    const ferny::SaveStatus saved = window.saveCurrentAs(saveAsPath);
                    std::printf("smoke: save status=%s\n", ferny::MainWindow::statusName(saved));
                    if (saved != ferny::SaveStatus::Ok) {
                        std::fflush(stdout);
                        QApplication::exit(1);
                        return;
                    }
                }
            }
            const QByteArray toolbarCommands =
                window.toolbarCommandIds().join(QLatin1Char(',')).toUtf8();
            const bool toolbarIcons = window.toolbarIconOnly();
            const bool toolbarResources = window.toolbarIconsAvailable();
            const QByteArray toolbar =
                toolbarCommands + " icons=" +
                (toolbarIcons && toolbarResources ? QByteArray("yes") : QByteArray("no"));
            std::printf("smoke: toolbar=%s\n", toolbar.constData());
            if (parser.isSet(smokeShortcut)) {
                const QByteArray shortcutKeys = core.commandShortcuts(parser.value(smokeShortcut))
                                                    .join(QLatin1Char(','))
                                                    .toUtf8();
                std::printf("smoke: shortcut %s\n",
                            shortcutKeys.isEmpty() ? "none" : shortcutKeys.constData());
            }
            if (parser.isSet(smokePrefs)) {
                // H5.2: every descriptor has exactly one editor, the approved
                // category layout is in place and the unchanged values pass
                // core validation when applied.
                ferny::PreferencesDialog dialog(core);
                QStringList expected = core.settingKeys();
                QStringList actual = dialog.editedKeys();
                expected.sort();
                actual.sort();
                const QStringList rejected = dialog.apply();
                const QByteArray pages = dialog.pageTitles().join(QLatin1Char('|')).toUtf8();
                const bool layoutOk = pages == "General|Editor|Files|Templates|Security";
                const bool ok =
                    actual == expected && !actual.isEmpty() && rejected.isEmpty() && layoutOk;
                std::printf("smoke: prefs coverage=%s rejected=%d pages=%s\n", ok ? "ok" : "failed",
                            static_cast<int>(rejected.size()), pages.constData());
            }
            // The current document, whatever opened it (H2.5b templates).
            std::printf(
                "smoke: usages %d calltip=%s autoc=%s hover=%s\n", window.usageCount(),
                editor->send(SCI_CALLTIPACTIVE) != 0 ? "yes" : "no",
                editor->send(SCI_AUTOCACTIVE) != 0 ? "yes" : "no",
                window.lastHover().isEmpty()
                    ? "none"
                    : window.lastHover().section(QLatin1Char('\n'), 0, 0).toUtf8().constData());
            std::printf(
                "smoke: current file=%s filetype=%s length=%lld modified=%s\n",
                core.documentPath(editor->documentId())
                    .section(QLatin1Char('/'), -1)
                    .toUtf8()
                    .constData(),
                editor->filetypeId().isEmpty() ? "none" : editor->filetypeId().toUtf8().constData(),
                static_cast<long long>(editor->length()), editor->isModified() ? "yes" : "no");
            const QByteArray plugins = window.pluginSummary().join(QLatin1Char(',')).toUtf8();
            std::printf("smoke: plugins %s\n", plugins.isEmpty() ? "none" : plugins.constData());
            std::printf("smoke: split %s\n", window.splitSummary().toUtf8().constData());
            std::printf("smoke: spelling %d\n", window.spellingCount());
            const QByteArray preview = window.previewText().simplified().left(300).toUtf8();
            std::printf("smoke: preview %s\n", preview.constData());
            const QByteArray message = window.statusBar()->currentMessage().toUtf8();
            std::printf("smoke: message %s\n", message.constData());
            // Menus in the interface language (H1.6b).
            QStringList menus;
            for (const QAction *menu : window.menuBar()->actions()) {
                menus.append(menu->text());
            }
            const QByteArray menuTitles = menus.join(QLatin1Char('|')).toUtf8();
            std::printf("smoke: menus %s\n", menuTitles.constData());
            if (!core.projectRoot().isEmpty()) {
                std::printf("smoke: project %s patterns=%s\n",
                            core.projectName().toUtf8().constData(),
                            core.projectPatterns().join(QLatin1Char(',')).toUtf8().constData());
            }
            // Status bar and sidebar (H1.6a). Scintilla reports cursor and
            // selection changes when it next updates the view.
            QApplication::processEvents();
            const QByteArray fields = window.statusFields().join(QLatin1Char('|')).toUtf8();
            std::printf("smoke: status %s\n", fields.constData());
            const QByteArray documents = window.documentNames().join(QLatin1Char('|')).toUtf8();
            std::printf("smoke: documents %s\n", documents.constData());
            const QByteArray symbolList = window.symbolEntries().join(QLatin1Char('|')).toUtf8();
            std::printf("smoke: symbols %s\n", symbolList.constData());
            std::printf("smoke: symbols-source %s\n", window.symbolsSource().toUtf8().constData());
            if (parser.isSet(smokeBrowse)) {
                const QByteArray names =
                    window.fileBrowser()->topLevelNames().join(QLatin1Char('|')).toUtf8();
                std::printf("smoke: browse %s %s\n", browseListed ? "ok" : "failed",
                            names.constData());
                const QByteArray git =
                    window.fileBrowser()->topLevelStatus().join(QLatin1Char(',')).toUtf8();
                std::printf("smoke: git %s\n", git.constData());
            }
            if (parser.isSet(sessionFile)) {
                // Byte count only: the Scribble's text stays out of the output.
                std::printf(
                    "smoke: scribble loaded=%lld\n",
                    static_cast<long long>(window.messageWindow()->scribble().toUtf8().size()));
                if (parser.isSet(smokeScribble)) {
                    window.messageWindow()->setScribble(parser.value(smokeScribble));
                }
                std::printf("smoke: session %s\n", window.saveSession() ? "saved" : "failed");
            }
            std::fflush(stdout);
            QApplication::exit(0);
        });
    }

    return QApplication::exec();
}
