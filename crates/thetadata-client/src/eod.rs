//! Typed stock EOD pull streams. Raw generated helpers retain their original API.
pub use crate::bounded::DecodeLimits;
use crate::{ClientConfig, ThetaClient, api, bounded, wire};
use chrono::Datelike;
pub use chrono::NaiveDate;
use std::{sync::Arc, time::Duration};
use thetadata_core::{DataBatch, Table};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore},
    time::{Instant, timeout_at},
};

/// Safe categories: never retain a remote message, payload, identity or URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EodError {
    #[error("invalid stock EOD input")]
    Input,
    #[error("invalid stock EOD configuration")]
    Configuration,
    #[error("stock EOD connection failed")]
    Connection,
    #[error("stock EOD query deadline exceeded")]
    Deadline,
    #[error("stock EOD response idle timeout")]
    Idle,
    #[error("stock EOD cancelled")]
    Cancelled,
    #[error("no stock EOD data found")]
    NoData,
    #[error("stock EOD remote status: {0}")]
    Remote(tonic::Code),
    #[error("invalid stock EOD data")]
    Decode,
    #[error("invalid stock EOD schema")]
    Schema,
    #[error("unsupported stock EOD response")]
    Unsupported,
    #[error("stock EOD resource limit: {0}")]
    Resource(&'static str),
}
fn status(error: tonic::Status) -> EodError {
    match error.code() {
        tonic::Code::NotFound => EodError::NoData,
        code => EodError::Remote(code),
    }
}

