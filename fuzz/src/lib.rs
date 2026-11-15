//! Shared settings for the fuzz targets.
#![forbid(unsafe_code)]

use praevis_thetadata_arrow::{ColumnarDecoder, Limits};

/// Small decoder limits, so a single case cannot exhaust the fuzzer's
/// memory (the production defaults allow 64 MiB messages).
pub fn decoder() -> ColumnarDecoder {
    let mut limits = Limits::default();
    limits.encoded_bytes = 1 << 20;
    limits.decompressed_bytes = 4 << 20;
    limits.text_bytes = 64 << 10;
    limits.rows_per_message = 10_000;
    limits.buffered_bytes = 16 << 20;
    limits.zstd_window_log = 20;
    ColumnarDecoder::new(limits).expect("valid limits")
}
