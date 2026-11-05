//! Python bindings: a thin pass-through over Arrow (P1 PR 6a, plan D.2).
//!
//! Every query runs in Rust (scheduler, transport, decoding into Arrow) with
//! the GIL released. Results leave through the Arrow PyCapsule interface
//! (`__arrow_c_stream__`, `__arrow_c_array__`), so pyarrow, Polars and pandas
//! import the buffers without copying and Python creates no object per row.
#[rustfmt::skip]
mod generated;
mod kwargs;

use arrow_array::ffi::to_ffi;
use arrow_array::ffi_stream::FFI_ArrowArrayStream;
use arrow_array::{Array, RecordBatch, RecordBatchIterator, StructArray};
use arrow_schema::{Schema, SchemaRef};
use kwargs::Kwargs;
use praevis_thetadata_arrow::{ColumnarDecoder, Limits, ParquetOptions};
use praevis_thetadata_auth::{AuthClient, AuthConfig, Credentials, Environment};
use praevis_thetadata_client::endpoints::Endpoint;
use praevis_thetadata_client::{
    ClientConfig, JobStream, ParquetOutputError, QueryError, QueryStream, SchedulerPolicy,
    ThetaClient, Tier, plan as plan_parts,
};
use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyStopIteration, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyCapsule, PyDict, PyTuple};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

create_exception!(
    _native,
    ThetaDataError,
    PyException,
    "A query or output failure."
);
create_exception!(
    _native,
    QueryFailed,
    ThetaDataError,
    "A query failed; args: (kind, detail, grpc_code or None)."
);
create_exception!(
    _native,
    OutputFailed,
    ThetaDataError,
    "A local output failed; args: (kind, detail)."
);

/// One runtime for the process; Python threads block on it with the GIL released.
fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(std::thread::available_parallelism().map_or(2, |n| n.get().min(4)))
            .thread_name("praevis-thetadata")
            .enable_all()
            .build()
            .expect("tokio runtime")
    })
}

fn kind(error: QueryError) -> &'static str {
    match error {
        QueryError::Input => "input",
        QueryError::Configuration => "configuration",
        QueryError::Connection => "connection",
        QueryError::Deadline => "deadline",
        QueryError::Idle => "idle",
        QueryError::Cancelled => "cancelled",
        QueryError::NoData => "no_data",
        QueryError::Remote(_) => "remote",
        QueryError::Decode => "decode",
        QueryError::Schema => "schema",
        QueryError::Unsupported => "unsupported",
        QueryError::Resource(_) => "resource",
    }
}
/// `QueryFailed(kind, detail, grpc_code)`: the gRPC code name (for example
/// `"Unauthenticated"`) for remote statuses, otherwise `None`.
fn query_error(error: QueryError) -> PyErr {
    let code = match error {
        QueryError::Remote(code) => Some(format!("{code:?}")),
        _ => None,
    };
    QueryFailed::new_err((kind(error), error.to_string(), code))
}
fn output_error(error: ParquetOutputError) -> PyErr {
    match error {
        ParquetOutputError::Query(error) => query_error(error),
        ParquetOutputError::Sink(error) => OutputFailed::new_err(("output", error.to_string())),
    }
}

/// Runs one typed request; implemented for each way of consuming a result.
pub(crate) trait Runner {
    type Output;
    fn run<E>(self, request: E) -> PyResult<Self::Output>
    where
        E: Endpoint + Clone + Send + Sync + 'static;
}

struct Collect<'a, 'py> {
    py: Python<'py>,
    client: &'a ThetaClient,
    plan: bool,
}
impl Runner for Collect<'_, '_> {
    type Output = Table;
    fn run<E>(self, request: E) -> PyResult<Table>
    where
        E: Endpoint + Clone + Send + Sync + 'static,
    {
        let client = self.client.clone();
        let plan = self.plan;
        let batches = self
            .py
            .detach(move || {
                runtime().block_on(async move {
                    if plan {
                        client.job(&request).await?.collect().await
                    } else {
                        client.query(&request).await?.collect().await
                    }
                })
            })
            .map_err(query_error)?;
        Ok(Table::new(batches))
    }
}

