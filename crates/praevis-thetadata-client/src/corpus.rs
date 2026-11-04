//! Adversarial decoder corpus (EOD-L3-021, deferred item 2, decision D-G).
//!
//! Plain Rust, deterministic and dependency-free. Structured catalogues
//! (exhaustive truncations, bit flips and interesting bytes, varint and length
//! edges, wire-type and tag matrices, groups, deep nesting, repeated fields,
//! envelope fields and ZSTD framing) plus seeded random mutations go through
//! the envelope parser, the bounded decoder (numeric and Table) and the legacy
//! raw decoder. Every input must be accepted within the limits or rejected
//! with a classified error, never a panic. Differential oracles compare the
//! envelope parser with prost, the bounded decoder with the legacy decoder,
//! and the columnar Arrow decoder with both (P1 PR 3).
//!
//! The short run is part of `cargo test`. The long run is manual:
//!
//! ```text
//! PRAEVIS_CORPUS_CASES=2000000 PRAEVIS_CORPUS_SEED=7 \
//!   cargo test --release -p praevis-thetadata-client --lib corpus -- --ignored --nocapture
//! ```
//!
//! Coverage-guided fuzzing is a separate roadmap item for after 0.1.0.
use crate::{bounded, decode, envelope, eod::EodError, wire};
use bounded::DecodeLimits;
use praevis_thetadata_arrow as columnar;
use praevis_thetadata_core::{Table, Value};
use praevis_thetadata_proto::endpoints::{
    CompressionDescription, DataTable, DataValue, DataValueList, Price, ZonedDateTime,
    data_value::DataType,
};
use prost::Message;
use std::collections::BTreeMap;

fn columnar_limits(limits: &DecodeLimits) -> columnar::Limits {
    columnar::Limits {
        encoded_bytes: limits.encoded_bytes,
        decompressed_bytes: limits.decompressed_bytes,
        headers: limits.headers,
        header_bytes: limits.header_bytes,
        text_bytes: limits.text_bytes,
        rows_per_message: limits.rows,
        buffered_bytes: limits.allocated_bytes,
        zstd_window_log: limits.zstd_window_log,
    }
}

/// A comparable cell: prices as Decimal128(38, 10) unscaled values (the legacy
/// `Price` keeps its wire scale, so equal numbers may differ in representation)
/// and timestamps as the legacy RFC 3339 text.
#[derive(Debug, PartialEq)]
enum Exact {
    Null,
    Text(String),
    Integer(i64),
    Price(i128),
    Boolean(bool),
    Timestamp(String),
}

fn legacy_exact(table: &Table) -> (Vec<String>, Vec<Vec<Exact>>) {
    let rows = table
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|value| match value {
                    Value::Null => Exact::Null,
                    Value::Text(v) => Exact::Text(v.clone()),
                    Value::Integer(v) => Exact::Integer(*v),
                    Value::Boolean(v) => Exact::Boolean(*v),
                    Value::Price(p) => {
                        Exact::Price(i128::from(p.mantissa) * 10i128.pow((p.exponent + 10) as u32))
                    }
                    Value::Timestamp(v) => Exact::Timestamp(v.clone()),
                })
                .collect()
        })
        .collect();
    (table.headers.clone(), rows)
}

fn arrow_exact(batch: &columnar::arrow_array::RecordBatch) -> (Vec<String>, Vec<Vec<Exact>>) {
    use columnar::arrow_array::Array;
    use columnar::arrow_array::cast::AsArray;
    use columnar::arrow_array::types::{Decimal128Type, Int64Type, TimestampMillisecondType};
    use columnar::arrow_schema::DataType;
    let headers = batch
        .schema()
        .fields()
        .iter()
        .map(|f| f.name().clone())
        .collect();
    let rows = (0..batch.num_rows())
        .map(|row| {
            batch
                .columns()
                .iter()
                .map(|column| {
                    if column.is_null(row) || column.data_type() == &DataType::Null {
                        return Exact::Null;
                    }
                    match column.data_type() {
                        DataType::Utf8 => Exact::Text(column.as_string::<i32>().value(row).into()),
                        DataType::Int64 => {
                            Exact::Integer(column.as_primitive::<Int64Type>().value(row))
                        }
                        DataType::Boolean => Exact::Boolean(column.as_boolean().value(row)),
                        DataType::Decimal128(38, 10) => {
                            Exact::Price(column.as_primitive::<Decimal128Type>().value(row))
                        }
                        DataType::Timestamp(_, Some(tz)) => {
                            let ms = column.as_primitive::<TimestampMillisecondType>().value(row);
                            let zone = if tz.as_ref() == columnar::UTC { 1 } else { 0 };
                            Exact::Timestamp(
                                praevis_thetadata_core::timestamp(ms as u64, zone).unwrap(),
                            )
                        }
                        other => panic!("unexpected Arrow type {other:?}"),
                    }
                })
                .collect()
        })
        .collect();
    (headers, rows)
}