#[derive(Debug, Clone)]
pub struct StockEodRequest {
    symbol: String,
    start: NaiveDate,
    end: NaiveDate,
}
impl StockEodRequest {
    pub fn new(
        symbol: impl Into<String>,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Self, EodError> {
        let symbol = symbol.into();
        if symbol.is_empty()
            || symbol.trim() != symbol
            || symbol
                .chars()
                .any(|c| c.is_control() || c == ',' || c == '*')
            || !(1..=9999).contains(&start.year())
            || !(1..=9999).contains(&end.year())
            || start > end
        {
            return Err(EodError::Input);
        }
        Ok(Self { symbol, start, end })
    }
    fn wire(self) -> api::StockHistoryEodRequestQuery {
        api::StockHistoryEodRequestQuery {
            symbol: self.symbol,
            start_date: self.start.format("%Y-%m-%d").to_string(),
            end_date: self.end.format("%Y-%m-%d").to_string(),
        }
    }
}

/// Shared by clones of a client, independent of the raw helper settings.
#[derive(Debug, Clone)]
pub struct EodPolicy {
    pub decode: DecodeLimits,
    pub concurrent_jobs: usize,
    pub shared_bytes: usize,
}
impl Default for EodPolicy {
    fn default() -> Self {
        Self {
            decode: DecodeLimits::default(),
            concurrent_jobs: 2,
            shared_bytes: 512 << 20,
        }
    }
}
pub(crate) struct Pool {
    policy: EodPolicy,
    jobs: Arc<Semaphore>,
    memory: Arc<Semaphore>,
}
impl Pool {
    pub(crate) fn new(policy: EodPolicy) -> Result<Arc<Self>, EodError> {
        policy
            .decode
            .validate()
            .map_err(|_| EodError::Configuration)?;
        if policy.concurrent_jobs == 0
            || policy.concurrent_jobs > 64
            || policy.shared_bytes > u32::MAX as usize
            || policy.shared_bytes > Semaphore::MAX_PERMITS
            || policy
                .decode
                .allocated_bytes
                .checked_add(policy.decode.schema_bytes()?)
                .is_none_or(|minimum| minimum > policy.shared_bytes)
        {
            return Err(EodError::Configuration);
        }
        Ok(Arc::new(Self {
            jobs: Arc::new(Semaphore::new(policy.concurrent_jobs)),
            memory: Arc::new(Semaphore::new(policy.shared_bytes)),
            policy,
        }))
    }
}
fn budgets(config: &ClientConfig) -> Result<(), EodError> {
    for duration in [
        config.connect_timeout,
        config.request_timeout,
        config.idle_timeout,
    ] {
        if duration.is_zero() || Instant::now().checked_add(duration).is_none() {
            return Err(EodError::Configuration);
        }
    }
    Ok(())
}
impl ThetaClient {
    /// Configure a shared finite decode policy before opening the connection.
    pub async fn with_eod_policy(
        config: ClientConfig,
        session: crate::Session,
        policy: EodPolicy,
    ) -> Result<Self, EodError> {
        budgets(&config)?;
        let pool = Pool::new(policy)?;
        let connect_timeout = config.connect_timeout;
        let mut client = tokio::time::timeout(connect_timeout, Self::with_session(config, session))
            .await
            .map_err(|_| EodError::Connection)?
            .map_err(|error| match error {
                crate::Error::Config(_) => EodError::Configuration,
                _ => EodError::Connection,
            })?;
        client.eod_pool = pool;
        Ok(client)
    }
    /// Consume batches without timestamp formatting or implicit collection.
    ///
    /// ```no_run
    /// use thetadata_client::{ThetaClient, StockEodRequest, EodError};
    /// async fn consume(client: &ThetaClient, request: StockEodRequest) -> Result<(), EodError> {
    ///     let mut stream = client.stock_eod_batches(request).await?;
    ///     while let Some(batch) = stream.next_batch().await? {
    ///         // Process borrowed rows now. Previously delivered rows remain
    ///         // partial if a later call fails; no implicit accumulation occurs.
    ///         for row in batch.rows() { std::hint::black_box(row); }
    ///     }
    ///     // Reached clean EOF without an error: this query is complete.
    ///     Ok(())
    /// }
    /// ```
    pub async fn stock_eod_batches(
        &self,
        request: StockEodRequest,
    ) -> Result<EodBatchStream, EodError> {
        self.eod_stream(request, false).await
    }
    pub async fn stock_eod(&self, request: StockEodRequest) -> Result<EodTableStream, EodError> {
        Ok(EodTableStream {
            inner: self.eod_stream(request, true).await?,
        })
    }
    async fn eod_stream(
        &self,
        request: StockEodRequest,
        table: bool,
    ) -> Result<EodBatchStream, EodError> {
        budgets(&self.config)?;
        let deadline = Instant::now()
            .checked_add(self.config.request_timeout)
            .ok_or(EodError::Configuration)?;
        let pool = self.eod_pool.clone();
        let schema_lease = timeout_at(
            deadline,
            pool.memory
                .clone()
                .acquire_many_owned(pool.policy.decode.schema_bytes()? as u32),
        )
        .await
        .map_err(|_| EodError::Deadline)?
        .map_err(|_| EodError::Cancelled)?;
        let mut request = tonic::Request::new(api::StockHistoryEodRequest {
            query_info: Some(self.query_info()),
            params: Some(request.wire()),
        });
        request.set_timeout(deadline.saturating_duration_since(Instant::now()));
        let mut stub = self
            .stub
            .clone()
            .max_decoding_message_size(pool.policy.decode.encoded_bytes + 1024);
        let inner = timeout_at(deadline, stub.get_stock_history_eod(request))
            .await
            .map_err(|_| EodError::Deadline)?
            .map_err(status)?
            .into_inner();
        Ok(EodBatchStream {
            inner: Some(inner),
            pool,
            deadline,
            idle: self.config.idle_timeout,
            schema: None,
            schema_lease: Some(Arc::new(schema_lease)),
            terminal: false,
            table,
        })
    }
}

enum Output {
    Batch(DataBatch),
    Table(Table),
}
pub struct EodBatchStream {
    inner: Option<tonic::Streaming<wire::ResponseData>>,
    pool: Arc<Pool>,
    deadline: Instant,
    idle: Duration,
    schema: Option<Arc<[String]>>,
    schema_lease: Option<Arc<OwnedSemaphorePermit>>,
    terminal: bool,
    table: bool,
}
impl EodBatchStream {
    pub fn cancel(&mut self) {
        self.inner = None;
        self.schema = None;
        self.schema_lease = None;
    }
    fn finish(&mut self) {
        self.cancel();
        self.terminal = true;
    }
    /// A terminal error is returned once. Later Ok(None) is fusion, not proof
    /// that previously delivered partial results are complete. Dropping a polled
    /// pending call cancels this stream; its next call reports Cancelled once.
    pub async fn next_batch(&mut self) -> Result<Option<DataBatch>, EodError> {
        match self.next_output().await? {
            Some(Output::Batch(batch)) => Ok(Some(batch)),
            None => Ok(None),
            _ => unreachable!(),
        }
    }
    async fn next_output(&mut self) -> Result<Option<Output>, EodError> {
        if self.terminal {
            return Ok(None);
        }
        // Taking the transport before the first await makes dropping a pending
        // future terminal: no consumed response can later be silently skipped.
        let Some(mut inner) = self.inner.take() else {
            self.finish();
            return Err(EodError::Cancelled);
        };
        let result = timeout_at(self.deadline, async {
            loop {
                if Instant::now() >= self.deadline {
                    return Err(EodError::Deadline);
                }
                // Admit before receiving/allocating a ResponseData. Blocking jobs
                // retain both permits even when their JoinHandle is abandoned.
                let memory = self
                    .pool
                    .memory
                    .clone()
                    .acquire_many_owned(self.pool.policy.decode.allocated_bytes as u32)
                    .await
                    .map_err(|_| EodError::Cancelled)?;
                let job = self
                    .pool
                    .jobs
                    .clone()
                    .acquire_owned()
                    .await
                    .map_err(|_| EodError::Cancelled)?;
                let idle_deadline = Instant::now()
                    .checked_add(self.idle)
                    .ok_or(EodError::Configuration)?
                    .min(self.deadline);
                let response = timeout_at(idle_deadline, inner.message())
                    .await
                    .map_err(|_| {
                        if Instant::now() >= self.deadline {
                            EodError::Deadline
                        } else {
                            EodError::Idle
                        }
                    })?
                    .map_err(status)?;
                let Some(response) = response else {
                    return Ok(None);
                };
                let limits = self.pool.policy.decode.clone();
                let schema = self.schema.clone();
                let schema_lease = self.schema_lease.clone();
                let table = self.table;
                let (output, headers, _memory, _job, _schema_lease) =
                    tokio::task::spawn_blocking(move || {
                        let batch = bounded::decode(response, &limits, schema, table);
                        let headers = batch.as_ref().ok().map(|b| b.headers().clone());
                        let output = batch.map(|batch| {
                            if table {
                                Output::Table(batch.into_table())
                            } else {
                                Output::Batch(batch)
                            }
                        });
                        (output, headers, memory, job, schema_lease)
                    })
                    .await
                    .map_err(|_| EodError::Decode)?;
                if Instant::now() >= self.deadline {
                    return Err(EodError::Deadline);
                }
                let output = output?;
                let headers = headers.unwrap();
                if headers.is_empty() {
                    continue;
                }
                if self.schema.is_none() {
                    self.schema = Some(headers);
                }
                return Ok(Some(output));
            }
        })
        .await
        .unwrap_or(Err(EodError::Deadline));
        match &result {
            Ok(Some(_)) => self.inner = Some(inner),
            _ => self.finish(),
        }
        result
    }
}
pub struct EodTableStream {
    inner: EodBatchStream,
}
impl EodTableStream {
    pub fn cancel(&mut self) {
        self.inner.cancel();
    }
    /// Explicit formatting adapter over the same bounded numeric stream engine.
    /// See EodBatchStream::next_batch for partial-result and cancellation rules.
    pub async fn next_batch(&mut self) -> Result<Option<Table>, EodError> {
        match self.inner.next_output().await? {
            Some(Output::Table(table)) => Ok(Some(table)),
            None => Ok(None),
            _ => unreachable!(),
        }
    }
}
