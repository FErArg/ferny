//! The `FernyCore` QObject.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QByteArray, QString, QStringList};
use ferny_build::{self as build, Placeholders, UserCommands};
use ferny_config::geany::{self, ImportPlan};
use ferny_config::keybindings::Keybindings;
use ferny_config::project::{self as project, GeanyProjectPlan, Project, ProjectError};
use ferny_config::settings::{self as config, Kind, Settings};
use ferny_config::templates::{self as templates, Comment, Snippets, Values};
use ferny_core::browse::{self, EntryKind, Listing};
use ferny_core::command::{self, Dispatch, MenuItem, Registry};
use ferny_core::document::{DocumentId, Documents, OpenError, ReadOnlyReason, SaveError};
use ferny_core::editing::{self, CommentTokens};
use ferny_core::editorconfig::{self, EditorConfig, IndentStyle};
use ferny_core::fsio::{self, ExternalState};
use ferny_core::ipc::{self, EndpointState};
use ferny_core::location::{self, Location};
use ferny_core::search::{self as core_search, FileSearch, Query, SearchError};
use ferny_core::session::{self, MAX_CLOSED_TABS, MAX_SCRIBBLE_BYTES, RecentList, Session};
use ferny_core::text::{self, Eol};
use ferny_core::trust::{TrustError, TrustStore};
use ferny_lsp::client::{Event as LspEvent, Server as LspServer};
use ferny_lsp::config::{self as lsp_config, ServerConfig};
use ferny_plugin::{
    Action, Answer, DocInfo, Grants, Manifest, Plugin, PluginError, ProcessOutcome, Source,
};
use ferny_syntax::symbols::{self as syntax_symbols, Symbol};
use ferny_syntax::{Catalog, DEFAULT_SCHEME, ResolvedStyle, Style};

// SAFETY: the `unsafe extern "C++"` block only declares types provided by
// cxx-qt-lib (`QString`, `QByteArray`, `QStringList`) with the layout
// cxx-qt-lib defines for them; cxx and cxx-qt generate and check the FFI
// glue. No other unsafe code is written by hand in this module
// (`unsafe_code` is allowed for this module only, in `lib.rs`).
/// FFI declarations shared with C++ (generated into `core_object.cxxqt.h`).
#[cxx_qt::bridge]
pub mod qobject {
    /// Result of opening or reloading a document, mapped to translated
    /// messages by the UI.
    #[repr(i32)]
    #[namespace = "ferny"]
    enum LoadStatus {
        /// The document was read; the output buffer holds its UTF-8 text.
        Ok,
        /// The path does not exist.
        NotFound,
        /// Access was refused.
        PermissionDenied,
        /// The path is not a regular file.
        NotAFile,
        /// The file exceeds the size limit.
        TooLarge,
        /// The contents look binary.
        Binary,
        /// The requested encoding is unknown.
        UnknownEncoding,
        /// No document has this id.
        UnknownDocument,
        /// Any other I/O error.
        IoError,
    }

    /// Result of saving a document.
    #[repr(i32)]
    #[namespace = "ferny"]
    enum SaveStatus {
        /// Saved.
        Ok,
        /// New document: ask for a path ("save as").
        NoPath,
        /// The document is read-only.
        ReadOnly,
        /// The encoding cannot represent the text.
        Unencodable,
        /// Changed on disk by another program; save again with `force`.
        ModifiedExternally,
        /// The target is missing.
        NotFound,
        /// The target or its directory is not writable.
        PermissionDenied,
        /// The target is a directory or special file.
        NotAFile,
        /// No document has this id.
        UnknownDocument,
        /// Any other I/O error.
        IoError,
    }

    /// State of a document's file on disk.
    #[repr(i32)]
    #[namespace = "ferny"]
    enum ExternalStatus {
        /// Not changed by another program.
        Unchanged,
        /// Changed or replaced by another program.
        Modified,
        /// Deleted.
        Deleted,
    }

    /// Line ending of a document (Scintilla's `SC_EOL_*` values).
    #[repr(i32)]
    #[namespace = "ferny"]
    enum EolMode {
        /// `\r\n`.
        CrLf = 0,
        /// `\r`.
        Cr = 1,
        /// `\n`.
        Lf = 2,
    }

    /// Why a document is read-only.
    #[repr(i32)]
    #[namespace = "ferny"]
    enum ReadOnlyState {
        /// Writable.
        Writable,
        /// The file has no write permission.
        Permissions,
        /// Malformed bytes were replaced while decoding.
        MalformedEncoding,
    }

    /// Facts about an open document.
    #[namespace = "ferny"]
    struct DocumentInfo {
        /// Line ending for new lines.
        eol: EolMode,
        /// The file mixes line endings.
        mixed_eol: bool,
        /// The file has a byte-order mark.
        bom: bool,
        /// Read-only state.
        read_only: ReadOnlyState,
        /// Larger than the threshold for heavy features.
        large: bool,
        /// The document has a file (false for new documents).
        has_path: bool,
    }

    /// A cursor position (1-based; 0 means "not set").
    #[namespace = "ferny"]
    struct CursorPos {
        /// Line.
        line: u32,
        /// Column.
        column: u32,
    }

    /// A resolved style (colours as 0xRRGGBB).
    #[namespace = "ferny"]
    struct StyleSpec {
        /// Lexilla style number (or 0 for named styles).
        style: u32,
        /// Foreground colour.
        fore: u32,
        /// Background colour.
        back: u32,
        /// Bold.
        bold: bool,
        /// Italic.
        italic: bool,
        /// Extends to the end of the line.
        fill_eol: bool,
        /// False if the requested style does not exist.
        found: bool,
    }

    /// Indentation defaults of a filetype.
    #[namespace = "ferny"]
    struct IndentSpec {
        /// Columns per indentation level (-1 = not specified).
        width: i32,
        /// 1 = tabs, 0 = spaces, -1 = not specified.
        use_tabs: i32,
    }

    /// `.editorconfig` properties of a document (-1 = not specified).
    #[namespace = "ferny"]
    struct EditorConfigSpec {
        /// 0 = spaces, 1 = tabs.
        indent_style: i32,
        /// Columns per indentation level.
        indent_size: i32,
        /// Columns per tab.
        tab_width: i32,
    }

    /// Result of a search operation.
    #[repr(i32)]
    #[namespace = "ferny"]
    enum SearchStatus {
        /// A match was found / the operation ran.
        Found,
        /// No match.
        NotFound,
        /// The regular expression is invalid or too large.
        InvalidPattern,
        /// The pattern is empty or too long.
        BadLength,
    }

    /// What a Geany import would do.
    #[namespace = "ferny"]
    struct GeanyImportSummary {
        /// Settings that would be set.
        settings: i32,
        /// Shortcuts that would be set.
        keybindings: i32,
        /// Filetype and colour-scheme files that would be written.
        files: i32,
        /// Entries not imported (listed by `geanyImportNotImported`).
        not_imported: i32,
    }

    /// Result of preparing a build command (H2.4).
    #[repr(i32)]
    #[namespace = "ferny"]
    enum BuildStatus {
        /// Ready: program, arguments and folder are set.
        Ready,
        /// No command of that kind for this filetype.
        NoCommand,
        /// The template is invalid or uses shell syntax.
        Invalid,
        /// The program was not found (or only in the project's folder).
        NotFound,
        /// The folder is not trusted and the command reads project content.
        Untrusted,
    }

    /// Result of trusting a folder (H2.1).
    #[repr(i32)]
    #[namespace = "ferny"]
    enum TrustStatus {
        /// Trusted (and saved, when there is a store file).
        Trusted,
        /// Missing or not a folder.
        NotAFolder,
        /// The file-system root is never trusted.
        Root,
        /// Too many trusted folders.
        Full,
        /// The store could not be saved; nothing changed.
        Failed,
    }

    /// Kind of an entry of the Files sidebar.
    #[repr(i32)]
    #[namespace = "ferny"]
    enum BrowseKind {
        /// A directory.
        Directory,
        /// A file.
        File,
        /// A link to a directory (never entered).
        DirectoryLink,
        /// A link to a file, or a broken link.
        FileLink,
    }

    /// State of the single-instance endpoint.
    #[repr(i32)]
    #[namespace = "ferny"]
    enum EndpointStatus {
        /// Nothing there: listen.
        Absent,
        /// Our socket: connect, or remove it if stale.
        Ours,
        /// Not ours or not a socket: run a separate instance.
        Foreign,
        /// The directory is unsafe or the path too long: run separately.
        Unusable,
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt string.
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qbytearray.h");
        /// Qt byte array.
        type QByteArray = cxx_qt_lib::QByteArray;

        include!("cxx-qt-lib/qstringlist.h");
        /// Qt string list.
        type QStringList = cxx_qt_lib::QStringList;
    }

