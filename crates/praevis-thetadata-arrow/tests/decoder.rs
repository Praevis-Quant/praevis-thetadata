//! Columnar decoder behavior: exact types, null handling, coalescing, schema
//! and type errors, failure isolation and limits. The differential oracle
//! against the legacy decoder runs in the client's adversarial corpus.
use arrow_array::cast::AsArray;
use arrow_array::types::{Decimal128Type, Int64Type, TimestampMillisecondType};
use arrow_array::{Array, RecordBatch};
use arrow_schema::{DataType, TimeUnit};
use praevis_thetadata_arrow::{ColumnKind, ColumnarDecoder, DecodeError, Limits};
use praevis_thetadata_proto::endpoints::{
    CompressionDescription, DataTable, DataValue, DataValueList, Price, ResponseData,
    ZonedDateTime, data_value::DataType as Wire,
};
use prost::Message;

fn text(v: &str) -> Option<Wire> {
    Some(Wire::Text(v.into()))
}
fn number(v: i64) -> Option<Wire> {
    Some(Wire::Number(v))
}
fn price(value: i32, scale: i32) -> Option<Wire> {
    Some(Wire::Price(Price {
        value,
        r#type: scale,
    }))
}
fn time(epoch_ms: u64, zone: i32) -> Option<Wire> {
    Some(Wire::Timestamp(ZonedDateTime {
        epoch_ms,
        zone,
        nanos: 0,
        precision: 0,
    }))
}
fn null() -> Option<Wire> {
    Some(Wire::NullValue(0))
}

fn table(headers: &[&str], rows: Vec<Vec<Option<Wire>>>) -> DataTable {
    DataTable {
        headers: headers.iter().map(|h| (*h).into()).collect(),
        data_table: rows
            .into_iter()
            .map(|row| DataValueList {
                values: row
                    .into_iter()
                    .map(|data_type| DataValue { data_type })
                    .collect(),
            })
            .collect(),
    }
}
fn response(table: &DataTable, zstd: bool) -> ResponseData {
    let bytes = table.encode_to_vec();
    ResponseData {
        compressed_data: if zstd {
            zstd::stream::encode_all(bytes.as_slice(), 1).unwrap()
        } else {
            bytes
        },
        compression_description: Some(CompressionDescription {
            algo: i32::from(zstd),
            level: 0,
        }),
        ..Default::default()
    }
}
fn decoder() -> ColumnarDecoder {
    ColumnarDecoder::new(Limits::default()).unwrap()
}
fn decode(tables: &[DataTable]) -> RecordBatch {
    let mut decoder = decoder();
    for t in tables {
        decoder.push(&response(t, true)).unwrap();
    }
    decoder.flush().unwrap()
}

