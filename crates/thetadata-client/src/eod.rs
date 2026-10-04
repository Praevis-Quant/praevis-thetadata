//! Typed stock EOD pull streams. Raw generated helpers retain their original API.
pub use crate::bounded::DecodeLimits;
use crate::{ClientConfig, ThetaClient, api, bounded, envelope, framing, wire};
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
    /// Maximum simultaneous typed queries, including paused consumers (1..=64).
    pub concurrent_streams: usize,
    /// Actual uncompressed envelope bytes eligible for inline work. 0 forces
    /// offload; capped at 4096. ZSTD always uses a bounded worker.
    pub inline_bytes: usize,
}
impl Default for EodPolicy {
    fn default() -> Self {
        Self {
            decode: DecodeLimits::default(),
            concurrent_jobs: 2,
            shared_bytes: 512 << 20,
            concurrent_streams: 16,
            // Scheduling evidence is platform/workload dependent. Keep the
            // portable offload default; measured consumers can opt in up to 4 KiB.
            inline_bytes: 0,
        }
    }
}
pub(crate) struct Pool {
    policy: EodPolicy,
    jobs: Arc<Semaphore>,
    schemas: Arc<Semaphore>,
    streams: Arc<Semaphore>,
    #[cfg(test)]
    gate: std::sync::Mutex<Option<Arc<tests::Gate>>>,
}
fn units(bytes: usize) -> Result<usize, EodError> {
    bytes
        .checked_add(1023)
        .map(|n| n / 1024)
        .ok_or(EodError::Configuration)
}
impl Pool {
    pub(crate) fn new(policy: EodPolicy) -> Result<Arc<Self>, EodError> {
        policy
            .decode
            .validate()
            .map_err(|_| EodError::Configuration)?;
        let total = policy.shared_bytes / 1024;
        let batch = units(policy.decode.allocated_bytes)?;
        let schema = units(policy.decode.schema_bytes()?)?;
        if policy.concurrent_jobs == 0
            || policy.concurrent_jobs > 64
            || !(1..=64).contains(&policy.concurrent_streams)
            || policy.inline_bytes > 4096
            || total > u32::MAX as usize
            || total > Semaphore::MAX_PERMITS
            || batch.checked_add(schema).is_none_or(|n| n > total)
        {
            return Err(EodError::Configuration);
        }
        // Partition the shared budget so idle schema leases cannot consume the
        // last batch reservation and deadlock all streams. Round leases up and
        // the total down to KiB; this also avoids byte-permit limits on 32-bit.
        let jobs = policy.concurrent_jobs.min((total - schema) / batch);
        Ok(Arc::new(Self {
            streams: Arc::new(Semaphore::new(policy.concurrent_streams)),
            jobs: Arc::new(Semaphore::new(jobs)),
            schemas: Arc::new(Semaphore::new(total - jobs * batch)),
            #[cfg(test)]
            gate: std::sync::Mutex::new(None),
            policy,
        }))
    }
    pub(crate) fn connection_window(&self) -> u32 {
        // Each idle stream can withhold its entire HTTP/2 receive window. Keep
        // one extra window available so admitted streams can always progress.
        (self.policy.concurrent_streams as u32 + 1) * STREAM_WINDOW
    }
}
pub(crate) const STREAM_WINDOW: u32 = 2 << 20;
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
        let client =
            tokio::time::timeout(connect_timeout, Self::connect(config, session, pool, true))
                .await
                .map_err(|_| EodError::Connection)?
                .map_err(|error| match error {
                    crate::Error::Config(_) => EodError::Configuration,
                    _ => EodError::Connection,
                })?;
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
        let stream_lease = timeout_at(deadline, pool.streams.clone().acquire_owned())
            .await
            .map_err(|_| EodError::Deadline)?
            .map_err(|_| EodError::Cancelled)?;
        let schema_lease = timeout_at(
            deadline,
            pool.schemas
                .clone()
                .acquire_many_owned(units(pool.policy.decode.schema_bytes()?)? as u32),
        )
        .await
        .map_err(|_| EodError::Deadline)?
        .map_err(|_| EodError::Cancelled)?;
        let mut request = tonic::Request::new(api::StockHistoryEodRequest {
            query_info: Some(self.query_info()),
            params: Some(request.wire()),
        });
        request.set_timeout(deadline.saturating_duration_since(Instant::now()));
        let failure = framing::Failure::default();
        let mut stub = tonic::client::Grpc::new(framing::GuardedChannel {
            channel: self.channel.clone(),
            limit: pool.policy.decode.encoded_bytes + 1024,
            failure: failure.clone(),
        })
        .max_decoding_message_size(pool.policy.decode.encoded_bytes + 1024);
        check_deadline(deadline)?;
        let response = timeout_at(deadline, async {
            stub.ready().await.map_err(|_| EodError::Connection)?;
            check_deadline(deadline)?;
            stub.server_streaming(
                request,
                tonic::codegen::http::uri::PathAndQuery::from_static(
                    "/BetaEndpoints.BetaThetaTerminal/GetStockHistoryEod",
                ),
                envelope::EnvelopeCodec,
            )
            .await
            .map_err(|error| failure.classify(error))
        })
        .await
        .map_err(|_| EodError::Deadline)?;
        check_deadline(deadline)?;
        let inner = response?.into_inner();
        Ok(EodBatchStream {
            inner: Some(inner),
            pool,
            deadline,
            idle: self.config.idle_timeout,
            schema: None,
            schema_lease: Some(Arc::new(schema_lease)),
            terminal: false,
            table,
            failure,
            stream_lease: Some(stream_lease),
        })
    }
}