enum Source {
    Query(Box<QueryStream>),
    Job(Box<JobStream>),
}
struct Stream<'a, 'py> {
    py: Python<'py>,
    client: &'a ThetaClient,
    plan: bool,
}
impl Runner for Stream<'_, '_> {
    type Output = BatchIterator;
    fn run<E>(self, request: E) -> PyResult<BatchIterator>
    where
        E: Endpoint + Clone + Send + Sync + 'static,
    {
        let client = self.client.clone();
        let plan = self.plan;
        let source = self
            .py
            .detach(move || {
                runtime().block_on(async move {
                    Ok::<_, QueryError>(if plan {
                        Source::Job(Box::new(client.job(&request).await?))
                    } else {
                        Source::Query(Box::new(client.query(&request).await?))
                    })
                })
            })
            .map_err(query_error)?;
        Ok(BatchIterator {
            source: Mutex::new(Some(source)),
        })
    }
}

struct Parquet<'a, 'py> {
    py: Python<'py>,
    client: &'a ThetaClient,
    plan: bool,
    path: String,
    options: ParquetOptions,
}
impl Runner for Parquet<'_, '_> {
    type Output = (usize, usize, u64);
    fn run<E>(self, request: E) -> PyResult<(usize, usize, u64)>
    where
        E: Endpoint + Clone + Send + Sync + 'static,
    {
        let Self {
            py,
            client,
            plan,
            path,
            options,
        } = self;
        let client = client.clone();
        let summary = py
            .detach(move || {
                runtime().block_on(async move {
                    if plan {
                        let job = client.job(&request).await?;
                        job.to_parquet(&path, options).await
                    } else {
                        let stream = client.query(&request).await?;
                        stream.to_parquet(&path, options).await
                    }
                })
            })
            .map_err(output_error)?;
        Ok((summary.rows, summary.row_groups, summary.bytes))
    }
}

/// The wire query and the number of planned parts, for tests and inspection.
struct Describe {
    plan: bool,
}
impl Runner for Describe {
    type Output = (String, usize);
    fn run<E>(self, request: E) -> PyResult<(String, usize)>
    where
        E: Endpoint + Clone + Send + Sync + 'static,
    {
        let parts = if self.plan {
            plan_parts(&request).len()
        } else {
            1
        };
        Ok((format!("{:?}", request.to_query()), parts))
    }
}

/// A client for the direct gRPC service, from exactly one credential source.
/// The Python package's `Client` chooses it in the ADR-0020 order.
#[pyclass(module = "praevis_thetadata._native", frozen)]
struct Client {
    inner: ThetaClient,
}