#[test]
fn every_wire_kind_maps_to_an_exact_arrow_type() {
    let batch = decode(&[table(
        &["root", "size", "price", "ny", "utc", "flag"],
        vec![
            vec![
                text("AAPL"),
                number(-7),
                price(12345, 8),
                time(1_700_000_000_123, 0),
                time(5, 1),
                Some(Wire::Boolean(true)),
            ],
            vec![
                text("MSFT"),
                number(i64::MAX),
                price(i32::MIN, 19),
                time(0, 0),
                time(6, 1),
                Some(Wire::Boolean(false)),
            ],
            vec![null(), None, price(99, 0), null(), null(), null()],
            vec![
                text(""),
                number(0),
                price(-1, 1),
                time(1, 0),
                time(7, 1),
                Some(Wire::Boolean(true)),
            ],
        ],
    )]);
    let types: Vec<_> = batch
        .schema()
        .fields()
        .iter()
        .map(|f| f.data_type().clone())
        .collect();
    assert_eq!(
        types,
        [
            DataType::Utf8,
            DataType::Int64,
            DataType::Decimal128(38, 10),
            DataType::Timestamp(TimeUnit::Millisecond, Some("America/New_York".into())),
            DataType::Timestamp(TimeUnit::Millisecond, Some("UTC".into())),
            DataType::Boolean,
        ]
    );
    assert_eq!(
        praevis_thetadata_arrow::timestamp_type(1),
        Some(types[4].clone())
    );
    let root = batch.column(0).as_string::<i32>();
    assert_eq!(
        (root.value(0), root.value(1), root.is_null(2), root.value(3)),
        ("AAPL", "MSFT", true, "")
    );
    let size = batch.column(1).as_primitive::<Int64Type>();
    assert_eq!(
        (size.value(0), size.value(1), size.is_null(2)),
        (-7, i64::MAX, true)
    );
    // unscaled = value × 10^scale: 123.45, -2147483648, scale 0 is null, -1e-9.
    let price = batch.column(2).as_primitive::<Decimal128Type>();
    assert_eq!(price.value(0), 1_234_500_000_000);
    assert_eq!(price.value(1), i128::from(i32::MIN) * 10i128.pow(19));
    assert!(price.is_null(2));
    assert_eq!(price.value(3), -10);
    assert_eq!(price.value_as_string(0), "123.4500000000");
    let ny = batch.column(3).as_primitive::<TimestampMillisecondType>();
    assert_eq!(
        (ny.value(0), ny.value(1), ny.is_null(2)),
        (1_700_000_000_123, 0, true)
    );
    let flag = batch.column(5).as_boolean();
    assert_eq!(
        (flag.value(0), flag.value(1), flag.is_null(2)),
        (true, false, true)
    );
}

#[test]
fn nulls_before_the_first_value_are_backfilled_and_types_persist() {
    let mut decoder = decoder();
    let header = ["a", "b"];
    decoder
        .push(&response(
            &table(&header, vec![vec![null(), null()]]),
            false,
        ))
        .unwrap();
    decoder
        .push(&response(
            &table(&header, vec![vec![number(3), None]]),
            false,
        ))
        .unwrap();
    let first = decoder.flush().unwrap();
    let a = first.column(0).as_primitive::<Int64Type>();
    assert_eq!((a.len(), a.is_null(0), a.value(1)), (2, true, 3));
    // A column with no value yet in a batch is Null.
    assert_eq!(first.column(1).data_type(), &DataType::Null);
    assert_eq!(first.column(1).len(), 2);
    // The next batch keeps a's type even when it holds only nulls.
    decoder
        .push(&response(
            &table(&header, vec![vec![null(), text("x")]]),
            false,
        ))
        .unwrap();
    let second = decoder.flush().unwrap();
    assert_eq!(second.column(0).data_type(), &DataType::Int64);
    assert!(second.column(0).is_null(0));
    assert_eq!(second.column(1).data_type(), &DataType::Utf8);
    assert_eq!(decoder.flush(), None);
}

/// Deterministic xorshift.
struct Rng(u64);
impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}

fn random_row(rng: &mut Rng) -> Vec<Option<Wire>> {
    let maybe = |rng: &mut Rng, value: Option<Wire>| if rng.below(4) == 0 { null() } else { value };
    let n = rng.below(1000) as i64;
    vec![
        maybe(rng, text(&format!("S{n}"))),
        maybe(rng, number(n - 500)),
        maybe(rng, price(n as i32 * 7 - 3000, 1 + (n % 19) as i32)),
        maybe(rng, time(1_700_000_000_000 + n as u64, 0)),
        maybe(rng, Some(Wire::Boolean(n % 2 == 0))),
    ]
}