fn limits() -> DecodeLimits {
    DecodeLimits {
        encoded_bytes: 4096,
        decompressed_bytes: 4096,
        headers: 4,
        rows: 8,
        cells: 24,
        text_bytes: 16,
        header_bytes: 16,
        // ZSTD reserves a fixed 16 MiB context plus the 1 MiB window; a budget just
        // above it lets compressed inputs reach the decoder.
        allocated_bytes: 18 << 20,
        zstd_window_log: 20,
    }
}

/// Deterministic xorshift; reproducible failures without a dependency.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push(value as u8 | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}
fn key(out: &mut Vec<u8>, tag: u64, wire_type: u64) {
    varint(out, (tag << 3) | wire_type);
}
fn ld(tag: u64, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    key(&mut out, tag, 2);
    varint(&mut out, body.len() as u64);
    out.extend_from_slice(body);
    out
}
fn cell(data_type: DataType) -> DataValue {
    DataValue {
        data_type: Some(data_type),
    }
}
/// Encoded `DataTable` seeds: every value kind, and one at the limits.
fn seeds() -> Vec<Vec<u8>> {
    let row = |values| DataValueList { values };
    vec![
        DataTable {
            headers: vec!["a".into(), "b".into(), "c".into()],
            data_table: vec![
                row(vec![
                    cell(DataType::Number(-1)),
                    cell(DataType::Price(Price {
                        value: 12345,
                        r#type: 8,
                    })),
                    cell(DataType::Timestamp(ZonedDateTime {
                        epoch_ms: 1_700_000_000_000,
                        zone: 1,
                        ..Default::default()
                    })),
                ]),
                row(vec![
                    cell(DataType::NullValue(0)),
                    cell(DataType::Boolean(true)),
                    cell(DataType::Text("é".into())),
                ]),
            ],
        }
        .encode_to_vec(),
        DataTable {
            headers: vec!["x".into(), "y".into(), "z".into()],
            data_table: (0..8)
                .map(|_| {
                    row((0..3)
                        .map(|_| cell(DataType::Text("0123456789abcde".into())))
                        .collect())
                })
                .collect(),
        }
        .encode_to_vec(),
    ]
}
fn envelope_bytes(payload: &[u8], algo: i32, original_size: i32) -> Vec<u8> {
    wire::ResponseData {
        compressed_data: payload.to_vec(),
        compression_description: Some(CompressionDescription { algo, level: 1 }),
        original_size,
        flat_file_manifest: None,
    }
    .encode_to_vec()
}
fn zstd(bytes: &[u8]) -> Vec<u8> {
    zstd::stream::encode_all(bytes, 1).unwrap()
}

