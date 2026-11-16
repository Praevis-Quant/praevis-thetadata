//! Untrusted bytes as a zstd-compressed table: the frame handling (window
//! limit, declared size, truncation) and whatever it decompresses to.
#![no_main]
use libfuzzer_sys::fuzz_target;
use praevis_thetadata_proto::endpoints::{CompressionDescription, ResponseData};
use prost::Message;

fuzz_target!(|data: &[u8]| {
    // The first four bytes choose the declared original size.
    let (size, payload) = data.split_at(data.len().min(4));
    let mut declared = [0u8; 4];
    declared[..size.len()].copy_from_slice(size);
    let response = ResponseData {
        compression_description: Some(CompressionDescription {
            algo: 1,
            ..Default::default()
        }),
        compressed_data: payload.to_vec(),
        original_size: i32::from_le_bytes(declared),
        ..Default::default()
    };
    let mut decoder = praevis_thetadata_fuzz::decoder();
    let _ = decoder.push_encoded(&response.encode_to_vec());
    let _ = decoder.flush();
});