#[test]
fn any_message_split_and_flush_points_give_the_same_rows() {
    let header = ["s", "n", "p", "t", "b"];
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for _ in 0..50 {
        let rows: Vec<_> = (0..rng.below(60) + 1)
            .map(|_| random_row(&mut rng))
            .collect();
        let full = decode(&[table(&header, rows.clone())]);
        let mut decoder = decoder();
        let (mut start, mut offset) = (0, 0);
        let check = |part: RecordBatch, offset: &mut usize| {
            let expected = full.slice(*offset, part.num_rows());
            for (index, column) in part.columns().iter().enumerate() {
                if column.data_type() == &DataType::Null {
                    // No value of this column in the part yet.
                    assert_eq!(expected.column(index).logical_null_count(), part.num_rows());
                } else {
                    assert_eq!(column.as_ref(), expected.column(index).as_ref());
                }
            }
            *offset += part.num_rows();
        };
        while start < rows.len() {
            let end = (start + 1 + rng.below(8) as usize).min(rows.len());
            let message = table(&header, rows[start..end].to_vec());
            decoder
                .push(&response(&message, rng.below(2) == 0))
                .unwrap();
            start = end;
            if rng.below(3) == 0
                && let Some(part) = decoder.flush()
            {
                check(part, &mut offset);
            }
        }
        if let Some(part) = decoder.flush() {
            check(part, &mut offset);
        }
        assert_eq!(offset, rows.len());
    }
}

#[test]
fn header_changes_ragged_rows_and_duplicates_are_schema_errors() {
    let cases = [
        vec![
            table(&["a"], vec![vec![number(1)]]),
            table(&["b"], vec![vec![number(1)]]),
        ],
        vec![
            table(&["a", "b"], vec![]),
            table(&["a"], vec![vec![number(1)]]),
        ],
        vec![table(&["a"], vec![]), table(&["a", "b"], vec![])],
        vec![table(&["a", "b"], vec![vec![number(1)]])],
        vec![table(&["a"], vec![vec![number(1), number(2)]])],
        vec![table(&["a", "a"], vec![])],
        vec![table(&[], vec![vec![number(1)]])],
        vec![table(&["a"], vec![]), table(&[], vec![vec![number(1)]])],
    ];
    for messages in cases {
        let mut decoder = decoder();
        let results: Vec<_> = messages
            .iter()
            .map(|m| decoder.push(&response(m, false)))
            .collect();
        assert_eq!(
            results.last().unwrap(),
            &Err(DecodeError::Schema),
            "{messages:?}"
        );
    }
    // Header-only and empty messages are fine; later ones may omit headers
    // only when they carry no rows.
    let mut decoder = decoder();
    for m in [
        table(&["a"], vec![]),
        table(&[], vec![]),
        table(&["a"], vec![vec![number(1)]]),
    ] {
        decoder.push(&response(&m, false)).unwrap();
    }
    assert_eq!(decoder.headers(), Some(&["a".to_owned()][..]));
}

#[test]
fn mixed_kinds_and_zones_are_column_type_errors() {
    for second in [
        text("x"),
        price(1, 10),
        time(1, 1),
        Some(Wire::Boolean(true)),
    ] {
        let mut decoder = decoder();
        let m = table(&["a"], vec![vec![number(1)], vec![second.clone()]]);
        assert_eq!(
            decoder.push(&response(&m, false)),
            Err(DecodeError::ColumnType { column: 0 })
        );
    }
    let mut decoder = decoder();
    let m = table(
        &["x", "t"],
        vec![vec![null(), time(1, 0)], vec![null(), time(1, 1)]],
    );
    assert_eq!(
        decoder.push(&response(&m, false)),
        Err(DecodeError::ColumnType { column: 1 })
    );
}

