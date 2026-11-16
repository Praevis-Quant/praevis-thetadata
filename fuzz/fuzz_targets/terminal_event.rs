//! A Theta Terminal WebSocket event (JSON text) as the stream crate parses it.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = praevis_thetadata_stream::parse(text);
    }
});
