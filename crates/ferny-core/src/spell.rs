//! Spell checking (H4.2c, threats T6, T7): Hunspell dictionaries read with
//! `spellbook` (pure Rust). Dictionaries come only from the folders the UI
//! passes (the user's, then the system's read-only ones); nothing is
//! downloaded. The user's own words live in a plain list.

use std::path::{Path, PathBuf};

use crate::fsio::{self, FileMode};

/// Largest `.aff` or `.dic` file read, in bytes.
pub const MAX_DICTIONARY_BYTES: u64 = 32 * 1024 * 1024;
/// Largest text checked at once, in bytes.
pub const MAX_CHECK_BYTES: usize = 1024 * 1024;
/// Most misspellings reported for one text.
pub const MAX_MISSPELLINGS: usize = 10_000;
/// Longest word checked or offered suggestions, in bytes.
pub const MAX_WORD_BYTES: usize = 64;
/// Most suggestions offered.
pub const MAX_SUGGESTIONS: usize = 8;
/// Largest personal word list, in bytes.
pub const MAX_PERSONAL_BYTES: u64 = 1024 * 1024;

/// Why a dictionary could not be used. No paths inside.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SpellError {
    /// The language name is not like `es_ES` or `en`.
    #[error("invalid language")]
    Language,
    /// No `<language>.aff` + `.dic` in the folders.
    #[error("no dictionary for this language")]
    NotFound,
    /// Unreadable, too large, not UTF-8 or malformed.
    #[error("unusable dictionary")]
    Invalid,
}

/// Whether `language` looks like `en`, `es_ES` or `pt_BR`.
pub fn valid_language(language: &str) -> bool {
    let (lang, region) = language.split_once('_').unwrap_or((language, ""));
    (2..=3).contains(&lang.len())
        && lang.chars().all(|c| c.is_ascii_alphabetic())
        && (region.is_empty()
            || ((2..=4).contains(&region.len())
                && region.chars().all(|c| c.is_ascii_alphanumeric())))
}

fn read(path: &Path, limit: u64) -> Result<String, SpellError> {
    let bytes = fsio::read_limited(path, limit)
        .map_err(|_| SpellError::Invalid)?
        .bytes;
    String::from_utf8(bytes).map_err(|_| SpellError::Invalid)
}

/// A loaded dictionary plus the user's words.
pub struct Speller {
    dictionary: spellbook::Dictionary,
    personal: Option<PathBuf>,
}

impl Speller {
    /// Loads `<language>.aff`/`.dic` from the first folder of `folders` that
    /// has both, then the words of `personal` (one per line), if given.
    pub fn load(
        language: &str,
        folders: &[PathBuf],
        personal: Option<&Path>,
    ) -> Result<Self, SpellError> {
        if !valid_language(language) {
            return Err(SpellError::Language);
        }
        let (aff, dic) = folders
            .iter()
            .map(|f| {
                (
                    f.join(format!("{language}.aff")),
                    f.join(format!("{language}.dic")),
                )
            })
            .find(|(aff, dic)| aff.is_file() && dic.is_file())
            .ok_or(SpellError::NotFound)?;
        let mut dictionary = spellbook::Dictionary::new(
            &read(&aff, MAX_DICTIONARY_BYTES)?,
            &read(&dic, MAX_DICTIONARY_BYTES)?,
        )
        .map_err(|_| SpellError::Invalid)?;
        if let Some(words) = personal.and_then(|p| read(p, MAX_PERSONAL_BYTES).ok()) {
            for word in words.lines().map(str::trim).filter(|w| valid_word(w)) {
                let _ = dictionary.add(word);
            }
        }
        Ok(Self {
            dictionary,
            personal: personal.map(Path::to_path_buf),
        })
    }

    /// Byte ranges of the misspelled words of `text` (letters and inner
    /// apostrophes; words with digits are skipped). Texts over
    /// `MAX_CHECK_BYTES` are not checked.
    pub fn misspelled(&self, text: &str) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        if text.len() > MAX_CHECK_BYTES {
            return out;
        }
        let mut start: Option<usize> = None;
        let mut has_digit = false;
        let check = |s: usize, e: usize, has_digit: bool, out: &mut Vec<(usize, usize)>| {
            let word = text[s..e].trim_end_matches('\'');
            if !has_digit
                && word.chars().count() > 1
                && word.len() <= MAX_WORD_BYTES
                && out.len() < MAX_MISSPELLINGS
                && !self.dictionary.check(word)
            {
                out.push((s, s + word.len()));
            }
        };
        for (i, c) in text.char_indices() {
            let part = c.is_alphanumeric() || (c == '\'' && start.is_some());
            match (part, start) {
                (true, None) => {
                    start = Some(i);
                    has_digit = c.is_numeric();
                }
                (true, Some(_)) => has_digit |= c.is_numeric(),
                (false, Some(s)) => {
                    check(s, i, has_digit, &mut out);
                    start = None;
                }
                (false, None) => {}
            }
        }
        if let Some(s) = start {
            check(s, text.len(), has_digit, &mut out);
        }
        out
    }

    /// Up to `MAX_SUGGESTIONS` corrections for `word`.
    pub fn suggest(&self, word: &str) -> Vec<String> {
        if !valid_word(word) {
            return Vec::new();
        }
        let mut out = Vec::new();
        self.dictionary.suggest(word, &mut out);
        out.truncate(MAX_SUGGESTIONS);
        out
    }

    /// Adds `word` to the dictionary and to the personal list (owner-only,
    /// written atomically). False if the word is not acceptable or the list
    /// cannot be written.
    pub fn add(&mut self, word: &str) -> bool {
        if !valid_word(word) || self.dictionary.add(word).is_err() {
            return false;
        }
        let Some(path) = &self.personal else {
            return true;
        };
        let mut words = read(path, MAX_PERSONAL_BYTES).unwrap_or_default();
        if !words.is_empty() && !words.ends_with('\n') {
            words.push('\n');
        }
        words.push_str(word);
        words.push('\n');
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        u64::try_from(words.len()).is_ok_and(|n| n <= MAX_PERSONAL_BYTES)
            && fsio::write_atomic_with_mode(path, words.as_bytes(), None, FileMode::OwnerOnly)
                .is_ok()
    }
}

/// A word that may be checked, suggested or added: letters (and inner
/// apostrophes), no spaces or controls, at most `MAX_WORD_BYTES`.
fn valid_word(word: &str) -> bool {
    !word.is_empty()
        && word.len() <= MAX_WORD_BYTES
        && word.chars().all(|c| c.is_alphabetic() || c == '\'')
        && !word.starts_with('/')
}
