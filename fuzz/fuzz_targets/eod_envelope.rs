//! The client's own bounded envelope parser and stock EOD decoders (the
//! table and the numeric paths) on untrusted bytes.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    praevis_thetadata_client::fuzzing::eod_envelope(data);
});