/// Classify one encoded envelope through every decoder and check the oracles.
/// Returns the outcome category; panics only on an oracle violation.
fn check(bytes: &[u8], limits: &DecodeLimits, stats: &mut Stats) -> String {
    let parsed = envelope::parse(bytes.to_vec(), limits);
    // Oracle 1: the bounded envelope parser agrees with prost on every input
    // both accept, and never accepts what prost rejects.
    match (&parsed, wire::ResponseData::decode(bytes)) {
        (Ok(ours), Ok(prost)) => {
            assert_eq!(ours.compressed_data, prost.compressed_data, "{bytes:?}");
            assert_eq!(
                ours.compression_description, prost.compression_description,
                "{bytes:?}"
            );
            assert_eq!(ours.original_size, prost.original_size, "{bytes:?}");
        }
        (Ok(_), Err(_)) => panic!("envelope accepted what prost rejects: {bytes:?}"),
        (Err(_), _) => {}
    }
    // Oracle 6: the columnar decoder's in-place envelope reader classifies
    // like the bounded envelope parser, and otherwise equals pushing the
    // parsed response.
    let mut in_place = columnar::ColumnarDecoder::new(columnar_limits(limits)).unwrap();
    let in_place = in_place.push_encoded(bytes).map(|_| in_place.flush());
    let response = match parsed {
        Ok(response) => response,
        Err(error) => {
            match &in_place {
                Err(ours) => assert_eq!(classify_columnar(ours), format!("{error:?}"), "{bytes:?}"),
                Ok(_) => panic!("in-place envelope accepted what the parser rejects: {bytes:?}"),
            }
            return format!("envelope {error:?}");
        }
    };
    let results: Vec<_> = [false, true]
        .into_iter()
        .map(|table| bounded::decode(response.clone(), limits, None, table))
        .collect();
    // Oracle 2: numeric and Table modes classify every input identically.
    assert_eq!(
        results[0].as_ref().map(|b| b.cells().len()).map_err(|e| *e),
        results[1].as_ref().map(|b| b.cells().len()).map_err(|e| *e),
        "{bytes:?}"
    );
    let mut arrow = columnar::ColumnarDecoder::new(columnar_limits(limits)).unwrap();
    let arrow = arrow.push(&response).map(|_| arrow.flush());
    assert_eq!(in_place, arrow, "{bytes:?}");
    let legacy = decode::decode(response, limits.decompressed_bytes);
    // Oracle 5: the columnar decoder accepts only what the bounded decoder
    // accepts (apart from the bounded decoder's cell and allocation budgets,
    // which it does not have) and rejects with the same class. Its one extra
    // class is mixing kinds or zones in a column, which Arrow cannot
    // represent; it stops there, before any later value the bounded decoder
    // may reject.
    match (&arrow, &results[0]) {
        (Ok(_), Ok(_)) => stats.columnar_agree += 1,
        (_, Err(EodError::Resource("cells" | "allocation bytes"))) => {}
        // A corrupt ZSTD frame that also expands past the limit: which of the
        // two is reported first depends on the output buffer size.
        (Err(columnar::DecodeError::Resource("decompressed bytes")), Err(EodError::Decode))
        | (Err(columnar::DecodeError::Decode), Err(EodError::Resource("decompressed bytes"))) => {
            stats.columnar_zstd_order += 1
        }
        (Err(columnar::DecodeError::ColumnType { .. }), _) => stats.columnar_type_only += 1,
        (Err(ours), Err(theirs)) => {
            assert_eq!(classify_columnar(ours), format!("{theirs:?}"), "{bytes:?}")
        }
        (ours, theirs) => panic!("columnar {ours:?} vs bounded {theirs:?}: {bytes:?}"),
    }
    match (&results[0], legacy) {
        (Ok(batch), legacy) => {
            // Oracle 3: accepted output stays within every count limit.
            assert!(batch.row_count() <= limits.rows, "{bytes:?}");
            assert!(batch.headers().len() <= limits.headers, "{bytes:?}");
            assert!(batch.cells().len() <= limits.cells, "{bytes:?}");
            // Oracle 4: when the unbounded legacy decoder also accepts, both
            // decoders produce the same exact values.
            match legacy {
                Ok(table) => {
                    if let Ok(arrow) = &arrow {
                        let expected = legacy_exact(&table);
                        let actual = arrow.as_ref().map(arrow_exact);
                        // A message with headers and no rows yields no batch.
                        match actual {
                            Some(actual) => assert_eq!(actual, expected, "{bytes:?}"),
                            None => assert!(table.rows.is_empty(), "{bytes:?}"),
                        }
                    }
                    let ours = results[0].clone().unwrap().into_table();
                    assert_eq!(ours, table, "{bytes:?}");
                    stats.agree += 1;
                }
                Err(error) => {
                    *stats
                        .legacy_only_rejects
                        .entry(format!("{error:?}"))
                        .or_default() += 1
                }
            }
            "accepted".into()
        }
        (Err(error), _) => format!("{error:?}"),
    }
}

