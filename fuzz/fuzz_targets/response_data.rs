//! A response envelope from the network: protobuf, optional zstd, then the
//! columnar decoder. Any input must fail cleanly or decode within limits.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut decoder = praevis_thetadata_fuzz::decoder();
    // Feed it twice: the second message exercises the schema and the
    // already-buffered rows of the first.
    if decoder.push_encoded(data).is_ok() {
        let _ = decoder.push_encoded(data);
    }
    let _ = decoder.flush();
});