    extern "RustQt" {
        /// Entry point of the Rust core for the Qt UI.
        #[qobject]
        #[namespace = "ferny"]
        type FernyCore = super::FernyCoreRust;

        /// Ferny's version string.
        #[qinvokable]
        fn version(self: &FernyCore) -> QString;

        /// Creates an empty, untitled document and returns its id.
        #[qinvokable]
        #[cxx_name = "newDocument"]
        fn new_document(self: Pin<&mut FernyCore>) -> u64;

        /// Opens `path`; `encoding` empty means detect. On `Ok`, `id` and
        /// `contents` (UTF-8) are set; otherwise `contents` is cleared.
        #[qinvokable]
        #[cxx_name = "openDocument"]
        fn open_document(
            self: Pin<&mut FernyCore>,
            path: &QString,
            encoding: &QString,
            id: &mut u64,
            contents: &mut QByteArray,
        ) -> LoadStatus;

        /// Reads a document's file again; `encoding` empty means detect.
        #[qinvokable]
        #[cxx_name = "reloadDocument"]
        fn reload_document(
            self: Pin<&mut FernyCore>,
            id: u64,
            encoding: &QString,
            contents: &mut QByteArray,
        ) -> LoadStatus;

        /// Saves UTF-8 `contents` to the document's file.
        #[qinvokable]
        #[cxx_name = "saveDocument"]
        fn save_document(
            self: Pin<&mut FernyCore>,
            id: u64,
            contents: &QByteArray,
            force: bool,
        ) -> SaveStatus;

        /// HTML preview of Markdown `utf8` (H4.2b): raw HTML as text, no
        /// images, safe links only; empty if too large.
        #[qinvokable]
        #[cxx_name = "markdownHtml"]
        fn markdown_html(self: &FernyCore, utf8: &QByteArray) -> QString;

        /// Loads the spelling dictionary `language` from the first of
        /// `folders` that has it, plus the words of `personal` (H4.2c):
        /// 0 ready, 1 invalid language, 2 not found, 3 unusable.
        #[qinvokable]
        #[cxx_name = "spellLoad"]
        fn spell_load(
            self: Pin<&mut FernyCore>,
            language: &QString,
            folders: &QStringList,
            personal: &QString,
        ) -> i32;

        /// Misspelled words of `utf8` as "start end" byte ranges (none
        /// without a dictionary).
        #[qinvokable]
        #[cxx_name = "spellMisspelled"]
        fn spell_misspelled(self: &FernyCore, utf8: &QByteArray) -> QStringList;

        /// Corrections for `word`.
        #[qinvokable]
        #[cxx_name = "spellSuggest"]
        fn spell_suggest(self: &FernyCore, word: &QString) -> QStringList;

        /// Adds `word` to the personal list; false if refused.
        #[qinvokable]
        #[cxx_name = "spellAdd"]
        fn spell_add(self: Pin<&mut FernyCore>, word: &QString) -> bool;

        /// Asks git for the status of the repository holding `dir`, in the
        /// background (H4.2d); false when the folder is not trusted or git
        /// is not found (nothing runs then).
        #[qinvokable]
        #[cxx_name = "gitStatus"]
        fn git_status(self: Pin<&mut FernyCore>, dir: &QString) -> bool;

        /// Asks git for the blame of the file `path`, in the background;
        /// false when its folder is not trusted or git is not found.
        #[qinvokable]
        #[cxx_name = "gitBlame"]
        fn git_blame(self: Pin<&mut FernyCore>, path: &QString) -> bool;

        /// Folder for backup copies before saving (H4.2a; empty = none).
        #[qinvokable]
        #[cxx_name = "setBackupDir"]
        fn set_backup_dir(self: Pin<&mut FernyCore>, path: &QString);

        /// Saves UTF-8 `contents` to `path` and makes it the document's file.
        #[qinvokable]
        #[cxx_name = "saveDocumentAs"]
        fn save_document_as(
            self: Pin<&mut FernyCore>,
            id: u64,
            path: &QString,
            contents: &QByteArray,
        ) -> SaveStatus;

        /// Changes the encoding used for the next save. False if unknown.
        #[qinvokable]
        #[cxx_name = "setDocumentEncoding"]
        fn set_document_encoding(self: Pin<&mut FernyCore>, id: u64, encoding: &QString) -> bool;

        /// Facts about a document (defaults for unknown ids).
        #[qinvokable]
        #[cxx_name = "documentInfo"]
        fn document_info(self: &FernyCore, id: u64) -> DocumentInfo;

        /// Name of the document's encoding (empty for unknown ids).
        #[qinvokable]
        #[cxx_name = "documentEncoding"]
        fn document_encoding(self: &FernyCore, id: u64) -> QString;

        /// The document's file (empty for new documents).
        #[qinvokable]
        #[cxx_name = "documentPath"]
        fn document_path(self: &FernyCore, id: u64) -> QString;

        /// Whether the document's file changed on disk.
        #[qinvokable]
        #[cxx_name = "externalState"]
        fn external_state(self: &FernyCore, id: u64) -> ExternalStatus;

        /// Accepts the file as it is now on disk (user kept the editor text).
        #[qinvokable]
        #[cxx_name = "acceptExternalState"]
        fn accept_external_state(self: Pin<&mut FernyCore>, id: u64);

        /// Forgets a document.
        #[qinvokable]
        #[cxx_name = "closeDocument"]
        fn close_document(self: Pin<&mut FernyCore>, id: u64);

        /// Encodings the user can choose.
        #[qinvokable]
        #[cxx_name = "supportedEncodings"]
        fn supported_encodings(self: &FernyCore) -> QStringList;

        /// The open document whose file is `path` (canonical comparison),
        /// or 0.
        #[qinvokable]
        #[cxx_name = "findDocument"]
        fn find_document(self: &FernyCore, path: &QString) -> u64;

        /// Interprets command-line arguments (`file`, `file:line[:col]`,
        /// `+line`) relative to `cwd` as the pending files to open.
        #[qinvokable]
        #[cxx_name = "setPendingFromArguments"]
        fn set_pending_from_arguments(self: Pin<&mut FernyCore>, args: &QStringList, cwd: &QString);

        /// Files waiting to be opened (from arguments, IPC or the session).
        #[qinvokable]
        #[cxx_name = "pendingPaths"]
        fn pending_paths(self: &FernyCore) -> QStringList;

        /// Cursor of a pending file.
        #[qinvokable]
        #[cxx_name = "pendingCursor"]
        fn pending_cursor(self: &FernyCore, index: i32) -> CursorPos;

        /// Forgets the pending files.
        #[qinvokable]
        #[cxx_name = "clearPending"]
        fn clear_pending(self: Pin<&mut FernyCore>);

        /// The pending files as a single-instance request (empty on error).
        #[qinvokable]
        #[cxx_name = "encodeOpenRequest"]
        fn encode_open_request(self: &FernyCore) -> QByteArray;

        /// Validates a request from another instance and makes its files
        /// the pending ones. False (and nothing changed) if invalid.
        #[qinvokable]
        #[cxx_name = "acceptOpenRequest"]
        fn accept_open_request(self: Pin<&mut FernyCore>, request: &QByteArray) -> bool;

        /// Largest request accepted, in bytes.
        #[qinvokable]
        #[cxx_name = "maxRequestBytes"]
        fn max_request_bytes(self: &FernyCore) -> u32;

        /// Prepares the private endpoint directory and reports what is at
        /// `socket` inside it.
        #[qinvokable]
        #[cxx_name = "checkEndpoint"]
        fn check_endpoint(
            self: &FernyCore,
            directory: &QString,
            socket: &QString,
        ) -> EndpointStatus;

        /// Filetype id for a file (patterns, then shebang in `first_line`);
        /// empty for plain text.
        #[qinvokable]
        #[cxx_name = "detectFiletype"]
        fn detect_filetype(self: &FernyCore, path: &QString, first_line: &QString) -> QString;

        /// Every filetype id, ordered by display name.
        #[qinvokable]
        #[cxx_name = "filetypeIds"]
        fn filetype_ids(self: &FernyCore) -> QStringList;

        /// Display name of a filetype (empty if unknown).
        #[qinvokable]
        #[cxx_name = "filetypeName"]
        fn filetype_name(self: &FernyCore, id: &QString) -> QString;

        /// Lexilla lexer of a filetype (empty for plain text).
        #[qinvokable]
        #[cxx_name = "filetypeLexer"]
        fn filetype_lexer(self: &FernyCore, id: &QString) -> QString;

        /// Keyword lists of a filetype, indexed by Lexilla set number.
        #[qinvokable]
        #[cxx_name = "filetypeKeywords"]
        fn filetype_keywords(self: &FernyCore, id: &QString) -> QStringList;

        /// Lexer properties as alternating key, value.
        #[qinvokable]
        #[cxx_name = "filetypeProperties"]
        fn filetype_properties(self: &FernyCore, id: &QString) -> QStringList;

        /// Resolves the styles of a filetype against a scheme and keeps them
        /// for `preparedStyle`. Returns how many there are.
        #[qinvokable]
        #[cxx_name = "prepareStyles"]
        fn prepare_styles(self: Pin<&mut FernyCore>, id: &QString, scheme: &QString) -> i32;

        /// A style resolved by `prepareStyles`; index -1 is the default style.
        #[qinvokable]
        #[cxx_name = "preparedStyle"]
        fn prepared_style(self: &FernyCore, index: i32) -> StyleSpec;

        /// An editor named style of a scheme (`caret`, `selection`,
        /// `current_line`, `margin_line_number`, …).
        #[qinvokable]
        #[cxx_name = "namedStyle"]
        fn named_style(self: &FernyCore, scheme: &QString, name: &QString) -> StyleSpec;

        /// Comments or uncomments whole lines (`text` without a final line
        /// ending) with the filetype's tokens. False if it has none.
        #[qinvokable]
        #[cxx_name = "toggleComment"]
        fn toggle_comment(
            self: &FernyCore,
            id: &QString,
            text: &QString,
            out: &mut QString,
        ) -> bool;

        /// Indentation defaults of a filetype (-1 where it has none).
        #[qinvokable]
        #[cxx_name = "filetypeIndent"]
        fn filetype_indent(self: &FernyCore, id: &QString) -> IndentSpec;

        /// Indentation found in the first lines of UTF-8 text (-1 = not found).
        #[qinvokable]
        #[cxx_name = "detectIndent"]
        fn detect_indent(self: &FernyCore, utf8: &QByteArray) -> IndentSpec;

        /// Number of bidirectional control characters in UTF-8 text (T7).
        #[qinvokable]
        #[cxx_name = "countBidiControls"]
        fn count_bidi_controls(self: &FernyCore, utf8: &QByteArray) -> i32;

        /// Characters drawn as labels: alternating character, label.
        #[qinvokable]
        #[cxx_name = "controlCharacterLabels"]
        fn control_character_labels(self: &FernyCore) -> QStringList;

        /// `.editorconfig` indentation of a document.
        #[qinvokable]
        #[cxx_name = "editorConfig"]
        fn editor_config(self: &FernyCore, id: u64) -> EditorConfigSpec;

        /// Applies `trim_trailing_whitespace` and `insert_final_newline` to
        /// the text about to be saved. False if nothing changes.
        #[qinvokable]
        #[cxx_name = "applySaveTransforms"]
        fn apply_save_transforms(
            self: &FernyCore,
            id: u64,
            text: &QByteArray,
            out: &mut QByteArray,
        ) -> bool;

        /// Finds the next (or previous) match from byte offset `from` in
        /// UTF-8 `text`, wrapping around. Flags: 1 regex, 2 match case,
        /// 4 whole word. Sets the byte range on `Found`.
        #[qinvokable]
        #[cxx_name = "findInText"]
        fn find_in_text(
            self: &FernyCore,
            text: &QByteArray,
            pattern: &QString,
            flags: u32,
            from: i64,
            forward: bool,
            start: &mut i64,
            end: &mut i64,
        ) -> SearchStatus;

        /// The replacement for the match exactly at `start..end`.
        #[qinvokable]
        #[cxx_name = "replacementFor"]
        fn replacement_for(
            self: &FernyCore,
            text: &QByteArray,
            pattern: &QString,
            flags: u32,
            start: i64,
            end: i64,
            replacement: &QString,
            out: &mut QString,
        ) -> SearchStatus;

        /// Replaces every match; `out` is the new text.
        #[qinvokable]
        #[cxx_name = "replaceAll"]
        fn replace_all(
            self: &FernyCore,
            text: &QByteArray,
            pattern: &QString,
            flags: u32,
            replacement: &QString,
            out: &mut QByteArray,
            count: &mut i32,
        ) -> SearchStatus;

        /// Starts a find in files in a background thread (cancelling a
        /// running one); `file_patterns` are space-separated globs.
        /// `findInFilesFinished` is emitted when it ends.
        #[qinvokable]
        #[cxx_name = "startFindInFiles"]
        fn start_find_in_files(
            self: Pin<&mut FernyCore>,
            directory: &QString,
            pattern: &QString,
            flags: u32,
            file_patterns: &QString,
        ) -> SearchStatus;

        /// Asks a running find in files to stop.
        #[qinvokable]
        #[cxx_name = "cancelFindInFiles"]
        fn cancel_find_in_files(self: &FernyCore);

        /// Number of results of the last find in files.
        #[qinvokable]
        #[cxx_name = "fileHitCount"]
        fn file_hit_count(self: &FernyCore) -> i32;

        /// File of a result.
        #[qinvokable]
        #[cxx_name = "fileHitPath"]
        fn file_hit_path(self: &FernyCore, index: i32) -> QString;

        /// Position of a result (1-based).
        #[qinvokable]
        #[cxx_name = "fileHitCursor"]
        fn file_hit_cursor(self: &FernyCore, index: i32) -> CursorPos;

        /// Line excerpt of a result.
        #[qinvokable]
        #[cxx_name = "fileHitText"]
        fn file_hit_text(self: &FernyCore, index: i32) -> QString;

        /// Emitted on the Qt thread when a find in files ends.
        #[qsignal]
        #[cxx_name = "findInFilesFinished"]
        fn find_in_files_finished(
            self: Pin<&mut FernyCore>,
            hits: i32,
            truncated: bool,
            cancelled: bool,
            searched: i32,
            skipped: i32,
        );

        /// Every colour scheme id, the default first, then by display name.
        #[qinvokable]
        #[cxx_name = "colorSchemeIds"]
        fn color_scheme_ids(self: &FernyCore) -> QStringList;

        /// Display name of a colour scheme (empty if unknown).
        #[qinvokable]
        #[cxx_name = "colorSchemeName"]
        fn color_scheme_name(self: &FernyCore, id: &QString) -> QString;

        /// Loads the settings layers (empty paths are skipped).
        #[qinvokable]
        #[cxx_name = "loadSettings"]
        fn load_settings(self: Pin<&mut FernyCore>, system: &QString, user: &QString);

        /// Problems found while loading settings and keybindings.
        #[qinvokable]
        #[cxx_name = "configWarnings"]
        fn config_warnings(self: &FernyCore) -> QStringList;

        /// Every setting key, in display order.
        #[qinvokable]
        #[cxx_name = "settingKeys"]
        fn setting_keys(self: &FernyCore) -> QStringList;

        /// English source title of a setting.
        #[qinvokable]
        #[cxx_name = "settingTitle"]
        fn setting_title(self: &FernyCore, key: &QString) -> QString;

        /// Kind of a setting: bool, int, choice, font, encoding, scheme.
        #[qinvokable]
        #[cxx_name = "settingKind"]
        fn setting_kind(self: &FernyCore, key: &QString) -> QString;

        /// Range of an int setting (min, max) or the choices of a choice.
        #[qinvokable]
        #[cxx_name = "settingOptions"]
        fn setting_options(self: &FernyCore, key: &QString) -> QStringList;

        /// Current value as text.
        #[qinvokable]
        #[cxx_name = "settingValue"]
        fn setting_value(self: &FernyCore, key: &QString) -> QString;

        /// Sets a value (validated). False if unknown or invalid.
        #[qinvokable]
        #[cxx_name = "setSetting"]
        fn set_setting(self: Pin<&mut FernyCore>, key: &QString, value: &QString) -> bool;

        /// Writes the user settings file. False on failure.
        #[qinvokable]
        #[cxx_name = "saveSettings"]
        fn save_settings(self: &FernyCore, path: &QString) -> bool;

        /// Loads keybinding overrides.
        #[qinvokable]
        #[cxx_name = "loadKeybindings"]
        fn load_keybindings(self: Pin<&mut FernyCore>, path: &QString);

        /// Reloads the catalog with the user filetypes and colour schemes
        /// under `config_dir`; returns the number of skipped files.
        #[qinvokable]
        #[cxx_name = "loadUserCatalog"]
        fn load_user_catalog(self: Pin<&mut FernyCore>, config_dir: &QString) -> i32;

        /// Reads a Geany configuration directory and keeps the import plan.
        #[qinvokable]
        #[cxx_name = "previewGeanyImport"]
        fn preview_geany_import(
            self: Pin<&mut FernyCore>,
            geany_dir: &QString,
        ) -> GeanyImportSummary;

        /// Names of everything the previewed import would not import.
        #[qinvokable]
        #[cxx_name = "geanyImportNotImported"]
        fn geany_import_not_imported(self: &FernyCore) -> QStringList;

        /// Applies the previewed import (settings and shortcuts in memory,
        /// files under `config_dir`); returns what was rejected.
        #[qinvokable]
        #[cxx_name = "applyGeanyImport"]
        fn apply_geany_import(self: Pin<&mut FernyCore>, config_dir: &QString) -> QStringList;

        /// Every command id, ordered.
        #[qinvokable]
        #[cxx_name = "commandIds"]
        fn command_ids(self: &FernyCore) -> QStringList;

        /// Sets the shortcuts of a command (`reset` restores the default).
        #[qinvokable]
        #[cxx_name = "setCommandShortcuts"]
        fn set_command_shortcuts(
            self: Pin<&mut FernyCore>,
            command_id: &QString,
            keys: &QStringList,
            reset: bool,
        ) -> bool;

        /// Other commands that use `key`.
        #[qinvokable]
        #[cxx_name = "shortcutConflicts"]
        fn shortcut_conflicts(self: &FernyCore, command_id: &QString, key: &QString)
        -> QStringList;

        /// Writes the keybindings file. False on failure.
        #[qinvokable]
        #[cxx_name = "saveKeybindings"]
        fn save_keybindings(self: &FernyCore, path: &QString) -> bool;

        /// Makes the loaded session's files the pending ones.
        #[qinvokable]
        #[cxx_name = "queueSessionFiles"]
        fn queue_session_files(self: Pin<&mut FernyCore>);

        /// Loads `session.toml` (files, recent list, geometry, colour
        /// scheme). False if missing or invalid.
        #[qinvokable]
        #[cxx_name = "loadSession"]
        fn load_session(self: Pin<&mut FernyCore>, path: &QString) -> bool;

        /// Active tab index of the loaded session.
        #[qinvokable]
        #[cxx_name = "sessionActiveIndex"]
        fn session_active_index(self: &FernyCore) -> i32;

        /// Window geometry of the loaded session (opaque, may be empty).
        #[qinvokable]
        #[cxx_name = "sessionGeometry"]
        fn session_geometry(self: &FernyCore) -> QByteArray;

        /// Starts collecting the files of the session to save.
        #[qinvokable]
        #[cxx_name = "clearSessionFiles"]
        fn clear_session_files(self: Pin<&mut FernyCore>);

        /// Adds an open file (in tab order) to the session to save.
        #[qinvokable]
        #[cxx_name = "addSessionFile"]
        fn add_session_file(self: Pin<&mut FernyCore>, path: &QString, cursor: CursorPos);

        /// Writes `session.toml` (owner-only, atomic). False on failure.
        #[qinvokable]
        #[cxx_name = "saveSession"]
        fn save_session(
            self: Pin<&mut FernyCore>,
            path: &QString,
            active: i32,
            geometry: &QByteArray,
            layout: &QByteArray,
            scribble: &QString,
            project: &QString,
        ) -> bool;

        /// Root of the project that was open when the session was saved.
        #[qinvokable]
        #[cxx_name = "sessionProject"]
        fn session_project(self: &FernyCore) -> QString;

        /// Opens the project in `folder` (it holds `.ferny/project.toml`):
        /// 0 opened, 1 no project there, 2 invalid project file.
        #[qinvokable]
        #[cxx_name = "openProject"]
        fn open_project(self: Pin<&mut FernyCore>, folder: &QString) -> i32;

        /// Entries of the open project that were ignored.
        #[qinvokable]
        #[cxx_name = "projectWarnings"]
        fn project_warnings(self: &FernyCore) -> QStringList;

        /// Closes the open project (its session is saved by the caller).
        #[qinvokable]
        #[cxx_name = "closeProject"]
        fn close_project(self: Pin<&mut FernyCore>);

        /// Name of the open project; empty if none.
        #[qinvokable]
        #[cxx_name = "projectName"]
        fn project_name(self: &FernyCore) -> QString;

        /// Root of the open project; empty if none.
        #[qinvokable]
        #[cxx_name = "projectRoot"]
        fn project_root(self: &FernyCore) -> QString;

        /// Base folder of the open project; empty if none.
        #[qinvokable]
        #[cxx_name = "projectBase"]
        fn project_base(self: &FernyCore) -> QString;

        /// File patterns of the open project.
        #[qinvokable]
        #[cxx_name = "projectPatterns"]
        fn project_patterns(self: &FernyCore) -> QStringList;

        /// The project's indentation for a file at `path` (inside the
        /// project), else -1 fields.
        #[qinvokable]
        #[cxx_name = "projectIndent"]
        fn project_indent(self: &FernyCore, path: &QString) -> IndentSpec;

        /// Creates `.ferny/project.toml` in `folder`: 0 created, 1 one
        /// exists already, 2 invalid name or folder or write error.
        #[qinvokable]
        #[cxx_name = "createProject"]
        fn create_project(self: &FernyCore, folder: &QString, name: &QString) -> i32;

        /// Previews importing a Geany project; returns the target folder,
        /// empty if the file cannot be imported.
        #[qinvokable]
        #[cxx_name = "previewGeanyProject"]
        fn preview_geany_project(self: Pin<&mut FernyCore>, file: &QString) -> QString;

        /// What the previewed Geany project import brings.
        #[qinvokable]
        #[cxx_name = "geanyProjectImported"]
        fn geany_project_imported(self: &FernyCore) -> QStringList;

        /// What the previewed Geany project import leaves out.
        #[qinvokable]
        #[cxx_name = "geanyProjectNotImported"]
        fn geany_project_not_imported(self: &FernyCore) -> QStringList;

        /// Writes the previewed import (never overwrites a project).
        #[qinvokable]
        #[cxx_name = "applyGeanyProject"]
        fn apply_geany_project(self: &FernyCore) -> bool;

        /// Session file of the open project under `config_dir`.
        #[qinvokable]
        #[cxx_name = "projectSessionFile"]
        fn project_session_file(self: &FernyCore, config_dir: &QString) -> QString;

        /// Writes the collected session files to `path` (owner-only).
        #[qinvokable]
        #[cxx_name = "saveProjectSession"]
        fn save_project_session(self: &FernyCore, path: &QString, active: i32) -> bool;

        /// Makes the files of the project session at `path` pending;
        /// returns the active index, -1 if there is none.
        #[qinvokable]
        #[cxx_name = "queueProjectSession"]
        fn queue_project_session(self: Pin<&mut FernyCore>, path: &QString) -> i32;

        /// Dock layout of the loaded session (opaque, may be empty).
        #[qinvokable]
        #[cxx_name = "sessionLayout"]
        fn session_layout(self: &FernyCore) -> QByteArray;

        /// Scribble notes of the loaded session.
        #[qinvokable]
        #[cxx_name = "sessionScribble"]
        fn session_scribble(self: &FernyCore) -> QString;

        /// Largest Scribble saved, in UTF-8 bytes.
        #[qinvokable]
        #[cxx_name = "maxScribbleBytes"]
        fn max_scribble_bytes(self: &FernyCore) -> i32;

        /// Words of `utf8` that complete `prefix` (bounded; H2.3).
        #[qinvokable]
        #[cxx_name = "completeWords"]
        fn complete_words(self: &FernyCore, utf8: &QByteArray, prefix: &QString) -> QStringList;

        /// Extracts the symbols of `utf8` for `filetype` and keeps them;
        /// returns how many, or -1 if there are none to show (filetype
        /// without a grammar, file too large, time budget exceeded).
        #[qinvokable]
        #[cxx_name = "extractSymbols"]
        fn extract_symbols(self: Pin<&mut FernyCore>, filetype: &QString, utf8: &QByteArray)
        -> i32;

        /// Name of symbol `index` (control and bidi characters escaped).
        #[qinvokable]
        #[cxx_name = "symbolName"]
        fn symbol_name(self: &FernyCore, index: i32) -> QString;

        /// Kind of symbol `index` (`function`, `class`, …; English).
        #[qinvokable]
        #[cxx_name = "symbolKind"]
        fn symbol_kind(self: &FernyCore, index: i32) -> QString;

        /// 1-based line of symbol `index` (0 if out of range).
        #[qinvokable]
        #[cxx_name = "symbolLine"]
        fn symbol_line(self: &FernyCore, index: i32) -> u32;

        /// Loads the snippets (the user's `snippets.conf` under
        /// `config_dir`, else Geany's) and remembers `config_dir` for the
        /// user's templates (H2.5b). An empty folder means built-ins only.
        #[qinvokable]
        #[cxx_name = "loadTemplates"]
        fn load_templates(self: Pin<&mut FernyCore>, config_dir: &QString);

        /// Names of the file templates (built-in and the user's).
        #[qinvokable]
        #[cxx_name = "fileTemplateNames"]
        fn file_template_names(self: &FernyCore) -> QStringList;

        /// The file template `name` expanded (developer data from the
        /// settings; `year`, `date`, `datetime` formatted by the caller);
        /// `filetype` is set to the template's filetype id.
        #[qinvokable]
        #[cxx_name = "expandFileTemplate"]
        fn expand_file_template(
            self: &FernyCore,
            name: &QString,
            dates: &QStringList,
            filetype: &mut QString,
        ) -> QString;

        /// Expands snippet `keyword` for `filetype` (an id): the text and
        /// the cursor's character offset in it (-1 if none). False if
        /// there is no such snippet.
        #[qinvokable]
        #[cxx_name = "expandSnippet"]
        fn expand_snippet(
            self: &FernyCore,
            filetype: &QString,
            keyword: &QString,
            layout: &QStringList,
            text: &mut QString,
            cursor: &mut i32,
        ) -> bool;

        /// Loads the user's `build.toml`; returns the warnings.
        #[qinvokable]
        #[cxx_name = "loadBuildCommands"]
        fn load_build_commands(self: Pin<&mut FernyCore>, path: &QString) -> QStringList;

        /// Writes the user's `build.toml` (owner-only, atomic).
        #[qinvokable]
        #[cxx_name = "saveBuildCommands"]
        fn save_build_commands(self: &FernyCore, path: &QString) -> bool;

        /// Template of `command` (`compile`, `build`, `lint`, `execute`)
        /// for `filetype`: the user's, else Geany's; empty if none.
        #[qinvokable]
        #[cxx_name = "buildTemplate"]
        fn build_template(self: &FernyCore, filetype: &QString, command: &QString) -> QString;

        /// Sets the user's template (empty = back to Geany's). False if
        /// invalid (shell syntax, unknown placeholder, …).
        #[qinvokable]
        #[cxx_name = "setBuildTemplate"]
        fn set_build_template(
            self: Pin<&mut FernyCore>,
            filetype: &QString,
            command: &QString,
            text: &QString,
        ) -> bool;

        /// Prepares `command` (a filetype command or `make`) for the file
        /// at `path`: resolves the program and expands the arguments. Make
        /// needs the file's folder to be trusted.
        #[qinvokable]
        #[cxx_name = "prepareBuild"]
        fn prepare_build(
            self: &FernyCore,
            filetype: &QString,
            command: &QString,
            path: &QString,
            line: u32,
            program: &mut QString,
            arguments: &mut QStringList,
            work_dir: &mut QString,
        ) -> BuildStatus;

        /// Program output made safe to show (escape sequences removed,
        /// control and bidi characters escaped).
        #[qinvokable]
        #[cxx_name = "neutraliseOutput"]
        fn neutralise_output(self: &FernyCore, bytes: &QByteArray) -> QString;

        /// The file location an output line points to; false if none.
        #[qinvokable]
        #[cxx_name = "outputLocation"]
        fn output_location(
            self: &FernyCore,
            line: &QString,
            file: &mut QString,
            cursor: &mut CursorPos,
        ) -> bool;

        /// Starts the local log at `path` (H3.2; empty: no log).
        #[qinvokable]
        #[cxx_name = "logInit"]
        fn log_init(self: &FernyCore, path: &QString);

        /// Writes an event to the local log (never contents).
        #[qinvokable]
        #[cxx_name = "logWrite"]
        fn log_write(self: &FernyCore, component: &QString, message: &QString);

        /// Finds the plugins under `config_dir/plugins` and reads the
        /// grants (none in safe mode); returns their ids. Problems go to
        /// the log and to `pluginWarnings`.
        #[qinvokable]
        #[cxx_name = "loadPlugins"]
        fn load_plugins(
            self: Pin<&mut FernyCore>,
            config_dir: &QString,
            safe_mode: bool,
        ) -> QStringList;

        /// Problems found by `loadPlugins` and `startPlugins`.
        #[qinvokable]
        #[cxx_name = "pluginWarnings"]
        fn plugin_warnings(self: &FernyCore) -> QStringList;

        /// External candidates excluded from execution pending verification.
        #[qinvokable]
        #[cxx_name = "pluginQuarantinedIds"]
        fn plugin_quarantined_ids(self: &FernyCore) -> QStringList;

        /// Display name and version of plugin `id`.
        #[qinvokable]
        #[cxx_name = "pluginName"]
        fn plugin_name(self: &FernyCore, id: &QString) -> QString;

        /// Permissions plugin `id` asks for.
        #[qinvokable]
        #[cxx_name = "pluginPermissions"]
        fn plugin_permissions(self: &FernyCore, id: &QString) -> QStringList;

        /// Whether plugin `id` needs the user's decision (new, or its
        /// manifest changed).
        #[qinvokable]
        #[cxx_name = "pluginNeedsDecision"]
        fn plugin_needs_decision(self: &FernyCore, id: &QString) -> bool;

        /// `bundled` or `user` (H3.4).
        #[qinvokable]
        #[cxx_name = "pluginOrigin"]
        fn plugin_origin(self: &FernyCore, id: &QString) -> QString;

        /// Whether the user decided about plugin `id` (its current
        /// manifest).
        #[qinvokable]
        #[cxx_name = "pluginDecided"]
        fn plugin_decided(self: &FernyCore, id: &QString) -> bool;

        /// Whether plugin `id` is enabled.
        #[qinvokable]
        #[cxx_name = "pluginEnabled"]
        fn plugin_enabled(self: &FernyCore, id: &QString) -> bool;

        /// Permissions granted to plugin `id`.
        #[qinvokable]
        #[cxx_name = "pluginGranted"]
        fn plugin_granted(self: &FernyCore, id: &QString) -> QStringList;

        /// Exports `text` (UTF-8, one style per byte in `styles`) as `kind`
        /// (`html` or `latex`) to `path`, atomically (H4.1a). `looks` holds
        /// `"<style> <fore> <back> <bold> <italic>"` (colours `0xRRGGBB`).
        #[qinvokable]
        #[cxx_name = "exportDocument"]
        fn export_document(
            self: &FernyCore,
            kind: &QString,
            path: &QString,
            text: &QByteArray,
            styles: &QByteArray,
            looks: &QStringList,
            background: i32,
            title: &QString,
            tab_width: i32,
        ) -> bool;

        /// Writes `bytes` to `path` atomically (a PDF export, H4.1a).
        #[qinvokable]
        #[cxx_name = "writeExport"]
        fn write_export(self: &FernyCore, path: &QString, bytes: &QByteArray) -> bool;

        /// Kills the plugins' processes and their children (Ferny quits;
        /// audit F2).
        #[qinvokable]
        #[cxx_name = "stopPluginProcesses"]
        fn stop_plugin_processes(self: &FernyCore);

        /// Forgets plugin `id`'s first-use answers and saves.
        #[qinvokable]
        #[cxx_name = "pluginResetConfirmations"]
        fn plugin_reset_confirmations(self: Pin<&mut FernyCore>, id: &QString) -> bool;

        /// Records the user's decision and saves `plugin-grants.toml`.
        #[qinvokable]
        #[cxx_name = "decidePlugin"]
        fn decide_plugin(
            self: Pin<&mut FernyCore>,
            id: &QString,
            enabled: bool,
            granted: &QStringList,
        ) -> bool;

        /// Starts every enabled plugin, `ferny.tr` in `language` (`en`,
        /// `es`…); returns how many run.
        #[qinvokable]
        #[cxx_name = "startPlugins"]
        fn start_plugins(self: Pin<&mut FernyCore>, language: &QString) -> i32;

        /// Records the user's first-use answer for `permission` of plugin
        /// `id` (H3.3b); false if there was nothing to answer.
        #[qinvokable]
        #[cxx_name = "pluginConfirm"]
        fn plugin_confirm(
            self: Pin<&mut FernyCore>,
            id: &QString,
            permission: &QString,
            allow: bool,
        ) -> bool;

        /// The user's keys for plugin command `id`, minus built-in ones
        /// (H3.3c).
        #[qinvokable]
        #[cxx_name = "pluginCommandShortcuts"]
        fn plugin_command_shortcuts(self: &FernyCore, id: &QString) -> QStringList;

        /// Whether a running plugin provides `kind` (`completion`, `hover`,
        /// `format`) for `filetype`, or (`before_save`) handles saves.
        #[qinvokable]
        #[cxx_name = "pluginHasProvider"]
        fn plugin_has_provider(self: &FernyCore, kind: &QString, filetype: &QString) -> bool;

        /// Asks the plugins' `kind` providers (H3.3c), or runs the
        /// `before_save` handlers (`kind` = `before_save`, `text` = the
        /// document). Returns pairs kind, text like `pluginEmit`, plus
        /// `item` (completion, merged), `text` (hover, formatted document
        /// or the text to save); none = no answer.
        #[qinvokable]
        #[cxx_name = "pluginProvide"]
        fn plugin_provide(
            self: Pin<&mut FernyCore>,
            kind: &QString,
            path: &QString,
            filetype: &QString,
            line: i32,
            column: i32,
            text: &QString,
            word: &QString,
        ) -> QStringList;

        /// Full ids of the commands of running plugins
        /// (`plugin.<plugin id>.<id>`).
        #[qinvokable]
        #[cxx_name = "pluginCommandIds"]
        fn plugin_command_ids(self: &FernyCore) -> QStringList;

        /// Menu title of plugin command `id`.
        #[qinvokable]
        #[cxx_name = "pluginCommandTitle"]
        fn plugin_command_title(self: &FernyCore, id: &QString) -> QString;

        /// Runs every plugin's handlers of `event` for the current document
        /// (empty path and filetype: none). Returns actions as pairs
        /// kind, text: `status`, `message`, `error`.
        #[qinvokable]
        #[cxx_name = "pluginEmit"]
        fn plugin_emit(
            self: Pin<&mut FernyCore>,
            event: &QString,
            has_document: bool,
            path: &QString,
            filetype: &QString,
        ) -> QStringList;

        /// Runs plugin command `id` on the current document. Returns
        /// actions as pairs kind, text: `status`, `message`, `error`,
        /// `insert`, `replace_selection`, `clipboard`, `confirm` (text
        /// `<plugin id> <permission>`). `clipboard` is the clipboard text,
        /// given only to plugins with that permission.
        #[qinvokable]
        #[cxx_name = "pluginRunCommand"]
        fn plugin_run_command(
            self: Pin<&mut FernyCore>,
            id: &QString,
            path: &QString,
            filetype: &QString,
            line: i32,
            column: i32,
            text: &QString,
            selection: &QString,
            clipboard: &QString,
        ) -> QStringList;

        /// `running`, `failed`, `disabled` or `undecided`.
        #[qinvokable]
        #[cxx_name = "pluginStatus"]
        fn plugin_status(self: &FernyCore, id: &QString) -> QString;

        /// Loads the server list (built-in, then `lsp.toml` at `path`, an
        /// empty path for built-ins only); returns the warnings.
        #[qinvokable]
        #[cxx_name = "loadLsp"]
        fn load_lsp(self: Pin<&mut FernyCore>, path: &QString) -> QStringList;

        /// Sends document `id` to its language server, starting the server
        /// if needed: 0 synced, 1 no server for the filetype, 2 workspace
        /// not trusted, 3 secret-looking file (never sent), 4 server not
        /// found, 5 could not start, 6 restarted too often.
        #[qinvokable]
        #[cxx_name = "lspOpen"]
        fn lsp_open(
            self: Pin<&mut FernyCore>,
            id: u64,
            path: &QString,
            filetype: &QString,
            utf8: &QByteArray,
        ) -> i32;

        /// The text of document `id` changed.
        #[qinvokable]
        #[cxx_name = "lspChange"]
        fn lsp_change(self: Pin<&mut FernyCore>, id: u64, utf8: &QByteArray);

        /// Document `id` was saved.
        #[qinvokable]
        #[cxx_name = "lspSave"]
        fn lsp_save(self: &FernyCore, id: u64);

        /// Document `id` was closed.
        #[qinvokable]
        #[cxx_name = "lspClose"]
        fn lsp_close(self: Pin<&mut FernyCore>, id: u64);

        /// Shuts every server down (at quit).
        #[qinvokable]
        #[cxx_name = "lspShutdownAll"]
        fn lsp_shutdown_all(self: Pin<&mut FernyCore>);

        /// Name of the server for `filetype`; empty if none.
        #[qinvokable]
        #[cxx_name = "lspServerName"]
        fn lsp_server_name(self: &FernyCore, filetype: &QString) -> QString;

        /// Asks the server of document `id` (H3.1b): `kind` 1 completion,
        /// 2 hover, 3 signature help, 4 definition, 5 references, at byte
        /// `offset` of `utf8` (sent first as the current text). Returns a
        /// token for `lspAnswer`, 0 if the document has no ready server.
        #[qinvokable]
        #[cxx_name = "lspRequest"]
        fn lsp_request(
            self: Pin<&mut FernyCore>,
            id: u64,
            kind: i32,
            utf8: &QByteArray,
            offset: i32,
        ) -> i64;

        /// `lspRequest` with an argument (H3.1c): kind 6 rename (`extra`
        /// is the new name), 7 formatting (`extra` is the indentation
        /// width, negative for tabs); H4.2a: 8 document symbols, 9
        /// workspace symbols (`extra` is the query); their answer texts are
        /// "<kind> <name>".
        #[qinvokable]
        #[cxx_name = "lspRequestWith"]
        fn lsp_request_with(
            self: Pin<&mut FernyCore>,
            id: u64,
            kind: i32,
            utf8: &QByteArray,
            offset: i32,
            extra: &QString,
        ) -> i64;

        /// Files the last rename or formatting answer changes (already
        /// checked: inside the workspace, no file operations).
        #[qinvokable]
        #[cxx_name = "lspEditFiles"]
        fn lsp_edit_files(self: &FernyCore) -> QStringList;

        /// Why the last edit answer was refused; empty if it was not.
        #[qinvokable]
        #[cxx_name = "lspEditRefusal"]
        fn lsp_edit_refusal(self: &FernyCore) -> QString;

        /// Number of edits for file `index`.
        #[qinvokable]
        #[cxx_name = "lspEditCount"]
        fn lsp_edit_count(self: &FernyCore, index: i32) -> i32;

        /// The text of file `index` (`utf8`) with its edits applied; false
        /// in `ok` if they overlap.
        #[qinvokable]
        #[cxx_name = "lspApplyEdits"]
        fn lsp_apply_edits(
            self: &FernyCore,
            index: i32,
            utf8: &QByteArray,
            ok: &mut bool,
        ) -> QByteArray;

        /// Items of the last answer.
        #[qinvokable]
        #[cxx_name = "lspAnswerCount"]
        fn lsp_answer_count(self: &FernyCore) -> i32;

        /// Text of item `index`: completion text, hover text, signature
        /// label (neutralised); for locations the line's text when known.
        #[qinvokable]
        #[cxx_name = "lspAnswerText"]
        fn lsp_answer_text(self: &FernyCore, index: i32) -> QString;

        /// Signature help: byte range of the active parameter in the
        /// label (line = start, column = end); 0,0 if none.
        #[qinvokable]
        #[cxx_name = "lspAnswerParameter"]
        fn lsp_answer_parameter(self: &FernyCore) -> CursorPos;

        /// Locations: the file of item `index`.
        #[qinvokable]
        #[cxx_name = "lspAnswerPath"]
        fn lsp_answer_path(self: &FernyCore, index: i32) -> QString;

        /// Locations: 1-based line of item `index`.
        #[qinvokable]
        #[cxx_name = "lspAnswerLine"]
        fn lsp_answer_line(self: &FernyCore, index: i32) -> u32;

        /// Locations: byte offset of item `index` in `utf8` (that file's
        /// text), in the answering server's position encoding.
        #[qinvokable]
        #[cxx_name = "lspAnswerOffset"]
        fn lsp_answer_offset(self: &FernyCore, index: i32, utf8: &QByteArray) -> i32;

        /// Converts the diagnostics of document `id` against its current
        /// text and keeps them; returns how many.
        #[qinvokable]
        #[cxx_name = "lspDiagnostics"]
        fn lsp_diagnostics(self: Pin<&mut FernyCore>, id: u64, utf8: &QByteArray) -> i32;

        /// Byte range of diagnostic `index` (start, end).
        #[qinvokable]
        #[cxx_name = "lspDiagnosticRange"]
        fn lsp_diagnostic_range(self: &FernyCore, index: i32) -> CursorPos;

        /// Severity of diagnostic `index` (1 error … 4 hint).
        #[qinvokable]
        #[cxx_name = "lspDiagnosticSeverity"]
        fn lsp_diagnostic_severity(self: &FernyCore, index: i32) -> i32;

        /// Message of diagnostic `index` (neutralised).
        #[qinvokable]
        #[cxx_name = "lspDiagnosticMessage"]
        fn lsp_diagnostic_message(self: &FernyCore, index: i32) -> QString;

        /// Loads `trust.toml` (an empty path keeps trust in memory only).
        /// False if the file was invalid: nothing is trusted then.
        #[qinvokable]
        #[cxx_name = "loadTrust"]
        fn load_trust(self: Pin<&mut FernyCore>, path: &QString) -> bool;

        /// Whether `path` (file or folder) is inside a trusted folder.
        #[qinvokable]
        #[cxx_name = "isTrusted"]
        fn is_trusted(self: &FernyCore, path: &QString) -> bool;

        /// Trusts `folder` and everything below it, and saves the store.
        #[qinvokable]
        #[cxx_name = "trustFolder"]
        fn trust_folder(self: Pin<&mut FernyCore>, folder: &QString) -> TrustStatus;

        /// Trusted folders, as stored.
        #[qinvokable]
        #[cxx_name = "trustedFolders"]
        fn trusted_folders(self: &FernyCore) -> QStringList;

        /// Stops trusting `folder` (as listed) and saves the store.
        #[qinvokable]
        #[cxx_name = "revokeFolder"]
        fn revoke_folder(self: Pin<&mut FernyCore>, folder: &QString) -> bool;

        /// Lists `dir` for the Files sidebar and keeps the listing; returns
        /// the number of entries, or -1 if it cannot be listed.
        #[qinvokable]
        #[cxx_name = "listDirectory"]
        fn list_directory(self: Pin<&mut FernyCore>, dir: &QString, show_hidden: bool) -> i32;

        /// Full path of entry `index` of the last listing.
        #[qinvokable]
        #[cxx_name = "listingPath"]
        fn listing_path(self: &FernyCore, index: i32) -> QString;

        /// Name to show for entry `index` (control characters escaped).
        #[qinvokable]
        #[cxx_name = "listingName"]
        fn listing_name(self: &FernyCore, index: i32) -> QString;

        /// Kind of entry `index` of the last listing.
        #[qinvokable]
        #[cxx_name = "listingKind"]
        fn listing_kind(self: &FernyCore, index: i32) -> BrowseKind;

        /// Entries of the last listing that were not shown.
        #[qinvokable]
        #[cxx_name = "listingOmitted"]
        fn listing_omitted(self: &FernyCore) -> i32;

        /// `name` with control and bidi characters escaped, for display.
        #[qinvokable]
        #[cxx_name = "displayName"]
        fn display_name(self: &FernyCore, name: &QString) -> QString;

        /// Recent files, most recent first.
        #[qinvokable]
        #[cxx_name = "recentFiles"]
        fn recent_files(self: &FernyCore) -> QStringList;

        /// Puts `path` first in the recent files.
        #[qinvokable]
        #[cxx_name = "addRecentFile"]
        fn add_recent_file(self: Pin<&mut FernyCore>, path: &QString);

        /// Remembers a closed tab for "reopen closed tab".
        #[qinvokable]
        #[cxx_name = "pushClosedTab"]
        fn push_closed_tab(self: Pin<&mut FernyCore>, path: &QString, cursor: CursorPos);

        /// Takes the most recently closed tab. False if none.
        #[qinvokable]
        #[cxx_name = "popClosedTab"]
        fn pop_closed_tab(
            self: Pin<&mut FernyCore>,
            path: &mut QString,
            cursor: &mut CursorPos,
        ) -> bool;

        /// Ids of the top-level menus, in display order.
        #[qinvokable]
        #[cxx_name = "menuIds"]
        fn menu_ids(self: &FernyCore) -> QStringList;

        /// Untranslated title of a menu (empty if unknown).
        #[qinvokable]
        #[cxx_name = "menuTitle"]
        fn menu_title(self: &FernyCore, menu_id: &QString) -> QString;

        /// Command ids of a menu in display order; an empty string marks a
        /// separator. Empty list if the menu is unknown.
        #[qinvokable]
        #[cxx_name = "menuItems"]
        fn menu_items(self: &FernyCore, menu_id: &QString) -> QStringList;

        /// Untranslated title of a command (empty if unknown).
        #[qinvokable]
        #[cxx_name = "commandTitle"]
        fn command_title(self: &FernyCore, command_id: &QString) -> QString;

        /// Default key sequences of a command, in Qt's portable notation.
        #[qinvokable]
        #[cxx_name = "commandShortcuts"]
        fn command_shortcuts(self: &FernyCore, command_id: &QString) -> QStringList;

        /// Runs a command. Core commands run immediately; UI commands emit
        /// `commandRequested`. Returns false for unknown ids.
        #[qinvokable]
        #[cxx_name = "invokeCommand"]
        fn invoke_command(self: Pin<&mut FernyCore>, command_id: &QString) -> bool;

        /// Git status of the repository holding the folder asked for
        /// (H4.2d): `root` and entries "<letter> <absolute path>"; empty
        /// when there is no repository or git failed.
        #[qsignal]
        #[cxx_name = "gitStatusReady"]
        fn git_status_ready(self: Pin<&mut FernyCore>, root: QString, entries: QStringList);

        /// Git blame of `path` (H4.2d): lines "<line>\t<commit> <author>
        /// <date> <summary>"; empty when git has nothing for it.
        #[qsignal]
        #[cxx_name = "gitBlameReady"]
        fn git_blame_ready(self: Pin<&mut FernyCore>, path: QString, lines: QStringList);

        /// Actions from plugin work that finished in the background
        /// (H3.3b), as kind/text pairs like `pluginEmit`.
        #[qsignal]
        #[cxx_name = "pluginActions"]
        fn plugin_actions(self: Pin<&mut FernyCore>, actions: QStringList);

        /// Asks the UI to run a UI command (emitted by `invokeCommand`).
        #[qsignal]
        #[cxx_name = "commandRequested"]
        fn command_requested(self: Pin<&mut FernyCore>, command_id: QString);

        /// New diagnostics for document `id` (H3.1a).
        #[qsignal]
        #[cxx_name = "lspDiagnosticsChanged"]
        fn lsp_diagnostics_changed(self: Pin<&mut FernyCore>, id: u64);

        /// The answer to request `token` (kind as in `lspRequest`) arrived.
        #[qsignal]
        #[cxx_name = "lspAnswer"]
        fn lsp_answer(self: Pin<&mut FernyCore>, token: i64, kind: i32);

        /// A language server stopped; `crashed` if it was not asked to.
        #[qsignal]
        #[cxx_name = "lspServerStopped"]
        fn lsp_server_stopped(self: Pin<&mut FernyCore>, name: QString, crashed: bool);
    }

