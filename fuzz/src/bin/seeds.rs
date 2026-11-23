//! Writes the seed corpora: synthetic tables from the emulator's fixtures
//! and synthetic Terminal events. Never market data.
//!
//! Usage: seeds <output directory>; writes <dir>/<target>/<n> files.
use praevis_thetadata_emulator::fixture::{self, Shape};
use prost::Message;
use std::path::Path;

fn write(dir: &Path, target: &str, seeds: Vec<Vec<u8>>) {
    let out = dir.join(target);
    std::fs::create_dir_all(&out).expect("seed directory");
    for (n, seed) in seeds.into_iter().enumerate() {
        std::fs::write(out.join(format!("seed-{n:02}")), seed).expect("seed");
    }
}

fn main() {
    let dir = std::env::args()
        .nth(1)
        .expect("usage: seeds <output directory>");
    let dir = Path::new(&dir);
    let mut envelopes = Vec::new();
    let mut payloads = Vec::new();
    for shape in [Shape::Mixed, Shape::Nulls, Shape::Timestamps, Shape::Prices] {
        for rows in [0, 1, 7] {
            let (table, _) = fixture::table(shape, rows);
            for zstd in [false, true] {
                envelopes.push(fixture::response(&table, zstd).encode_to_vec());
            }
            let response = fixture::response(&table, true);
            let mut payload = response.original_size.to_le_bytes().to_vec();
            payload.extend(response.compressed_data);
            payloads.push(payload);
        }
    }
    write(dir, "response_data", envelopes.clone());
    write(dir, "eod_envelope", envelopes.clone());
    write(dir, "legacy_table", envelopes.clone());
    // gRPC frames: the first byte chooses the chunk size; then length-
    // prefixed messages (flag 0, big-endian length), one per envelope.
    let frames = envelopes
        .iter()
        .take(8)
        .enumerate()
        .map(|(n, envelope)| {
            let mut frame = vec![n as u8];
            frame.push(0);
            frame.extend((envelope.len() as u32).to_be_bytes());
            frame.extend(envelope);
            frame
        })
        .collect();
    write(dir, "grpc_framing", frames);
    write(dir, "zstd_payload", payloads);
    let events = [
        r#"{"header":{"type":"STATUS","status":"CONNECTED"}}"#,
        r#"{"header":{"type":"REQ_RESPONSE","response":"SUBSCRIBED","req_id":1}}"#,
        r#"{"header":{"type":"QUOTE","status":"CONNECTED"},"contract":{"security_type":"OPTION","root":"SYNTH","expiration":20300101,"strike":100000,"right":"C"},"quote":{"ms_of_day":1000,"bid_size":1,"bid_exchange":1,"bid":1.25,"bid_condition":0,"ask_size":1,"ask_exchange":1,"ask":1.5,"ask_condition":0,"date":20300101}}"#,
        r#"{"header":{"type":"TRADE","status":"CONNECTED"},"contract":{"security_type":"STOCK","root":"SYNTH"},"trade":{"ms_of_day":1000,"sequence":1,"size":10,"condition":0,"price":2.5,"exchange":1,"date":20300101}}"#,
    ];
    write(
        dir,
        "terminal_event",
        events.iter().map(|e| e.as_bytes().to_vec()).collect(),
    );
}
