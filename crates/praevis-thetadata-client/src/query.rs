//! Generic typed query streams: any generated endpoint, decoded into exact,
//! coalesced Arrow batches (P1 PR 4a).
//!
//! The stream keeps the stock EOD stream's guarantees: the shared stream
//! admission, one overall deadline, an idle timeout, the guarded framing, a
//! terminal error returned once, and a dropped pending call cancelling the
//! stream. Frames are decoded in place from the bytes the framing assembled
//! (no `ResponseData` or payload copy): inline below `inline_bytes`, on the
//! blocking pool above it.
use crate::endpoints::Endpoint;
use crate::eod::{QueryError, budgets, check_deadline};
use crate::scheduler::retryable;
use crate::{ThetaClient, envelope, framing};
use praevis_thetadata_arrow::arrow_array::{ArrayRef, RecordBatch, new_null_array};
use praevis_thetadata_arrow::arrow_schema::{DataType, Field, Schema};
use praevis_thetadata_arrow::{ColumnarDecoder, DecodeError, Limits};
use std::{sync::Arc, time::Duration};
use tokio::{
    sync::OwnedSemaphorePermit,
    time::{Instant, timeout_at},
};

/// When a stream emits a batch. The first rows are emitted as soon as their
/// message arrives, so time to first row is the service's first-message time.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct BatchPolicy {
    /// Emit when this many rows are buffered (default 65,536).
    pub rows: usize,
    /// Emit when the buffered, accounted bytes reach this (default 8 MiB).
    pub bytes: usize,
    /// Emit buffered rows this long after the first of them (default 50 ms),
    /// even while the next message has not arrived.
    pub latency: Duration,
    /// Encoded messages up to this size are decoded on the receiving task;
    /// larger ones on the blocking pool (default 64 KiB).
    pub inline_bytes: usize,
}
impl Default for BatchPolicy {
    fn default() -> Self {
        Self {
            rows: 65_536,
            bytes: 8 << 20,
            latency: Duration::from_millis(50),
            inline_bytes: 64 << 10,
        }
    }
}
impl BatchPolicy {
    fn validate(&self) -> Result<(), QueryError> {
        if self.rows == 0 || self.bytes == 0 || self.latency.is_zero() {
            return Err(QueryError::Configuration);
        }
        Ok(())
    }
}

/// Per-call options for [`ThetaClient::query_with`] and
/// [`ThetaClient::job_with`]. Each one left unset uses the client's setting.
///
/// **The timeout covers time spent queued.** It is a wall-clock budget that
/// starts when the request is submitted, before it waits for a scheduler
/// slot, and ends when its stream ends. A request that never gets a slot
/// fails with [`QueryError::Deadline`] when its budget runs out, so a busy
/// scheduler cannot hold a caller longer than it asked for. In a job, each
/// part has its own budget, from the moment the job submits it.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct RequestOptions {
    /// How rows are coalesced into batches.
    pub batch: BatchPolicy,
    /// The overall deadline of a request (each part, in a job), including
    /// time queued for a slot. Default: `ClientConfig::request_timeout`.
    pub timeout: Option<Duration>,
    /// The longest wait between messages once a request streams. Default:
    /// `ClientConfig::idle_timeout`.
    pub idle_timeout: Option<Duration>,
    /// Jobs only: parts holding a scheduler slot at once, from 1 to the
    /// scheduler's concurrency; the read-ahead window scales with it.
    /// Default: the scheduler's concurrency. `query_with` ignores it.
    pub concurrency: Option<usize>,
}
with_setters!(RequestOptions {
    with_batch => batch: BatchPolicy,
    with_timeout => timeout: Option<Duration>,
    with_idle_timeout => idle_timeout: Option<Duration>,
    with_concurrency => concurrency: Option<usize>,
});
impl RequestOptions {
    pub(crate) fn validate(&self) -> Result<(), QueryError> {
        self.batch.validate()?;
        if self.timeout.is_some_and(|t| t.is_zero())
            || self.idle_timeout.is_some_and(|t| t.is_zero())
            || self.concurrency == Some(0)
        {
            return Err(QueryError::Configuration);
        }
        Ok(())
    }
}

fn map_decode(error: DecodeError) -> QueryError {
    match error {
        DecodeError::Decode | DecodeError::Poisoned => QueryError::Decode,
        DecodeError::Unsupported => QueryError::Unsupported,
        DecodeError::Schema | DecodeError::ColumnType { .. } => QueryError::Schema,
        DecodeError::Resource(name) => QueryError::Resource(name),
        DecodeError::BufferFull => QueryError::Resource("buffered bytes"),
        DecodeError::Configuration => QueryError::Configuration,
        // `DecodeError` is non-exhaustive; an unknown failure is a decode error.
        _ => QueryError::Decode,
    }
}

