//! Open documents: where they come from, how they are encoded and whether
//! they changed on disk.
//!
//! Scintilla owns the text (`docs/DESIGN.md` §3); the core keeps what is
//! needed to read and write it faithfully: path, encoding, BOM, line
//! endings, the file stamp and the read-only state. Every opened file is
//! untrusted input (`docs/THREAT-MODEL.md`, T6); saving follows T9.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use encoding_rs::{Encoding, UTF_8};

use crate::editorconfig::{self, Charset, EditorConfig};
use crate::fsio::{self, ExternalState, FileStamp, ReadError, WriteError};
use crate::text::{self, DecodeError, Eol};

pub use crate::fsio::{LARGE_DOCUMENT_BYTES, MAX_DOCUMENT_BYTES};

/// Identifies an open document for the lifetime of the process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocumentId(u64);

impl DocumentId {
    /// The raw value, for the bridge.
    pub fn get(self) -> u64 {
        self.0
    }

    /// Rebuilds an id received from the bridge.
    pub fn from_raw(raw: u64) -> Self {
        Self(raw)
    }
}

/// Why a document could not be opened or reloaded. Carries no path or
/// content, so it is safe to log; the UI maps each variant to a
/// translated message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OpenError {
    /// The path does not exist.
    #[error("file not found")]
    NotFound,
    /// The operating system refused access.
    #[error("permission denied")]
    PermissionDenied,
    /// The path is a directory, device, FIFO, socket or other non-regular file.
    #[error("not a regular file")]
    NotAFile,
    /// The file is larger than [`MAX_DOCUMENT_BYTES`].
    #[error("file exceeds the size limit")]
    TooLarge,
    /// The contents look binary.
    #[error("binary file")]
    Binary,
    /// The requested encoding is unknown or cannot be written back.
    #[error("unknown encoding")]
    UnknownEncoding,
    /// No document has this id.
    #[error("unknown document")]
    UnknownDocument,
    /// Any other I/O failure.
    #[error("I/O error")]
    Io,
}

impl From<ReadError> for OpenError {
    fn from(err: ReadError) -> Self {
        match err {
            ReadError::NotFound => Self::NotFound,
            ReadError::PermissionDenied => Self::PermissionDenied,
            ReadError::NotAFile => Self::NotAFile,
            ReadError::TooLarge => Self::TooLarge,
            ReadError::Io(_) => Self::Io,
        }
    }
}

impl From<DecodeError> for OpenError {
    fn from(err: DecodeError) -> Self {
        match err {
            DecodeError::Binary => Self::Binary,
        }
    }
}

/// Why a document could not be saved. Nothing was written unless stated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SaveError {
    /// The document has no file yet: use "save as".
    #[error("document has no path")]
    NoPath,
    /// The document is read-only.
    #[error("document is read-only")]
    ReadOnly,
    /// The encoding cannot represent some character of the text.
    #[error("text not representable in the encoding")]
    Unencodable,
    /// The file changed on disk since it was read or saved; saving again
    /// with `force` overwrites it.
    #[error("modified by another program")]
    ModifiedExternally,
    /// The target is missing (e.g. a dangling symlink) or its directory is.
    #[error("target not found")]
    NotFound,
    /// The target or its directory is not writable.
    #[error("permission denied")]
    PermissionDenied,
    /// The target is a directory or a special file.
    #[error("not a regular file")]
    NotAFile,
    /// No document has this id.
    #[error("unknown document")]
    UnknownDocument,
    /// Any other I/O failure.
    #[error("I/O error")]
    Io,
}

impl From<WriteError> for SaveError {
    fn from(err: WriteError) -> Self {
        match err {
            WriteError::NotFound => Self::NotFound,
            WriteError::PermissionDenied => Self::PermissionDenied,
            WriteError::NotAFile => Self::NotAFile,
            WriteError::ModifiedExternally => Self::ModifiedExternally,
            WriteError::Io(_) => Self::Io,
        }
    }
}

/// Why a document is read-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadOnlyReason {
    /// The file has no write permission.
    Permissions,
    /// Decoding replaced malformed bytes; saving would corrupt the file.
    /// Reload with the right encoding, or "save as" deliberately.
    MalformedEncoding,
}

/// What the core knows about an open document.
#[derive(Debug, Clone)]
pub struct Document {
    path: Option<PathBuf>,
    encoding: &'static Encoding,
    bom: bool,
    eol: Eol,
    mixed_eol: bool,
    stamp: Option<FileStamp>,
    read_only: Option<ReadOnlyReason>,
    large: bool,
    editor_config: EditorConfig,
}