    // Find in files reports back to the Qt thread.
    impl cxx_qt::Threading for FernyCore {}
}

use qobject::{
    BrowseKind, BuildStatus, CursorPos, DocumentInfo, EditorConfigSpec, EndpointStatus, EolMode,
    ExternalStatus, GeanyImportSummary, IndentSpec, LoadStatus, ReadOnlyState, SaveStatus,
    SearchStatus, StyleSpec, TrustStatus,
};

/// Rust state behind the `FernyCore` QObject.
pub struct FernyCoreRust {
    /// The spell checker (H4.2c), once a dictionary is loaded.
    speller: Option<ferny_core::spell::Speller>,
    /// Where backup copies go (H4.2a); `None` = no copies.
    backup_dir: Option<PathBuf>,
    commands: Registry,
    documents: Documents,
    /// Files to open next (arguments, IPC request or session).
    pending: Vec<Location>,
    /// Session loaded at start, and the one being saved.
    session: Session,
    closed_tabs: RecentList<Location>,
    /// Styles resolved by `prepareStyles`.
    prepared: (Style, Vec<ResolvedStyle>),
    /// Results of the last finished find in files.
    file_search: FileSearch,
    /// Layered settings (H1.5a).
    settings: Settings,
    /// Filetypes and colour schemes: built-in plus the user's (H1.5b).
    catalog: Catalog,
    /// The last previewed Geany import.
    geany_plan: ImportPlan,
    /// The last directory listed for the Files sidebar.
    listing: Listing,
    /// Symbols of the last document asked for (H2.2).
    symbols: Vec<Symbol>,
    /// The user's build commands (H2.4).
    build_commands: UserCommands,
    /// Language servers (H3.1a).
    lsp: LspState,
    /// Plugins (H3.2).
    plugins: PluginState,
    /// Snippets and the folder of the user's templates (H2.5b).
    snippets: Snippets,
    templates_dir: Option<PathBuf>,
    /// The open project and its ignored entries (H2.5a).
    project: Option<Project>,
    project_warnings: Vec<String>,
    /// The last previewed Geany project import.
    geany_project: GeanyProjectPlan,
    /// The global session's files while a project is open: the project
    /// keeps its own (H2.5a).
    global_files: Option<Vec<Location>>,
    /// Trusted folders (H2.1) and where they are saved (empty: memory only).
    trust: TrustStore,
    trust_path: PathBuf,
    /// The store file exists but is not valid; kept aside on the next change.
    trust_invalid: bool,
    /// Keybinding overrides (H1.5a).
    keybindings: Keybindings,
    /// Cancellation flag of the running find in files.
    search_cancel: Arc<AtomicBool>,
    /// Generation of the latest find in files (older results are dropped).
    search_generation: Arc<AtomicU64>,
}