impl ThetaClient {
    /// Stream any generated endpoint as exact Arrow batches.
    ///
    /// ```no_run
    /// use praevis_thetadata_client::{ThetaClient, QueryError, NaiveDate, endpoints};
    /// async fn consume(client: &ThetaClient) -> Result<(), QueryError> {
    ///     let day = NaiveDate::from_ymd_opt(2024, 6, 14).unwrap();
    ///     let request = endpoints::StockHistoryEod::new("AAPL", day, day);
    ///     let mut stream = client.query(&request).await?;
    ///     while let Some(batch) = stream.next_batch().await? {
    ///         std::hint::black_box(batch.num_rows());
    ///     }
    ///     Ok(())
    /// }
    /// ```
    pub async fn query<E: Endpoint>(&self, request: &E) -> Result<QueryStream, QueryError> {
        self.query_with(request, RequestOptions::default()).await
    }

    /// [`ThetaClient::query`] with per-call options: batching, an overall
    /// timeout (which covers time queued for a slot) and an idle timeout.
    pub async fn query_with<E: Endpoint>(
        &self,
        request: &E,
        options: RequestOptions,
    ) -> Result<QueryStream, QueryError> {
        budgets(&self.config)?;
        options.validate()?;
        let RequestOptions {
            batch,
            timeout,
            idle_timeout,
            ..
        } = options;
        // The budget starts now, before admission: queue time counts.
        let deadline = Instant::now()
            .checked_add(timeout.unwrap_or(self.config.request_timeout))
            .ok_or(QueryError::Configuration)?;
        let idle = idle_timeout.unwrap_or(self.config.idle_timeout);
        let pool = self.eod_pool.clone();
        let decode = &pool.policy.decode;
        let limits = Limits::default()
            .with_encoded_bytes(decode.encoded_bytes)
            .with_decompressed_bytes(decode.decompressed_bytes)
            .with_headers(decode.headers)
            .with_header_bytes(decode.header_bytes)
            .with_text_bytes(decode.text_bytes)
            .with_rows_per_message(decode.rows)
            // Room for a full batch plus one more message.
            .with_buffered_bytes(
                batch
                    .bytes
                    .saturating_add(decode.decompressed_bytes.saturating_mul(2)),
            )
            .with_zstd_window_log(decode.zstd_window_log);
        let decoder = ColumnarDecoder::new(limits)
            .map_err(map_decode)?
            // Observed column kinds keep a column typed when a response leaves it
            // entirely null (ADR-0009 addendum).
            .with_column_kinds(E::COLUMNS);
        // Admission and retry (P1 PR 4b): a scheduler slot is held from open
        // to the end of the stream. Only RESOURCE_EXHAUSTED and UNAVAILABLE
        // at open are retried; nothing has been delivered yet, so a retry
        // cannot duplicate rows.
        let scheduler = self.scheduler.clone();
        let mut attempt = 0;
        let (inner, failure, stream_lease, slot) = loop {
            let slot = scheduler.admit(deadline).await?;
            match self.open(request, deadline, &pool).await {
                Ok((inner, failure, lease)) => break (inner, failure, lease, slot),
                Err(error) if retryable(error) && attempt < scheduler.policy.retries => {
                    drop(slot);
                    attempt += 1;
                    let resume = Instant::now() + scheduler.backoff(attempt);
                    if resume >= deadline {
                        return Err(error);
                    }
                    tokio::time::sleep_until(resume).await;
                }
                Err(error) => return Err(error),
            }
        };
        Ok(QueryStream {
            inner: Some(inner),
            decoder: Some(Box::new(decoder)),
            pool,
            deadline,
            idle,
            batch,
            failure,
            leases: Some((stream_lease, slot)),
            emitted: false,
            first_buffered: None,
            pending: None,
            terminal: false,
        })
    }

    /// Open one request: the stream admission lease, then the gRPC call.
    async fn open<E: Endpoint>(
        &self,
        request: &E,
        deadline: Instant,
        pool: &crate::eod::Pool,
    ) -> Result<
        (
            tonic::Streaming<Vec<u8>>,
            framing::Failure,
            OwnedSemaphorePermit,
        ),
        QueryError,
    > {
        let decode = &pool.policy.decode;
        let stream_lease = timeout_at(deadline, pool.streams.clone().acquire_owned())
            .await
            .map_err(|_| QueryError::Deadline)?
            .map_err(|_| QueryError::Cancelled)?;
        let mut grpc_request =
            tonic::Request::new(E::request(self.query_info(), request.to_query()));
        grpc_request.set_timeout(deadline.saturating_duration_since(Instant::now()));
        let failure = framing::Failure::default();
        // Item 8, option 2a: the body assembles each message and parks it here.
        let parked = framing::ParkedFrame::default();
        let limit = decode.encoded_bytes + 1024;
        let mut stub = tonic::client::Grpc::new(framing::GuardedChannel {
            channel: self.channel.clone(),
            limit,
            failure: failure.clone(),
            parked: parked.clone(),
        })
        .max_decoding_message_size(limit);
        check_deadline(deadline)?;
        let response = timeout_at(deadline, async {
            stub.ready().await.map_err(|_| QueryError::Connection)?;
            check_deadline(deadline)?;
            stub.server_streaming(
                grpc_request,
                tonic::codegen::http::uri::PathAndQuery::from_static(E::PATH),
                envelope::EnvelopeCodec::<E::Request>::new(parked),
            )
            .await
            .map_err(|error| failure.classify(error))
        })
        .await
        .map_err(|_| QueryError::Deadline)?;
        check_deadline(deadline)?;
        Ok((response?.into_inner(), failure, stream_lease))
    }
}

