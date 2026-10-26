//! Text encodings and line endings.
//!
//! Decoding order:
//! a byte-order mark, then valid UTF-8, then `chardetng` on a bounded
//! prefix. Encoding fails closed: characters the target encoding cannot
//! represent are an error, never silently replaced.

use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE};

/// Bytes inspected by the encoding detector and the binary check.
pub const DETECTION_PREFIX_BYTES: usize = 64 * 1024;

/// Encodings offered to the user for "reload with" and "set encoding",
/// by their WHATWG names. Every one of them can also be written back.
pub const SUPPORTED_ENCODINGS: &[&str] = &[
    "UTF-8",
    "UTF-16LE",
    "UTF-16BE",
    "windows-1252",
    "ISO-8859-2",
    "ISO-8859-3",
    "ISO-8859-4",
    "ISO-8859-5",
    "ISO-8859-6",
    "ISO-8859-7",
    "ISO-8859-8",
    "ISO-8859-10",
    "ISO-8859-13",
    "ISO-8859-14",
    "ISO-8859-15",
    "ISO-8859-16",
    "windows-1250",
    "windows-1251",
    "windows-1253",
    "windows-1254",
    "windows-1255",
    "windows-1256",
    "windows-1257",
    "windows-1258",
    "windows-874",
    "KOI8-R",
    "KOI8-U",
    "IBM866",
    "macintosh",
    "Shift_JIS",
    "EUC-JP",
    "ISO-2022-JP",
    "GBK",
    "gb18030",
    "Big5",
    "EUC-KR",
];

/// Line-ending convention of a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eol {
    /// `\n` (Unix, macOS).
    Lf,
    /// `\r\n` (Windows).
    CrLf,
    /// `\r` (classic Mac OS).
    Cr,
}

impl Eol {
    /// The platform's convention, used for new documents and for files
    /// without line breaks.
    pub fn native() -> Self {
        if cfg!(windows) { Self::CrLf } else { Self::Lf }
    }
}

/// Dominant line ending of `text` and whether more than one kind occurs.
pub fn detect_eol(text: &str) -> (Eol, bool) {
    let (mut lf, mut crlf, mut cr) = (0usize, 0usize, 0usize);
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\r' if bytes.get(i + 1) == Some(&b'\n') => {
                crlf += 1;
                i += 1;
            }
            b'\r' => cr += 1,
            b'\n' => lf += 1,
            _ => {}
        }
        i += 1;
    }
    let kinds = [lf, crlf, cr].iter().filter(|&&n| n > 0).count();
    let eol = if lf == 0 && crlf == 0 && cr == 0 {
        Eol::native()
    } else if crlf >= lf && crlf >= cr {
        Eol::CrLf
    } else if lf >= cr {
        Eol::Lf
    } else {
        Eol::Cr
    };
    (eol, kinds > 1)
}

/// Result of decoding a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    /// The text, as UTF-8.
    pub text: String,
    /// The encoding the bytes were decoded with.
    pub encoding: &'static Encoding,
    /// Whether the bytes started with a byte-order mark (not included in
    /// `text`).
    pub bom: bool,
    /// Whether malformed sequences were replaced with U+FFFD. Saving such
    /// a text would not reproduce the original bytes.
    pub malformed: bool,
}

/// Why bytes could not be decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    /// Not valid UTF-8 and contains NUL bytes: treated as binary.
    #[error("binary content")]
    Binary,
}

/// Why text could not be encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EncodeError {
    /// The encoding cannot represent some character of the text.
    #[error("text not representable in the encoding")]
    Unencodable,
}

/// Looks up an encoding the user can choose, by label (case-insensitive,
/// WHATWG labels such as `latin1` included). Labels whose encoder would
/// write a different encoding (`replacement`, …) are refused.
pub fn encoding_for_label(label: &str) -> Option<&'static Encoding> {
    let encoding = Encoding::for_label(label.trim().as_bytes())?;
    let writable =
        encoding == UTF_16LE || encoding == UTF_16BE || encoding.output_encoding() == encoding;
    writable.then_some(encoding)
}

/// Decodes file bytes. With `forced`, that encoding is used (a matching
/// BOM is still stripped); otherwise it is detected.
pub fn decode(bytes: &[u8], forced: Option<&'static Encoding>) -> Result<Decoded, DecodeError> {
    if let Some((bom_encoding, bom_len)) = Encoding::for_bom(bytes)
        && (forced.is_none() || forced == Some(bom_encoding))
    {
        return Ok(decode_with(bom_encoding, &bytes[bom_len..], true));
    }
    let encoding = match forced {
        Some(encoding) => encoding,
        None if std::str::from_utf8(bytes).is_ok() => UTF_8,
        None => {
            let prefix = &bytes[..bytes.len().min(DETECTION_PREFIX_BYTES)];
            if prefix.contains(&0) {
                return Err(DecodeError::Binary);
            }
            detect(prefix, prefix.len() == bytes.len())
        }
    };
    Ok(decode_with(encoding, bytes, false))
}

fn detect(prefix: &[u8], whole_file: bool) -> &'static Encoding {
    let mut detector = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Deny);
    detector.feed(prefix, whole_file);
    // UTF-8 was ruled out: the whole file is not valid UTF-8.
    detector.guess(None, chardetng::Utf8Detection::Deny)
}

fn decode_with(encoding: &'static Encoding, bytes: &[u8], bom: bool) -> Decoded {
    let (text, malformed) = encoding.decode_without_bom_handling(bytes);
    Decoded {
        text: text.into_owned(),
        encoding,
        bom,
        malformed,
    }
}

/// Encodes `text` for writing, with a BOM if `bom` is set and the
/// encoding is a Unicode one.
pub fn encode(text: &str, encoding: &'static Encoding, bom: bool) -> Result<Vec<u8>, EncodeError> {
    if encoding == UTF_8 {
        let mut out = Vec::with_capacity(text.len() + 3);
        if bom {
            out.extend_from_slice(b"\xEF\xBB\xBF");
        }
        out.extend_from_slice(text.as_bytes());
        return Ok(out);
    }
    if encoding == UTF_16LE || encoding == UTF_16BE {
        let little = encoding == UTF_16LE;
        let mut out = Vec::with_capacity(text.len() * 2 + 2);
        let mut push = |unit: u16| {
            out.extend_from_slice(&if little {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            });
        };
        if bom {
            push(0xFEFF);
        }
        text.encode_utf16().for_each(&mut push);
        return Ok(out);
    }
    if encoding.output_encoding() != encoding {
        return Err(EncodeError::Unencodable);
    }
    let (bytes, _, unmappable) = encoding.encode(text);
    if unmappable {
        return Err(EncodeError::Unencodable);
    }
    Ok(bytes.into_owned())
}