/// The bounded decoder's error name for a columnar error of the same class.
fn classify_columnar(error: &columnar::DecodeError) -> String {
    use columnar::DecodeError as E;
    match error {
        E::Decode => "Decode".into(),
        E::Unsupported => "Unsupported".into(),
        E::Schema => "Schema".into(),
        E::Resource(name) => format!("Resource({name:?})"),
        other => format!("{other:?}"),
    }
}

#[derive(Default)]
struct Stats {
    outcomes: BTreeMap<String, u64>,
    agree: u64,
    columnar_agree: u64,
    columnar_type_only: u64,
    columnar_zstd_order: u64,
    legacy_only_rejects: BTreeMap<String, u64>,
}
impl Stats {
    fn record(&mut self, bytes: &[u8], limits: &DecodeLimits) -> String {
        let outcome = check(bytes, limits, self);
        *self.outcomes.entry(outcome.clone()).or_default() += 1;
        outcome
    }
}

/// Every prefix, every single-bit flip and every position set to an
/// interesting byte, for both the inner table and the whole envelope.
fn exhaustive_byte_edits(stats: &mut Stats, limits: &DecodeLimits) -> u64 {
    let mut cases = 0;
    for seed in seeds() {
        for compressed in [false, true] {
            let inner_variants = |inner: &[u8]| {
                if compressed {
                    envelope_bytes(&zstd(inner), 1, 0)
                } else {
                    envelope_bytes(inner, 0, 0)
                }
            };
            let envelope = inner_variants(&seed);
            for target in [&seed, &envelope] {
                let wrap = |edited: Vec<u8>| {
                    if std::ptr::eq(target, &seed) {
                        inner_variants(&edited)
                    } else {
                        edited
                    }
                };
                for end in 0..target.len() {
                    stats.record(&wrap(target[..end].to_vec()), limits);
                    cases += 1;
                }
                for at in 0..target.len() {
                    for bit in 0..8 {
                        let mut edited = target.clone();
                        edited[at] ^= 1 << bit;
                        stats.record(&wrap(edited), limits);
                        cases += 1;
                    }
                    for byte in [0x00, 0x01, 0x7f, 0x80, 0xff] {
                        let mut edited = target.clone();
                        edited[at] = byte;
                        stats.record(&wrap(edited), limits);
                        cases += 1;
                    }
                }
            }
        }
    }
    cases
}

