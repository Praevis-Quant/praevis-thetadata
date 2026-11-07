//! Streaming Parquet output (P1 PR 5).
//!
//! Batches are written as they arrive, in row groups of up to
//! [`ParquetOptions::row_group_rows`] rows. The file is written beside its
//! target as `<name>.partial` and renamed into place only by
//! [`ParquetSink::finish`], so a failed or abandoned query never leaves a file
//! at the target path.
//!
//! Parquet fixes the schema in the file. A query column with no value yet is
//! `Null` and may become typed later (the agreed rule), so the sink holds
//! batches back while any column is still `Null`, up to
//! [`ParquetOptions::pending_bytes`], then writes them with the final types. A
//! column still `Null` when the cap is reached is written as `Null`; if it is
//! typed later, writing fails with [`SinkError::Schema`].
//!
//! **Decimal columns are written without min/max statistics by default**
//! ([`ParquetOptions::decimal_statistics`]). With them, the `parquet` writer
//! (60.1) keeps each row group's decimal min/max as slices of the whole
//! written batch's converted values until the file is closed. That pins up
//! to 2 × 16 bytes × batch rows per decimal column per row group: memory
//! that grows with the file (about 31 bytes per row for 4 decimal columns
//! in 65,536-row batches, measured by `examples/job_memory.rs` in the client
//! crate). Statistics stay on every
//! other column, so time and integer filters keep their pushdown.
//!
//! This is an upstream bug, reported as apache/arrow-rs#12027 and fixed by
//! apache/arrow-rs#12028 (merged 2026-11-03, not in a release yet). It
//! affects decimals with precision above 18; ours are precision 38
//! ([`crate::PRICE_PRECISION`]). Once a `parquet` release contains the fix,
//! the default is to be re-measured and statistics re-enabled (ROADMAP).
use arrow_array::{ArrayRef, RecordBatch, new_null_array};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use parquet::arrow::ArrowWriter;
use parquet::basic::{Compression, ZstdLevel};
use parquet::file::properties::{EnabledStatistics, WriterProperties};
use parquet::schema::types::ColumnPath;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct ParquetOptions {
    /// ZSTD level (1-22), or `None` for uncompressed pages. Default 1.
    pub zstd_level: Option<i32>,
    /// Rows per row group (default 131,072).
    pub row_group_rows: usize,
    /// Batches held back while a column is still `Null`, in bytes of Arrow
    /// memory (default 64 MiB).
    pub pending_bytes: usize,
    /// Replace an existing file at the target path (default `false`).
    pub overwrite: bool,
    /// Write min/max statistics for decimal (price) columns too (default
    /// `false`). Enables readers' pushdown on prices, at a memory cost that
    /// grows with the file: up to 2 × 16 bytes × batch rows per decimal
    /// column per row group, held until [`ParquetSink::finish`] (see the
    /// module documentation; upstream apache/arrow-rs#12027, fixed by
    /// #12028 but not yet released).
    pub decimal_statistics: bool,
}
impl Default for ParquetOptions {
    fn default() -> Self {
        Self {
            zstd_level: Some(1),
            row_group_rows: 128 << 10,
            pending_bytes: 64 << 20,
            overwrite: false,
            decimal_statistics: false,
        }
    }
}

/// Local output failures. No path or library message is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SinkError {
    #[error("the target file exists")]
    Exists,
    #[error("file system error: {0:?}")]
    Io(std::io::ErrorKind),
    #[error("Parquet encoding failed")]
    Encoding,
    /// Columns changed, a column changed type, or a column written as `Null`
    /// was typed later.
    #[error("batch schema does not match the file")]
    Schema,
    #[error("invalid Parquet options")]
    Configuration,
}

/// What was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParquetSummary {
    pub rows: usize,
    pub row_groups: usize,
    pub bytes: u64,
}

pub struct ParquetSink {
    target: PathBuf,
    partial: PathBuf,
    options: ParquetOptions,
    file: Option<File>,
    writer: Option<ArrowWriter<File>>,
    /// The file's schema once fixed.
    schema: Option<SchemaRef>,
    names: Option<Vec<String>>,
    types: Vec<DataType>,
    pending: Vec<RecordBatch>,
    pending_bytes: usize,
    rows: usize,
    done: bool,
}

fn io(error: std::io::Error) -> SinkError {
    SinkError::Io(error.kind())
}