impl Document {
    fn untitled() -> Self {
        Self {
            path: None,
            encoding: UTF_8,
            bom: false,
            eol: Eol::native(),
            mixed_eol: false,
            stamp: None,
            read_only: None,
            large: false,
            editor_config: EditorConfig::default(),
        }
    }

    /// The file, or `None` for a new document.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Encoding used to read and write the file.
    pub fn encoding(&self) -> &'static Encoding {
        self.encoding
    }

    /// Whether the file has (and will be written with) a byte-order mark.
    pub fn bom(&self) -> bool {
        self.bom
    }

    /// Dominant line ending; new lines should use it.
    pub fn eol(&self) -> Eol {
        self.eol
    }

    /// Whether the file mixes line endings.
    pub fn mixed_eol(&self) -> bool {
        self.mixed_eol
    }

    /// Why the document is read-only, if it is.
    pub fn read_only(&self) -> Option<ReadOnlyReason> {
        self.read_only
    }

    /// Whether the document exceeds [`LARGE_DOCUMENT_BYTES`].
    pub fn is_large(&self) -> bool {
        self.large
    }

    /// `.editorconfig` properties for the document's file (H1.3c).
    pub fn editor_config(&self) -> &EditorConfig {
        &self.editor_config
    }
}

/// The set of open documents.
#[derive(Debug, Default)]
pub struct Documents {
    next_id: u64,
    documents: BTreeMap<DocumentId, Document>,
}

impl Documents {
    /// No open documents.
    pub fn new() -> Self {
        Self::default()
    }

    fn insert(&mut self, document: Document) -> DocumentId {
        self.next_id += 1;
        let id = DocumentId(self.next_id);
        self.documents.insert(id, document);
        id
    }

    /// Creates an empty, untitled UTF-8 document with native line endings.
    pub fn new_document(&mut self) -> DocumentId {
        self.insert(Document::untitled())
    }

    /// Creates an empty, untitled document with the given encoding label
    /// (falls back to UTF-8 if unknown) and line ending.
    pub fn new_document_with(&mut self, encoding: &str, eol: Eol) -> DocumentId {
        let mut document = Document::untitled();
        document.encoding = text::encoding_for_label(encoding).unwrap_or(UTF_8);
        document.eol = eol;
        self.insert(document)
    }

    /// Opens `path`, detecting the encoding unless `encoding` (a label) is
    /// given. Returns the new id and the text as UTF-8.
    pub fn open(
        &mut self,
        path: &Path,
        encoding: Option<&str>,
    ) -> Result<(DocumentId, String), OpenError> {
        let (document, text) = load(path, encoding)?;
        Ok((self.insert(document), text))
    }

    /// Reads the file of `id` again, optionally with another encoding, and
    /// returns the new text. On error the document is unchanged.
    pub fn reload(&mut self, id: DocumentId, encoding: Option<&str>) -> Result<String, OpenError> {
        let path = self
            .get(id)
            .ok_or(OpenError::UnknownDocument)?
            .path
            .clone()
            .ok_or(OpenError::NotFound)?;
        let (document, text) = load(&path, encoding)?;
        self.documents.insert(id, document);
        Ok(text)
    }

    /// Saves `text` (UTF-8, line endings as in the editor) to the
    /// document's file. Without `force`, refuses if the file changed on
    /// disk since it was read or last saved.
    pub fn save(&mut self, id: DocumentId, text: &str, force: bool) -> Result<(), SaveError> {
        let document = self
            .documents
            .get_mut(&id)
            .ok_or(SaveError::UnknownDocument)?;
        if document.read_only.is_some() {
            return Err(SaveError::ReadOnly);
        }
        let path = document.path.clone().ok_or(SaveError::NoPath)?;
        let bytes = text::encode(text, document.encoding, document.bom)
            .map_err(|_| SaveError::Unencodable)?;
        let expected = if force { None } else { document.stamp.as_ref() };
        let stamp = fsio::write_atomic(&path, &bytes, expected)?;
        document.stamp = Some(stamp);
        document.large = stamp.len() > LARGE_DOCUMENT_BYTES;
        Ok(())
    }

    /// Saves `text` to `path` (which the user chose, so an existing file
    /// there is overwritten) and makes it the document's file. Allowed for
    /// read-only documents: the user is deliberately writing a new file.
    pub fn save_as(&mut self, id: DocumentId, path: &Path, text: &str) -> Result<(), SaveError> {
        let document = self
            .documents
            .get_mut(&id)
            .ok_or(SaveError::UnknownDocument)?;
        let bytes = text::encode(text, document.encoding, document.bom)
            .map_err(|_| SaveError::Unencodable)?;
        let stamp = fsio::write_atomic(path, &bytes, None)?;
        document.editor_config = editorconfig::for_file(&editorconfig::absolute(path));
        document.path = Some(path.to_path_buf());
        document.stamp = Some(stamp);
        document.read_only = None;
        document.large = stamp.len() > LARGE_DOCUMENT_BYTES;
        Ok(())
    }