impl Default for FernyCoreRust {
    fn default() -> Self {
        Self {
            commands: command::builtin_registry(),
            documents: Documents::new(),
            pending: Vec::new(),
            session: Session::default(),
            closed_tabs: RecentList::new(MAX_CLOSED_TABS),
            prepared: (
                Style {
                    fore: 0,
                    back: 0xffffff,
                    bold: false,
                    italic: false,
                },
                Vec::new(),
            ),
            file_search: FileSearch::default(),
            settings: Settings::default(),
            catalog: self_catalog(),
            geany_plan: ImportPlan::default(),
            listing: Listing::default(),
            symbols: Vec::new(),
            build_commands: UserCommands::default(),
            lsp: LspState::default(),
            plugins: PluginState::default(),
            snippets: Snippets::load(None),
            templates_dir: None,
            project: None,
            project_warnings: Vec::new(),
            geany_project: GeanyProjectPlan::default(),
            global_files: None,
            trust: TrustStore::default(),
            backup_dir: None,
            speller: None,
            trust_path: PathBuf::new(),
            trust_invalid: false,
            keybindings: Keybindings::default(),
            search_cancel: Arc::new(AtomicBool::new(false)),
            search_generation: Arc::new(AtomicU64::new(0)),
        }
    }
}

/// Plugin state of the bridge (H3.2).
#[derive(Default)]
struct PluginState {
    found: Vec<(Source, Manifest)>,
    safe_mode: bool,
    grants: Grants,
    grants_path: PathBuf,
    running: Vec<Plugin>,
    failed: Vec<String>,
    warnings: Vec<String>,
}

/// Turns plugin `index`'s call result into kind/text pairs in `out`. A
/// plugin out of time is stopped for the session (H3.3a). Returns whether
/// it still runs.
fn plugin_outcome(
    state: &mut PluginState,
    index: usize,
    result: Result<Vec<Action>, PluginError>,
    out: &mut Vec<String>,
) -> bool {
    let id = state.running[index].manifest.id.clone();
    match result {
        Ok(actions) => {
            for action in actions {
                let (kind, text) = match action {
                    Action::Status(t) => ("status", t),
                    Action::Message(t) => ("message", t),
                    Action::Insert(t) => ("insert", t),
                    Action::ReplaceSelection(t) => ("replace_selection", t),
                    Action::ClipboardSet(t) => ("clipboard", t),
                    Action::Confirm(permission) => ("confirm", format!("{id} {permission}")),
                };
                out.push(kind.to_owned());
                out.push(text);
            }
            true
        }
        Err(err) => {
            let component = format!("plugin {id}");
            ferny_core::log::write(&component, &format!("error: {err}"));
            out.push("error".to_owned());
            out.push(ferny_core::browse::display_name(&format!(
                "plugin {id}: {err}"
            )));
            if err.is_time_limit() {
                ferny_core::log::write(&component, "disabled for this session");
                state.running.remove(index);
                state.failed.push(id);
                return false;
            }
            true
        }
    }
}

/// Restarts allowed per server and workspace in one session.
const MAX_LSP_RESTARTS: u32 = 3;

struct RunningServer {
    name: String,
    server: LspServer,
}

struct OpenDocument {
    key: String,
    uri: String,
    version: i64,
}

/// Language-server state of the bridge (H3.1a).
struct LspState {
    servers: Vec<ServerConfig>,
    running: HashMap<String, RunningServer>,
    starts: HashMap<String, u32>,
    documents: HashMap<u64, OpenDocument>,
    /// Diagnostics as received, by URI.
    received: HashMap<String, Vec<ferny_lsp::Diagnostic>>,
    /// The last converted list: (start, end, severity, message).
    converted: Vec<(u32, u32, i32, String)>,
    /// Requests waiting for an answer: (server key, request id) →
    /// (token, kind, document id).
    waiting: HashMap<(String, i64), (i64, i32, u64)>,
    next_token: i64,
    /// The last answer.
    answer: LspAnswer,
}

/// The last answer of a server, parsed and bounded (H3.1b).
#[derive(Default)]
struct LspAnswer {
    texts: Vec<String>,
    parameter: Option<(usize, usize)>,
    /// Locations: path, (line, character), line text.
    locations: Vec<(PathBuf, (u32, u32), String)>,
    /// Rename and formatting (H3.1c): edits per file, or why refused.
    edits: Vec<(PathBuf, Vec<ferny_lsp::TextEdit>)>,
    refusal: String,
    utf16: bool,
}

impl Default for LspState {
    fn default() -> Self {
        Self {
            // The built-in servers until `loadLsp` reads the user's file.
            servers: lsp_config::load(None).0,
            running: HashMap::new(),
            starts: HashMap::new(),
            documents: HashMap::new(),
            received: HashMap::new(),
            converted: Vec::new(),
            waiting: HashMap::new(),
            next_token: 0,
            answer: LspAnswer::default(),
        }
    }
}

/// Parses an answer of `kind` (see `lspRequest`). Locations become paths;
/// the text of their lines is read only for files inside the server's
/// workspace (bounded), never for anything else (H3.1b).
fn lsp_answer_from(
    kind: i32,
    result: &serde_json::Value,
    utf16: bool,
    workspace: &Path,
) -> LspAnswer {
    let mut answer = LspAnswer {
        utf16,
        ..LspAnswer::default()
    };
    match kind {
        1 => answer.texts = ferny_lsp::parse_completion(result),
        2 => {
            let text = ferny_lsp::parse_hover(result);
            if !text.is_empty() {
                answer.texts.push(text);
            }
        }
        3 => {
            if let Some((label, parameter)) = ferny_lsp::parse_signature(result) {
                answer.texts.push(label);
                answer.parameter = parameter;
            }
        }
        6 => match ferny_lsp::parse_workspace_edit(result) {
            None => answer.refusal = "unsupported or too large".to_owned(),
            Some(files) => {
                for (uri, edits) in files {
                    let path = ferny_lsp::uri_to_path(&uri).filter(|p| {
                        std::fs::canonicalize(p).is_ok_and(|p| p.starts_with(workspace))
                    });
                    match path {
                        Some(path) => answer.edits.push((path, edits)),
                        None => {
                            // A partial rename would break the code: none.
                            answer.edits.clear();
                            answer.refusal = "outside the workspace".to_owned();
                            break;
                        }
                    }
                }
            }
        },
        // Symbols (H4.2a): text "<kind> <name>"; document symbols have no
        // path (they belong to the asked document) and only sidebar kinds.
        8 | 9 => {
            for (name, symbol_kind, (uri, position)) in ferny_lsp::parse_symbols(result) {
                // Workspace results keep every kind; the sidebar only its own.
                let Some(kind_name) =
                    ferny_lsp::symbol_kind_name(symbol_kind).or((kind == 9).then_some("symbol"))
                else {
                    continue;
                };
                let path = if uri.is_empty() {
                    PathBuf::new()
                } else {
                    match ferny_lsp::uri_to_path(&uri) {
                        Some(path) => path,
                        None => continue,
                    }
                };
                answer
                    .locations
                    .push((path, position, format!("{kind_name} {name}")));
            }
        }
        7 => match ferny_lsp::parse_text_edits(result) {
            None => answer.refusal = "unsupported or too large".to_owned(),
            Some(edits) => answer.edits.push((PathBuf::new(), edits)),
        },
        _ => {
            let mut cache: HashMap<PathBuf, Option<String>> = HashMap::new();
            for (uri, position) in ferny_lsp::parse_locations(result) {
                let Some(path) = ferny_lsp::uri_to_path(&uri) else {
                    continue;
                };
                let inside = std::fs::canonicalize(&path).is_ok_and(|p| p.starts_with(workspace));
                let text = if inside {
                    cache
                        .entry(path.clone())
                        .or_insert_with(|| {
                            ferny_core::fsio::read_limited(&path, 4 * 1024 * 1024)
                                .ok()
                                .and_then(|f| String::from_utf8(f.bytes).ok())
                        })
                        .as_deref()
                        .and_then(|t| t.lines().nth(position.0 as usize))
                        .map(|l| ferny_build::neutralise(l.trim().as_bytes()))
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                answer.locations.push((path, position, text));
            }
        }
    }
    answer
}

fn self_catalog() -> Catalog {
    Catalog::builtin().clone()
}

fn query_from(pattern: &QString, flags: u32) -> Query {
    Query {
        pattern: String::from(pattern),
        regex: flags & 1 != 0,
        match_case: flags & 2 != 0,
        whole_word: flags & 4 != 0,
    }
}

fn search_error(err: SearchError) -> SearchStatus {
    match err {
        SearchError::BadLength => SearchStatus::BadLength,
        SearchError::InvalidPattern => SearchStatus::InvalidPattern,
    }
}

fn byte_offset(text: &str, offset: i64) -> usize {
    usize::try_from(offset).unwrap_or(0).min(text.len())
}

fn cursor_of(location: &Location) -> CursorPos {
    CursorPos {
        line: location.line.unwrap_or(0),
        column: location.column.unwrap_or(0),
    }
}

fn location_from(path: &QString, cursor: &CursorPos) -> Location {
    Location {
        path: PathBuf::from(String::from(path)),
        line: (cursor.line > 0).then_some(cursor.line),
        column: (cursor.column > 0).then_some(cursor.column),
    }
}

fn style_spec(style: u32, look: Style, fill_eol: bool, found: bool) -> StyleSpec {
    StyleSpec {
        style,
        fore: look.fore,
        back: look.back,
        bold: look.bold,
        italic: look.italic,
        fill_eol,
        found,
    }
}

fn path_list<'a>(paths: impl Iterator<Item = &'a Path>) -> QStringList {
    paths
        .map(|path| QString::from(path.to_string_lossy().as_ref()))
        .collect()
}

/// An encoding label from the UI; empty means "detect".
fn encoding_label(encoding: &QString) -> Option<String> {
    let label = String::from(encoding);
    (!label.trim().is_empty()).then_some(label)
}

/// UTF-8 text from the editor. Invalid UTF-8 cannot be written faithfully
/// in any encoding, so it is reported as unencodable.
fn editor_text(contents: &QByteArray) -> Result<&str, SaveStatus> {
    std::str::from_utf8(contents.as_slice()).map_err(|_| SaveStatus::Unencodable)
}

impl qobject::FernyCore {
    fn documents_mut(self: Pin<&mut Self>) -> &mut Documents {
        &mut self.rust_mut().get_mut().documents
    }

    fn version(&self) -> QString {
        QString::from(ferny_core::VERSION)
    }

    fn new_document(self: Pin<&mut Self>) -> u64 {
        let encoding = self.settings.text("files.default_encoding").to_owned();
        let eol = match self.settings.text("files.default_eol") {
            "lf" => Eol::Lf,
            "crlf" => Eol::CrLf,
            "cr" => Eol::Cr,
            _ => Eol::native(),
        };
        self.documents_mut().new_document_with(&encoding, eol).get()
    }

    fn open_document(
        self: Pin<&mut Self>,
        path: &QString,
        encoding: &QString,
        id: &mut u64,
        contents: &mut QByteArray,
    ) -> LoadStatus {
        *contents = QByteArray::default();
        let path = PathBuf::from(String::from(path));
        let label = encoding_label(encoding);
        match self.documents_mut().open(&path, label.as_deref()) {
            Ok((document, text)) => {
                *id = document.get();
                *contents = QByteArray::from(text.as_bytes());
                LoadStatus::Ok
            }
            Err(err) => load_status(err),
        }
    }

    fn reload_document(
        self: Pin<&mut Self>,
        id: u64,
        encoding: &QString,
        contents: &mut QByteArray,
    ) -> LoadStatus {
        *contents = QByteArray::default();
        let label = encoding_label(encoding);
        match self
            .documents_mut()
            .reload(DocumentId::from_raw(id), label.as_deref())
        {
            Ok(text) => {
                *contents = QByteArray::from(text.as_bytes());
                LoadStatus::Ok
            }
            Err(err) => load_status(err),
        }
    }

    fn save_document(
        self: Pin<&mut Self>,
        id: u64,
        contents: &QByteArray,
        force: bool,
    ) -> SaveStatus {
        let text = match editor_text(contents) {
            Ok(text) => text,
            Err(status) => return status,
        };
        // Save Actions (H4.2a): the version on disk first, if asked for.
        if self.settings.bool("files.backup_copies")
            && let (Some(dir), Some(path)) = (
                self.backup_dir.as_ref(),
                self.documents
                    .get(DocumentId::from_raw(id))
                    .and_then(|d| d.path()),
            )
        {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            if let Err(ferny_core::backup::BackupError::Failed) =
                ferny_core::backup::backup(dir, path, now)
            {
                ferny_core::log::write("backup", "copy failed; saving anyway");
            }
        }
        match self
            .documents_mut()
            .save(DocumentId::from_raw(id), text, force)
        {
            Ok(()) => SaveStatus::Ok,
            Err(err) => save_status(err),
        }
    }

    fn save_document_as(
        self: Pin<&mut Self>,
        id: u64,
        path: &QString,
        contents: &QByteArray,
    ) -> SaveStatus {
        let text = match editor_text(contents) {
            Ok(text) => text,
            Err(status) => return status,
        };
        let path = PathBuf::from(String::from(path));
        match self
            .documents_mut()
            .save_as(DocumentId::from_raw(id), &path, text)
        {
            Ok(()) => SaveStatus::Ok,
            Err(err) => save_status(err),
        }
    }

    fn set_document_encoding(self: Pin<&mut Self>, id: u64, encoding: &QString) -> bool {
        self.documents_mut()
            .set_encoding(DocumentId::from_raw(id), &String::from(encoding))
            .is_ok()
    }

    fn document_info(&self, id: u64) -> DocumentInfo {
        let Some(document) = self.documents.get(DocumentId::from_raw(id)) else {
            return DocumentInfo {
                eol: EolMode::Lf,
                mixed_eol: false,
                bom: false,
                read_only: ReadOnlyState::Writable,
                large: false,
                has_path: false,
            };
        };
        DocumentInfo {
            eol: match document.eol() {
                Eol::Lf => EolMode::Lf,
                Eol::CrLf => EolMode::CrLf,
                Eol::Cr => EolMode::Cr,
            },
            mixed_eol: document.mixed_eol(),
            bom: document.bom(),
            read_only: match document.read_only() {
                None => ReadOnlyState::Writable,
                Some(ReadOnlyReason::Permissions) => ReadOnlyState::Permissions,
                Some(ReadOnlyReason::MalformedEncoding) => ReadOnlyState::MalformedEncoding,
            },
            large: document.is_large(),
            has_path: document.path().is_some(),
        }
    }

    fn document_encoding(&self, id: u64) -> QString {
        self.documents
            .get(DocumentId::from_raw(id))
            .map(|document| QString::from(document.encoding().name()))
            .unwrap_or_default()
    }

    fn markdown_html(&self, utf8: &QByteArray) -> QString {
        let text = String::from_utf8_lossy(utf8.as_slice());
        ferny_core::markdown::to_html(&text)
            .map(|html| QString::from(html.as_str()))
            .unwrap_or_default()
    }

    fn spell_load(
        self: Pin<&mut Self>,
        language: &QString,
        folders: &QStringList,
        personal: &QString,
    ) -> i32 {
        let folders: Vec<PathBuf> = folders
            .iter()
            .map(|f| PathBuf::from(String::from(f)))
            .collect();
        let personal = String::from(personal);
        let personal = (!personal.is_empty()).then(|| PathBuf::from(personal));
        let result = ferny_core::spell::Speller::load(
            &String::from(language),
            &folders,
            personal.as_deref(),
        );
        let status = match &result {
            Ok(_) => 0,
            Err(ferny_core::spell::SpellError::Language) => 1,
            Err(ferny_core::spell::SpellError::NotFound) => 2,
            Err(ferny_core::spell::SpellError::Invalid) => 3,
        };
        self.rust_mut().get_mut().speller = result.ok();
        status
    }

    fn spell_misspelled(&self, utf8: &QByteArray) -> QStringList {
        let Some(speller) = &self.speller else {
            return QStringList::default();
        };
        let text = String::from_utf8_lossy(utf8.as_slice());
        speller
            .misspelled(&text)
            .iter()
            .map(|(s, e)| QString::from(format!("{s} {e}").as_str()))
            .collect()
    }