#[pymethods]
impl Client {
    #[new]
    #[pyo3(signature = (*, api_key = None, email = None, password = None, creds_file = None, environment = "prod", concurrency = None, per_minute = None, retries = 3, request_timeout = 300.0, idle_timeout = 60.0))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        py: Python<'_>,
        api_key: Option<String>,
        email: Option<String>,
        password: Option<String>,
        creds_file: Option<std::path::PathBuf>,
        environment: &str,
        concurrency: Option<usize>,
        per_minute: Option<u32>,
        retries: u32,
        request_timeout: f64,
        idle_timeout: f64,
    ) -> PyResult<Self> {
        // Exactly one source, already chosen by the Python layer in the
        // ADR-0020 order. The file is parsed like the CLI parses it.
        let credentials = match (creds_file, api_key, email, password) {
            (Some(path), None, None, None) => Credentials::from_file(&path).map_err(|_| {
                QueryFailed::new_err(("credentials", "could not read the credentials file"))
            })?,
            (None, Some(key), None, None) => Credentials::api_key(key.trim()),
            (None, None, Some(email), Some(password)) => {
                Credentials::email_password(email, password)
            }
            _ => {
                return Err(PyValueError::new_err(
                    "give exactly one of api_key, email and password, or creds_file",
                ));
            }
        };
        let environment = match environment {
            "prod" => Environment::Prod,
            "stage" => Environment::Stage,
            _ => return Err(PyValueError::new_err("environment: 'prod' or 'stage'")),
        };
        let seconds = |name: &str, value: f64| {
            Duration::try_from_secs_f64(value)
                .ok()
                .filter(|d| !d.is_zero())
                .ok_or_else(|| PyValueError::new_err(format!("{name}: positive seconds")))
        };
        let config = ClientConfig {
            request_timeout: seconds("request_timeout", request_timeout)?,
            idle_timeout: seconds("idle_timeout", idle_timeout)?,
            ..Default::default()
        };

        let inner = py.detach(move || {
            runtime().block_on(async move {
                let auth = AuthClient::new(AuthConfig {
                    environment,
                    ..Default::default()
                })
                .map_err(|_| query_error(QueryError::Configuration))?;
                // The key is never logged or echoed in errors.
                let session = auth.authenticate(&credentials).await.map_err(|_| {
                    QueryFailed::new_err(("authentication", "authentication failed"))
                })?;
                let client = ThetaClient::with_session(config, session)
                    .await
                    .map_err(|_| query_error(QueryError::Connection))?;
                let policy = scheduler_policy(client.tier(), concurrency, per_minute, retries);
                client.with_scheduler(policy).map_err(query_error)
            })
        })?;
        Ok(Self { inner })
    }

    /// The account's tier from the login response: `"FREE"`, `"VALUE"`,
    /// `"STANDARD"` or `"PRO"` (the highest of its subscriptions).
    #[getter]
    fn tier(&self) -> &'static str {
        self.inner.tier().name()
    }

    /// The scheduler limits in use: `(concurrency, per_minute)`, where
    /// `per_minute` is `None` for no per-minute limit.
    #[getter]
    fn scheduler(&self) -> (usize, Option<u32>) {
        let policy = self.inner.scheduler_policy();
        (policy.concurrency, policy.per_minute)
    }

    /// Run `endpoint` (a vendor name such as `stock_history_eod`, or an RPC
    /// name) and return the whole result. With `plan`, long ranges are split
    /// by date and merged in order.
    #[pyo3(signature = (endpoint, *, plan = true, **kwargs))]
    fn query(
        &self,
        py: Python<'_>,
        endpoint: &str,
        plan: bool,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Table> {
        let runner = Collect {
            py,
            client: &self.inner,
            plan,
        };
        generated::dispatch(endpoint, Kwargs::new(kwargs), runner)
    }

    /// Like `query`, but returns an iterator of batches as they arrive.
    #[pyo3(signature = (endpoint, *, plan = true, **kwargs))]
    fn batches(
        &self,
        py: Python<'_>,
        endpoint: &str,
        plan: bool,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<BatchIterator> {
        let runner = Stream {
            py,
            client: &self.inner,
            plan,
        };
        generated::dispatch(endpoint, Kwargs::new(kwargs), runner)
    }

    /// Write the result to a Parquet file from Rust. Returns
    /// `(rows, row_groups, bytes)`; the file appears only when complete.
    #[pyo3(signature = (endpoint, path, *, overwrite = false, zstd_level = Some(1), plan = true, **kwargs))]
    #[allow(clippy::too_many_arguments)]
    fn to_parquet(
        &self,
        py: Python<'_>,
        endpoint: &str,
        path: String,
        overwrite: bool,
        zstd_level: Option<i32>,
        plan: bool,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<(usize, usize, u64)> {
        let runner = Parquet {
            py,
            client: &self.inner,
            plan,
            path,
            options: ParquetOptions {
                zstd_level,
                overwrite,
                ..Default::default()
            },
        };
        generated::dispatch(endpoint, Kwargs::new(kwargs), runner)
    }
}

/// The wire query `endpoint` would send, and how many parts a job plans.
#[pyfunction]
#[pyo3(signature = (endpoint, *, plan = true, **kwargs))]
fn describe(
    endpoint: &str,
    plan: bool,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<(String, usize)> {
    generated::dispatch(endpoint, Kwargs::new(kwargs), Describe { plan })
}

/// Every endpoint as `(vendor_name or "", rpc_name)`.
#[pyfunction]
fn endpoints() -> Vec<(&'static str, &'static str)> {
    generated::NAMES.to_vec()
}

/// A whole result: Arrow record batches with one schema. Implements the
/// Arrow PyCapsule stream interface, so `pyarrow.table(t)`,
/// `polars.DataFrame(t)` and friends import it without copying.
#[pyclass(module = "praevis_thetadata._native", frozen)]
struct Table {
    batches: Vec<RecordBatch>,
    schema: SchemaRef,
}
impl Table {
    fn new(batches: Vec<RecordBatch>) -> Self {
        let schema = batches
            .first()
            .map_or_else(|| Arc::new(Schema::empty()), RecordBatch::schema);
        Self { batches, schema }
    }
}

fn stream_capsule<'py>(
    py: Python<'py>,
    batches: Vec<RecordBatch>,
    schema: SchemaRef,
) -> PyResult<Bound<'py, PyCapsule>> {
    let reader = RecordBatchIterator::new(batches.into_iter().map(Ok), schema);
    let stream = FFI_ArrowArrayStream::new(Box::new(reader));
    // Dropping a stream the consumer did not move out releases it.
    PyCapsule::new_with_value(py, stream, c"arrow_array_stream")
}

#[pymethods]
impl Table {
    #[getter]
    fn num_rows(&self) -> usize {
        self.batches.iter().map(RecordBatch::num_rows).sum()
    }
    #[getter]
    fn column_names(&self) -> Vec<String> {
        self.schema
            .fields()
            .iter()
            .map(|f| f.name().clone())
            .collect()
    }
    fn __len__(&self) -> usize {
        self.num_rows()
    }
    #[pyo3(signature = (requested_schema = None))]
    fn __arrow_c_stream__<'py>(
        &self,
        py: Python<'py>,
        requested_schema: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyCapsule>> {
        // A requested schema is a hint; the exact types are kept.
        let _ = requested_schema;
        stream_capsule(py, self.batches.clone(), self.schema.clone())
    }
    /// The batches, each implementing the Arrow PyCapsule array interface.
    fn batches(&self) -> Vec<Batch> {
        self.batches
            .iter()
            .cloned()
            .map(|batch| Batch { batch })
            .collect()
    }
}

/// One record batch (Arrow PyCapsule array interface).
#[pyclass(module = "praevis_thetadata._native", frozen)]
struct Batch {
    batch: RecordBatch,
}

#[pymethods]
impl Batch {
    #[getter]
    fn num_rows(&self) -> usize {
        self.batch.num_rows()
    }
    #[getter]
    fn column_names(&self) -> Vec<String> {
        self.batch
            .schema()
            .fields()
            .iter()
            .map(|f| f.name().clone())
            .collect()
    }
    fn __len__(&self) -> usize {
        self.batch.num_rows()
    }
    #[pyo3(signature = (requested_schema = None))]
    fn __arrow_c_array__<'py>(
        &self,
        py: Python<'py>,
        requested_schema: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyTuple>> {
        let _ = requested_schema;
        let data = StructArray::from(self.batch.clone()).into_data();
        let (array, schema) =
            to_ffi(&data).map_err(|e| PyValueError::new_err(format!("Arrow export: {e}")))?;
        let schema = PyCapsule::new_with_value(py, schema, c"arrow_schema")?;
        let array = PyCapsule::new_with_value(py, array, c"arrow_array")?;
        PyTuple::new(py, [schema.into_any(), array.into_any()])
    }
    fn __arrow_c_stream__<'py>(
        &self,
        py: Python<'py>,
        requested_schema: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyCapsule>> {
        let _ = requested_schema;
        stream_capsule(py, vec![self.batch.clone()], self.batch.schema())
    }
}

/// Batches in order as they arrive; each step releases the GIL while it
/// waits. A column with no value yet is `Null` in early batches.
#[pyclass(module = "praevis_thetadata._native", frozen)]
struct BatchIterator {
    source: Mutex<Option<Source>>,
}

#[pymethods]
impl BatchIterator {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    fn __next__(&self, py: Python<'_>) -> PyResult<Batch> {
        let taken = self
            .source
            .lock()
            .map_err(|_| PyValueError::new_err("iterator poisoned"))?
            .take();
        let Some(mut source) = taken else {
            return Err(PyStopIteration::new_err(()));
        };
        let (source, next) = py.detach(move || {
            runtime().block_on(async move {
                let next = match &mut source {
                    Source::Query(stream) => stream.next_batch().await,
                    Source::Job(job) => job.next_batch().await,
                };
                (source, next)
            })
        });
        match next {
            Ok(Some(batch)) => {
                *self
                    .source
                    .lock()
                    .map_err(|_| PyValueError::new_err("iterator poisoned"))? = Some(source);
                Ok(Batch { batch })
            }
            Ok(None) => Err(PyStopIteration::new_err(())),
            Err(error) => Err(query_error(error)),
        }
    }
    /// Stop the query and release its requests.
    fn cancel(&self) {
        if let Ok(mut source) = self.source.lock() {
            *source = None;
        }
    }
}

/// The tier's limits with the caller's overrides: unset values come from
/// the tier, and a `per_minute` of 0 removes the per-minute limit.
fn scheduler_policy(
    tier: Tier,
    concurrency: Option<usize>,
    per_minute: Option<u32>,
    retries: u32,
) -> SchedulerPolicy {
    let base = SchedulerPolicy::for_tier(tier);
    SchedulerPolicy {
        concurrency: concurrency.unwrap_or(base.concurrency),
        per_minute: match per_minute {
            None => base.per_minute,
            Some(0) => None,
            Some(limit) => Some(limit),
        },
        retries,
        ..base
    }
}

/// Test seam for `scheduler_policy`: `(concurrency, per_minute)` for a tier
/// name, without logging in.
#[pyfunction]
#[pyo3(signature = (tier, concurrency = None, per_minute = None))]
fn _scheduler_policy(
    tier: &str,
    concurrency: Option<usize>,
    per_minute: Option<u32>,
) -> PyResult<(usize, Option<u32>)> {
    let tier = match tier {
        "FREE" => Tier::Free,
        "VALUE" => Tier::Value,
        "STANDARD" => Tier::Standard,
        "PRO" => Tier::Pro,
        _ => return Err(PyValueError::new_err("tier: FREE, VALUE, STANDARD or PRO")),
    };
    let policy = scheduler_policy(tier, concurrency, per_minute, 3);
    Ok((policy.concurrency, policy.per_minute))
}

/// Decode encoded `ResponseData` messages (one request's responses, as the
/// vendor baseline captures them) into a whole result, with the GIL released.
/// The offline counterpart of the vendor package's `_convert_response_stream`,
/// for conversion-only comparisons; no network.
#[pyfunction]
fn _decode_frames(py: Python<'_>, frames: Vec<Vec<u8>>) -> PyResult<Table> {
    let batches = py
        .detach(move || {
            let mut decoder = ColumnarDecoder::new(Limits::default())?;
            let mut out = Vec::new();
            for frame in &frames {
                decoder.push_encoded(frame)?;
                if decoder.buffered_rows() >= 65_536 {
                    out.extend(decoder.flush());
                }
            }
            out.extend(decoder.flush());
            Ok::<_, praevis_thetadata_arrow::DecodeError>(praevis_thetadata_client::unify(out))
        })
        .map_err(|e| PyValueError::new_err(format!("decode: {e:?}")))?;
    Ok(Table::new(batches))
}

/// Encoded synthetic `ResponseData` messages, one row each as the vendor
/// sends them (2 timestamps, 6 prices, `columns - 8` integers), for offline
/// tests of the capture tooling.
#[pyfunction]
#[pyo3(signature = (rows, *, columns = 16))]
fn _synthetic_frames(
    py: Python<'_>,
    rows: usize,
    columns: usize,
) -> PyResult<Vec<Py<pyo3::types::PyBytes>>> {
    use praevis_thetadata_proto::endpoints::{
        CompressionDescription, DataTable, DataValue, DataValueList, Price, ResponseData,
        ZonedDateTime, data_value::DataType,
    };
    use prost::Message;
    if columns < 8 {
        return Err(PyValueError::new_err("columns >= 8"));
    }
    let headers: Vec<String> = (0..columns).map(|c| format!("c{c}")).collect();
    let frames: Vec<Vec<u8>> = (0..rows as i64)
        .map(|r| {
            let mut values = vec![
                Some(DataType::Timestamp(ZonedDateTime {
                    epoch_ms: 1_718_380_800_000 + r as u64,
                    zone: 0,
                    ..Default::default()
                })),
                Some(DataType::Timestamp(ZonedDateTime {
                    epoch_ms: 1_718_402_400_000 + r as u64,
                    zone: 0,
                    ..Default::default()
                })),
            ];
            values.extend((0..6).map(|k| {
                Some(DataType::Price(Price {
                    value: 18_900 + (r % 97) as i32 + k,
                    r#type: 8,
                }))
            }));
            values.extend((8..columns).map(|k| Some(DataType::Number(r * 7 + k as i64))));
            let table = DataTable {
                headers: headers.clone(),
                data_table: vec![DataValueList {
                    values: values
                        .into_iter()
                        .map(|data_type| DataValue { data_type })
                        .collect(),
                }],
            };
            let bytes = table.encode_to_vec();
            ResponseData {
                original_size: bytes.len() as i32,
                compressed_data: zstd::bulk::compress(&bytes, 3).expect("zstd"),
                compression_description: Some(CompressionDescription { algo: 1, level: 3 }),
                flat_file_manifest: None,
            }
            .encode_to_vec()
        })
        .collect();
    Ok(frames
        .iter()
        .map(|f| pyo3::types::PyBytes::new(py, f).unbind())
        .collect())
}

/// Synthetic vendor-shaped rows through the real decoder, for tests and the
/// D.2 gates: `rows` rows of 2 timestamps, 6 prices and `columns - 8`
/// integers, `per_message` rows per message.
#[pyfunction]
#[pyo3(signature = (rows, *, columns = 16, per_message = 4096))]
fn _synthetic(py: Python<'_>, rows: usize, columns: usize, per_message: usize) -> PyResult<Table> {
    use praevis_thetadata_proto::endpoints::{
        CompressionDescription, DataTable, DataValue, DataValueList, Price, ResponseData,
        ZonedDateTime, data_value::DataType,
    };
    use prost::Message;
    if columns < 8 || per_message == 0 {
        return Err(PyValueError::new_err("columns >= 8 and per_message > 0"));
    }
    let batches = py.detach(move || {
        let headers: Vec<String> = (0..columns).map(|c| format!("c{c}")).collect();
        let mut decoder = ColumnarDecoder::new(Limits::default()).expect("default limits");
        let mut out = Vec::new();
        let mut start = 0;
        while start < rows {
            let end = (start + per_message).min(rows);
            let table = DataTable {
                headers: headers.clone(),
                data_table: (start..end)
                    .map(|r| {
                        let r = r as i64;
                        let mut values: Vec<Option<DataType>> = vec![
                            Some(DataType::Timestamp(ZonedDateTime {
                                epoch_ms: 1_718_380_800_000 + r as u64,
                                zone: 0,
                                ..Default::default()
                            })),
                            Some(DataType::Timestamp(ZonedDateTime {
                                epoch_ms: 1_718_402_400_000 + r as u64,
                                zone: 0,
                                ..Default::default()
                            })),
                        ];
                        values.extend((0..6).map(|k| {
                            Some(DataType::Price(Price {
                                value: 18_900 + (r % 97) as i32 + k,
                                r#type: 8,
                            }))
                        }));
                        values
                            .extend((8..columns).map(|k| Some(DataType::Number(r * 7 + k as i64))));
                        DataValueList {
                            values: values
                                .into_iter()
                                .map(|data_type| DataValue { data_type })
                                .collect(),
                        }
                    })
                    .collect(),
            };
            let bytes = table.encode_to_vec();
            decoder
                .push(&ResponseData {
                    original_size: bytes.len() as i32,
                    compressed_data: zstd::bulk::compress(&bytes, 1).expect("zstd"),
                    compression_description: Some(CompressionDescription { algo: 1, level: 1 }),
                    flat_file_manifest: None,
                })
                .expect("synthetic rows decode");
            if decoder.buffered_rows() >= 65_536 {
                out.extend(decoder.flush());
            }
            start = end;
        }
        out.extend(decoder.flush());
        out
    });
    Ok(Table::new(batches))
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add("ThetaDataError", m.py().get_type::<ThetaDataError>())?;
    m.add("QueryFailed", m.py().get_type::<QueryFailed>())?;
    m.add("OutputFailed", m.py().get_type::<OutputFailed>())?;
    m.add_class::<Client>()?;
    m.add_class::<Table>()?;
    m.add_class::<Batch>()?;
    m.add_class::<BatchIterator>()?;
    m.add_function(wrap_pyfunction!(describe, m)?)?;
    m.add_function(wrap_pyfunction!(endpoints, m)?)?;
    m.add_function(wrap_pyfunction!(_synthetic, m)?)?;
    m.add_function(wrap_pyfunction!(_decode_frames, m)?)?;
    m.add_function(wrap_pyfunction!(_scheduler_policy, m)?)?;
    m.add_function(wrap_pyfunction!(_synthetic_frames, m)?)?;
    Ok(())
}