enum Output {
    Batch(DataBatch),
    Table(Table),
}
enum Input {
    Frame(Vec<u8>),
    Parsed(wire::ResponseData),
}
impl Input {
    fn response(self, limits: &DecodeLimits) -> Result<wire::ResponseData, EodError> {
        match self {
            Self::Frame(bytes) => envelope::parse(bytes, limits),
            Self::Parsed(response) => Ok(response),
        }
    }
}
fn check_deadline(deadline: Instant) -> Result<(), EodError> {
    if Instant::now() >= deadline {
        Err(EodError::Deadline)
    } else {
        Ok(())
    }
}
pub struct EodBatchStream {
    inner: Option<tonic::Streaming<Vec<u8>>>,
    pool: Arc<Pool>,
    deadline: Instant,
    idle: Duration,
    schema: Option<Arc<[String]>>,
    schema_lease: Option<Arc<OwnedSemaphorePermit>>,
    terminal: bool,
    table: bool,
    failure: framing::Failure,
    stream_lease: Option<OwnedSemaphorePermit>,
}
impl EodBatchStream {
    pub fn cancel(&mut self) {
        self.inner = None;
        self.schema = None;
        self.schema_lease = None;
        self.stream_lease = None;
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
        let Some(inner) = self.inner.take() else {
            self.finish();
            return Err(EodError::Cancelled);
        };
        // A dropped pending future drops transport and its stream slot together.
        // Tuple fields drop in order: release transport before admitting its
        // replacement, including when the enclosing pending future is dropped.
        let mut receiving = (inner, self.stream_lease.take());
        let result = timeout_at(self.deadline, async {
            loop {
                if Instant::now() >= self.deadline {
                    return Err(EodError::Deadline);
                }
                // Each slot reserves the per-batch allocation ceiling. Acquire
                // it before the codec allocates the owned envelope frame.
                let job = self
                    .pool
                    .jobs
                    .clone()
                    .acquire_owned()
                    .await
                    .map_err(|_| EodError::Cancelled)?;
                check_deadline(self.deadline)?;
                let idle_deadline = Instant::now()
                    .checked_add(self.idle)
                    .ok_or(EodError::Configuration)?
                    .min(self.deadline);
                let response = timeout_at(idle_deadline, receiving.0.message())
                    .await
                    .map_err(|_| {
                        if Instant::now() >= self.deadline {
                            EodError::Deadline
                        } else {
                            EodError::Idle
                        }
                    })?
                    .map_err(|error| self.failure.classify(error))?;
                let Some(response) = response else {
                    return Ok(None);
                };
                check_deadline(self.deadline)?;
                let limits = self.pool.policy.decode.clone();
                let schema = self.schema.clone();
                let schema_lease = self.schema_lease.clone();
                let table = self.table;
                let (input, inline) = if response.len() <= self.pool.policy.inline_bytes {
                    let response = envelope::parse(response, &limits)?;
                    let inline = response
                        .compression_description
                        .as_ref()
                        .is_none_or(|d| d.algo == 0);
                    (Input::Parsed(response), inline)
                } else {
                    (Input::Frame(response), false)
                };
                #[cfg(test)]
                let gate = self.pool.gate.lock().unwrap().clone();
                let work = move || {
                    #[cfg(test)]
                    if let Some(gate) = gate {
                        gate.wait();
                    }
                    let batch = input
                        .response(&limits)
                        .and_then(|response| bounded::decode(response, &limits, schema, table));
                    let headers = batch.as_ref().ok().map(|b| b.headers().clone());
                    let output = batch.map(|batch| {
                        if table {
                            Output::Table(batch.into_table())
                        } else {
                            Output::Batch(batch)
                        }
                    });
                    (output, headers, job, schema_lease)
                };
                let (output, headers, _job, _schema_lease) = if inline {
                    work()
                } else {
                    tokio::task::spawn_blocking(work)
                        .await
                        .map_err(|_| EodError::Decode)?
                };
                if Instant::now() >= self.deadline {
                    return Err(EodError::Deadline);
                }
                let output = output?;
                let headers = headers.unwrap();
                if headers.is_empty() {
                    // An already-buffered sequence of tiny empty responses must
                    // not monopolize an executor when the inline path is used.
                    drop((output, _job, _schema_lease));
                    tokio::task::yield_now().await;
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
        // timeout_at polls a ready inner future before its timer. Preserve local
        // deadline priority even for ready EOF/status at the expiration boundary.
        let result = check_deadline(self.deadline).and(result);
        match &result {
            Ok(Some(_)) => {
                let (inner, stream_lease) = receiving;
                self.inner = Some(inner);
                self.stream_lease = stream_lease;
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixture as fixture;
    use std::sync::{Condvar, Mutex};

    pub(super) struct Gate {
        entered: Semaphore,
        released: Mutex<bool>,
        changed: Condvar,
        thread: Mutex<Option<std::thread::ThreadId>>,
    }
    impl Gate {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                entered: Semaphore::new(0),
                released: Mutex::new(false),
                changed: Condvar::new(),
                thread: Mutex::new(None),
            })
        }
        pub(super) fn wait(&self) {
            *self.thread.lock().unwrap() = Some(std::thread::current().id());
            self.entered.add_permits(1);
            let mut ready = self.released.lock().unwrap();
            while !*ready {
                ready = self.changed.wait(ready).unwrap();
            }
        }
        fn release(&self) {
            *self.released.lock().unwrap() = true;
            self.changed.notify_all();
        }
    }
    struct Release(Arc<Gate>);
    impl Drop for Release {
        fn drop(&mut self) {
            self.0.release();
        }
    }
    fn request() -> StockEodRequest {
        StockEodRequest::new(
            "SYNTHETIC",
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 16).unwrap(),
        )
        .unwrap()
    }
    async fn client(server: &fixture::Fixture, deadline: Duration) -> ThetaClient {
        ThetaClient::with_eod_policy(
            ClientConfig {
                endpoint: Some(server.endpoint.clone()),
                allow_insecure: true,
                request_timeout: deadline,
                ..Default::default()
            },
            fixture::session().await,
            EodPolicy {
                concurrent_jobs: 1,
                inline_bytes: 0,
                ..Default::default()
            },
        )
        .await
        .unwrap()
    }
    #[tokio::test]
    async fn cancelled_running_work_retains_budget_and_queued_clones_cannot_oversubscribe() {
        let wire = fixture::table(fixture::Shape::Mixed, 1).0;
        let server = fixture::Fixture::start(fixture::Script {
            messages: vec![Ok(fixture::response(&wire, false))],
            ..Default::default()
        });
        let client = client(&server, Duration::from_secs(10)).await;
        let pool = client.eod_pool.clone();
        let schema_capacity = pool.schemas.available_permits();
        let gate = Gate::new();
        let release = Release(gate.clone());
        *pool.gate.lock().unwrap() = Some(gate.clone());
        let mut stream = client.stock_eod_batches(request()).await.unwrap();
        let mut pending = Box::pin(stream.next_batch());
        tokio::select! {
            permit = gate.entered.acquire() => permit.unwrap().forget(),
            result = &mut pending => panic!("worker did not block: {result:?}"),
        }
        drop(pending);
        assert_eq!(stream.next_batch().await.unwrap_err(), EodError::Cancelled);
        assert_eq!(pool.jobs.available_permits(), 0);
        assert!(pool.schemas.available_permits() < schema_capacity);
        let clone = client.clone();
        assert!(Arc::ptr_eq(&pool, &clone.eod_pool));
        let mut queued = clone.stock_eod_batches(request()).await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(20), queued.next_batch())
                .await
                .is_err()
        );
        assert_eq!(queued.next_batch().await.unwrap_err(), EodError::Cancelled);
        assert_eq!(pool.jobs.available_permits(), 0);
        drop(release);
        let permit =
            tokio::time::timeout(Duration::from_secs(5), pool.jobs.clone().acquire_owned())
                .await
                .unwrap()
                .unwrap();
        drop(permit);
        // The worker has finished and its abandoned output/leases are dropped.
        for _ in 0..100 {
            if pool.schemas.available_permits() == schema_capacity {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(pool.schemas.available_permits(), schema_capacity);
        assert_eq!(pool.jobs.available_permits(), 1);
    }
    #[tokio::test]
    async fn deadline_during_active_decode_retains_reservation_until_worker_exit() {
        let wire = fixture::table(fixture::Shape::Nulls, 1).0;
        let server = fixture::Fixture::start(fixture::Script {
            messages: vec![Ok(fixture::response(&wire, false))],
            ..Default::default()
        });
        let client = client(&server, Duration::from_millis(250)).await;
        let pool = client.eod_pool.clone();
        let gate = Gate::new();
        let release = Release(gate.clone());
        *pool.gate.lock().unwrap() = Some(gate.clone());
        let mut stream = client.stock_eod_batches(request()).await.unwrap();
        let mut pending = Box::pin(stream.next_batch());
        tokio::select! { permit = gate.entered.acquire() => permit.unwrap().forget(), result = &mut pending => panic!("worker did not start: {result:?}") }
        assert_eq!(pending.await.unwrap_err(), EodError::Deadline);
        assert_eq!(pool.jobs.available_permits(), 0);
        assert!(stream.next_batch().await.unwrap().is_none());
        drop(release);
        let permit =
            tokio::time::timeout(Duration::from_secs(5), pool.jobs.clone().acquire_owned())
                .await
                .unwrap()
                .unwrap();
        drop(permit);
        assert_eq!(pool.jobs.available_permits(), 1);
    }
    #[tokio::test]
    async fn schema_partition_cannot_consume_last_batch_slot() {
        let limits = DecodeLimits::default();
        let batch = units(limits.allocated_bytes).unwrap();
        let schema = units(limits.schema_bytes().unwrap()).unwrap();
        let pool = Pool::new(EodPolicy {
            shared_bytes: (batch + schema) * 1024,
            ..Default::default()
        })
        .unwrap();
        let schema_lease = pool
            .schemas
            .clone()
            .acquire_many_owned(schema as u32)
            .await
            .unwrap();
        assert_eq!(pool.schemas.available_permits(), 0);
        let job = pool.jobs.clone().try_acquire_owned().unwrap();
        assert!(pool.jobs.clone().try_acquire_owned().is_err());
        assert!(pool.schemas.clone().try_acquire_owned().is_err());
        drop((job, schema_lease));
        assert_eq!(pool.jobs.available_permits(), 1);
        assert_eq!(pool.schemas.available_permits(), schema);
        assert!(
            Pool::new(EodPolicy {
                shared_bytes: (batch + schema) * 1024 - 1,
                ..Default::default()
            })
            .is_err()
        );
    }

    #[tokio::test]
    async fn only_actual_small_none_frames_inline_and_delivery_does_not_prefetch() {
        let executor = std::thread::current().id();
        for compressed in [false, true] {
            let (wire, _) =
                fixture::table(fixture::Shape::Nulls, if compressed { 1000 } else { 1 });
            let mut response = fixture::response(&wire, compressed);
            response.original_size = 1; // Not evidence that compressed work is small.
            let server = fixture::Fixture::start(fixture::Script {
                messages: vec![Ok(response); 2],
                ..Default::default()
            });
            let client = ThetaClient::with_eod_policy(
                ClientConfig {
                    endpoint: Some(server.endpoint.clone()),
                    allow_insecure: true,
                    idle_timeout: Duration::from_millis(100),
                    request_timeout: Duration::from_secs(5),
                    ..Default::default()
                },
                fixture::session().await,
                EodPolicy {
                    inline_bytes: 4096,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
            let gate = Gate::new();
            gate.release();
            *client.eod_pool.gate.lock().unwrap() = Some(gate.clone());
            let mut stream = client.stock_eod_batches(request()).await.unwrap();
            assert!(stream.next_batch().await.unwrap().is_some());
            assert_eq!(*gate.thread.lock().unwrap() == Some(executor), !compressed);
            // Caller pauses beyond idle_timeout. No worker runs on the next
            // already-buffered response until next_batch is requested again.
            tokio::time::sleep(Duration::from_millis(150)).await;
            assert_eq!(gate.entered.available_permits(), 1);
            assert!(stream.next_batch().await.unwrap().is_some());
            assert_eq!(gate.entered.available_permits(), 2);
        }
    }
    #[tokio::test]
    async fn admission_deadlines_release_waiters_and_expired_streams_do_no_work() {
        let server = fixture::Fixture::start(fixture::Script {
            messages: vec![Ok(fixture::response(
                &fixture::table(fixture::Shape::Mixed, 1).0,
                false,
            ))],
            ..Default::default()
        });
        let client = client(&server, Duration::from_millis(150)).await;
        let pool = client.eod_pool.clone();
        let capacity = pool.schemas.available_permits();
        let schemas = pool
            .schemas
            .clone()
            .acquire_many_owned(capacity as u32)
            .await
            .unwrap();
        assert_eq!(
            client.stock_eod_batches(request()).await.err().unwrap(),
            EodError::Deadline
        );
        assert!(server.requests.lock().unwrap().is_empty());
        drop(schemas);
        let job = pool.jobs.clone().acquire_owned().await.unwrap();
        let mut stream = client.stock_eod_batches(request()).await.unwrap();
        assert_eq!(stream.next_batch().await.unwrap_err(), EodError::Deadline);
        assert!(stream.next_batch().await.unwrap().is_none());
        drop(job);
        assert_eq!(pool.schemas.available_permits(), capacity);
        let gate = Gate::new();
        gate.release();
        *pool.gate.lock().unwrap() = Some(gate.clone());
        let mut stream = client.stock_eod_batches(request()).await.unwrap();
        stream.deadline = Instant::now();
        stream.idle = Duration::ZERO;
        assert_eq!(stream.next_batch().await.unwrap_err(), EodError::Deadline);
        assert_eq!(gate.entered.available_permits(), 0);
        assert_eq!(pool.jobs.available_permits(), 1);
        assert_eq!(pool.schemas.available_permits(), capacity);
    }
    #[tokio::test]
    async fn paused_streams_cannot_exhaust_connection_credit_for_active_decode() {
        let wire = fixture::table(fixture::Shape::Nulls, 100_000).0;
        let server = fixture::Fixture::start(fixture::Script {
            messages: vec![Ok(fixture::response(&wire, false))],
            ..Default::default()
        });
        let client = ThetaClient::with_eod_policy(
            ClientConfig {
                endpoint: Some(server.endpoint.clone()),
                allow_insecure: true,
                idle_timeout: Duration::from_secs(5),
                request_timeout: Duration::from_secs(20),
                ..Default::default()
            },
            fixture::session().await,
            EodPolicy {
                concurrent_streams: 8,
                concurrent_jobs: 1,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let mut paused = Vec::new();
        for _ in 0..7 {
            paused.push(client.stock_eod_batches(request()).await.unwrap());
        }
        let mut active = client.stock_eod_batches(request()).await.unwrap();
        let batch = active.next_batch().await.unwrap().unwrap();
        assert_eq!(batch.row_count(), 100_000);
        assert_eq!(batch.cells().len(), 800_000);
        assert!(active.next_batch().await.unwrap().is_none());
        assert_eq!(client.eod_pool.streams.available_permits(), 1);
        drop(paused);
        assert_eq!(client.eod_pool.streams.available_permits(), 8);
    }
    #[tokio::test]
    async fn stream_admission_bounds_dispatch_and_releases_on_pending_drop() {
        let server = fixture::Fixture::start(fixture::Script {
            messages: vec![Ok(fixture::response(
                &fixture::table(fixture::Shape::Nulls, 1).0,
                false,
            ))],
            message_delay: Duration::from_secs(2),
            ..Default::default()
        });
        let client = ThetaClient::with_eod_policy(
            ClientConfig {
                endpoint: Some(server.endpoint.clone()),
                allow_insecure: true,
                request_timeout: Duration::from_millis(250),
                ..Default::default()
            },
            fixture::session().await,
            EodPolicy {
                concurrent_streams: 1,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let mut stream = client.stock_eod_batches(request()).await.unwrap();
        assert_eq!(
            client.stock_eod_batches(request()).await.err().unwrap(),
            EodError::Deadline
        );
        assert_eq!(server.requests.lock().unwrap().len(), 1);
        // Use a fresh deadline for the pending-future cancellation part.
        stream.deadline = Instant::now() + Duration::from_secs(5);
        assert!(
            tokio::time::timeout(Duration::from_millis(20), stream.next_batch())
                .await
                .is_err()
        );
        assert_eq!(client.eod_pool.streams.available_permits(), 1);
        assert_eq!(stream.next_batch().await.unwrap_err(), EodError::Cancelled);
        let replacement = client.stock_eod_batches(request()).await.unwrap();
        assert_eq!(server.requests.lock().unwrap().len(), 2);
        drop(replacement);
        assert_eq!(client.eod_pool.streams.available_permits(), 1);
        for concurrent_streams in [0, 65] {
            assert!(
                Pool::new(EodPolicy {
                    concurrent_streams,
                    ..Default::default()
                })
                .is_err()
            );
        }
    }
}