impl ParquetSink {
    /// Create `<path>.partial` beside `path`. Refuses an existing target
    /// unless `overwrite` is set; an old `.partial` file is replaced.
    pub fn create(path: impl AsRef<Path>, options: ParquetOptions) -> Result<Self, SinkError> {
        if options.row_group_rows == 0
            || options
                .zstd_level
                .is_some_and(|level| ZstdLevel::try_new(level).is_err())
        {
            return Err(SinkError::Configuration);
        }
        let target = path.as_ref().to_path_buf();
        if !options.overwrite && target.exists() {
            return Err(SinkError::Exists);
        }
        let mut name = target
            .file_name()
            .ok_or(SinkError::Configuration)?
            .to_os_string();
        name.push(".partial");
        let partial = target.with_file_name(name);
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&partial)
            .map_err(io)?;
        Ok(Self {
            target,
            partial,
            options,
            file: Some(file),
            writer: None,
            schema: None,
            names: None,
            types: Vec::new(),
            pending: Vec::new(),
            pending_bytes: 0,
            rows: 0,
            done: false,
        })
    }

    /// Append one batch.
    pub fn write(&mut self, batch: &RecordBatch) -> Result<(), SinkError> {
        let names: Vec<String> = batch
            .schema()
            .fields()
            .iter()
            .map(|f| f.name().clone())
            .collect();
        match &self.names {
            None => {
                self.types = vec![DataType::Null; names.len()];
                self.names = Some(names);
            }
            Some(known) if *known != names => return Err(SinkError::Schema),
            Some(_) => {}
        }
        for (column, known) in batch.columns().iter().zip(self.types.iter_mut()) {
            match (column.data_type(), &*known) {
                (DataType::Null, _) => {}
                (typed, DataType::Null) => *known = typed.clone(),
                (typed, expected) if typed == expected => {}
                _ => return Err(SinkError::Schema),
            }
        }
        self.rows += batch.num_rows();
        if self.writer.is_some() {
            return self.write_now(batch);
        }
        self.pending_bytes = self
            .pending_bytes
            .saturating_add(batch.get_array_memory_size());
        self.pending.push(batch.clone());
        if !self.types.contains(&DataType::Null) || self.pending_bytes >= self.options.pending_bytes
        {
            self.open()?;
        }
        Ok(())
    }

    /// Fix the schema with the types known now and write the held batches.
    fn open(&mut self) -> Result<(), SinkError> {
        let names = self.names.as_ref().ok_or(SinkError::Schema)?;
        let schema: SchemaRef = Arc::new(Schema::new(
            names
                .iter()
                .zip(&self.types)
                .map(|(name, data_type)| Field::new(name, data_type.clone(), true))
                .collect::<Vec<_>>(),
        ));
        let mut properties = WriterProperties::builder()
            .set_compression(match self.options.zstd_level {
                Some(level) => Compression::ZSTD(
                    ZstdLevel::try_new(level).map_err(|_| SinkError::Configuration)?,
                ),
                None => Compression::UNCOMPRESSED,
            })
            .set_max_row_group_row_count(Some(self.options.row_group_rows));
        if !self.options.decimal_statistics {
            for (name, data_type) in names.iter().zip(&self.types) {
                if matches!(data_type, DataType::Decimal128(_, _)) {
                    properties = properties.set_column_statistics_enabled(
                        ColumnPath::new(vec![name.clone()]),
                        EnabledStatistics::None,
                    );
                }
            }
        }
        let properties = properties.build();
        let file = self.file.take().ok_or(SinkError::Configuration)?;
        self.writer = Some(
            ArrowWriter::try_new(file, schema.clone(), Some(properties))
                .map_err(|_| SinkError::Encoding)?,
        );
        self.schema = Some(schema);
        for batch in std::mem::take(&mut self.pending) {
            self.write_now(&batch)?;
        }
        self.pending_bytes = 0;
        Ok(())
    }

    /// Give `Null` columns the file's type, then write.
    fn write_now(&mut self, batch: &RecordBatch) -> Result<(), SinkError> {
        let schema = self.schema.clone().ok_or(SinkError::Schema)?;
        let columns: Vec<ArrayRef> = batch
            .columns()
            .iter()
            .zip(schema.fields())
            .map(
                |(column, field)| match (column.data_type(), field.data_type()) {
                    (DataType::Null, DataType::Null) => Ok(column.clone()),
                    (DataType::Null, typed) => Ok(new_null_array(typed, column.len())),
                    (typed, expected) if typed == expected => Ok(column.clone()),
                    // Written as Null, typed later: the file cannot change.
                    _ => Err(SinkError::Schema),
                },
            )
            .collect::<Result<_, _>>()?;
        let batch = RecordBatch::try_new(schema, columns).map_err(|_| SinkError::Schema)?;
        self.writer
            .as_mut()
            .ok_or(SinkError::Schema)?
            .write(&batch)
            .map_err(|_| SinkError::Encoding)
    }

    /// Write what is held, close the file and move it to the target path.
    /// With no batch at all, no file is produced (`Ok` with zero rows).
    pub fn finish(mut self) -> Result<ParquetSummary, SinkError> {
        if self.names.is_none() {
            self.discard();
            return Ok(ParquetSummary {
                rows: 0,
                row_groups: 0,
                bytes: 0,
            });
        }
        if self.writer.is_none() {
            self.open()?;
        }
        let writer = self.writer.take().ok_or(SinkError::Schema)?;
        let metadata = writer.close().map_err(|_| SinkError::Encoding)?;
        let row_groups = metadata.num_row_groups();
        if self.options.overwrite && self.target.exists() {
            std::fs::remove_file(&self.target).map_err(io)?;
        }
        std::fs::rename(&self.partial, &self.target).map_err(io)?;
        self.done = true;
        let bytes = std::fs::metadata(&self.target).map_err(io)?.len();
        Ok(ParquetSummary {
            rows: self.rows,
            row_groups,
            bytes,
        })
    }

    fn discard(&mut self) {
        self.writer = None;
        self.file = None;
        let _ = std::fs::remove_file(&self.partial);
        self.done = true;
    }
}

impl Drop for ParquetSink {
    /// An unfinished sink removes its partial file.
    fn drop(&mut self) {
        if !self.done {
            self.discard();
        }
    }
}
