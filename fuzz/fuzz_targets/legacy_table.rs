//! The legacy table decoder (`query()` before the columnar path):
//! untrusted `ResponseData` bytes, optional zstd, then the table. Any input
//! must fail cleanly or decode within the batch limit.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    praevis_thetadata_client::fuzzing::legacy_table(data);
});
