//! Parquet output for query and job streams (P1 PR 5). Encoding and file
//! writes run on the blocking pool, one batch at a time, so the async runtime
//! never waits on the disk or on compression.
use crate::eod::QueryError;
use crate::job::JobStream;
use crate::query::QueryStream;
use praevis_thetadata_arrow::arrow_array::RecordBatch;
use praevis_thetadata_arrow::{ParquetOptions, ParquetSink, ParquetSummary, SinkError};
use std::path::Path;

/// A query failure or a local output failure. Either way, nothing is left at
/// the target path: the file appears only when the whole stream is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ParquetOutputError {
    #[error(transparent)]
    Query(#[from] QueryError),
    #[error(transparent)]
    Sink(#[from] SinkError),
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, ParquetOutputError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| ParquetOutputError::Sink(SinkError::Encoding))
}

/// The two stream kinds, for one writer.
trait Batches {
    async fn next(&mut self) -> Result<Option<RecordBatch>, QueryError>;
}
impl Batches for QueryStream {
    async fn next(&mut self) -> Result<Option<RecordBatch>, QueryError> {
        self.next_batch().await
    }
}
impl Batches for JobStream {
    async fn next(&mut self) -> Result<Option<RecordBatch>, QueryError> {
        self.next_batch().await
    }
}

/// Drain `source` into a Parquet file at `path`.
async fn write(
    path: &Path,
    options: ParquetOptions,
    source: &mut impl Batches,
) -> Result<ParquetSummary, ParquetOutputError> {
    let path = path.to_path_buf();
    let mut sink = blocking(move || ParquetSink::create(path, options)).await??;
    while let Some(batch) = source.next().await? {
        sink = blocking(move || sink.write(&batch).map(|()| sink)).await??;
    }
    Ok(blocking(move || sink.finish()).await??)
}

impl QueryStream {
    /// Write the whole stream to a Parquet file. See [`ParquetSink`] for the
    /// file guarantees and how `Null` columns are handled.
    pub async fn to_parquet(
        mut self,
        path: impl AsRef<Path>,
        options: ParquetOptions,
    ) -> Result<ParquetSummary, ParquetOutputError> {
        write(path.as_ref(), options, &mut self).await
    }
}

impl JobStream {
    /// Write the whole job, in date order, to one Parquet file.
    pub async fn to_parquet(
        mut self,
        path: impl AsRef<Path>,
        options: ParquetOptions,
    ) -> Result<ParquetSummary, ParquetOutputError> {
        write(path.as_ref(), options, &mut self).await
    }
}