    fn spell_suggest(&self, word: &QString) -> QStringList {
        self.speller
            .as_ref()
            .map(|s| {
                s.suggest(&String::from(word))
                    .iter()
                    .map(|w| QString::from(ferny_core::browse::display_name(w).as_str()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn spell_add(self: Pin<&mut Self>, word: &QString) -> bool {
        let word = String::from(word);
        self.rust_mut()
            .get_mut()
            .speller
            .as_mut()
            .is_some_and(|s| s.add(&word))
    }

    /// Runs git with each argument list in turn in `dir` on a thread
    /// (H4.2d); `done` gets the outputs on the Qt thread. Trusted folders
    /// only; git resolved like build programs; reduced environment.
    fn run_git(
        self: Pin<&mut Self>,
        dir: PathBuf,
        steps: Vec<Vec<String>>,
        done: impl FnOnce(Pin<&mut qobject::FernyCore>, Vec<ProcessOutcome>) + Send + 'static,
    ) -> bool {
        if !self.trust.is_trusted(&dir) {
            return false;
        }
        let path_var = std::env::var_os("PATH").unwrap_or_default();
        let Ok(git) = build::resolve("git", &dir, &dir, &path_var, false) else {
            return false;
        };
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let mut env = ferny_lsp::server_environment(std::env::vars_os(), &[], &dir);
            env.push(("GIT_TERMINAL_PROMPT".into(), "0".into()));
            let mut outcomes = Vec::new();
            let mut cwd = dir;
            for args in steps {
                let outcome = ferny_plugin::run_process(
                    &git,
                    &args,
                    &cwd,
                    &env,
                    std::time::Duration::from_secs(10),
                );
                let ok = outcome.code == Some(0);
                // `rev-parse --show-toplevel` moves the next steps to the root.
                if ok && args.iter().any(|a| a == "--show-toplevel") {
                    cwd = PathBuf::from(String::from_utf8_lossy(&outcome.stdout).trim());
                }
                outcomes.push(outcome);
                if !ok {
                    break;
                }
            }
            let _ = qt_thread.queue(move |core| done(core, outcomes));
        });
        true
    }

    fn git_status(self: Pin<&mut Self>, dir: &QString) -> bool {
        let dir = PathBuf::from(String::from(dir));
        self.run_git(
            dir,
            vec![
                ferny_core::git::toplevel_args(),
                ferny_core::git::status_args(),
            ],
            |core, outcomes| {
                let (mut root, mut entries) = (String::new(), QStringList::default());
                if let [top, status] = outcomes.as_slice()
                    && status.code == Some(0)
                {
                    root = String::from_utf8_lossy(&top.stdout).trim().to_owned();
                    for (path, letter) in ferny_core::git::parse_status(&status.stdout) {
                        let full = Path::new(&root).join(path.trim_end_matches('/'));
                        entries.append(QString::from(
                            format!("{letter} {}", full.to_string_lossy()).as_str(),
                        ));
                    }
                }
                core.git_status_ready(QString::from(root.as_str()), entries);
            },
        )
    }

    fn git_blame(self: Pin<&mut Self>, path: &QString) -> bool {
        let path = PathBuf::from(String::from(path));
        let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
            return false;
        };
        let name = name.to_string_lossy().into_owned();
        let shown = QString::from(path.to_string_lossy().as_ref());
        self.run_git(
            dir.to_path_buf(),
            vec![ferny_core::git::blame_args(&name)],
            move |core, outcomes| {
                let mut lines = QStringList::default();
                if let Some(blame) = outcomes.first().filter(|o| o.code == Some(0)) {
                    for l in ferny_core::git::parse_blame(&blame.stdout) {
                        lines.append(QString::from(
                            format!(
                                "{}\t{} {} {} {}",
                                l.line, l.commit, l.author, l.date, l.summary
                            )
                            .as_str(),
                        ));
                    }
                }
                core.git_blame_ready(shown, lines);
            },
        )
    }

    fn set_backup_dir(self: Pin<&mut Self>, path: &QString) {
        let path = String::from(path);
        self.rust_mut().get_mut().backup_dir = (!path.is_empty()).then(|| PathBuf::from(path));
    }

    fn document_path(&self, id: u64) -> QString {
        self.documents
            .get(DocumentId::from_raw(id))
            .and_then(|document| document.path())
            .map(|path| QString::from(path.to_string_lossy().as_ref()))
            .unwrap_or_default()
    }

    fn external_state(&self, id: u64) -> ExternalStatus {
        match self.documents.external_state(DocumentId::from_raw(id)) {
            ExternalState::Unchanged => ExternalStatus::Unchanged,
            ExternalState::Modified => ExternalStatus::Modified,
            ExternalState::Deleted => ExternalStatus::Deleted,
        }
    }

    fn accept_external_state(self: Pin<&mut Self>, id: u64) {
        self.documents_mut()
            .accept_external_state(DocumentId::from_raw(id));
    }

    fn close_document(self: Pin<&mut Self>, id: u64) {
        self.documents_mut().close(DocumentId::from_raw(id));
    }

    fn supported_encodings(&self) -> QStringList {
        text::SUPPORTED_ENCODINGS
            .iter()
            .map(|name| QString::from(*name))
            .collect()
    }

    fn find_document(&self, path: &QString) -> u64 {
        self.documents
            .find_by_path(Path::new(&String::from(path)))
            .map_or(0, DocumentId::get)
    }

    fn set_pending_from_arguments(self: Pin<&mut Self>, args: &QStringList, cwd: &QString) {
        let args: Vec<String> = args.iter().map(String::from).collect();
        let cwd = PathBuf::from(String::from(cwd));
        self.rust_mut().get_mut().pending = location::parse_arguments(&args, &cwd);
    }

    fn pending_paths(&self) -> QStringList {
        path_list(self.pending.iter().map(|l| l.path.as_path()))
    }

    fn pending_cursor(&self, index: i32) -> CursorPos {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.pending.get(i))
            .map_or(CursorPos { line: 0, column: 0 }, cursor_of)
    }

    fn clear_pending(self: Pin<&mut Self>) {
        self.rust_mut().get_mut().pending.clear();
    }

    fn encode_open_request(&self) -> QByteArray {
        ipc::encode_request(&self.pending)
            .map(|bytes| QByteArray::from(bytes.as_slice()))
            .unwrap_or_default()
    }

    fn accept_open_request(self: Pin<&mut Self>, request: &QByteArray) -> bool {
        match ipc::decode_request(request.as_slice()) {
            Ok(locations) => {
                self.rust_mut().get_mut().pending = locations;
                true
            }
            Err(_) => false,
        }
    }

    fn max_request_bytes(&self) -> u32 {
        u32::try_from(ipc::MAX_REQUEST_BYTES).unwrap_or(u32::MAX)
    }

    fn check_endpoint(&self, directory: &QString, socket: &QString) -> EndpointStatus {
        let directory = PathBuf::from(String::from(directory));
        let socket = PathBuf::from(String::from(socket));
        if !ipc::socket_path_fits(&socket) || ipc::prepare_directory(&directory).is_err() {
            return EndpointStatus::Unusable;
        }
        match ipc::endpoint_state(&socket) {
            EndpointState::Absent => EndpointStatus::Absent,
            EndpointState::Ours => EndpointStatus::Ours,
            EndpointState::Foreign => EndpointStatus::Foreign,
        }
    }

    fn load_session(self: Pin<&mut Self>, path: &QString) -> bool {
        match session::load(Path::new(&String::from(path))) {
            Ok(loaded) => {
                self.rust_mut().get_mut().session = loaded;
                true
            }
            Err(_) => false,
        }
    }

    fn detect_filetype(&self, path: &QString, first_line: &QString) -> QString {
        self.catalog
            .detect(&String::from(path), &String::from(first_line))
            .map(|ft| QString::from(ft.id.as_str()))
            .unwrap_or_default()
    }

    fn filetype_ids(&self) -> QStringList {
        let mut filetypes: Vec<_> = self.catalog.filetypes().iter().collect();
        filetypes.sort_by_key(|ft| ft.name.to_lowercase());
        filetypes
            .into_iter()
            .map(|ft| QString::from(ft.id.as_str()))
            .collect()
    }

    fn filetype_name(&self, id: &QString) -> QString {
        self.catalog
            .filetype(&String::from(id))
            .map(|ft| QString::from(ft.name.as_str()))
            .unwrap_or_default()
    }

    fn filetype_lexer(&self, id: &QString) -> QString {
        self.catalog
            .filetype(&String::from(id))
            .map(|ft| QString::from(ft.lexer.as_str()))
            .unwrap_or_default()
    }

    fn filetype_keywords(&self, id: &QString) -> QStringList {
        let Some(ft) = self.catalog.filetype(&String::from(id)) else {
            return QStringList::default();
        };
        let count = ft.keywords.iter().map(|k| k.set + 1).max().unwrap_or(0);
        (0..count)
            .map(|set| {
                ft.keywords
                    .iter()
                    .find(|k| k.set == set)
                    .map(|k| QString::from(k.words.as_str()))
                    .unwrap_or_default()
            })
            .collect()
    }

    fn filetype_properties(&self, id: &QString) -> QStringList {
        let Some(ft) = self.catalog.filetype(&String::from(id)) else {
            return QStringList::default();
        };
        ft.properties
            .iter()
            .flat_map(|(k, v)| [QString::from(k.as_str()), QString::from(v.as_str())])
            .collect()
    }

    fn prepare_styles(self: Pin<&mut Self>, id: &QString, scheme: &QString) -> i32 {
        let catalog = &self.catalog;
        let scheme = String::from(scheme);
        let prepared = match catalog.filetype(&String::from(id)) {
            Some(ft) => catalog.resolve_styles(ft, &scheme),
            None => {
                let named = catalog.named_styles(&scheme);
                let default = named.get("default").copied();
                (default.unwrap_or(self.prepared.0), Vec::new())
            }
        };
        let count = i32::try_from(prepared.1.len()).unwrap_or(0);
        self.rust_mut().get_mut().prepared = prepared;
        count
    }

    fn prepared_style(&self, index: i32) -> StyleSpec {
        let (default, styles) = &self.prepared;
        if index < 0 {
            return style_spec(32, *default, false, true);
        }
        usize::try_from(index)
            .ok()
            .and_then(|i| styles.get(i))
            .map_or(style_spec(0, *default, false, false), |s| {
                style_spec(s.style, s.look, s.fill_eol, true)
            })
    }

    fn named_style(&self, scheme: &QString, name: &QString) -> StyleSpec {
        let named = self.catalog.named_styles(&String::from(scheme));
        match named.get(&String::from(name)) {
            Some(look) => style_spec(0, *look, false, true),
            None => style_spec(0, self.prepared.0, false, false),
        }
    }

    fn toggle_comment(&self, id: &QString, text: &QString, out: &mut QString) -> bool {
        let Some(ft) = self.catalog.filetype(&String::from(id)) else {
            return false;
        };
        let token = |key: &str| ft.settings.get(key).filter(|v| !v.is_empty()).cloned();
        let tokens = CommentTokens {
            single: token("comment_single"),
            open: token("comment_open"),
            close: token("comment_close"),
        };
        match editing::toggle_comment(&String::from(text), &tokens) {
            Some(result) => {
                *out = QString::from(result.as_str());
                true
            }
            None => false,
        }
    }

    fn filetype_indent(&self, id: &QString) -> IndentSpec {
        let ft = self.catalog.filetype(&String::from(id));
        let get = |key: &str| ft.and_then(|ft| ft.indentation.get(key));
        let width = get("width")
            .and_then(|w| w.parse::<i32>().ok())
            .filter(|w| (1..=16).contains(w))
            .unwrap_or(-1);
        // Geany: 0 = spaces, 1 = tabs, 2 = tabs and spaces.
        let use_tabs = match get("type").map(String::as_str) {
            Some("1") => 1,
            Some("0") => 0,
            _ => -1,
        };
        IndentSpec { width, use_tabs }
    }

    fn detect_indent(&self, utf8: &QByteArray) -> IndentSpec {
        let text = String::from_utf8_lossy(utf8.as_slice());
        match editing::detect_indent(&text) {
            None => IndentSpec {
                width: -1,
                use_tabs: -1,
            },
            Some(found) => IndentSpec {
                width: i32::try_from(found.width).map_or(-1, |w| if w > 0 { w } else { -1 }),
                use_tabs: i32::from(found.use_tabs),
            },
        }
    }

    fn count_bidi_controls(&self, utf8: &QByteArray) -> i32 {
        let text = String::from_utf8_lossy(utf8.as_slice());
        i32::try_from(editing::count_bidi_controls(&text)).unwrap_or(i32::MAX)
    }

    fn control_character_labels(&self) -> QStringList {
        editing::BIDI_CONTROLS
            .iter()
            .chain(editing::INVISIBLE_CHARACTERS)
            .flat_map(|(c, label)| [QString::from(c.to_string().as_str()), QString::from(*label)])
            .collect()
    }

    fn editor_config(&self, id: u64) -> EditorConfigSpec {
        let config = self
            .documents
            .get(DocumentId::from_raw(id))
            .map(|d| *d.editor_config())
            .unwrap_or_default();
        EditorConfigSpec {
            indent_style: match config.indent_style {
                Some(IndentStyle::Space) => 0,
                Some(IndentStyle::Tab) => 1,
                None => -1,
            },
            indent_size: config.indent_size.map_or(-1, i32::from),
            tab_width: config.tab_width.map_or(-1, i32::from),
        }
    }

    fn apply_save_transforms(&self, id: u64, text: &QByteArray, out: &mut QByteArray) -> bool {
        let Some(document) = self.documents.get(DocumentId::from_raw(id)) else {
            return false;
        };
        let Ok(text) = std::str::from_utf8(text.as_slice()) else {
            return false;
        };
        // .editorconfig wins; the user settings fill what it leaves open.
        let from_project = document.editor_config();
        let effective = EditorConfig {
            trim_trailing_whitespace: from_project
                .trim_trailing_whitespace
                .or(Some(self.settings.bool("files.trim_trailing_whitespace"))),
            insert_final_newline: from_project
                .insert_final_newline
                .or(Some(self.settings.bool("files.ensure_final_newline"))),
            ..*from_project
        };
        match editorconfig::apply_on_save(text, &effective, document.eol()) {
            Some(changed) => {
                *out = QByteArray::from(changed.as_bytes());
                true
            }
            None => false,
        }
    }

    fn find_in_text(
        &self,
        text: &QByteArray,
        pattern: &QString,
        flags: u32,
        from: i64,
        forward: bool,
        start: &mut i64,
        end: &mut i64,
    ) -> SearchStatus {
        let text = String::from_utf8_lossy(text.as_slice());
        let from = byte_offset(&text, from);
        match core_search::find(&text, &query_from(pattern, flags), from, forward) {
            Ok(Some((s, e))) => {
                *start = i64::try_from(s).unwrap_or(0);
                *end = i64::try_from(e).unwrap_or(0);
                SearchStatus::Found
            }
            Ok(None) => SearchStatus::NotFound,
            Err(err) => search_error(err),
        }
    }

    fn replacement_for(
        &self,
        text: &QByteArray,
        pattern: &QString,
        flags: u32,
        start: i64,
        end: i64,
        replacement: &QString,
        out: &mut QString,
    ) -> SearchStatus {
        let text = String::from_utf8_lossy(text.as_slice());
        let (start, end) = (byte_offset(&text, start), byte_offset(&text, end));
        let query = query_from(pattern, flags);
        match core_search::replacement_for(&text, &query, start, end, &String::from(replacement)) {
            Ok(Some(value)) => {
                *out = QString::from(value.as_str());
                SearchStatus::Found
            }
            Ok(None) => SearchStatus::NotFound,
            Err(err) => search_error(err),
        }
    }

    fn replace_all(
        &self,
        text: &QByteArray,
        pattern: &QString,
        flags: u32,
        replacement: &QString,
        out: &mut QByteArray,
        count: &mut i32,
    ) -> SearchStatus {
        let text = String::from_utf8_lossy(text.as_slice());
        match core_search::replace_all(
            &text,
            &query_from(pattern, flags),
            &String::from(replacement),
        ) {
            Ok((_, 0)) => SearchStatus::NotFound,
            Ok((replaced, n)) => {
                *out = QByteArray::from(replaced.as_bytes());
                *count = i32::try_from(n).unwrap_or(i32::MAX);
                SearchStatus::Found
            }
            Err(err) => search_error(err),
        }
    }

    fn start_find_in_files(
        self: Pin<&mut Self>,
        directory: &QString,
        pattern: &QString,
        flags: u32,
        file_patterns: &QString,
    ) -> SearchStatus {
        let query = query_from(pattern, flags);
        if let Err(err) = query.compile() {
            return search_error(err);
        }
        // Cancel the previous search and start a new generation.
        self.search_cancel.store(true, Ordering::Relaxed);
        let cancel = Arc::new(AtomicBool::new(false));
        let generation = self.search_generation.fetch_add(1, Ordering::Relaxed) + 1;
        let generations = Arc::clone(&self.search_generation);
        let root = PathBuf::from(String::from(directory));
        let patterns: Vec<String> = String::from(file_patterns)
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        let qt_thread = self.qt_thread();
        let worker_cancel = Arc::clone(&cancel);
        self.rust_mut().get_mut().search_cancel = cancel;
        std::thread::spawn(move || {
            let result = core_search::find_in_files(&root, &query, &patterns, &worker_cancel)
                .unwrap_or_default();
            // If the object is gone the queue fails; nothing to report then.
            let _ = qt_thread.queue(move |mut core| {
                if generations.load(Ordering::Relaxed) != generation {
                    return; // a newer search replaced this one
                }
                let count = |n: usize| i32::try_from(n).unwrap_or(i32::MAX);
                let (hits, truncated, cancelled) =
                    (count(result.hits.len()), result.truncated, result.cancelled);
                let (searched, skipped) =
                    (count(result.files_searched), count(result.files_skipped));
                core.as_mut().rust_mut().get_mut().file_search = result;
                core.find_in_files_finished(hits, truncated, cancelled, searched, skipped);
            });
        });
        SearchStatus::Found
    }

    fn cancel_find_in_files(&self) {
        self.search_cancel.store(true, Ordering::Relaxed);
    }

    fn file_hit_count(&self) -> i32 {
        i32::try_from(self.file_search.hits.len()).unwrap_or(i32::MAX)
    }

    fn file_hit(&self, index: i32) -> Option<&core_search::Hit> {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.file_search.hits.get(i))
    }

    fn file_hit_path(&self, index: i32) -> QString {
        self.file_hit(index)
            .map(|h| QString::from(h.path.to_string_lossy().as_ref()))
            .unwrap_or_default()
    }

    fn file_hit_cursor(&self, index: i32) -> CursorPos {
        self.file_hit(index)
            .map_or(CursorPos { line: 0, column: 0 }, |h| CursorPos {
                line: h.line,
                column: h.column,
            })
    }

    fn file_hit_text(&self, index: i32) -> QString {
        self.file_hit(index)
            .map(|h| QString::from(h.excerpt.as_str()))
            .unwrap_or_default()
    }

    fn color_scheme_ids(&self) -> QStringList {
        let mut schemes: Vec<_> = self.catalog.schemes().iter().collect();
        schemes.sort_by_key(|s| (s.id != DEFAULT_SCHEME, s.name.to_lowercase()));
        schemes
            .into_iter()
            .map(|s| QString::from(s.id.as_str()))
            .collect()
    }

    fn color_scheme_name(&self, id: &QString) -> QString {
        self.catalog
            .scheme(&String::from(id))
            .map(|s| QString::from(s.name.as_str()))
            .unwrap_or_default()
    }

    fn load_settings(self: Pin<&mut Self>, system: &QString, user: &QString) {
        let system = String::from(system);
        let user = String::from(user);
        let path = |p: &String| (!p.is_empty()).then(|| PathBuf::from(p));
        let settings = Settings::load(path(&system).as_deref(), path(&user).as_deref());
        self.rust_mut().get_mut().settings = settings;
    }

    fn config_warnings(&self) -> QStringList {
        self.settings
            .warnings()
            .iter()
            .chain(self.keybindings.warnings())
            .map(|w| QString::from(w.as_str()))
            .collect()
    }

    fn setting_keys(&self) -> QStringList {
        config::DESCRIPTORS
            .iter()
            .map(|d| QString::from(d.key))
            .collect()
    }

    fn setting_title(&self, key: &QString) -> QString {
        config::descriptor(&String::from(key))
            .map(|d| QString::from(d.title))
            .unwrap_or_default()
    }

    fn setting_kind(&self, key: &QString) -> QString {
        let kind = config::descriptor(&String::from(key)).map(|d| match d.kind {
            Kind::Bool => "bool",
            Kind::Int { .. } => "int",
            Kind::Choice(_) => "choice",
            Kind::Font => "font",
            Kind::Text => "text",
            Kind::Encoding => "encoding",
            Kind::Scheme => "scheme",
        });
        QString::from(kind.unwrap_or_default())
    }

    fn setting_options(&self, key: &QString) -> QStringList {
        match config::descriptor(&String::from(key)).map(|d| d.kind) {
            Some(Kind::Int { min, max }) => [min, max]
                .iter()
                .map(|n| QString::from(n.to_string().as_str()))
                .collect(),
            Some(Kind::Choice(choices)) => choices.iter().map(|c| QString::from(*c)).collect(),
            _ => QStringList::default(),
        }
    }

    fn setting_value(&self, key: &QString) -> QString {
        self.settings
            .get(&String::from(key))
            .map(|v| QString::from(v.to_text().as_str()))
            .unwrap_or_default()
    }

    fn set_setting(self: Pin<&mut Self>, key: &QString, value: &QString) -> bool {
        let (key, value) = (String::from(key), String::from(value));
        self.rust_mut().get_mut().settings.set(&key, &value)
    }

    fn save_settings(&self, path: &QString) -> bool {
        self.settings.save(Path::new(&String::from(path))).is_ok()
    }

    fn load_keybindings(self: Pin<&mut Self>, path: &QString) {
        let path = PathBuf::from(String::from(path));
        let keybindings = Keybindings::load(&path, &self.commands);
        self.rust_mut().get_mut().keybindings = keybindings;
    }

