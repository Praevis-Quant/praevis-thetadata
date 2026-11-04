//! Streaming Parquet output: exact round trips, row groups, the Null-to-typed
//! rule, and the partial-file guarantees (P1 PR 5).
#![cfg(feature = "parquet")]
use arrow_array::{ArrayRef, Int64Array, RecordBatch, new_null_array};
use arrow_schema::DataType;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use praevis_thetadata_arrow::{
    ColumnarDecoder, Limits, ParquetOptions, ParquetSink, ParquetSummary, SinkError,
};
use praevis_thetadata_proto::endpoints::{
    CompressionDescription, DataTable, DataValue, DataValueList, Price, ResponseData,
    ZonedDateTime, data_value::DataType as Wire,
};
use prost::Message;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "praevis-parquet-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir(&dir).unwrap();
        Self(dir)
    }
    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    fn names(&self) -> Vec<String> {
        let mut names: Vec<_> = std::fs::read_dir(&self.0)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Every wire kind, through the real decoder, so the file holds exact types.
fn decoded(rows: usize) -> RecordBatch {
    let table = DataTable {
        headers: ["s", "n", "p", "ny", "utc", "b"].map(String::from).to_vec(),
        data_table: (0..rows as i64)
            .map(|r| DataValueList {
                values: [
                    Some(Wire::Text(format!("S{r}"))),
                    Some(Wire::Number(i64::MAX - r)),
                    Some(Wire::Price(Price {
                        value: -12345 + r as i32,
                        r#type: 8,
                    })),
                    Some(Wire::Timestamp(ZonedDateTime {
                        epoch_ms: 1_718_380_800_000 + r as u64,
                        zone: 0,
                        ..Default::default()
                    })),
                    if r % 3 == 0 {
                        None
                    } else {
                        Some(Wire::Timestamp(ZonedDateTime {
                            epoch_ms: r as u64,
                            zone: 1,
                            ..Default::default()
                        }))
                    },
                    Some(Wire::Boolean(r % 2 == 0)),
                ]
                .into_iter()
                .map(|data_type| DataValue { data_type })
                .collect(),
            })
            .collect(),
    };
    let mut decoder = ColumnarDecoder::new(Limits::default()).unwrap();
    decoder
        .push(&ResponseData {
            compressed_data: table.encode_to_vec(),
            compression_description: Some(CompressionDescription { algo: 0, level: 0 }),
            ..Default::default()
        })
        .unwrap();
    decoder.flush().unwrap()
}
fn read(path: &PathBuf) -> Vec<RecordBatch> {
    ParquetRecordBatchReaderBuilder::try_new(std::fs::File::open(path).unwrap())
        .unwrap()
        .with_batch_size(1 << 20)
        .build()
        .unwrap()
        .map(Result::unwrap)
        .collect()
}
fn int_or_null(name: &str, value: Option<i64>, rows: usize) -> (String, ArrayRef) {
    let column: ArrayRef = match value {
        Some(v) => Arc::new(Int64Array::from(vec![v; rows])),
        None => new_null_array(&DataType::Null, rows),
    };
    (name.into(), column)
}
fn batch(columns: Vec<(String, ArrayRef)>) -> RecordBatch {
    RecordBatch::try_from_iter(columns).unwrap()
}

#[test]
fn exact_types_round_trip_through_zstd_and_uncompressed_files() {
    for zstd_level in [Some(1), Some(9), None] {
        let dir = Dir::new();
        let path = dir.file("out.parquet");
        let original = decoded(300);
        let mut sink = ParquetSink::create(
            &path,
            ParquetOptions {
                zstd_level,
                ..Default::default()
            },
        )
        .unwrap();
        sink.write(&original).unwrap();
        let summary = sink.finish().unwrap();
        assert_eq!((summary.rows, summary.row_groups), (300, 1));
        assert!(summary.bytes > 0);
        let back = read(&path);
        assert_eq!(back, std::slice::from_ref(&original), "{zstd_level:?}");
        // Decimal128(38, 10) and the time zones survive.
        let types: Vec<_> = back[0]
            .schema()
            .fields()
            .iter()
            .map(|f| f.data_type().clone())
            .collect();
        assert_eq!(types[2], DataType::Decimal128(38, 10));
        assert_eq!(
            types[3],
            praevis_thetadata_arrow::timestamp_type(0).unwrap()
        );
        assert_eq!(
            types[4],
            praevis_thetadata_arrow::timestamp_type(1).unwrap()
        );
        assert_eq!(dir.names(), ["out.parquet"]);
    }
}

#[test]
fn batches_stream_into_bounded_row_groups() {
    let dir = Dir::new();
    let path = dir.file("groups.parquet");
    let options = ParquetOptions {
        row_group_rows: 250,
        ..Default::default()
    };
    let mut sink = ParquetSink::create(&path, options).unwrap();
    for _ in 0..10 {
        sink.write(&decoded(100)).unwrap();
    }
    let summary = sink.finish().unwrap();
    assert_eq!((summary.rows, summary.row_groups), (1000, 4));
    assert_eq!(
        read(&path).iter().map(RecordBatch::num_rows).sum::<usize>(),
        1000
    );
}

#[test]
fn null_columns_are_held_back_until_typed() {
    let dir = Dir::new();
    let path = dir.file("late.parquet");
    let mut sink = ParquetSink::create(&path, ParquetOptions::default()).unwrap();
    sink.write(&batch(vec![
        int_or_null("a", Some(1), 2),
        int_or_null("b", None, 2),
    ]))
    .unwrap();
    // Nothing is fixed yet: only the partial file exists.
    assert_eq!(dir.names(), ["late.parquet.partial"]);
    sink.write(&batch(vec![
        int_or_null("a", Some(2), 1),
        int_or_null("b", Some(7), 1),
    ]))
    .unwrap();
    sink.write(&batch(vec![
        int_or_null("a", Some(3), 1),
        int_or_null("b", None, 1),
    ]))
    .unwrap();
    sink.finish().unwrap();
    let back = read(&path);
    let all: Vec<_> = back
        .iter()
        .flat_map(|b| (0..b.num_rows()).map(move |r| b.column(1).is_null(r)))
        .collect();
    assert_eq!(all, [true, true, false, true]);
    assert!(
        back.iter()
            .all(|b| b.column(1).data_type() == &DataType::Int64)
    );
}

#[test]
fn a_column_written_as_null_cannot_be_typed_later() {
    let dir = Dir::new();
    let path = dir.file("capped.parquet");
    // A tiny hold-back cap fixes the schema at once, with b as Null.
    let options = ParquetOptions {
        pending_bytes: 1,
        ..Default::default()
    };
    let mut sink = ParquetSink::create(&path, options).unwrap();
    sink.write(&batch(vec![
        int_or_null("a", Some(1), 2),
        int_or_null("b", None, 2),
    ]))
    .unwrap();
    sink.write(&batch(vec![
        int_or_null("a", Some(2), 1),
        int_or_null("b", None, 1),
    ]))
    .unwrap();
    assert_eq!(
        sink.write(&batch(vec![
            int_or_null("a", Some(3), 1),
            int_or_null("b", Some(7), 1)
        ])),
        Err(SinkError::Schema)
    );
    drop(sink);
    // An abandoned sink leaves nothing behind.
    assert!(dir.names().is_empty());
    // Still Null at the end is a valid file with a Null column.
    let mut sink = ParquetSink::create(&path, ParquetOptions::default()).unwrap();
    sink.write(&batch(vec![
        int_or_null("a", Some(1), 2),
        int_or_null("b", None, 2),
    ]))
    .unwrap();
    sink.finish().unwrap();
    assert_eq!(read(&path)[0].column(1).data_type(), &DataType::Null);
}

#[test]
fn files_are_never_overwritten_by_accident_or_left_partial() {
    let dir = Dir::new();
    let path = dir.file("t.parquet");
    std::fs::write(&path, b"keep").unwrap();
    assert_eq!(
        ParquetSink::create(&path, ParquetOptions::default()).err(),
        Some(SinkError::Exists)
    );
    let mut sink = ParquetSink::create(
        &path,
        ParquetOptions {
            overwrite: true,
            ..Default::default()
        },
    )
    .unwrap();
    sink.write(&decoded(3)).unwrap();
    // Until finish, the old file is untouched.
    assert_eq!(std::fs::read(&path).unwrap(), b"keep");
    sink.finish().unwrap();
    assert_eq!(read(&path)[0].num_rows(), 3);
    // Other columns, or another type, are schema errors.
    let other = dir.file("o.parquet");
    let mut sink = ParquetSink::create(&other, ParquetOptions::default()).unwrap();
    sink.write(&batch(vec![int_or_null("a", Some(1), 1)]))
        .unwrap();
    assert_eq!(
        sink.write(&batch(vec![int_or_null("z", Some(1), 1)])),
        Err(SinkError::Schema)
    );
    let text: ArrayRef = Arc::new(arrow_array::StringArray::from(vec!["x"]));
    assert_eq!(
        sink.write(&batch(vec![("a".into(), text)])),
        Err(SinkError::Schema)
    );
    drop(sink);
    // No batch at all: no file.
    let none = dir.file("none.parquet");
    let sink = ParquetSink::create(&none, ParquetOptions::default()).unwrap();
    assert_eq!(
        sink.finish(),
        Ok(ParquetSummary {
            rows: 0,
            row_groups: 0,
            bytes: 0
        })
    );
    assert!(!none.exists());
    assert_eq!(dir.names(), ["t.parquet"]);
    for bad in [
        ParquetOptions {
            row_group_rows: 0,
            ..Default::default()
        },
        ParquetOptions {
            zstd_level: Some(99),
            ..Default::default()
        },
    ] {
        assert_eq!(
            ParquetSink::create(dir.file("x.parquet"), bad).err(),
            Some(SinkError::Configuration)
        );
    }
}