/// A stream of exact Arrow batches for one query.
///
/// Column types are fixed by their first value. A column with no value yet
/// in a batch is `Null`; the only schema change between batches is such a
/// column becoming typed. [`QueryStream::collect`] applies the final types to
/// earlier batches.
pub struct QueryStream {
    inner: Option<tonic::Streaming<Vec<u8>>>,
    decoder: Option<Box<ColumnarDecoder>>,
    pool: Arc<crate::eod::Pool>,
    deadline: Instant,
    idle: Duration,
    batch: BatchPolicy,
    failure: framing::Failure,
    /// The stream admission lease and the scheduler slot, released together.
    leases: Option<(OwnedSemaphorePermit, OwnedSemaphorePermit)>,
    /// A batch has been returned, so later rows coalesce.
    emitted: bool,
    /// When the oldest buffered row arrived.
    first_buffered: Option<Instant>,
    /// A terminal error held back until the rows before it are delivered.
    pending: Option<QueryError>,
    terminal: bool,
}

enum Step {
    Message(Vec<u8>),
    End,
    Flush,
}

impl QueryStream {
    /// Stop the query and release its resources; the next call reports
    /// nothing more.
    pub fn cancel(&mut self) {
        self.inner = None;
        self.decoder = None;
        self.leases = None;
        self.terminal = true;
    }

    fn finish(&mut self) {
        self.cancel();
    }

    /// The next batch, `Ok(None)` at the end, or a terminal error, returned
    /// once. Rows decoded before an error are delivered first. Dropping a
    /// pending call cancels the stream; its next call reports `Cancelled`.
    pub async fn next_batch(&mut self) -> Result<Option<RecordBatch>, QueryError> {
        if let Some(error) = self.pending.take() {
            self.finish();
            return Err(error);
        }
        if self.terminal {
            return Ok(None);
        }
        // Take the transport and decoder before the first await: dropping a
        // pending call then leaves the stream without them, which is terminal.
        let (Some(inner), Some(decoder)) = (self.inner.take(), self.decoder.take()) else {
            self.finish();
            return Err(QueryError::Cancelled);
        };
        // Fields drop in order if this call is dropped: transport first.
        let mut state = (inner, Some(decoder), self.leases.take());
        let result = self.next_inner(&mut state.0, &mut state.1).await;
        let result = check_deadline(self.deadline).and(result);
        match (result, state.1) {
            (Ok(Some(batch)), Some(decoder)) => {
                self.inner = Some(state.0);
                self.decoder = Some(decoder);
                self.leases = state.2;
                Ok(Some(batch))
            }
            (Ok(_), _) => {
                self.finish();
                Ok(None)
            }
            (Err(error), decoder) => {
                // Rows decoded before the failure are delivered first.
                let earlier = decoder.and_then(|mut d| self.take_batch(&mut d));
                self.finish();
                match earlier {
                    Some(batch) => {
                        self.pending = Some(error);
                        Ok(Some(batch))
                    }
                    None => Err(error),
                }
            }
        }
    }

    fn flush_due(&self, decoder: &ColumnarDecoder) -> bool {
        decoder.buffered_rows() > 0
            && (!self.emitted
                || decoder.buffered_rows() >= self.batch.rows
                || decoder.buffered_bytes() >= self.batch.bytes
                || self
                    .first_buffered
                    .is_some_and(|t| t.elapsed() >= self.batch.latency))
    }

    fn take_batch(&mut self, decoder: &mut ColumnarDecoder) -> Option<RecordBatch> {
        let batch = decoder.flush()?;
        self.emitted = true;
        self.first_buffered = None;
        Some(batch)
    }

