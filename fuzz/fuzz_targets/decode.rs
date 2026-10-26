//! Fuzz target: encoding detection, decoding, line-ending detection and
//! re-encoding of arbitrary file bytes (threat T6).
#![no_main]

use ferny_core::text;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let Ok(decoded) = text::decode(bytes, None) else {
        return;
    };
    let _ = text::detect_eol(&decoded.text);
    if decoded.malformed {
        return;
    }
    // Unicode files must be written back byte for byte.
    let unicode = ["UTF-8", "UTF-16LE", "UTF-16BE"].contains(&decoded.encoding.name());
    if unicode {
        let encoded = text::encode(&decoded.text, decoded.encoding, decoded.bom)
            .expect("Unicode encodings represent every character");
        assert_eq!(encoded, bytes, "lossless round trip for {}", decoded.encoding.name());
    }
});
