//! Fuzz target: single-instance requests from another process (threat T8).
#![no_main]

use ferny_core::ipc;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let Ok(locations) = ipc::decode_request(bytes) else {
        return;
    };
    assert!(locations.len() <= ipc::MAX_REQUEST_FILES);
    for location in &locations {
        assert!(location.path.is_absolute());
        assert!(location.line != Some(0) && location.column != Some(0));
    }
    // Decoding may normalize accepted numeric spellings (for example `02` to
    // `2`), so compare semantic locations after encoding instead of requiring
    // the original wire bytes to be preserved.
    let encoded = ipc::encode_request(&locations).expect("decoded request encodes");
    assert_eq!(ipc::decode_request(&encoded), Ok(locations));
});