/// Hand-built protobuf edge cases inside a `DataTable`, wrapped uncompressed.
fn structural_catalogue() -> Vec<(&'static str, Vec<u8>)> {
    let mut cases = Vec::new();
    let text = |s: &str| ld(1, s.as_bytes());
    // Length and varint edges on a header field.
    for (name, length) in [
        ("length zero", 0u64),
        ("length remaining+1", 2),
        ("length 2^31", 1 << 31),
        ("length 2^32", 1 << 32),
        ("length u64::MAX", u64::MAX),
    ] {
        let mut out = Vec::new();
        key(&mut out, 1, 2);
        varint(&mut out, length);
        out.push(b'a');
        cases.push((name, out));
    }
    let mut overlong = Vec::new();
    key(&mut overlong, 1, 2);
    overlong.extend_from_slice(&[
        0x81, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x00,
    ]);
    cases.push(("11-byte varint length", overlong));
    let mut padded = Vec::new();
    key(&mut padded, 1, 2);
    padded.extend_from_slice(&[0x81, 0x00, b'a']);
    cases.push(("non-canonical padded length", padded));
    let mut unterminated = Vec::new();
    key(&mut unterminated, 1, 2);
    unterminated.extend_from_slice(&[0x80, 0x80]);
    cases.push(("unterminated varint", unterminated));
    // Tag edges.
    for (name, tag) in [
        ("field 0", 0u64),
        ("max field", (1 << 29) - 1),
        ("field 2^29", 1 << 29),
    ] {
        let mut out = Vec::new();
        key(&mut out, tag, 0);
        varint(&mut out, 1);
        cases.push((name, out));
    }
    // Every wire type on each known field of each nested message level.
    let value = |body: Vec<u8>| ld(2, &ld(1, &body));
    for wire_type in 0..8u64 {
        for (name, build) in [
            ("header", &(|k: Vec<u8>| k) as &dyn Fn(Vec<u8>) -> Vec<u8>),
            ("row", &|k: Vec<u8>| ld(2, &k)),
            ("cell", &|k: Vec<u8>| value(k)),
            ("price", &|k: Vec<u8>| value(ld(3, &k))),
            ("timestamp", &|k: Vec<u8>| value(ld(4, &k))),
        ] {
            for tag in 1..=6u64 {
                let mut field = Vec::new();
                key(&mut field, tag, wire_type);
                match wire_type {
                    0 => varint(&mut field, 7),
                    1 => field.extend_from_slice(&[0; 8]),
                    2 => field.extend_from_slice(&[1, b'a']),
                    5 => field.extend_from_slice(&[0; 4]),
                    _ => {}
                }
                let _ = name;
                cases.push(("wire-type matrix", build(field)));
            }
        }
    }
    // Groups: unbalanced start, lone end, and balanced unknown groups.
    let mut start = Vec::new();
    key(&mut start, 9, 3);
    cases.push(("unterminated group", start.clone()));
    let mut end = Vec::new();
    key(&mut end, 9, 4);
    cases.push(("lone end group", end.clone()));
    let mut balanced = start.clone();
    balanced.extend_from_slice(&end);
    balanced.extend(text("a"));
    cases.push(("balanced unknown group", balanced.clone()));
    cases.push(("group inside a cell", [text("h"), value(balanced)].concat()));
    // Deep nesting. Unknown length-delimited fields are skipped by length, so
    // depth is irrelevant; groups are skipped recursively under prost's
    // recursion limit (100), so 50 levels are accepted and 1000 rejected.
    let mut nested = Vec::new();
    for _ in 0..1000 {
        nested = ld(15, &nested);
    }
    cases.push((
        "1000-deep unknown messages in a cell",
        [text("h"), value(nested)].concat(),
    ));
    let groups = |depth: usize| {
        let mut groups = Vec::new();
        for _ in 0..depth {
            let mut level = Vec::new();
            key(&mut level, 9, 3);
            level.extend_from_slice(&groups);
            key(&mut level, 9, 4);
            groups = level;
        }
        groups
    };
    cases.push((
        "50-deep groups in a cell",
        [text("h"), value(groups(50))].concat(),
    ));
    cases.push(("1000-deep groups at top level", groups(1000)));
    cases.push((
        "1000-deep groups in a cell",
        [text("h"), value(groups(1000))].concat(),
    ));
    // Repeated scalar and oneof fields: last value wins, messages merge.
    let mut number_twice = Vec::new();
    key(&mut number_twice, 2, 0);
    varint(&mut number_twice, 5);
    key(&mut number_twice, 2, 0);
    varint(&mut number_twice, 6);
    cases.push(("number twice", [text("h"), value(number_twice)].concat()));
    let price_parts = [ld(3, &[0x08, 0x05]), ld(3, &[0x10, 0x03])].concat();
    cases.push((
        "price merged from two parts",
        [text("h"), value(price_parts)].concat(),
    ));
    let switch = [ld(1, b"t"), {
        let mut n = Vec::new();
        key(&mut n, 2, 0);
        varint(&mut n, 1);
        n
    }]
    .concat();
    cases.push(("oneof switches kind", [text("h"), value(switch)].concat()));
    // Out-of-range enumerations and values.
    for (name, body) in [
        ("price scale 99", ld(3, &[0x08, 0x01, 0x10, 99])),
        (
            "price scale -1",
            ld(3, &[0x08, 0x01, 0x10, 0xff, 0xff, 0xff, 0xff, 0x0f]),
        ),
        ("timestamp zone 999", ld(4, &[0x08, 0x01, 0x10, 0xe7, 0x07])),
        ("timestamp epoch u64::MAX", {
            let mut t = Vec::new();
            key(&mut t, 1, 0);
            varint(&mut t, u64::MAX);
            ld(4, &t)
        }),
        ("timestamp nanos", ld(4, &[0x08, 0x01, 0x18, 0x01])),
        ("timestamp precision 2", ld(4, &[0x08, 0x01, 0x20, 0x02])),
        ("invalid UTF-8 text", ld(1, &[0xc3, 0x28])),
        ("null value 7", vec![0x28, 0x07]),
        ("boolean 2", vec![0x30, 0x02]),
    ] {
        cases.push((name, [text("h"), value(body)].concat()));
    }
    cases.push(("invalid UTF-8 header", ld(1, &[0xff, 0xfe])));
    cases.push((
        "ragged rows",
        [text("a"), text("b"), value(vec![0x30, 0x00])].concat(),
    ));
    cases.push(("rows without headers", value(vec![0x30, 0x00])));
    cases
}