    /// Changes the encoding used for the next save (not a reload). Unicode
    /// encodings keep the BOM setting; others never have one.
    pub fn set_encoding(&mut self, id: DocumentId, label: &str) -> Result<(), OpenError> {
        let encoding = text::encoding_for_label(label).ok_or(OpenError::UnknownEncoding)?;
        let document = self
            .documents
            .get_mut(&id)
            .ok_or(OpenError::UnknownDocument)?;
        document.encoding = encoding;
        if !is_unicode(encoding) {
            document.bom = false;
        }
        if document.read_only == Some(ReadOnlyReason::MalformedEncoding) {
            document.read_only = None;
        }
        Ok(())
    }

    /// Whether the file of `id` changed on disk. New documents and unknown
    /// ids are `Unchanged`.
    pub fn external_state(&self, id: DocumentId) -> ExternalState {
        match self.get(id) {
            Some(Document {
                path: Some(path),
                stamp: Some(stamp),
                ..
            }) => fsio::external_state(path, stamp),
            _ => ExternalState::Unchanged,
        }
    }

    /// Accepts the current file on disk as the known version (the user
    /// chose to keep the editor's text). The next save then overwrites it
    /// without asking again.
    pub fn accept_external_state(&mut self, id: DocumentId) {
        if let Some(document) = self.documents.get_mut(&id)
            && let Some(path) = &document.path
        {
            document.stamp = std::fs::metadata(path)
                .ok()
                .map(|m| FileStamp::from_metadata(&m));
        }
    }

    /// Forgets a document.
    pub fn close(&mut self, id: DocumentId) {
        self.documents.remove(&id);
    }

    /// The open document whose file is `path`, comparing canonical paths
    /// (symlinks and `..` resolved), so one file is never open twice.
    pub fn find_by_path(&self, path: &Path) -> Option<DocumentId> {
        let wanted = std::fs::canonicalize(path).ok()?;
        self.documents.iter().find_map(|(id, document)| {
            let candidate = std::fs::canonicalize(document.path.as_ref()?).ok()?;
            (candidate == wanted).then_some(*id)
        })
    }

    /// Looks up a document.
    pub fn get(&self, id: DocumentId) -> Option<&Document> {
        self.documents.get(&id)
    }
}

fn is_unicode(encoding: &'static Encoding) -> bool {
    encoding == UTF_8 || encoding == encoding_rs::UTF_16LE || encoding == encoding_rs::UTF_16BE
}

fn load(path: &Path, encoding: Option<&str>) -> Result<(Document, String), OpenError> {
    let editor_config = editorconfig::for_file(&editorconfig::absolute(path));
    // An explicit choice of the user wins over `charset` in .editorconfig.
    let label = encoding.or(editor_config.charset.map(Charset::label));
    let forced = match label {
        Some(label) => Some(text::encoding_for_label(label).ok_or(OpenError::UnknownEncoding)?),
        None => None,
    };
    let file = fsio::read_limited(path, MAX_DOCUMENT_BYTES)?;
    let decoded = text::decode(&file.bytes, forced)?;
    let (mut eol, mixed_eol) = text::detect_eol(&decoded.text);
    // `end_of_line` decides only for files without line endings yet; existing
    // line endings are never converted silently.
    let has_eol = decoded.text.contains(['\n', '\r']);
    if !has_eol && let Some(wanted) = editor_config.end_of_line {
        eol = wanted;
    }
    // `utf-8-bom` adds a BOM on save; `utf-8` never strips an existing one.
    let bom =
        decoded.bom || (encoding.is_none() && editor_config.charset == Some(Charset::Utf8Bom));
    let read_only = if decoded.malformed {
        Some(ReadOnlyReason::MalformedEncoding)
    } else if file.read_only {
        Some(ReadOnlyReason::Permissions)
    } else {
        None
    };
    let document = Document {
        path: Some(path.to_path_buf()),
        encoding: decoded.encoding,
        bom,
        eol,
        mixed_eol,
        stamp: Some(file.stamp),
        read_only,
        large: file.stamp.len() > LARGE_DOCUMENT_BYTES,
        editor_config,
    };
    Ok((document, decoded.text))
}

/// Reads and decodes a file without keeping it open.
pub fn read_text_file(path: &Path) -> Result<String, OpenError> {
    load(path, None).map(|(_, text)| text)
}