#[test]
fn a_failed_message_keeps_only_the_rows_before_it() {
    let mut decoder = decoder();
    let header = ["a", "b"];
    decoder
        .push(&response(
            &table(
                &header,
                vec![vec![number(1), null()], vec![number(2), null()]],
            ),
            true,
        ))
        .unwrap();
    // Row 3 is fine and fixes b's type; row 4 fails.
    let bad = table(
        &header,
        vec![vec![number(3), text("late")], vec![text("bad"), null()]],
    );
    assert_eq!(
        decoder.push(&response(&bad, true)),
        Err(DecodeError::ColumnType { column: 0 })
    );
    assert_eq!(decoder.buffered_rows(), 2);
    assert_eq!(
        decoder.push(&response(&table(&header, vec![]), true)),
        Err(DecodeError::Poisoned)
    );
    let batch = decoder.flush().unwrap();
    assert_eq!(batch.num_rows(), 2);
    assert_eq!(
        batch.column(0).as_primitive::<Int64Type>().values(),
        &[1, 2]
    );
    // b's type came only from the failed message, so it is not reported.
    assert_eq!(batch.column(1).data_type(), &DataType::Null);
    assert_eq!(decoder.flush(), None);
}

#[test]
fn unsupported_inputs_are_classified() {
    let mut nanos = table(&["t"], vec![vec![time(1, 0)]]);
    let cases: Vec<(ResponseData, DecodeError)> = vec![
        (
            ResponseData {
                flat_file_manifest: Some(Default::default()),
                ..Default::default()
            },
            DecodeError::Unsupported,
        ),
        (
            ResponseData {
                compression_description: Some(CompressionDescription { algo: 2, level: 0 }),
                ..Default::default()
            },
            DecodeError::Unsupported,
        ),
        (
            response(&table(&["p"], vec![vec![price(1, 20)]]), false),
            DecodeError::Decode,
        ),
        (
            response(&table(&["t"], vec![vec![time(1, 2)]]), false),
            DecodeError::Decode,
        ),
        (
            response(&table(&["t"], vec![vec![time(u64::MAX, 0)]]), false),
            DecodeError::Decode,
        ),
        (
            ResponseData {
                compressed_data: vec![0x0a, 0x02, 0xff, 0xfe],
                ..Default::default()
            },
            DecodeError::Decode,
        ),
        (
            ResponseData {
                compressed_data: vec![0x0a, 0x05],
                ..Default::default()
            },
            DecodeError::Decode,
        ),
        (
            ResponseData {
                compressed_data: vec![1, 2, 3],
                compression_description: Some(CompressionDescription { algo: 1, level: 0 }),
                ..Default::default()
            },
            DecodeError::Decode,
        ),
    ];
    for (response, expected) in cases {
        assert_eq!(decoder().push(&response), Err(expected), "{response:?}");
    }
    for (n, p) in [(1, 0), (0, 1)] {
        if let Some(Wire::Timestamp(t)) = &mut nanos.data_table[0].values[0].data_type {
            (t.nanos, t.precision) = (n, p);
        }
        assert_eq!(
            decoder().push(&response(&nanos, false)),
            Err(DecodeError::Unsupported)
        );
    }
}