    async fn next_inner(
        &mut self,
        inner: &mut tonic::Streaming<Vec<u8>>,
        slot: &mut Option<Box<ColumnarDecoder>>,
    ) -> Result<Option<RecordBatch>, QueryError> {
        loop {
            let decoder = slot.as_mut().ok_or(QueryError::Cancelled)?;
            check_deadline(self.deadline)?;
            if self.flush_due(decoder)
                && let Some(batch) = self.take_batch(decoder)
            {
                return Ok(Some(batch));
            }
            let idle = Instant::now()
                .checked_add(self.idle)
                .ok_or(QueryError::Configuration)?
                .min(self.deadline);
            let flush_at = self
                .first_buffered
                .map(|t| t + self.batch.latency)
                .filter(|_| decoder.buffered_rows() > 0);
            let wait_until = flush_at.map_or(idle, |f| f.min(idle));
            let step = match timeout_at(wait_until, inner.message()).await {
                Ok(Ok(Some(frame))) => Step::Message(frame),
                Ok(Ok(None)) => Step::End,
                Ok(Err(status)) => return Err(self.failure.classify(status)),
                Err(_) if flush_at.is_some_and(|f| f <= Instant::now()) => Step::Flush,
                Err(_) if Instant::now() >= self.deadline => return Err(QueryError::Deadline),
                Err(_) => return Err(QueryError::Idle),
            };
            match step {
                Step::Flush => continue,
                Step::End => return Ok(self.take_batch(decoder)),
                Step::Message(frame) => {
                    check_deadline(self.deadline)?;
                    let before = decoder.buffered_rows();
                    self.decode(slot, frame).await?;
                    let decoder = slot.as_ref().ok_or(QueryError::Cancelled)?;
                    if decoder.buffered_rows() > before && self.first_buffered.is_none() {
                        self.first_buffered = Some(Instant::now());
                    }
                }
            }
        }
    }

    /// Decode one frame inline or on the blocking pool. A full buffer is
    /// flushed by the caller's thresholds first, so `BufferFull` here means
    /// one message larger than the buffer allows.
    async fn decode(
        &mut self,
        slot: &mut Option<Box<ColumnarDecoder>>,
        frame: Vec<u8>,
    ) -> Result<(), QueryError> {
        if frame.len() <= self.batch.inline_bytes {
            let decoder = slot.as_mut().ok_or(QueryError::Cancelled)?;
            decoder.push_encoded(&frame).map_err(map_decode)?;
            return Ok(());
        }
        let job = timeout_at(self.deadline, self.pool.jobs.clone().acquire_owned())
            .await
            .map_err(|_| QueryError::Deadline)?
            .map_err(|_| QueryError::Cancelled)?;
        // The decoder moves to the worker and back; if this call is dropped
        // while it runs, the stream has no decoder and is terminal.
        let mut moved = slot.take().ok_or(QueryError::Cancelled)?;
        let (moved, result) = tokio::task::spawn_blocking(move || {
            let result = moved.push_encoded(&frame);
            drop((frame, job));
            (moved, result)
        })
        .await
        .map_err(|_| QueryError::Decode)?;
        *slot = Some(moved);
        result.map(|_| ()).map_err(map_decode)
    }

    /// Read the whole stream. Columns that were `Null` in earlier batches and
    /// typed later are given their final type (as all-null columns), so every
    /// batch has the same schema.
    pub async fn collect(mut self) -> Result<Vec<RecordBatch>, QueryError> {
        let mut batches = Vec::new();
        while let Some(batch) = self.next_batch().await? {
            batches.push(batch);
        }
        Ok(unify(batches))
    }
}

/// Give every batch the final type of each column (the last non-`Null` one).
pub fn unify(batches: Vec<RecordBatch>) -> Vec<RecordBatch> {
    let Some(last) = batches.last() else {
        return batches;
    };
    let names: Vec<String> = last
        .schema()
        .fields()
        .iter()
        .map(|f| f.name().clone())
        .collect();
    let mut types: Vec<DataType> = vec![DataType::Null; names.len()];
    for batch in &batches {
        for (index, column) in batch.columns().iter().enumerate() {
            if column.data_type() != &DataType::Null {
                types[index] = column.data_type().clone();
            }
        }
    }
    let schema = Arc::new(Schema::new(
        names
            .iter()
            .zip(&types)
            .map(|(name, data_type)| Field::new(name, data_type.clone(), true))
            .collect::<Vec<_>>(),
    ));
    batches
        .into_iter()
        .map(|batch| {
            if batch.schema() == schema {
                return batch;
            }
            let columns: Vec<ArrayRef> = batch
                .columns()
                .iter()
                .zip(&types)
                .map(|(column, data_type)| {
                    if column.data_type() == &DataType::Null && data_type != &DataType::Null {
                        new_null_array(data_type, column.len())
                    } else {
                        column.clone()
                    }
                })
                .collect();
            RecordBatch::try_new(schema.clone(), columns).expect("only Null columns are retyped")
        })
        .collect()
}

with_setters!(BatchPolicy {
    with_rows => rows: usize,
    with_bytes => bytes: usize,
    with_latency => latency: Duration,
    with_inline_bytes => inline_bytes: usize,
});