/// Envelope field edges around a valid uncompressed payload.
fn envelope_catalogue(limits: &DecodeLimits) -> Vec<(&'static str, Vec<u8>)> {
    let payload = seeds()[0].clone();
    let limit = limits.decompressed_bytes as i32;
    let mut cases = vec![
        ("empty envelope", Vec::new()),
        ("algo 2", envelope_bytes(&payload, 2, 0)),
        ("algo -1", envelope_bytes(&payload, -1, 0)),
        ("algo i32::MAX", envelope_bytes(&payload, i32::MAX, 0)),
    ];
    for (name, size) in [
        ("original size -1", -1),
        ("original size 1", 1),
        ("original size at limit", limit),
        ("original size over limit", limit + 1),
        ("original size i32::MAX", i32::MAX),
    ] {
        cases.push((name, envelope_bytes(&payload, 0, size)));
    }
    let mut manifest = envelope_bytes(&payload, 0, 0);
    manifest.extend(ld(4, &[0x08, 0x01]));
    cases.push(("flat-file manifest", manifest));
    let twice = [ld(1, b"garbage"), envelope_bytes(&payload, 0, 0)].concat();
    cases.push(("payload twice, last wins", twice));
    let merged = [ld(1, &payload), ld(2, &[0x08, 0x01]), ld(2, &[0x10, 0x05])].concat();
    cases.push((
        "compression description merged, algo 1 on raw bytes",
        merged,
    ));
    let mut overhead = envelope_bytes(&payload, 0, 0);
    overhead.extend(ld(9, &[0; 2000]));
    cases.push(("unknown field over the overhead allowance", overhead));
    let mut payload_varint = Vec::new();
    key(&mut payload_varint, 1, 0);
    varint(&mut payload_varint, 1);
    cases.push(("payload with varint wire type", payload_varint));
    cases.push(("oversized payload", envelope_bytes(&vec![0; 5000], 0, 0)));
    cases
}

/// ZSTD framing edges.
fn zstd_catalogue() -> Vec<(&'static str, Vec<u8>)> {
    let payload = seeds()[0].clone();
    let frame = zstd(&payload);
    let mut skippable = vec![0x50, 0x2a, 0x4d, 0x18];
    skippable.extend_from_slice(&4u32.to_le_bytes());
    skippable.extend_from_slice(b"skip");
    let empty_frame = zstd(&[]);
    let mut checksummed = zstd::bulk::Compressor::new(1).unwrap();
    checksummed
        .set_parameter(zstd::zstd_safe::CParameter::ChecksumFlag(true))
        .unwrap();
    let mut bad_checksum = checksummed.compress(&payload).unwrap();
    let last = bad_checksum.len() - 1;
    bad_checksum[last] ^= 1;
    let mut wide = zstd::stream::Encoder::new(Vec::new(), 1).unwrap();
    wide.set_parameter(zstd::zstd_safe::CParameter::WindowLog(27))
        .unwrap();
    wide.include_contentsize(false).unwrap();
    std::io::Write::write_all(&mut wide, &vec![7u8; 3 << 20]).unwrap();
    let wide = wide.finish().unwrap();
    // A single-shot frame declares its content size. With the single-segment
    // flag set and a one-byte size field, that byte follows the descriptor.
    let sized = zstd::bulk::compress(&payload, 1).unwrap();
    assert_eq!(
        sized[4] & 0xe3,
        0x20,
        "single segment, 1-byte content size, no dictionary"
    );
    let lie = |delta: i8| {
        let mut frame = sized.clone();
        frame[5] = frame[5].wrapping_add_signed(delta);
        frame
    };
    let bomb = zstd(&vec![0u8; 8 << 20]);
    let mut bad_magic = frame.clone();
    bad_magic[0] ^= 0xff;
    vec![
        ("valid frame", frame.clone()),
        (
            "trailing garbage",
            [frame.clone(), b"xyz".to_vec()].concat(),
        ),
        ("two frames", [frame.clone(), frame.clone()].concat()),
        (
            "skippable before",
            [skippable.clone(), frame.clone()].concat(),
        ),
        (
            "skippable after",
            [frame.clone(), skippable.clone()].concat(),
        ),
        ("only skippable", skippable),
        ("empty frame", empty_frame),
        ("empty input", Vec::new()),
        ("bad checksum", bad_checksum),
        ("window above the limit", wide),
        ("sized frame", sized.clone()),
        ("content size one larger", lie(1)),
        ("content size one smaller", lie(-1)),
        ("8 MiB bomb", bomb),
        ("bad magic", bad_magic),
        (
            "legacy v0.7 magic",
            vec![0x27, 0xb5, 0x2f, 0xfd, 0, 0, 0, 0],
        ),
    ]
}