#[test]
fn limits_bound_every_message_and_the_buffer() {
    let small = Limits {
        encoded_bytes: 4096,
        decompressed_bytes: 4096,
        headers: 2,
        header_bytes: 4,
        text_bytes: 4,
        rows_per_message: 3,
        buffered_bytes: 4096,
        zstd_window_log: 20,
    };
    let row = |v: &str| vec![text(v)];
    let cases: Vec<(ResponseData, &str)> = vec![
        (
            ResponseData {
                compressed_data: vec![0; 4097],
                ..Default::default()
            },
            "encoded bytes",
        ),
        (
            ResponseData {
                original_size: 4097,
                ..Default::default()
            },
            "declared bytes",
        ),
        (response(&table(&["a", "b", "c"], vec![]), false), "headers"),
        (response(&table(&["abcde"], vec![]), false), "header bytes"),
        (
            response(&table(&["a"], vec![row("abcde")]), false),
            "text bytes",
        ),
        (response(&table(&["a"], vec![row("a"); 4]), false), "rows"),
    ];
    for (response, name) in cases {
        let mut decoder = ColumnarDecoder::new(small.clone()).unwrap();
        assert_eq!(decoder.push(&response), Err(DecodeError::Resource(name)));
    }
    let bomb = ResponseData {
        compressed_data: zstd::stream::encode_all(vec![0u8; 1 << 20].as_slice(), 1).unwrap(),
        compression_description: Some(CompressionDescription { algo: 1, level: 0 }),
        ..Default::default()
    };
    let mut decoder = ColumnarDecoder::new(small.clone()).unwrap();
    assert_eq!(
        decoder.push(&bomb),
        Err(DecodeError::Resource("decompressed bytes"))
    );
    let mut window = zstd::bulk::Compressor::new(1).unwrap();
    window
        .set_parameter(zstd::zstd_safe::CParameter::WindowLog(24))
        .unwrap();
    let mut large = Limits {
        decompressed_bytes: 64 << 20,
        ..small.clone()
    };
    large.encoded_bytes = 64 << 20;
    let wide = ResponseData {
        compressed_data: window.compress(&vec![7u8; 20 << 20]).unwrap(),
        compression_description: Some(CompressionDescription { algo: 1, level: 0 }),
        ..Default::default()
    };
    let mut decoder = ColumnarDecoder::new(large).unwrap();
    assert_eq!(
        decoder.push(&wide),
        Err(DecodeError::Resource("zstd window"))
    );

    // A full buffer appends nothing and does not poison: flush and retry.
    let tight = Limits {
        buffered_bytes: 80,
        ..small
    };
    let mut decoder = ColumnarDecoder::new(tight).unwrap();
    let message = response(&table(&["a"], vec![row("x"), row("y")]), false);
    assert_eq!(decoder.push(&message), Ok(2));
    assert_eq!(decoder.push(&message), Err(DecodeError::BufferFull));
    assert_eq!(decoder.flush().unwrap().num_rows(), 2);
    assert_eq!(decoder.push(&message), Ok(2));
    assert!(decoder.buffered_bytes() > 0 && decoder.buffered_bytes() <= 80);

    for bad in [
        Limits {
            rows_per_message: 0,
            ..Limits::default()
        },
        Limits {
            zstd_window_log: 9,
            ..Limits::default()
        },
        Limits {
            zstd_window_log: 28,
            ..Limits::default()
        },
    ] {
        assert_eq!(
            ColumnarDecoder::new(bad).unwrap_err(),
            DecodeError::Configuration
        );
    }
}

#[test]
fn compressed_framing_matches_plain_and_frames_concatenate() {
    let t = table(
        &["a", "b"],
        vec![vec![number(1), text("x")], vec![number(2), null()]],
    );
    let plain = decode(std::slice::from_ref(&t));
    let mut decoder = decoder();
    // Two frames plus a skippable frame between them decode as one payload.
    let bytes = t.encode_to_vec();
    let (head, tail) = bytes.split_at(bytes.len() / 2);
    let mut framed = zstd::stream::encode_all(head, 3).unwrap();
    framed.extend_from_slice(&[0x50, 0x2a, 0x4d, 0x18, 2, 0, 0, 0, 9, 9]);
    framed.extend(zstd::stream::encode_all(tail, 3).unwrap());
    let response = ResponseData {
        compressed_data: framed,
        compression_description: Some(CompressionDescription { algo: 1, level: 0 }),
        ..Default::default()
    };
    assert_eq!(decoder.push(&response), Ok(2));
    assert_eq!(decoder.flush().unwrap(), plain);
    // A truncated frame is malformed.
    let mut truncated = response.clone();
    truncated
        .compressed_data
        .truncate(truncated.compressed_data.len() - 3);
    assert_eq!(decoder.push(&truncated), Err(DecodeError::Decode));
}

/// The calendar's observed columns (run `cal-local-001`), as the client
/// registers them.
const CALENDAR: &[(&str, ColumnKind)] = &[
    ("type", ColumnKind::Text),
    ("open", ColumnKind::Text),
    ("close", ColumnKind::Text),
];