    fn load_user_catalog(self: Pin<&mut Self>, config_dir: &QString) -> i32 {
        let dir = PathBuf::from(String::from(config_dir));
        let (catalog, warnings) = Catalog::with_user_dir(&dir);
        self.rust_mut().get_mut().catalog = catalog;
        i32::try_from(warnings.len()).unwrap_or(i32::MAX)
    }

    fn preview_geany_import(self: Pin<&mut Self>, geany_dir: &QString) -> GeanyImportSummary {
        let plan = geany::plan(Path::new(&String::from(geany_dir)), &self.commands);
        let count = |n: usize| i32::try_from(n).unwrap_or(i32::MAX);
        let summary = GeanyImportSummary {
            settings: count(plan.settings.len()),
            keybindings: count(plan.keybindings.len()),
            files: count(plan.files.len()),
            not_imported: count(plan.not_imported.len()),
        };
        self.rust_mut().get_mut().geany_plan = plan;
        summary
    }

    fn geany_import_not_imported(&self) -> QStringList {
        self.geany_plan
            .not_imported
            .iter()
            .map(|n| QString::from(n.as_str()))
            .collect()
    }

    fn apply_geany_import(self: Pin<&mut Self>, config_dir: &QString) -> QStringList {
        let dir = PathBuf::from(String::from(config_dir));
        let state = self.rust_mut().get_mut();
        let rejected = geany::apply(
            &state.geany_plan,
            &dir,
            &state.commands,
            &mut state.settings,
            &mut state.keybindings,
        );
        state.catalog = Catalog::with_user_dir(&dir).0;
        rejected.iter().map(|r| QString::from(r.as_str())).collect()
    }

    fn command_ids(&self) -> QStringList {
        self.commands
            .commands()
            .map(|c| QString::from(c.id().as_str()))
            .collect()
    }

    fn set_command_shortcuts(
        self: Pin<&mut Self>,
        command_id: &QString,
        keys: &QStringList,
        reset: bool,
    ) -> bool {
        let id = String::from(command_id);
        let keys: Vec<String> = keys.iter().map(String::from).collect();
        let state = self.rust_mut().get_mut();
        let value = (!reset).then_some(keys);
        state.keybindings.set(&state.commands, &id, value)
    }

    fn shortcut_conflicts(&self, command_id: &QString, key: &QString) -> QStringList {
        self.keybindings
            .conflicts(
                &self.commands,
                &String::from(command_id),
                &String::from(key),
            )
            .iter()
            .map(|id| QString::from(id.as_str()))
            .collect()
    }

    fn save_keybindings(&self, path: &QString) -> bool {
        self.keybindings
            .save(Path::new(&String::from(path)))
            .is_ok()
    }

    fn queue_session_files(self: Pin<&mut Self>) {
        let state = self.rust_mut().get_mut();
        state.pending = state.session.files.clone();
    }

    fn session_active_index(&self) -> i32 {
        i32::try_from(self.session.active).unwrap_or(0)
    }

    fn session_geometry(&self) -> QByteArray {
        QByteArray::from(self.session.geometry.as_slice())
    }

    fn clear_session_files(self: Pin<&mut Self>) {
        self.rust_mut().get_mut().session.files.clear();
    }

    fn add_session_file(self: Pin<&mut Self>, path: &QString, cursor: CursorPos) {
        let location = location_from(path, &cursor);
        self.rust_mut().get_mut().session.files.push(location);
    }

    fn save_session(
        self: Pin<&mut Self>,
        path: &QString,
        active: i32,
        geometry: &QByteArray,
        layout: &QByteArray,
        scribble: &QString,
        project: &QString,
    ) -> bool {
        let state = self.rust_mut().get_mut();
        let project = String::from(project);
        state.session.project = (!project.is_empty()).then(|| PathBuf::from(project));
        state.session.active = usize::try_from(active).unwrap_or(0);
        state.session.geometry = geometry.as_slice().to_vec();
        state.session.layout = layout.as_slice().to_vec();
        state.session.scribble = session::truncate_scribble(String::from(scribble));
        match &state.global_files {
            // While a project is open its files go to the project session.
            Some(files) => {
                let global = Session {
                    files: files.clone(),
                    ..state.session.clone()
                };
                session::save(Path::new(&String::from(path)), &global).is_ok()
            }
            None => session::save(Path::new(&String::from(path)), &state.session).is_ok(),
        }
    }

    fn session_project(&self) -> QString {
        self.session
            .project
            .as_ref()
            .map(|p| QString::from(p.to_string_lossy().as_ref()))
            .unwrap_or_default()
    }

    fn open_project(self: Pin<&mut Self>, folder: &QString) -> i32 {
        let result = project::open(Path::new(&String::from(folder)));
        let state = self.rust_mut().get_mut();
        match result {
            Ok((opened, warnings)) => {
                if state.global_files.is_none() {
                    state.global_files = Some(state.session.files.clone());
                }
                state.project = Some(opened);
                state.project_warnings = warnings;
                0
            }
            Err(ProjectError::NotFound) => 1,
            Err(_) => 2,
        }
    }

    fn project_warnings(&self) -> QStringList {
        self.project_warnings
            .iter()
            .map(|w| QString::from(w.as_str()))
            .collect()
    }

    fn close_project(self: Pin<&mut Self>) {
        let state = self.rust_mut().get_mut();
        state.project = None;
        state.project_warnings.clear();
        if let Some(files) = state.global_files.take() {
            state.session.files = files;
        }
    }

    fn project_name(&self) -> QString {
        self.project
            .as_ref()
            .map(|p| QString::from(p.name.as_str()))
            .unwrap_or_default()
    }

    fn project_root(&self) -> QString {
        self.project
            .as_ref()
            .map(|p| QString::from(p.root.to_string_lossy().as_ref()))
            .unwrap_or_default()
    }

    fn project_base(&self) -> QString {
        self.project
            .as_ref()
            .map(|p| QString::from(p.base.to_string_lossy().as_ref()))
            .unwrap_or_default()
    }

    fn project_patterns(&self) -> QStringList {
        self.project
            .iter()
            .flat_map(|p| p.patterns.iter())
            .map(|p| QString::from(p.as_str()))
            .collect()
    }

    /// The open project, if `path` is inside it (links resolved).
    fn project_of(&self, path: &Path) -> Option<&Project> {
        let real = std::fs::canonicalize(path).ok()?;
        self.project.as_ref().filter(|p| real.starts_with(&p.root))
    }

    fn project_indent(&self, path: &QString) -> IndentSpec {
        let project = self.project_of(Path::new(&String::from(path)));
        IndentSpec {
            width: project
                .and_then(|p| p.indent_width)
                .and_then(|w| i32::try_from(w).ok())
                .unwrap_or(-1),
            use_tabs: project.and_then(|p| p.indent_tabs).map_or(-1, i32::from),
        }
    }

    fn create_project(&self, folder: &QString, name: &QString) -> i32 {
        match project::create(Path::new(&String::from(folder)), &String::from(name)) {
            Ok(()) => 0,
            Err(ProjectError::Exists) => 1,
            Err(_) => 2,
        }
    }

    fn preview_geany_project(self: Pin<&mut Self>, file: &QString) -> QString {
        let plan = project::plan_geany_project(Path::new(&String::from(file)));
        let state = self.rust_mut().get_mut();
        state.geany_project = plan.unwrap_or_default();
        QString::from(state.geany_project.target.to_string_lossy().as_ref())
    }

    fn geany_project_imported(&self) -> QStringList {
        self.geany_project
            .imported
            .iter()
            .map(|i| QString::from(i.as_str()))
            .collect()
    }

    fn geany_project_not_imported(&self) -> QStringList {
        self.geany_project
            .not_imported
            .iter()
            .map(|i| QString::from(i.as_str()))
            .collect()
    }

    fn apply_geany_project(&self) -> bool {
        project::apply_geany_project(&self.geany_project).is_ok()
    }

    fn project_session_file(&self, config_dir: &QString) -> QString {
        self.project
            .as_ref()
            .map(|p| {
                let path = Path::new(&String::from(config_dir))
                    .join("projects")
                    .join(format!("{}.toml", project::session_id(&p.root)));
                QString::from(path.to_string_lossy().as_ref())
            })
            .unwrap_or_default()
    }

    fn save_project_session(&self, path: &QString, active: i32) -> bool {
        let data = Session {
            files: self.session.files.clone(),
            active: usize::try_from(active).unwrap_or(0),
            ..Session::default()
        };
        session::save(Path::new(&String::from(path)), &data).is_ok()
    }

    fn queue_project_session(self: Pin<&mut Self>, path: &QString) -> i32 {
        match session::load(Path::new(&String::from(path))) {
            Ok(loaded) => {
                let active = i32::try_from(loaded.active).unwrap_or(0);
                self.rust_mut().get_mut().pending = loaded.files;
                active
            }
            Err(_) => -1,
        }
    }

    fn session_layout(&self) -> QByteArray {
        QByteArray::from(self.session.layout.as_slice())
    }

    fn session_scribble(&self) -> QString {
        QString::from(self.session.scribble.as_str())
    }

    fn max_scribble_bytes(&self) -> i32 {
        i32::try_from(MAX_SCRIBBLE_BYTES).unwrap_or(i32::MAX)
    }

    fn list_directory(self: Pin<&mut Self>, dir: &QString, show_hidden: bool) -> i32 {
        let result = browse::list(Path::new(&String::from(dir)), show_hidden);
        let state = self.rust_mut().get_mut();
        match result {
            Ok(listing) => {
                state.listing = listing;
                i32::try_from(state.listing.entries.len()).unwrap_or(i32::MAX)
            }
            Err(_) => {
                state.listing = Listing::default();
                -1
            }
        }
    }

    fn listing_path(&self, index: i32) -> QString {
        self.listing_entry(index)
            .map(|e| QString::from(e.path.to_string_lossy().as_ref()))
            .unwrap_or_default()
    }

    fn listing_name(&self, index: i32) -> QString {
        self.listing_entry(index)
            .map(|e| QString::from(e.display.as_str()))
            .unwrap_or_default()
    }

    fn listing_kind(&self, index: i32) -> BrowseKind {
        match self.listing_entry(index).map(|e| e.kind) {
            Some(EntryKind::Directory) => BrowseKind::Directory,
            Some(EntryKind::DirectoryLink) => BrowseKind::DirectoryLink,
            Some(EntryKind::FileLink) => BrowseKind::FileLink,
            Some(EntryKind::File) | None => BrowseKind::File,
        }
    }

    fn listing_omitted(&self) -> i32 {
        i32::try_from(self.listing.omitted).unwrap_or(i32::MAX)
    }

    fn display_name(&self, name: &QString) -> QString {
        QString::from(browse::display_name(&String::from(name)).as_str())
    }

    fn complete_words(&self, utf8: &QByteArray, prefix: &QString) -> QStringList {
        let text = String::from_utf8_lossy(utf8.as_slice());
        editing::completions(&text, &String::from(prefix))
            .iter()
            .map(|w| QString::from(w.as_str()))
            .collect()
    }

    fn extract_symbols(self: Pin<&mut Self>, filetype: &QString, utf8: &QByteArray) -> i32 {
        let text = String::from_utf8_lossy(utf8.as_slice());
        let found = syntax_symbols::extract(&String::from(filetype), &text);
        let state = self.rust_mut().get_mut();
        match found {
            Ok(symbols) => {
                state.symbols = symbols;
                i32::try_from(state.symbols.len()).unwrap_or(i32::MAX)
            }
            Err(_) => {
                state.symbols.clear();
                -1
            }
        }
    }

    fn symbol_name(&self, index: i32) -> QString {
        self.symbol(index)
            .map(|s| QString::from(browse::display_name(&s.name).as_str()))
            .unwrap_or_default()
    }

    fn symbol_kind(&self, index: i32) -> QString {
        self.symbol(index)
            .map(|s| QString::from(s.kind.as_str()))
            .unwrap_or_default()
    }

    fn symbol_line(&self, index: i32) -> u32 {
        self.symbol(index).map_or(0, |s| s.line)
    }