fn random_mutations(stats: &mut Stats, limits: &DecodeLimits, cases: u64, seed: u64) {
    let seeds = seeds();
    let mut rng = Rng(seed.max(1));
    for case in 0..cases {
        let mut bytes = seeds[case as usize % seeds.len()].clone();
        for _ in 0..=rng.below(4) {
            if bytes.is_empty() {
                break;
            }
            let at = rng.below(bytes.len() as u64) as usize;
            match rng.below(7) {
                0 => bytes[at] ^= 1 << rng.below(8),
                1 => bytes.truncate(at),
                2 => bytes[at] = 0xff,
                3 => {
                    let end = (at + 1 + rng.below(8) as usize).min(bytes.len());
                    let copy = bytes[at..end].to_vec();
                    bytes.splice(at..at, copy);
                }
                4 => bytes.insert(at, rng.next() as u8),
                5 => {
                    let end = (at + 1 + rng.below(8) as usize).min(bytes.len());
                    bytes.drain(at..end);
                }
                _ => {
                    let other = &seeds[(case as usize + 1) % seeds.len()];
                    let from = rng.below(other.len() as u64) as usize;
                    let end = (from + 1 + rng.below(12) as usize).min(other.len());
                    bytes.splice(at..at, other[from..end].iter().copied());
                }
            }
        }
        let envelope = if rng.below(2) == 0 {
            envelope_bytes(&bytes, 0, 0)
        } else {
            let mut envelope = envelope_bytes(&zstd(&bytes), 1, 0);
            if rng.below(4) == 0 && !envelope.is_empty() {
                // Also mutate the compressed frame or the envelope itself.
                let at = rng.below(envelope.len() as u64) as usize;
                envelope[at] ^= 1 << rng.below(8);
            }
            envelope
        };
        stats.record(&envelope, limits);
    }
}