fn decode_with(kinds: &'static [(&'static str, ColumnKind)], t: &DataTable) -> RecordBatch {
    let mut decoder = decoder().with_column_kinds(kinds);
    decoder.push(&response(t, true)).unwrap();
    decoder.flush().unwrap()
}

#[test]
fn registered_kinds_keep_all_null_columns_typed_so_responses_share_a_schema() {
    let headers = ["type", "open", "close"];
    let closed = table(&headers, vec![vec![text("full_close"), null(), null()]]);
    let open = table(
        &headers,
        vec![vec![text("open"), text("09:30:00"), text("16:00:00")]],
    );
    // Without registered kinds: a closed day's open/close are Arrow Null (the
    // bug), so a closed and an open day do not share a schema.
    assert_eq!(
        decode(std::slice::from_ref(&closed))
            .schema()
            .field(1)
            .data_type(),
        &DataType::Null
    );
    // With them: Utf8 in both, nulls kept, one schema.
    let (a, b) = (decode_with(CALENDAR, &closed), decode_with(CALENDAR, &open));
    assert_eq!(a.schema(), b.schema());
    for column in 0..3 {
        assert_eq!(a.schema().field(column).data_type(), &DataType::Utf8);
    }
    assert_eq!((a.column(1).null_count(), a.column(2).null_count()), (1, 1));
    assert_eq!(b.column(1).as_string::<i32>().value(0), "09:30:00");
    // The open day's columns fit the closed day's schema as they are.
    assert!(RecordBatch::try_new(a.schema(), b.columns().to_vec()).is_ok());
}

#[test]
fn every_registered_kind_types_an_all_null_column_as_its_values_would() {
    let kinds: &'static [(&'static str, ColumnKind)] = &[
        ("t", ColumnKind::Text),
        ("n", ColumnKind::Integer),
        ("p", ColumnKind::Price),
        ("b", ColumnKind::Boolean),
        ("ny", ColumnKind::TimestampNewYork),
        ("utc", ColumnKind::TimestampUtc),
    ];
    let headers = ["t", "n", "p", "b", "ny", "utc"];
    let empty = decode_with(
        kinds,
        &table(&headers, vec![vec![null(); 6], vec![price(5, 0); 6]]),
    );
    let full = decode(&[table(
        &headers,
        vec![vec![
            text("x"),
            number(1),
            price(125, 8),
            Some(Wire::Boolean(true)),
            time(1, 0),
            time(1, 1),
        ]],
    )]);
    // An all-null response gets exactly the types the values decode to.
    assert_eq!(empty.schema(), full.schema());
    for (index, (_, kind)) in kinds.iter().enumerate() {
        assert_eq!(empty.schema().field(index).data_type(), &kind.data_type());
        assert_eq!(empty.column(index).null_count(), 2);
    }
}

#[test]
fn a_value_of_another_kind_than_registered_is_a_type_error() {
    let mut decoder = decoder().with_column_kinds(CALENDAR);
    let wrong = table(
        &["type", "open", "close"],
        vec![vec![text("open"), number(930), null()]],
    );
    assert!(matches!(
        decoder.push(&response(&wrong, true)),
        Err(DecodeError::ColumnType { column: 1 })
    ));
}

#[test]
fn registered_kinds_add_and_rename_nothing() {
    // Columns the response lacks are not invented; unregistered ones are
    // typed from their values as before; header order is kept.
    let batch = decode_with(
        CALENDAR,
        &table(
            &["close", "extra", "spare"],
            vec![vec![null(), number(1), null()]],
        ),
    );
    let fields: Vec<_> = batch
        .schema()
        .fields()
        .iter()
        .map(|f| (f.name().clone(), f.data_type().clone()))
        .collect();
    assert_eq!(
        fields,
        [
            ("close".to_string(), DataType::Utf8),
            ("extra".to_string(), DataType::Int64),
            ("spare".to_string(), DataType::Null),
        ]
    );
}