    fn symbol(&self, index: i32) -> Option<&Symbol> {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.symbols.get(i))
    }

    fn load_templates(self: Pin<&mut Self>, config_dir: &QString) {
        let dir = PathBuf::from(String::from(config_dir));
        let dir = (!dir.as_os_str().is_empty()).then_some(dir);
        let state = self.rust_mut().get_mut();
        state.snippets = Snippets::load(dir.as_deref());
        state.templates_dir = dir;
    }

    fn file_template_names(&self) -> QStringList {
        templates::file_templates(self.templates_dir.as_deref())
            .into_iter()
            .map(|(name, _)| QString::from(name.as_str()))
            .collect()
    }

    fn template_values(&self, dates: &QStringList, filename: &str) -> Values {
        let date = |i: isize| dates.get(i).map(String::from).unwrap_or_default();
        let setting = |key: &str| self.settings.text(key).to_owned();
        Values {
            developer: setting("templates.developer"),
            initials: setting("templates.initials"),
            mail: setting("templates.mail"),
            company: setting("templates.company"),
            version: setting("templates.version"),
            year: date(0),
            date: date(1),
            datetime: date(2),
            filename: filename.to_owned(),
            project: self
                .project
                .as_ref()
                .map(|p| p.name.clone())
                .unwrap_or_default(),
        }
    }

    fn expand_file_template(
        &self,
        name: &QString,
        dates: &QStringList,
        filetype: &mut QString,
    ) -> QString {
        let name = String::from(name);
        let all = templates::file_templates(self.templates_dir.as_deref());
        let Some((_, text)) = all.iter().find(|(n, _)| *n == name) else {
            return QString::default();
        };
        let ft = self.catalog.detect(&name, "");
        let get = |key: &str| {
            ft.and_then(|f| f.settings.get(key))
                .cloned()
                .unwrap_or_default()
        };
        let comment = Comment {
            open: get("comment_open"),
            close: get("comment_close"),
            single: get("comment_single"),
        };
        *filetype = QString::from(ft.map(|f| f.id.as_str()).unwrap_or_default());
        let values = self.template_values(dates, &name);
        QString::from(
            templates::expand(text, &values, &comment, self.templates_dir.as_deref()).as_str(),
        )
    }

    fn expand_snippet(
        &self,
        filetype: &QString,
        keyword: &QString,
        layout: &QStringList,
        text: &mut QString,
        cursor: &mut i32,
    ) -> bool {
        // layout: current line's indentation, one indentation step, EOL.
        let part = |i: isize| layout.get(i).map(String::from).unwrap_or_default();
        let name = self
            .catalog
            .filetype(&String::from(filetype))
            .map(|f| f.name.clone())
            .unwrap_or_default();
        let values = self.template_values(&QStringList::default(), "");
        match self.snippets.expand(
            &name,
            &String::from(keyword),
            &part(0),
            &part(1),
            &part(2),
            &values,
        ) {
            Some((expanded, offset)) => {
                *text = QString::from(expanded.as_str());
                *cursor = offset.and_then(|o| i32::try_from(o).ok()).unwrap_or(-1);
                true
            }
            None => false,
        }
    }

    fn load_build_commands(self: Pin<&mut Self>, path: &QString) -> QStringList {
        let (commands, warnings) = UserCommands::load(Path::new(&String::from(path)));
        self.rust_mut().get_mut().build_commands = commands;
        warnings.iter().map(|w| QString::from(w.as_str())).collect()
    }

    fn save_build_commands(&self, path: &QString) -> bool {
        let path = PathBuf::from(String::from(path));
        if let Some(parent) = path.parent()
            && std::fs::create_dir_all(parent).is_err()
        {
            return false;
        }
        fsio::write_atomic_with_mode(
            &path,
            self.build_commands.to_toml().as_bytes(),
            None,
            fsio::FileMode::OwnerOnly,
        )
        .is_ok()
    }

    fn template(&self, filetype: &str, command: &str) -> Option<String> {
        if command == "make" {
            return Some("make".to_owned());
        }
        self.build_commands
            .get(filetype, command)
            .map(str::to_owned)
            .or_else(|| self.catalog.filetype(filetype)?.build.get(command).cloned())
    }

    fn build_template(&self, filetype: &QString, command: &QString) -> QString {
        self.template(&String::from(filetype), &String::from(command))
            .map(|t| QString::from(t.as_str()))
            .unwrap_or_default()
    }

    fn set_build_template(
        self: Pin<&mut Self>,
        filetype: &QString,
        command: &QString,
        text: &QString,
    ) -> bool {
        self.rust_mut()
            .get_mut()
            .build_commands
            .set(
                &String::from(filetype),
                &String::from(command),
                &String::from(text),
            )
            .is_ok()
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_build(
        &self,
        filetype: &QString,
        command: &QString,
        path: &QString,
        line: u32,
        program: &mut QString,
        arguments: &mut QStringList,
        work_dir: &mut QString,
    ) -> BuildStatus {
        let command = String::from(command);
        let file = PathBuf::from(String::from(path));
        let Some(dir) = file.parent().map(Path::to_path_buf) else {
            return BuildStatus::NoCommand;
        };
        let in_project = self.project_of(&file);
        // The workspace is the project root inside a project (H2.5a), else
        // the file's folder (H2.1).
        let workspace = in_project.map_or(dir.clone(), |p| p.root.clone());
        let project_template = in_project.and_then(|p| p.build.get(&command).cloned());
        // Project commands and Make run project content (T1, R1).
        if (project_template.is_some() || command == "make") && !self.trust.is_trusted(&workspace) {
            return BuildStatus::Untrusted;
        }
        let Some(template) =
            project_template.or_else(|| self.template(&String::from(filetype), &command))
        else {
            return BuildStatus::NoCommand;
        };
        let mut values = Placeholders::for_file(&file, line);
        if let Some(p) = in_project {
            values.project = p.base.to_string_lossy().into_owned();
        }
        // As Geany: Make starts in the project's base folder, the file's
        // commands in the file's folder (%p reaches the base).
        let dir = match in_project {
            Some(p) if command == "make" => p.base.clone(),
            _ => dir,
        };
        let Ok(words) = build::split(&template).and_then(|w| build::expand(&w, &values)) else {
            return BuildStatus::Invalid;
        };
        // A program from the folder itself (`./%e`) may have been planted
        // there: only in trusted workspaces (phase-2 audit, finding F1).
        let local = !Path::new(&words[0]).is_absolute() && words[0].contains(['/', '\\']);
        if local && !self.trust.is_trusted(&workspace) {
            return BuildStatus::Untrusted;
        }
        let path_var = std::env::var_os("PATH").unwrap_or_default();
        // The workspace root, not the file's folder, is excluded from PATH:
        // a subfolder of an untrusted project must not supply a program (S1).
        let Ok(resolved) =
            build::resolve(&words[0], &dir, &workspace, &path_var, command == "execute")
        else {
            return BuildStatus::NotFound;
        };
        *program = QString::from(resolved.to_string_lossy().as_ref());
        *arguments = words[1..]
            .iter()
            .map(|w| QString::from(w.as_str()))
            .collect();
        *work_dir = QString::from(dir.to_string_lossy().as_ref());
        BuildStatus::Ready
    }

    fn neutralise_output(&self, bytes: &QByteArray) -> QString {
        QString::from(build::neutralise(bytes.as_slice()).as_str())
    }

    fn output_location(&self, line: &QString, file: &mut QString, cursor: &mut CursorPos) -> bool {
        match build::error_location(&String::from(line)) {
            Some(location) => {
                *file = QString::from(location.file.as_str());
                *cursor = CursorPos {
                    line: location.line,
                    column: location.column,
                };
                true
            }
            None => false,
        }
    }

    fn log_init(&self, path: &QString) {
        ferny_core::log::init(Path::new(&String::from(path)));
    }

    fn log_write(&self, component: &QString, message: &QString) {
        ferny_core::log::write(&String::from(component), &String::from(message));
    }

    fn load_plugins(self: Pin<&mut Self>, config_dir: &QString, safe_mode: bool) -> QStringList {
        let dir = PathBuf::from(String::from(config_dir));
        let state = &mut self.rust_mut().get_mut().plugins;
        *state = PluginState::default();
        // Safe mode still lists plugins, so the manager can disable a
        // broken one; nothing runs (H3.4).
        state.safe_mode = safe_mode;
        if safe_mode {
            ferny_core::log::write("plugins", "safe mode: no plugin started");
        }
        let (discovered, mut warnings) = ferny_plugin::discover(&dir.join("plugins"));
        let mut found = ferny_plugin::bundled();
        found.extend(
            discovered
                .into_iter()
                .map(|(dir, m)| (Source::Folder(dir), m)),
        );
        // Quarantined external candidates are intentionally not executable.
        // They become visible only after passing provenance/API/resource gates.
        for id in [
            "ferny-addons",
            "ferny-autoclose",
            "ferny-automark",
            "ferny-defineformat",
            "ferny-extraselection",
            "ferny-geanyinsertnum",
            "ferny-geniuspaste",
            "ferny-keyrecord",
            "ferny-lineoperations",
            "ferny-lipsum",
            "ferny-macros",
            "ferny-markdown",
            "ferny-overview",
            "ferny-pairtaghighlighter",
            "ferny-pretty-printer",
            "ferny-scope",
            "ferny-shiftcolumn",
            "ferny-spellcheck",
            "ferny-tableconvert",
            "ferny-updatechecker",
            "ferny-utils",
            "ferny-xmlsnippets",
        ] {
            warnings.push(format!(
                "plugin {id}: quarantined; provenance and compatibility pending"
            ));
        }
        for warning in &warnings {
            ferny_core::log::write("plugins", warning);
        }
        state.grants_path = dir.join("plugin-grants.toml");
        state.grants = Grants::load(&state.grants_path);
        state.found = found;
        state.warnings = warnings;
        state
            .found
            .iter()
            .map(|(_, m)| QString::from(m.id.as_str()))
            .collect()
    }

    fn plugin_warnings(&self) -> QStringList {
        self.plugins
            .warnings
            .iter()
            .map(|w| QString::from(w.as_str()))
            .collect()
    }

    fn plugin_quarantined_ids(&self) -> QStringList {
        [
            "ferny-addons",
            "ferny-autoclose",
            "ferny-automark",
            "ferny-defineformat",
            "ferny-extraselection",
            "ferny-geanyinsertnum",
            "ferny-geniuspaste",
            "ferny-keyrecord",
            "ferny-lineoperations",
            "ferny-lipsum",
            "ferny-macros",
            "ferny-markdown",
            "ferny-overview",
            "ferny-pairtaghighlighter",
            "ferny-pretty-printer",
            "ferny-scope",
            "ferny-shiftcolumn",
            "ferny-spellcheck",
            "ferny-tableconvert",
            "ferny-updatechecker",
            "ferny-utils",
            "ferny-xmlsnippets",
        ]
        .iter()
        .map(|id| QString::from(*id))
        .collect()
    }

    fn plugin_manifest(&self, id: &QString) -> Option<&Manifest> {
        let id = String::from(id);
        self.plugins
            .found
            .iter()
            .map(|(_, m)| m)
            .find(|m| m.id == id)
    }

    fn plugin_name(&self, id: &QString) -> QString {
        self.plugin_manifest(id)
            .map(|m| QString::from(format!("{} {}", m.name, m.version).as_str()))
            .unwrap_or_default()
    }

    fn plugin_permissions(&self, id: &QString) -> QStringList {
        self.plugin_manifest(id)
            .iter()
            .flat_map(|m| m.permissions.iter())
            .map(|p| QString::from(p.as_str()))
            .collect()
    }

    fn plugin_needs_decision(&self, id: &QString) -> bool {
        // Every plugin, including bundled plugins, stays disabled until the
        // user makes an explicit decision. Safe mode suppresses prompts.
        !self.plugins.safe_mode
            && self
                .plugin_manifest(id)
                .is_some_and(|m| self.plugins.grants.decision(m).is_none())
    }

    fn plugin_is_bundled(&self, id: &QString) -> bool {
        let id = String::from(id);
        self.plugins
            .found
            .iter()
            .any(|(s, m)| m.id == id && matches!(s, Source::Bundled(_)))
    }

    fn plugin_origin(&self, id: &QString) -> QString {
        QString::from(if self.plugin_is_bundled(id) {
            "bundled"
        } else {
            "user"
        })
    }

    fn plugin_decided(&self, id: &QString) -> bool {
        self.plugin_manifest(id)
            .is_some_and(|m| self.plugins.grants.decision(m).is_some())
    }

    fn plugin_enabled(&self, id: &QString) -> bool {
        self.plugin_manifest(id)
            .and_then(|m| self.plugins.grants.decision(m))
            .is_some_and(|g| g.enabled)
    }

    fn plugin_granted(&self, id: &QString) -> QStringList {
        self.plugin_manifest(id)
            .and_then(|m| self.plugins.grants.decision(m))
            .map(|g| {
                g.granted
                    .iter()
                    .map(|p| QString::from(p.as_str()))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[allow(clippy::too_many_arguments)]
    fn export_document(
        &self,
        kind: &QString,
        path: &QString,
        text: &QByteArray,
        styles: &QByteArray,
        looks: &QStringList,
        background: i32,
        title: &QString,
        tab_width: i32,
    ) -> bool {
        let text = String::from_utf8_lossy(text.as_slice());
        let mut table = std::collections::BTreeMap::new();
        for entry in looks.iter().map(String::from) {
            let f: Vec<&str> = entry.split(' ').collect();
            let num = |i: usize| f.get(i).and_then(|v| v.parse::<u32>().ok());
            if let (Some(style), Some(fore), Some(back), Some(bold), Some(italic)) =
                (num(0), num(1), num(2), num(3), num(4))
                && let Ok(style) = u8::try_from(style)
            {
                table.insert(
                    style,
                    ferny_core::export::StyleLook {
                        fore,
                        back,
                        bold: bold != 0,
                        italic: italic != 0,
                    },
                );
            }
        }
        let background = u32::try_from(background).unwrap_or(0xff_ffff);
        let document = match String::from(kind).as_str() {
            "html" => ferny_core::export::html(
                &text,
                styles.as_slice(),
                &table,
                background,
                &String::from(title),
            ),
            "latex" => ferny_core::export::latex(
                &text,
                styles.as_slice(),
                &table,
                usize::try_from(tab_width).unwrap_or(4),
            ),
            _ => return false,
        };
        self.write_export(path, &QByteArray::from(document.as_bytes()))
    }

    fn write_export(&self, path: &QString, bytes: &QByteArray) -> bool {
        let path = PathBuf::from(String::from(path));
        fsio::write_atomic(&path, bytes.as_slice(), None).is_ok()
    }

    fn stop_plugin_processes(&self) {
        ferny_plugin::kill_all_processes();
    }

    fn plugin_reset_confirmations(self: Pin<&mut Self>, id: &QString) -> bool {
        let Some(manifest) = self.plugin_manifest(id).cloned() else {
            return false;
        };
        let state = &mut self.rust_mut().get_mut().plugins;
        if !state.grants.reset_confirmations(&manifest) {
            return false;
        }
        ferny_core::log::write(
            &format!("plugin {}", manifest.id),
            "first-use answers reset",
        );
        state.grants.save(&state.grants_path)
    }

    fn decide_plugin(
        self: Pin<&mut Self>,
        id: &QString,
        enabled: bool,
        granted: &QStringList,
    ) -> bool {
        let Some(manifest) = self.plugin_manifest(id).cloned() else {
            return false;
        };
        let granted: Vec<String> = granted.iter().map(String::from).collect();
        let state = &mut self.rust_mut().get_mut().plugins;
        state.grants.decide(&manifest, enabled, &granted);
        ferny_core::log::write(
            &format!("plugin {}", manifest.id),
            &format!(
                "{} with permissions [{}]",
                if enabled { "enabled" } else { "disabled" },
                state
                    .grants
                    .decision(&manifest)
                    .map(|g| g.granted.join(", "))
                    .unwrap_or_default()
            ),
        );
        state.grants.save(&state.grants_path)
    }

    fn start_plugins(self: Pin<&mut Self>, language: &QString) -> i32 {
        let language = String::from(language);
        let state = &mut self.rust_mut().get_mut().plugins;
        let mut running = Vec::new();
        if state.safe_mode {
            return 0;
        }
        for (source, manifest) in &state.found {
            let Some(grant) = state.grants.decision(manifest).filter(|g| g.enabled) else {
                continue;
            };
            let component = format!("plugin {}", manifest.id);
            let log_component = component.clone();
            match Plugin::start_from_with_config(
                source,
                manifest.clone(),
                grant.granted.clone(),
                &language,
                state.grants_path.parent(),
                move |message| {
                    ferny_core::log::write(&log_component, message);
                },
            ) {
                Ok(plugin) => {
                    plugin.set_confirmations(&grant.confirmed, &grant.refused);
                    ferny_core::log::write(&component, "started");
                    running.push(plugin);
                }
                Err(err) => {
                    ferny_core::log::write(&component, &format!("not started: {err}"));
                    state
                        .warnings
                        .push(format!("plugin {}: {err}", manifest.id));
                    state.failed.push(manifest.id.clone());
                }
            }
        }
        state.running = running;
        i32::try_from(state.running.len()).unwrap_or(i32::MAX)
    }

    fn plugin_command_ids(&self) -> QStringList {
        self.plugins
            .running
            .iter()
            .flat_map(Plugin::commands)
            .map(|c| QString::from(c.id.as_str()))
            .collect()
    }

    fn plugin_command_title(&self, id: &QString) -> QString {
        let id = String::from(id);
        self.plugins
            .running
            .iter()
            .flat_map(Plugin::commands)
            .find(|c| c.id == id)
            .map(|c| QString::from(c.title.as_str()))
            .unwrap_or_default()
    }

    fn plugin_emit(
        mut self: Pin<&mut Self>,
        event: &QString,
        has_document: bool,
        path: &QString,
        filetype: &QString,
    ) -> QStringList {
        let event = String::from(event);
        let doc = has_document.then(|| DocInfo {
            path: String::from(path),
            filetype: String::from(filetype),
            ..DocInfo::default()
        });
        let project = self.trusted_project();
        let mut out = Vec::new();
        let mut index = 0;
        while index < self.plugins.running.len() {
            let plugin = &self.plugins.running[index];
            plugin.set_project(project.as_deref());
            let result = plugin.emit(&event, doc.as_ref());
            if self.as_mut().plugin_after_call(index, result, &mut out) {
                index += 1;
            }
        }
        out.into_iter().map(|t| QString::from(t.as_str())).collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn plugin_run_command(
        mut self: Pin<&mut Self>,
        id: &QString,
        path: &QString,
        filetype: &QString,
        line: i32,
        column: i32,
        text: &QString,
        selection: &QString,
        clipboard: &QString,
    ) -> QStringList {
        let id = String::from(id);
        let doc = DocInfo {
            path: String::from(path),
            filetype: String::from(filetype),
            line: u32::try_from(line).unwrap_or(0),
            column: u32::try_from(column).unwrap_or(0),
            text: String::from(text),
            selection: String::from(selection),
            clipboard: String::from(clipboard),
            word: String::new(),
        };
        let project = self.trusted_project();
        let mut out = Vec::new();
        if let Some(index) = self
            .plugins
            .running
            .iter()
            .position(|p| p.commands().iter().any(|c| c.id == id))
        {
            let plugin = &self.plugins.running[index];
            plugin.set_project(project.as_deref());
            let result = plugin.run_command(&id, Some(doc));
            self.as_mut().plugin_after_call(index, result, &mut out);
        }
        out.into_iter().map(|t| QString::from(t.as_str())).collect()
    }

    fn plugin_command_shortcuts(&self, id: &QString) -> QStringList {
        self.keybindings
            .plugin_keys(&self.commands, &String::from(id))
            .iter()
            .map(|k| QString::from(k.as_str()))
            .collect()
    }

    fn plugin_has_provider(&self, kind: &QString, filetype: &QString) -> bool {
        let (kind, filetype) = (String::from(kind), String::from(filetype));
        self.plugins.running.iter().any(|p| {
            if kind == "before_save" {
                p.has_before_save()
            } else {
                p.has_provider(&kind, &filetype)
            }
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn plugin_provide(
        mut self: Pin<&mut Self>,
        kind: &QString,
        path: &QString,
        filetype: &QString,
        line: i32,
        column: i32,
        text: &QString,
        word: &QString,
    ) -> QStringList {
        let kind = String::from(kind);
        let mut doc = DocInfo {
            path: String::from(path),
            filetype: String::from(filetype),
            line: u32::try_from(line).unwrap_or(0),
            column: u32::try_from(column).unwrap_or(0),
            text: String::from(text),
            word: String::from(word),
            ..DocInfo::default()
        };
        let project = self.trusted_project();
        let mut out = Vec::new();
        let mut answered: Option<String> = None;
        let mut index = 0;
        while index < self.plugins.running.len() {
            let plugin = &self.plugins.running[index];
            plugin.set_project(project.as_deref());
            let result = if kind == "before_save" {
                plugin
                    .before_save(&doc)
                    .map(|(t, actions)| (t.map(Answer::Text), actions))
            } else if answered.is_some() || !plugin.has_provider(&kind, &doc.filetype) {
                Ok((None, Vec::new()))
            } else {
                plugin.provide(&kind, &doc)
            };
            let mut answer = None;
            let result = result.map(|(a, actions)| {
                answer = a;
                actions
            });
            match answer {
                Some(Answer::Items(items)) => {
                    for item in items {
                        out.push("item".to_owned());
                        out.push(item);
                    }
                }
                // before_save chains: the next plugin gets this text.
                Some(Answer::Text(text)) if kind == "before_save" => {
                    doc.text.clone_from(&text);
                    answered = Some(text);
                }
                Some(Answer::Text(text)) => answered = Some(text),
                None => {}
            }
            if self.as_mut().plugin_after_call(index, result, &mut out) {
                index += 1;
            }
        }
        if let Some(text) = answered {
            out.push("text".to_owned());
            out.push(text);
        }
        out.into_iter().map(|t| QString::from(t.as_str())).collect()
    }

    fn plugin_confirm(
        self: Pin<&mut Self>,
        id: &QString,
        permission: &QString,
        allow: bool,
    ) -> bool {
        let id = String::from(id);
        let permission = String::from(permission);
        let state = &mut self.rust_mut().get_mut().plugins;
        let Some(manifest) = state
            .found
            .iter()
            .map(|(_, m)| m)
            .find(|m| m.id == id)
            .cloned()
        else {
            return false;
        };
        if !state.grants.confirm(&manifest, &permission, allow) {
            return false;
        }
        ferny_core::log::write(
            &format!("plugin {id}"),
            &format!(
                "{permission} {}",
                if allow { "confirmed" } else { "refused" }
            ),
        );
        let saved = state.grants.save(&state.grants_path);
        if let (Some(grant), Some(plugin)) = (
            state.grants.decision(&manifest),
            state.running.iter().find(|p| p.manifest.id == id),
        ) {
            plugin.set_confirmations(&grant.confirmed, &grant.refused);
        }
        saved
    }

    /// The open project's root while it is trusted (H3.3b).
    fn trusted_project(&self) -> Option<PathBuf> {
        self.project
            .as_ref()
            .map(|p| p.root.clone())
            .filter(|root| self.trust.is_trusted(root))
    }

    /// Converts a call's result and starts the processes it asked for.
    /// Returns whether plugin `index` still runs.
    fn plugin_after_call(
        mut self: Pin<&mut Self>,
        index: usize,
        result: Result<Vec<Action>, PluginError>,
        out: &mut Vec<String>,
    ) -> bool {
        let spawns = self.plugins.running[index].take_spawns();
        let id = self.plugins.running[index].manifest.id.clone();
        let cwd = self
            .trusted_project()
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("/"));
        for request in spawns {
            let qt_thread = self.qt_thread();
            let (id, cwd) = (id.clone(), cwd.clone());
            let path_var = std::env::var_os("PATH").unwrap_or_default();
            // Resolved like build commands: PATH without project folders (T2).
            let resolved = build::resolve(&request.program, &cwd, &cwd, &path_var, false);
            let name = Path::new(&request.program)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            ferny_core::log::write(&format!("plugin {id}"), &format!("spawn {name}"));
            std::thread::spawn(move || {
                let outcome = match resolved {
                    Ok(program) => {
                        let env = ferny_lsp::server_environment(std::env::vars_os(), &[], &cwd);
                        ferny_plugin::run_process(
                            &program,
                            &request.args,
                            &cwd,
                            &env,
                            ferny_plugin::PROCESS_TIMEOUT,
                        )
                    }
                    Err(_) => ProcessOutcome {
                        error: Some("program not found".to_owned()),
                        ..ProcessOutcome::default()
                    },
                };
                let _ = qt_thread.queue(move |mut core| {
                    core.as_mut().plugin_deliver(&id, request.token, &outcome);
                });
            });
        }
        let state = &mut self.as_mut().rust_mut().get_mut().plugins;
        plugin_outcome(state, index, result, out)
    }

    /// Gives a finished process to its plugin and reports the actions.
    fn plugin_deliver(mut self: Pin<&mut Self>, id: &str, token: u64, outcome: &ProcessOutcome) {
        let Some(index) = self
            .plugins
            .running
            .iter()
            .position(|p| p.manifest.id == id)
        else {
            return; // disabled meanwhile
        };
        let project = self.trusted_project();
        let plugin = &self.plugins.running[index];
        plugin.set_project(project.as_deref());
        let result = plugin.deliver(token, outcome);
        let mut out = Vec::new();
        self.as_mut().plugin_after_call(index, result, &mut out);
        if !out.is_empty() {
            let actions: QStringList = out.iter().map(|t| QString::from(t.as_str())).collect();
            self.plugin_actions(actions);
        }
    }

    fn plugin_status(&self, id: &QString) -> QString {
        let id_text = String::from(id);
        let status = if self.plugins.safe_mode {
            "safe-mode"
        } else if self
            .plugins
            .running
            .iter()
            .any(|p| p.manifest.id == id_text)
        {
            "running"
        } else if self.plugins.failed.contains(&id_text) {
            "failed"
        } else if self.plugin_needs_decision(id) {
            "undecided"
        } else {
            "disabled"
        };
        QString::from(status)
    }

    fn load_lsp(self: Pin<&mut Self>, path: &QString) -> QStringList {
        let path = PathBuf::from(String::from(path));
        let (servers, warnings) =
            lsp_config::load((!path.as_os_str().is_empty()).then_some(path.as_path()));
        self.rust_mut().get_mut().lsp.servers = servers;
        warnings.iter().map(|w| QString::from(w.as_str())).collect()
    }

    fn lsp_server_name(&self, filetype: &QString) -> QString {
        lsp_config::for_filetype(&self.lsp.servers, &String::from(filetype))
            .map(|s| QString::from(s.name.as_str()))
            .unwrap_or_default()
    }

    fn lsp_open(
        mut self: Pin<&mut Self>,
        id: u64,
        path: &QString,
        filetype: &QString,
        utf8: &QByteArray,
    ) -> i32 {
        let file = PathBuf::from(String::from(path));
        let filetype = String::from(filetype);
        let Some(config) = lsp_config::for_filetype(&self.lsp.servers, &filetype).cloned() else {
            return 1;
        };
        let Some(dir) = file.parent().map(Path::to_path_buf) else {
            return 1;
        };
        // The workspace: the project root inside a project, else the
        // file's folder; servers only for trusted ones (T1, R1).
        let workspace = self.project_of(&file).map_or(dir, |p| p.root.clone());
        if !self.trust.is_trusted(&workspace) {
            return 2;
        }
        if ferny_lsp::is_secret_file(&file) {
            return 3; // never sent (T16)
        }
        let key = format!("{}\u{0}{}", workspace.display(), config.name);
        if !self.lsp.running.contains_key(&key) {
            let starts = self.lsp.starts.get(&key).copied().unwrap_or(0);
            if starts > MAX_LSP_RESTARTS {
                return 6;
            }
            let program = if Path::new(&config.command[0]).is_absolute() {
                PathBuf::from(&config.command[0])
            } else {
                let path_var = std::env::var_os("PATH").unwrap_or_default();
                match build::resolve(&config.command[0], &workspace, &workspace, &path_var, false) {
                    Ok(program) => program,
                    Err(_) => return 4,
                }
            };
            let env = ferny_lsp::server_environment(std::env::vars_os(), &config.env, &workspace);
            let qt_thread = self.qt_thread();
            let event_key = key.clone();
            let spawned = LspServer::spawn(
                &program,
                &config.command[1..],
                &workspace,
                env,
                move |event| {
                    let key = event_key.clone();
                    let _ = qt_thread.queue(move |mut core| core.as_mut().lsp_event(&key, event));
                },
            );
            let Ok(server) = spawned else {
                ferny_core::log::write("lsp", &format!("{} could not be started", config.name));
                return 5;
            };
            ferny_core::log::write(
                "lsp",
                &format!("started {} ({})", config.name, program.display()),
            );
            let state = self.as_mut().rust_mut().get_mut();
            state.lsp.starts.insert(key.clone(), starts + 1);
            state.lsp.running.insert(
                key.clone(),
                RunningServer {
                    name: config.name.clone(),
                    server,
                },
            );
        }
        let text = String::from_utf8_lossy(utf8.as_slice()).into_owned();
        let uri = ferny_lsp::path_to_uri(&file);
        let state = self.as_mut().rust_mut().get_mut();
        if let Some(running) = state.lsp.running.get(&key) {
            running.server.did_open(&uri, &filetype, 1, &text);
        }
        state.lsp.documents.insert(
            id,
            OpenDocument {
                key,
                uri,
                version: 1,
            },
        );
        0
    }

    fn lsp_event(mut self: Pin<&mut Self>, key: &str, event: LspEvent) {
        match event {
            LspEvent::Ready => {}
            LspEvent::Response(request, result) => {
                let state = self.as_mut().rust_mut().get_mut();
                let Some((token, kind, document)) =
                    state.lsp.waiting.remove(&(key.to_owned(), request))
                else {
                    return; // not ours, or no longer awaited
                };
                let utf16 = state
                    .lsp
                    .running
                    .get(key)
                    .is_none_or(|r| !r.server.utf8_positions());
                let workspace = key
                    .split('\u{0}')
                    .next()
                    .map(PathBuf::from)
                    .unwrap_or_default();
                state.lsp.answer = lsp_answer_from(kind, &result, utf16, &workspace);
                if kind == 7 {
                    // Formatting changes only the document it was asked for.
                    let path = state
                        .lsp
                        .documents
                        .get(&document)
                        .and_then(|d| ferny_lsp::uri_to_path(&d.uri));
                    if let (Some(path), Some(first)) = (path, state.lsp.answer.edits.first_mut()) {
                        first.0 = path;
                    }
                }
                self.as_mut().lsp_answer(token, kind);
            }
            LspEvent::Diagnostics(uri, items) => {
                // Only for documents this server has open.
                let id = self
                    .lsp
                    .documents
                    .iter()
                    .find(|(_, d)| d.key == key && d.uri == uri)
                    .map(|(id, _)| *id);
                if let Some(id) = id {
                    self.as_mut()
                        .rust_mut()
                        .get_mut()
                        .lsp
                        .received
                        .insert(uri, items);
                    self.as_mut().lsp_diagnostics_changed(id);
                }
            }
            LspEvent::Exited(crashed) => {
                let state = self.as_mut().rust_mut().get_mut();
                let name = state
                    .lsp
                    .running
                    .remove(key)
                    .map(|r| r.name)
                    .unwrap_or_default();
                ferny_core::log::write(
                    "lsp",
                    &format!(
                        "{name} stopped{}",
                        if crashed { " unexpectedly" } else { "" }
                    ),
                );
                let uris: Vec<String> = state
                    .lsp
                    .documents
                    .values()
                    .filter(|d| d.key == key)
                    .map(|d| d.uri.clone())
                    .collect();
                state.lsp.documents.retain(|_, d| d.key != key);
                for uri in uris {
                    state.lsp.received.remove(&uri);
                }
                self.as_mut()
                    .lsp_server_stopped(QString::from(name.as_str()), crashed);
            }
        }
    }

    fn lsp_change(self: Pin<&mut Self>, id: u64, utf8: &QByteArray) {
        let state = self.rust_mut().get_mut();
        let Some(document) = state.lsp.documents.get_mut(&id) else {
            return;
        };
        document.version += 1;
        if let Some(running) = state.lsp.running.get(&document.key) {
            let text = String::from_utf8_lossy(utf8.as_slice());
            running
                .server
                .did_change(&document.uri, document.version, &text);
        }
    }

    fn lsp_save(&self, id: u64) {
        if let Some(document) = self.lsp.documents.get(&id)
            && let Some(running) = self.lsp.running.get(&document.key)
        {
            running.server.did_save(&document.uri);
        }
    }

    fn lsp_close(self: Pin<&mut Self>, id: u64) {
        let state = self.rust_mut().get_mut();
        if let Some(document) = state.lsp.documents.remove(&id) {
            if let Some(running) = state.lsp.running.get(&document.key) {
                running.server.did_close(&document.uri);
            }
            state.lsp.received.remove(&document.uri);
        }
    }

    fn lsp_shutdown_all(self: Pin<&mut Self>) {
        let state = self.rust_mut().get_mut();
        for running in state.lsp.running.values() {
            running.server.shutdown();
        }
        state.lsp.documents.clear();
    }

    fn lsp_request(
        self: Pin<&mut Self>,
        id: u64,
        kind: i32,
        utf8: &QByteArray,
        offset: i32,
    ) -> i64 {
        self.lsp_request_with(id, kind, utf8, offset, &QString::default())
    }

    fn lsp_request_with(
        self: Pin<&mut Self>,
        id: u64,
        kind: i32,
        utf8: &QByteArray,
        offset: i32,
        extra: &QString,
    ) -> i64 {
        let extra = String::from(extra);
        let state = self.rust_mut().get_mut();
        let Some(document) = state.lsp.documents.get_mut(&id) else {
            return 0;
        };
        // A server still starting queues the request until it is ready.
        let Some(running) = state.lsp.running.get(&document.key) else {
            return 0;
        };
        let text = String::from_utf8_lossy(utf8.as_slice());
        document.version += 1;
        running
            .server
            .did_change(&document.uri, document.version, &text);
        let offset = usize::try_from(offset).unwrap_or(0);
        let (line, character) =
            ferny_lsp::lsp_position(&text, offset, !running.server.utf8_positions());
        let position = serde_json::json!({ "line": line, "character": character });
        let target = serde_json::json!({ "uri": document.uri });
        let (method, params) = match kind {
            1 => (
                "textDocument/completion",
                serde_json::json!({ "textDocument": target, "position": position }),
            ),
            2 => (
                "textDocument/hover",
                serde_json::json!({ "textDocument": target, "position": position }),
            ),
            3 => (
                "textDocument/signatureHelp",
                serde_json::json!({ "textDocument": target, "position": position }),
            ),
            4 => (
                "textDocument/definition",
                serde_json::json!({ "textDocument": target, "position": position }),
            ),
            5 => (
                "textDocument/references",
                serde_json::json!({ "textDocument": target, "position": position,
                                    "context": { "includeDeclaration": true } }),
            ),
            6 if !extra.trim().is_empty() && !extra.chars().any(char::is_control) => (
                "textDocument/rename",
                serde_json::json!({ "textDocument": target, "position": position,
                                    "newName": extra.trim() }),
            ),
            7 => {
                let width: i64 = extra.parse().unwrap_or(4);
                (
                    "textDocument/formatting",
                    serde_json::json!({ "textDocument": target,
                        "options": { "tabSize": width.unsigned_abs().clamp(1, 16),
                                     "insertSpaces": width > 0 } }),
                )
            }
            8 => (
                "textDocument/documentSymbol",
                serde_json::json!({ "textDocument": target }),
            ),
            9 if extra.len() <= 256 && !extra.chars().any(char::is_control) => (
                "workspace/symbol",
                serde_json::json!({ "query": extra.trim() }),
            ),
            _ => return 0,
        };
        let request = running.server.request(method, params);
        // Few requests are ever in flight; drop stale ones beyond 64.
        if state.lsp.waiting.len() >= 64 {
            state.lsp.waiting.clear();
        }
        state.lsp.next_token += 1;
        let token = state.lsp.next_token;
        state
            .lsp
            .waiting
            .insert((document.key.clone(), request), (token, kind, id));
        token
    }

    fn lsp_edit_files(&self) -> QStringList {
        path_list(self.lsp.answer.edits.iter().map(|(p, _)| p.as_path()))
    }

    fn lsp_edit_refusal(&self) -> QString {
        QString::from(self.lsp.answer.refusal.as_str())
    }

    fn lsp_edit_count(&self, index: i32) -> i32 {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.lsp.answer.edits.get(i))
            .map_or(0, |(_, e)| i32::try_from(e.len()).unwrap_or(i32::MAX))
    }

    fn lsp_apply_edits(&self, index: i32, utf8: &QByteArray, ok: &mut bool) -> QByteArray {
        let edits = usize::try_from(index)
            .ok()
            .and_then(|i| self.lsp.answer.edits.get(i));
        let text = String::from_utf8_lossy(utf8.as_slice());
        let applied =
            edits.and_then(|(_, e)| ferny_lsp::apply_edits(&text, e, self.lsp.answer.utf16));
        *ok = applied.is_some();
        applied
            .map(|t| QByteArray::from(t.as_bytes()))
            .unwrap_or_default()
    }

    fn lsp_answer_count(&self) -> i32 {
        let n = if self.lsp.answer.locations.is_empty() {
            self.lsp.answer.texts.len()
        } else {
            self.lsp.answer.locations.len()
        };
        i32::try_from(n).unwrap_or(i32::MAX)
    }

    fn lsp_answer_text(&self, index: i32) -> QString {
        let index = usize::try_from(index).unwrap_or(usize::MAX);
        let answer = &self.lsp.answer;
        answer
            .texts
            .get(index)
            .or_else(|| answer.locations.get(index).map(|l| &l.2))
            .map(|t| QString::from(t.as_str()))
            .unwrap_or_default()
    }

    fn lsp_answer_parameter(&self) -> CursorPos {
        let (start, end) = self.lsp.answer.parameter.unwrap_or((0, 0));
        CursorPos {
            line: u32::try_from(start).unwrap_or(0),
            column: u32::try_from(end).unwrap_or(0),
        }
    }

    fn lsp_location(&self, index: i32) -> Option<&(PathBuf, (u32, u32), String)> {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.lsp.answer.locations.get(i))
    }

    fn lsp_answer_path(&self, index: i32) -> QString {
        self.lsp_location(index)
            .map(|l| QString::from(l.0.to_string_lossy().as_ref()))
            .unwrap_or_default()
    }

    fn lsp_answer_line(&self, index: i32) -> u32 {
        self.lsp_location(index)
            .map_or(0, |l| l.1.0.saturating_add(1))
    }

    fn lsp_answer_offset(&self, index: i32, utf8: &QByteArray) -> i32 {
        let Some(location) = self.lsp_location(index) else {
            return 0;
        };
        let text = String::from_utf8_lossy(utf8.as_slice());
        i32::try_from(ferny_lsp::byte_offset(
            &text,
            location.1,
            self.lsp.answer.utf16,
        ))
        .unwrap_or(0)
    }

    fn lsp_diagnostics(self: Pin<&mut Self>, id: u64, utf8: &QByteArray) -> i32 {
        let state = self.rust_mut().get_mut();
        state.lsp.converted.clear();
        let Some(document) = state.lsp.documents.get(&id) else {
            return 0;
        };
        let utf8_positions = state
            .lsp
            .running
            .get(&document.key)
            .is_some_and(|r| r.server.utf8_positions());
        let text = String::from_utf8_lossy(utf8.as_slice());
        let offset = |position| {
            u32::try_from(ferny_lsp::byte_offset(&text, position, !utf8_positions))
                .unwrap_or(u32::MAX)
        };
        if let Some(items) = state.lsp.received.get(&document.uri) {
            state.lsp.converted = items
                .iter()
                .map(|d| {
                    let (start, end) = (offset(d.start), offset(d.end));
                    (
                        start,
                        end.max(start),
                        i32::from(d.severity),
                        d.message.clone(),
                    )
                })
                .collect();
        }
        i32::try_from(state.lsp.converted.len()).unwrap_or(i32::MAX)
    }

    fn lsp_converted(&self, index: i32) -> Option<&(u32, u32, i32, String)> {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.lsp.converted.get(i))
    }

    fn lsp_diagnostic_range(&self, index: i32) -> CursorPos {
        self.lsp_converted(index)
            .map_or(CursorPos { line: 0, column: 0 }, |d| CursorPos {
                line: d.0,
                column: d.1,
            })
    }

    fn lsp_diagnostic_severity(&self, index: i32) -> i32 {
        self.lsp_converted(index).map_or(0, |d| d.2)
    }

    fn lsp_diagnostic_message(&self, index: i32) -> QString {
        self.lsp_converted(index)
            // Already neutralised (escape sequences removed, control and
            // bidi characters escaped); line breaks are kept.
            .map(|d| QString::from(d.3.as_str()))
            .unwrap_or_default()
    }

    fn load_trust(self: Pin<&mut Self>, path: &QString) -> bool {
        let path = PathBuf::from(String::from(path));
        let (store, ok) = if path.as_os_str().is_empty() {
            (TrustStore::default(), true)
        } else {
            TrustStore::load(&path)
        };
        let state = self.rust_mut().get_mut();
        state.trust = store;
        state.trust_path = path;
        state.trust_invalid = !ok;
        ok
    }

    fn is_trusted(&self, path: &QString) -> bool {
        self.trust.is_trusted(Path::new(&String::from(path)))
    }

    fn trust_folder(self: Pin<&mut Self>, folder: &QString) -> TrustStatus {
        let state = self.rust_mut().get_mut();
        let mut next = state.trust.clone();
        match next.trust(Path::new(&String::from(folder))) {
            Ok(_) => {}
            Err(TrustError::Root) => return TrustStatus::Root,
            Err(TrustError::Full) => return TrustStatus::Full,
            Err(TrustError::NotAFolder) => return TrustStatus::NotAFolder,
            Err(TrustError::Io) => return TrustStatus::Failed,
        }
        // An invalid store is renamed aside, never overwritten (S2).
        if state.trust_invalid && !state.trust_path.as_os_str().is_empty() {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            if TrustStore::set_invalid_aside(&state.trust_path, now).is_err() {
                return TrustStatus::Failed;
            }
            state.trust_invalid = false;
        }
        // Saved first: the change counts only once it is on disk.
        if !state.trust_path.as_os_str().is_empty() && next.save(&state.trust_path).is_err() {
            return TrustStatus::Failed;
        }
        state.trust = next;
        TrustStatus::Trusted
    }

    fn trusted_folders(&self) -> QStringList {
        path_list(self.trust.folders().iter().map(PathBuf::as_path))
    }

    fn revoke_folder(self: Pin<&mut Self>, folder: &QString) -> bool {
        let state = self.rust_mut().get_mut();
        let mut next = state.trust.clone();
        if !next.revoke(Path::new(&String::from(folder))) {
            return false;
        }
        if !state.trust_path.as_os_str().is_empty() && next.save(&state.trust_path).is_err() {
            return false;
        }
        state.trust = next;
        true
    }

    fn listing_entry(&self, index: i32) -> Option<&browse::Entry> {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.listing.entries.get(i))
    }

    fn recent_files(&self) -> QStringList {
        path_list(self.session.recent.iter().map(PathBuf::as_path))
    }

    fn add_recent_file(self: Pin<&mut Self>, path: &QString) {
        let path = PathBuf::from(String::from(path));
        self.rust_mut().get_mut().session.recent.push(path);
    }

    fn push_closed_tab(self: Pin<&mut Self>, path: &QString, cursor: CursorPos) {
        let location = location_from(path, &cursor);
        self.rust_mut().get_mut().closed_tabs.push(location);
    }

    fn pop_closed_tab(self: Pin<&mut Self>, path: &mut QString, cursor: &mut CursorPos) -> bool {
        match self.rust_mut().get_mut().closed_tabs.pop() {
            Some(location) => {
                *path = QString::from(location.path.to_string_lossy().as_ref());
                *cursor = cursor_of(&location);
                true
            }
            None => false,
        }
    }

    fn menu_ids(&self) -> QStringList {
        self.commands
            .menus()
            .iter()
            .map(|menu| QString::from(menu.id().as_str()))
            .collect()
    }

    fn menu_title(&self, menu_id: &QString) -> QString {
        self.commands
            .menu(&String::from(menu_id))
            .map(|menu| QString::from(menu.title()))
            .unwrap_or_default()
    }

    fn menu_items(&self, menu_id: &QString) -> QStringList {
        let Some(menu) = self.commands.menu(&String::from(menu_id)) else {
            return QStringList::default();
        };
        menu.items()
            .iter()
            .map(|item| match item {
                MenuItem::Command(id) => QString::from(id.as_str()),
                MenuItem::Separator => QString::default(),
            })
            .collect()
    }

    fn command_title(&self, command_id: &QString) -> QString {
        self.commands
            .get(&String::from(command_id))
            .map(|command| QString::from(command.title()))
            .unwrap_or_default()
    }

    fn command_shortcuts(&self, command_id: &QString) -> QStringList {
        self.keybindings
            .keys(&self.commands, &String::from(command_id))
            .iter()
            .map(|key| QString::from(key.as_str()))
            .collect()
    }

    fn invoke_command(self: Pin<&mut Self>, command_id: &QString) -> bool {
        let id = String::from(command_id);
        match self.commands.invoke(&id) {
            Ok(Dispatch::Done) => true,
            Ok(Dispatch::Ui) => {
                self.command_requested(QString::from(id.as_str()));
                true
            }
            Err(_) => false,
        }
    }
}

fn load_status(err: OpenError) -> LoadStatus {
    match err {
        OpenError::NotFound => LoadStatus::NotFound,
        OpenError::PermissionDenied => LoadStatus::PermissionDenied,
        OpenError::NotAFile => LoadStatus::NotAFile,
        OpenError::TooLarge => LoadStatus::TooLarge,
        OpenError::Binary => LoadStatus::Binary,
        OpenError::UnknownEncoding => LoadStatus::UnknownEncoding,
        OpenError::UnknownDocument => LoadStatus::UnknownDocument,
        OpenError::Io => LoadStatus::IoError,
    }
}

fn save_status(err: SaveError) -> SaveStatus {
    match err {
        SaveError::NoPath => SaveStatus::NoPath,
        SaveError::ReadOnly => SaveStatus::ReadOnly,
        SaveError::Unencodable => SaveStatus::Unencodable,
        SaveError::ModifiedExternally => SaveStatus::ModifiedExternally,
        SaveError::NotFound => SaveStatus::NotFound,
        SaveError::PermissionDenied => SaveStatus::PermissionDenied,
        SaveError::NotAFile => SaveStatus::NotAFile,
        SaveError::UnknownDocument => SaveStatus::UnknownDocument,
        SaveError::Io => SaveStatus::IoError,
    }
}