fn run(random_cases: u64, seed: u64) -> Stats {
    let limits = limits();
    let mut stats = Stats::default();
    let expect = |name: &str, bytes: Vec<u8>, stats: &mut Stats| -> String {
        let outcome = stats.record(&bytes, &limits);
        eprintln!("corpus case {name}: {outcome}");
        outcome
    };
    let mut named = BTreeMap::new();
    for (name, table) in structural_catalogue() {
        let plain = expect(name, envelope_bytes(&table, 0, 0), &mut stats);
        let compressed = expect(name, envelope_bytes(&zstd(&table), 1, 0), &mut stats);
        // Compression never changes how a table is classified.
        assert_eq!(plain, compressed, "{name}");
        if name != "wire-type matrix" {
            named.insert(name, plain);
        }
    }
    for (name, envelope) in envelope_catalogue(&limits) {
        named.insert(name, expect(name, envelope, &mut stats));
    }
    for (name, frame) in zstd_catalogue() {
        named.insert(name, expect(name, envelope_bytes(&frame, 1, 0), &mut stats));
    }
    // Pinned classifications of the catalogue edges.
    for (name, expected) in [
        ("11-byte varint length", "Decode"),
        ("length u64::MAX", "Decode"),
        ("length 2^32", "Decode"),
        ("non-canonical padded length", "accepted"),
        ("field 0", "Decode"),
        ("max field", "accepted"),
        ("lone end group", "Decode"),
        ("unterminated group", "Decode"),
        ("balanced unknown group", "accepted"),
        ("group inside a cell", "accepted"),
        ("50-deep groups in a cell", "accepted"),
        ("1000-deep groups in a cell", "Decode"),
        ("1000-deep unknown messages in a cell", "accepted"),
        ("1000-deep groups at top level", "Decode"),
        ("number twice", "accepted"),
        ("price merged from two parts", "accepted"),
        ("oneof switches kind", "accepted"),
        ("price scale 99", "Decode"),
        ("timestamp zone 999", "Decode"),
        ("timestamp epoch u64::MAX", "Decode"),
        ("timestamp nanos", "Unsupported"),
        ("timestamp precision 2", "Unsupported"),
        ("invalid UTF-8 text", "Decode"),
        ("invalid UTF-8 header", "Decode"),
        ("ragged rows", "Schema"),
        ("algo 2", "Unsupported"),
        (
            "original size over limit",
            "envelope Resource(\"declared bytes\")",
        ),
        (
            "original size i32::MAX",
            "envelope Resource(\"declared bytes\")",
        ),
        ("flat-file manifest", "envelope Unsupported"),
        ("payload twice, last wins", "accepted"),
        (
            "unknown field over the overhead allowance",
            "envelope Resource(\"envelope overhead\")",
        ),
        ("payload with varint wire type", "envelope Decode"),
        ("oversized payload", "envelope Resource(\"encoded bytes\")"),
        ("valid frame", "accepted"),
        ("two frames", "Resource(\"headers\")"),
        ("skippable before", "accepted"),
        ("skippable after", "accepted"),
        ("bad checksum", "Decode"),
        ("sized frame", "accepted"),
        ("content size one larger", "Decode"),
        ("content size one smaller", "Decode"),
        ("window above the limit", "Resource(\"zstd window\")"),
        ("8 MiB bomb", "Resource(\"decompressed bytes\")"),
        ("bad magic", "Decode"),
    ] {
        assert_eq!(named[name], expected, "{name}");
    }
    let edits = exhaustive_byte_edits(&mut stats, &limits);
    random_mutations(&mut stats, &limits, random_cases, seed);
    eprintln!(
        "corpus: {edits} exhaustive edits + {random_cases} random; outcomes {:?}; \
         decoders agreed on {} accepted inputs; legacy-only rejections {:?}; \
         columnar: {} accepted with the bounded decoder, {} column-type rejections, \
         {} ZSTD error-order pairs",
        stats.outcomes,
        stats.agree,
        stats.legacy_only_rejects,
        stats.columnar_agree,
        stats.columnar_type_only,
        stats.columnar_zstd_order
    );
    stats
}

fn env(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .map(|v| v.parse().expect("numeric corpus setting"))
        .unwrap_or(default)
}

#[test]
fn adversarial_corpus_short() {
    let stats = run(20_000, 0x2545_f491_4f6c_dd1d);
    for expected in [
        "accepted",
        "Decode",
        "Schema",
        "Unsupported",
        "envelope Decode",
    ] {
        assert!(
            stats.outcomes.get(expected).copied().unwrap_or(0) > 0,
            "{expected}: {:?}",
            stats.outcomes
        );
    }
    assert!(
        stats
            .outcomes
            .keys()
            .filter(|k| k.contains("Resource"))
            .count()
            >= 5
    );
    assert!(stats.agree > 1000, "{}", stats.agree);
    assert!(stats.columnar_agree > 1000, "{}", stats.columnar_agree);
    assert!(stats.columnar_type_only > 0, "{}", stats.columnar_type_only);
}

/// Manual long run; see the module documentation.
#[test]
#[ignore]
fn adversarial_corpus_long() {
    let cases = env("PRAEVIS_CORPUS_CASES", 2_000_000);
    let seed = env("PRAEVIS_CORPUS_SEED", 7);
    let started = std::time::Instant::now();
    run(cases, seed);
    eprintln!(
        "long corpus: {cases} random cases, seed {seed}, {:?}",
        started.elapsed()
    );
}
