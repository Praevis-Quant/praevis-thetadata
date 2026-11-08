//! The Parquet sink's memory stays flat as the file grows (options phase):
//! decimal columns are written without min/max statistics by default,
//! because the `parquet` writer keeps those statistics as slices of the
//! whole written batch until the file is closed. Own test binary: it
//! installs a counting allocator.
#![cfg(feature = "parquet")]
use arrow_array::{ArrayRef, Decimal128Array, Int64Array, RecordBatch};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use praevis_thetadata_arrow::{ParquetOptions, ParquetSink};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            LIVE.fetch_add(layout.size(), Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) };
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }
}
#[global_allocator]
static ALLOC: Counting = Counting;

const BATCH_ROWS: usize = 16_384;

/// Four Decimal128(38, 10) price columns and one Int64 column.
fn batch() -> RecordBatch {
    let price = |offset: i128| -> ArrayRef {
        Arc::new(
            Decimal128Array::from_iter_values(
                (0..BATCH_ROWS as i128).map(|r| 5_400_000_000_000 + (r % 500) * 1_000_000 + offset),
            )
            .with_precision_and_scale(38, 10)
            .unwrap(),
        )
    };
    RecordBatch::try_from_iter([
        ("bid", price(0)),
        ("ask", price(1)),
        ("bid2", price(2)),
        ("ask2", price(3)),
        (
            "n",
            Arc::new(Int64Array::from_iter_values(0..BATCH_ROWS as i64)) as ArrayRef,
        ),
    ])
    .unwrap()
}

/// Live heap growth (MiB) from the 20th row group to the 120th, before the
/// file is closed, and the written file's path.
fn growth(decimal_statistics: bool, path: &std::path::Path) -> f64 {
    let batch = batch();
    let mut sink = ParquetSink::create(
        path,
        // One batch per row group: many row groups, quickly.
        ParquetOptions::default()
            .with_row_group_rows(BATCH_ROWS)
            .with_overwrite(true)
            .with_decimal_statistics(decimal_statistics),
    )
    .unwrap();
    let mut at_20 = 0;
    for group in 1..=120 {
        sink.write(&batch).unwrap();
        if group == 20 {
            at_20 = LIVE.load(Ordering::Relaxed);
        }
    }
    let at_120 = LIVE.load(Ordering::Relaxed);
    sink.finish().unwrap();
    (at_120 as f64 - at_20 as f64) / (1 << 20) as f64
}

#[test]
fn memory_stays_flat_unless_decimal_statistics_are_asked_for() {
    let dir = std::env::temp_dir().join(format!("praevis-parquet-memory-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let flat = dir.join("flat.parquet");
    let with_statistics = dir.join("statistics.parquet");
    // 100 more row groups of 4 decimal columns: flat by default...
    let default_growth = growth(false, &flat);
    assert!(default_growth < 2.0, "default grew {default_growth:.1} MiB");
    // ...but each row group pins its batch's decimal values with statistics
    // (up to 2 x 16 B x 16,384 rows x 4 columns = 2 MiB per row group).
    let statistics_growth = growth(true, &with_statistics);
    eprintln!(
        "100 row groups: default +{default_growth:.1} MiB, with decimal statistics +{statistics_growth:.1} MiB"
    );
    assert!(
        statistics_growth > 50.0,
        "with decimal statistics grew only {statistics_growth:.1} MiB"
    );
    // The default file has no decimal statistics but keeps the others; the
    // opt-in writes them.
    let statistics = |path: &std::path::Path| -> Vec<bool> {
        let file = std::fs::File::open(path).unwrap();
        let builder = ParquetRecordBatchReaderBuilder::try_new(file).unwrap();
        let group = builder.metadata().row_group(0);
        (0..group.num_columns())
            .map(|c| group.column(c).statistics().is_some())
            .collect()
    };
    assert_eq!(statistics(&flat), [false, false, false, false, true]);
    assert_eq!(statistics(&with_statistics), [true, true, true, true, true]);
    std::fs::remove_dir_all(&dir).unwrap();
}
