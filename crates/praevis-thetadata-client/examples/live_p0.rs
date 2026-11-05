//! Phase P0 measurement against the live service (performance plan C.2 and E).
//! Manual only: never CI. Requires `--features live-tests`, `--confirm-live`
//! and `THETADATA_API_KEY` in the environment (never printed or stored).
//!
//! * `breakdown`: runs the FREE workloads of `bench/python-baseline/workloads.json`
//!   (W1, W2, W2b) and records, per request:
//!   - `e2e`: the public client (typed numeric and Table streams for stock EOD;
//!     the generated raw helper for option EOD);
//!   - `drain`: the generated gRPC stub consumed without conversion
//!     (headers, first message, last message, messages and bytes);
//!   - `cpu`: the drained responses decoded offline, repeated (legacy Table
//!     decoder, bounded numeric, bounded Table).
//! * `limits`: bounded, polite probes of the FREE tier: concurrency (2, then 3
//!   simultaneous tiny requests), the per-minute limit (paced sequential tiny
//!   requests) and batch sizes (the largest FREE EOD span and one option chain
//!   day). Every probe stops at the first `RESOURCE_EXHAUSTED` or any status
//!   other than `NOT_FOUND`.
//!
//! * `entitlements`: the FREE entitlement map (P1 PR 2): one small request per
//!   endpoint family through the generated typed requests, strictly sequential
//!   and paced, reading at most a few messages before dropping the stream.
//!   Per-endpoint refusals are recorded and the run continues; it stops at the
//!   first `RESOURCE_EXHAUSTED` or `UNAVAILABLE`. No concurrency. A one-time,
//!   user-approved exception to "never sweep endpoints" (CLAUDE.md); re-running
//!   it needs approval again. The private report keeps truncated vendor status
//!   text for a person to classify locally; the code never interprets it, and
//!   only classifications and gRPC codes are committed. `--set options-pro`
//!   is the options PRO map (one approved run, 2026-11-04): every option RPC
//!   once, each read for at most `--max-messages`. Flat files come last,
//!   through a second client on the same session with a 1 MiB message limit,
//!   reading one message: a larger first message is refused from its length
//!   prefix (`OverProbeLimit`), so a probe never pulls bulk data.
//! * `session`: the session probe (approved 2026-11-04): a second login, then
//!   one small request on the first session and one on the second, recording
//!   each gRPC code.
//!
//! Output is timings, counts, sizes and status codes only in
//! `artifacts/p0/<run-id>.json`: no row values, account email, subscription
//! fields or session token (the report is refused if it would contain them).
use clap::{Args, Parser, Subcommand};
use praevis_thetadata_client::endpoints::{self as typed, Endpoint};
use praevis_thetadata_client::{
    AuthClient, AuthConfig, ClientConfig, Credentials, Environment, EodError, EodPolicy, Error,
    NaiveDate, Session, StockEodRequest, ThetaClient,
};
use praevis_thetadata_proto::{beta_endpoints as api, endpoints as wire};
use prost::Message;
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

// The production decoders, compiled directly for CPU-only timing (the same
// pattern as eod_benchmark); no benchmark API is added to the library.
#[allow(dead_code)]
#[path = "../src/bounded.rs"]
mod bounded;
#[path = "../src/decode.rs"]
mod decode;
#[allow(dead_code)]
#[path = "../src/envelope.rs"]
mod envelope;
mod eod {
    pub use praevis_thetadata_client::EodError;
}

const LIMIT: usize = 64 * 1024 * 1024;
/// The flat-file probes' message limit. tonic refuses a message whose
/// declared length is larger before reading its body, so a probe receives at
/// most one such message plus what HTTP/2 flow control lets the server send
/// ahead (the 2 MiB stream window) before the stream is dropped.
const FLAT_FILE_PROBE_LIMIT: usize = 1024 * 1024;
const WORKLOADS: &str = include_str!("../../../bench/python-baseline/workloads.json");

#[derive(Parser)]
#[command(about = "P0 live measurements (private artifacts; manual only)")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Network/server vs decode breakdown for the FREE workloads.
    Breakdown(Breakdown),
    /// Bounded probes of the FREE limits.
    Limits(LimitsArgs),
    /// The FREE entitlement map: one paced request per endpoint family.
    Entitlements(EntitlementsArgs),
    /// P1 exit criterion a: the FREE-entitled endpoints through query(),
    /// compared value by value with the legacy decoder on a second request.
    Checks(ChecksArgs),
    /// The session probe: a second login, then one request on each session.
    Session(SessionArgs),
}

#[derive(Args)]
struct ChecksArgs {
    #[command(flatten)]
    common: Common,
    /// Historical trading day inside the FREE window.
    #[arg(long, default_value = "2024-06-14")]
    date: String,
    /// An option expiration listed on `--date`.
    #[arg(long, default_value = "2024-06-21")]
    expiration: String,
}

#[derive(Args)]
struct EntitlementsArgs {
    #[command(flatten)]
    common: Common,
    /// Historical trading day inside the FREE window.
    #[arg(long, default_value = "2024-06-14")]
    date: String,
    /// An option expiration listed on `--date`.
    #[arg(long, default_value = "2024-06-21")]
    expiration: String,
    /// Messages read per request before the stream is dropped.
    #[arg(long, default_value_t = 5, value_parser = clap::value_parser!(u32).range(1..=20))]
    max_messages: u32,
    /// Only these RPCs, comma-separated (for example after a stopped run).
    #[arg(long, value_delimiter = ',')]
    only: Vec<String>,
    /// `free`: the FREE map (one request per family). `options-pro`: every
    /// option RPC, once, on the PRO account (approved 2026-11-04).
    #[arg(long, default_value = "free", value_parser = ["free", "options-pro"])]
    set: String,
    /// A date among the 7 most recent calendar days, for the flat files
    /// (`options-pro` only).
    #[arg(long)]
    flat_file_date: Option<String>,
}

#[derive(Args)]
struct SessionArgs {
    #[command(flatten)]
    common: Common,
    /// Historical trading day inside the FREE stock window.
    #[arg(long, default_value = "2024-06-14")]
    date: String,
}

#[derive(Args)]
struct LimitsArgs {
    #[command(flatten)]
    common: Common,
    /// Run only one probe group (for example after a stopped run).
    #[arg(long, value_parser = ["batch-sizes"])]
    only: Option<String>,
}

#[derive(Args)]
struct Common {
    #[arg(long, required = true)]
    confirm_live: bool,
    /// Read THETADATA_API_KEY from this process environment.
    #[arg(long, required = true)]
    api_key_env: bool,
    #[arg(long)]
    run_id: String,
    /// Seconds between network requests.
    #[arg(long, default_value_t = 3.5)]
    min_interval: f64,
}

#[derive(Args)]
struct Breakdown {
    #[command(flatten)]
    common: Common,
    #[arg(long, default_value = "W1,W2")]
    workloads: String,
    #[arg(long, default_value_t = 3)]
    rounds: u32,
    #[arg(long, default_value_t = 5)]
    cpu_repeats: u32,
}

/// A status that ends the run (politeness/limit policy).
#[derive(Debug)]
struct Stop(String);

#[derive(Serialize, Default)]
struct Report {
    run_id: String,
    command: String,
    host: Host,
    results: Vec<serde_json::Value>,
    stopped: Option<String>,
}

#[derive(Serialize, Default)]
struct Host {
    os: &'static str,
    arch: &'static str,
    client_version: &'static str,
}

#[derive(Serialize, Default, Clone)]
struct Drain {
    status: String,
    headers_ns: u128,
    first_message_ns: u128,
    total_ns: u128,
    messages: usize,
    bytes: usize,
    max_message_bytes: usize,
}

fn status_name(code: tonic::Code) -> String {
    format!("{code:?}")
}

/// Classify a gRPC status: NOT_FOUND is recorded, anything else stops the run.
fn classify(code: tonic::Code) -> Result<String, Stop> {
    match code {
        tonic::Code::NotFound => Ok("NotFound".into()),
        other => Err(Stop(status_name(other))),
    }
}

struct Pacer {
    interval: Duration,
    last: Option<Instant>,
}
impl Pacer {
    async fn wait(&mut self) {
        if let Some(last) = self.last {
            let next = last + self.interval;
            if let Some(delay) = next.checked_duration_since(Instant::now()) {
                tokio::time::sleep(delay).await;
            }
        }
        self.last = Some(Instant::now());
    }
}

fn date(value: &str) -> NaiveDate {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").expect("workload dates are ISO")
}

/// The raw request each workload sends, mirroring the Python wrapper's fields.
enum Wire {
    Stock(api::StockHistoryEodRequestQuery),
    Option(api::OptionHistoryEodRequestQuery),
}
fn wire_request(method: &str, request: &serde_json::Value) -> Wire {
    let field = |name: &str| request[name].as_str().unwrap_or_default().to_owned();
    match method {
        "stock_history_eod" => Wire::Stock(api::StockHistoryEodRequestQuery {
            symbol: field("symbol"),
            start_date: field("start_date"),
            end_date: field("end_date"),
        }),
        "option_history_eod" => Wire::Option(api::OptionHistoryEodRequestQuery {
            contract_spec: Some(wire::ContractSpec {
                symbol: field("symbol"),
                expiration: field("expiration"),
                strike: Some(request["strike"].as_str().unwrap_or("*").to_owned()),
                right: Some(request["right"].as_str().unwrap_or("both").to_owned()),
            }),
            start_date: field("start_date"),
            end_date: field("end_date"),
            ..Default::default()
        }),
        other => panic!("no request builder for {other}"),
    }
}

async fn drain(
    client: &ThetaClient,
    request: &Wire,
) -> Result<(Drain, Vec<wire::ResponseData>), Stop> {
    let mut raw = client.raw_client().max_decoding_message_size(LIMIT);
    let start = Instant::now();
    let mut out = Drain::default();
    let response = match request {
        Wire::Stock(query) => {
            let mut request = tonic::Request::new(api::StockHistoryEodRequest {
                query_info: Some(client.query_info()),
                params: Some(query.clone()),
            });
            request.set_timeout(Duration::from_secs(120));
            raw.get_stock_history_eod(request).await
        }
        Wire::Option(query) => {
            let mut request = tonic::Request::new(api::OptionHistoryEodRequest {
                query_info: Some(client.query_info()),
                params: Some(query.clone()),
            });
            request.set_timeout(Duration::from_secs(120));
            raw.get_option_history_eod(request).await
        }
    };
    out.headers_ns = start.elapsed().as_nanos();
    let mut stream = match response {
        Ok(response) => response.into_inner(),
        Err(status) => {
            out.status = classify(status.code())?;
            out.total_ns = start.elapsed().as_nanos();
            return Ok((out, Vec::new()));
        }
    };
    let mut responses = Vec::new();
    loop {
        match stream.message().await {
            Ok(Some(message)) => {
                if responses.is_empty() {
                    out.first_message_ns = start.elapsed().as_nanos();
                }
                let size = message.encoded_len();
                out.bytes += size;
                out.max_message_bytes = out.max_message_bytes.max(size);
                responses.push(message);
            }
            Ok(None) => break,
            Err(status) => {
                out.status = classify(status.code())?;
                out.total_ns = start.elapsed().as_nanos();
                return Ok((out, responses));
            }
        }
    }
    out.total_ns = start.elapsed().as_nanos();
    out.messages = responses.len();
    out.status = "OK".into();
    Ok((out, responses))
}

/// End-to-end through the public client: (path, status, rows, ns).
async fn e2e(
    client: &ThetaClient,
    method: &str,
    request: &serde_json::Value,
    wire: &Wire,
) -> Result<Vec<serde_json::Value>, Stop> {
    let mut out = Vec::new();
    let eod = |error: EodError| match error {
        EodError::NoData => Ok("NotFound".to_owned()),
        EodError::Remote(code) => classify(code),
        other => Err(Stop(format!("{other:?}"))),
    };
    if method == "stock_history_eod" {
        let typed = StockEodRequest::new(
            request["symbol"].as_str().unwrap(),
            date(request["start_date"].as_str().unwrap()),
            date(request["end_date"].as_str().unwrap()),
        )
        .map_err(|_| Stop("invalid workload request".into()))?;
        for path in ["numeric", "table"] {
            let start = Instant::now();
            let mut rows = 0;
            let result: Result<(), EodError> = async {
                if path == "numeric" {
                    let mut stream = client.stock_eod_batches(typed.clone()).await?;
                    while let Some(batch) = stream.next_batch().await? {
                        rows += batch.row_count();
                    }
                } else {
                    let mut stream = client.stock_eod(typed.clone()).await?;
                    while let Some(table) = stream.next_batch().await? {
                        rows += table.rows.len();
                    }
                }
                Ok(())
            }
            .await;
            let status = match result {
                Ok(()) => "OK".to_owned(),
                Err(error) => eod(error)?,
            };
            out.push(
                serde_json::json!({"path": path, "status": status, "rows": rows,
                "ns": start.elapsed().as_nanos() as u64}),
            );
        }
    }
    // The generated raw helper (legacy Table decoder): the only option EOD path today.
    let start = Instant::now();
    let mut rows = 0;
    let result: Result<(), Error> = async {
        let mut stream = match wire {
            Wire::Stock(query) => client.stock_history_eod(query.clone()).await?,
            Wire::Option(query) => client.option_history_eod(query.clone()).await?,
        };
        while let Some(table) = stream.next_batch().await? {
            rows += table.rows.len();
        }
        Ok(())
    }
    .await;
    let status = match result {
        Ok(()) => "OK".to_owned(),
        Err(Error::NoData) => "NotFound".to_owned(),
        Err(Error::Rpc(status)) => classify(status.code())?,
        Err(_) => return Err(Stop("raw helper error".into())),
    };
    out.push(
        serde_json::json!({"path": "raw-helper", "status": status, "rows": rows,
        "ns": start.elapsed().as_nanos() as u64}),
    );
    Ok(out)
}

/// CPU-only decode of the drained responses: (decoder, median ns, rows).
fn cpu(responses: &[wire::ResponseData], repeats: u32) -> Vec<serde_json::Value> {
    let limits = bounded::DecodeLimits::default();
    let frames: Vec<Vec<u8>> = responses.iter().map(Message::encode_to_vec).collect();
    let mut out = Vec::new();
    for decoder in ["legacy-table", "bounded-numeric", "bounded-table"] {
        let mut samples = Vec::new();
        let mut rows = 0;
        for _ in 0..repeats {
            // Inputs are cloned before timing, as the transport would hand them over.
            let inputs: Vec<(Vec<u8>, wire::ResponseData)> = frames
                .iter()
                .cloned()
                .zip(responses.iter().cloned())
                .collect();
            let start = Instant::now();
            rows = 0;
            for (frame, response) in inputs {
                rows += match decoder {
                    "legacy-table" => decode::decode(response, LIMIT)
                        .map(|t| t.rows.len())
                        .unwrap_or(0),
                    other => envelope::parse(frame, &limits)
                        .and_then(|parsed| {
                            bounded::decode(parsed, &limits, None, other == "bounded-table")
                        })
                        .map(|batch| batch.row_count())
                        .unwrap_or(0),
                };
            }
            samples.push(start.elapsed().as_nanos() as u64);
        }
        samples.sort_unstable();
        out.push(
            serde_json::json!({"decoder": decoder, "median_ns": samples[samples.len() / 2],
            "samples_ns": samples, "rows": rows}),
        );
    }
    out
}

async fn breakdown(
    options: &Breakdown,
    client: &ThetaClient,
    report: &mut Report,
) -> Result<(), Stop> {
    let workloads: serde_json::Value =
        serde_json::from_str(WORKLOADS).expect("valid workloads.json");
    let mut pacer = Pacer {
        interval: Duration::from_secs_f64(options.common.min_interval),
        last: None,
    };
    for round in 0..options.rounds {
        for workload in options
            .workloads
            .split(',')
            .map(str::trim)
            .filter(|w| !w.is_empty())
        {
            let spec = &workloads[workload];
            let method = spec["method"]
                .as_str()
                .ok_or(Stop(format!("unknown workload {workload}")))?;
            for (index, request) in spec["requests"].as_array().unwrap().iter().enumerate() {
                let wire = wire_request(method, request);
                let mut entry =
                    serde_json::json!({"round": round, "workload": workload, "request": index});
                // Alternate e2e and drain order between rounds (AB/BA).
                let mut responses = Vec::new();
                for mode in if round % 2 == 0 {
                    ["e2e", "drain"]
                } else {
                    ["drain", "e2e"]
                } {
                    pacer.wait().await;
                    if mode == "e2e" {
                        entry["e2e"] =
                            serde_json::to_value(e2e(client, method, request, &wire).await?)
                                .unwrap();
                    } else {
                        let (summary, drained) = drain(client, &wire).await?;
                        entry["drain"] = serde_json::to_value(summary).unwrap();
                        responses = drained;
                    }
                }
                if !responses.is_empty() {
                    entry["cpu"] =
                        serde_json::to_value(cpu(&responses, options.cpu_repeats)).unwrap();
                }
                println!(
                    "{workload}[{index}] round {round}: drain={} e2e={}",
                    entry["drain"]["status"], entry["e2e"]
                );
                report.results.push(entry);
            }
        }
    }
    Ok(())
}

async fn limits(
    options: &LimitsArgs,
    client: &ThetaClient,
    report: &mut Report,
) -> Result<(), Stop> {
    if options.only.as_deref() != Some("batch-sizes") {
        concurrency_and_rate(client, report).await?;
        tokio::time::sleep(Duration::from_secs(60)).await; // let any per-minute window reset
    }
    batch_sizes(&options.common, client, report).await
}

async fn concurrency_and_rate(client: &ThetaClient, report: &mut Report) -> Result<(), Stop> {
    let tiny = Wire::Stock(api::StockHistoryEodRequestQuery {
        symbol: "AAPL".into(),
        start_date: "2024-01-02".into(),
        end_date: "2024-01-02".into(),
    });
    // 1. Concurrency: 2 then 3 simultaneous tiny requests, three trials each,
    //    10 s apart. Overlapping intervals mean parallel service; staggered
    //    headers mean queueing; an error status ends the probe.
    for concurrent in [2usize, 3] {
        for trial in 0..3 {
            tokio::time::sleep(Duration::from_secs(10)).await;
            let start = Instant::now();
            let results = futures_join(client, &tiny, concurrent).await;
            let mut timeline = Vec::new();
            for result in results {
                let (summary, _) = result?;
                timeline.push(serde_json::to_value(summary).unwrap());
            }
            println!(
                "concurrency {concurrent} trial {trial}: done in {:?}",
                start.elapsed()
            );
            report.results.push(
                serde_json::json!({"probe": "concurrency", "concurrent": concurrent,
                "trial": trial, "requests": timeline}),
            );
        }
    }
    // 2. Per-minute limit: paced sequential tiny requests, first at one per
    //    2.5 s (24/min) for 26 requests, then one per 1.5 s (40/min) for up to
    //    35 more. Stops at the first error and records when it happened.
    let probe_start = Instant::now();
    let mut sent = 0;
    for (interval, count) in [(2.5, 26), (1.5, 35)] {
        let mut pacer = Pacer {
            interval: Duration::from_secs_f64(interval),
            last: None,
        };
        for _ in 0..count {
            pacer.wait().await;
            sent += 1;
            match drain(client, &tiny).await {
                Ok((summary, _)) => report.results.push(serde_json::json!({"probe": "rate",
                    "index": sent, "interval_s": interval, "at_ms": probe_start.elapsed().as_millis() as u64,
                    "status": summary.status, "total_ns": summary.total_ns as u64})),
                Err(stop) => {
                    report.results.push(serde_json::json!({"probe": "rate", "index": sent,
                        "interval_s": interval, "at_ms": probe_start.elapsed().as_millis() as u64,
                        "status": stop.0}));
                    println!("rate probe stopped at request {sent} after {:?}: {}", probe_start.elapsed(), stop.0);
                    return Err(stop);
                }
            }
        }
        println!("rate probe: {sent} requests without error at {interval} s spacing");
    }
    Ok(())
}

fn stock(start: &str, end: &str) -> Wire {
    Wire::Stock(api::StockHistoryEodRequestQuery {
        symbol: "AAPL".into(),
        start_date: start.into(),
        end_date: end.into(),
    })
}

/// Probe group 3: date-span checks, then batch sizes.
///
/// A full-FREE-window EOD request (2023-06-01..2026-10-30) returned
/// INVALID_ARGUMENT. These four requests find out whether the span or the
/// start date was the cause; their INVALID_ARGUMENT and PERMISSION_DENIED
/// statuses are recorded, not fatal.
async fn batch_sizes(
    options: &Common,
    client: &ThetaClient,
    report: &mut Report,
) -> Result<(), Stop> {
    let mut pacer = Pacer {
        interval: Duration::from_secs_f64(options.min_interval.max(3.5)),
        last: None,
    };
    for (label, start, end) in [
        ("span-365d", "2025-10-31", "2026-10-30"),
        ("span-366d", "2025-10-30", "2026-10-30"),
        ("span-731d", "2024-10-30", "2026-10-30"),
        ("span-1y-from-free-start", "2023-06-01", "2024-05-31"),
    ] {
        pacer.wait().await;
        let status = match drain(client, &stock(start, end)).await {
            Ok((summary, _)) => summary.status,
            Err(Stop(name)) if name == "InvalidArgument" || name == "PermissionDenied" => name,
            Err(stop) => return Err(stop),
        };
        println!("span check {label} ({start}..{end}): {status}");
        report
            .results
            .push(serde_json::json!({"probe": "span", "request": label,
            "start": start, "end": end, "status": status}));
    }
    for (label, request) in [
        (
            "stock-eod-AAPL-2024-01-02..2024-12-31",
            stock("2024-01-02", "2024-12-31"),
        ),
        (
            "option-eod-SPY-2024-06-14-all",
            wire_request(
                "option_history_eod",
                &serde_json::json!({
            "symbol": "SPY", "expiration": "*", "start_date": "2024-06-14", "end_date": "2024-06-14",
            "strike": "*", "right": "both"}),
            ),
        ),
    ] {
        pacer.wait().await;
        let (summary, responses) = drain(client, &request).await?;
        let rows: Vec<usize> = responses
            .iter()
            .map(|r| {
                decode::decode(r.clone(), LIMIT)
                    .map(|t| t.rows.len())
                    .unwrap_or(0)
            })
            .collect();
        report
            .results
            .push(serde_json::json!({"probe": "batch-sizes", "request": label,
            "drain": summary, "rows_per_message": rows}));
        println!("batch sizes {label}: {} messages", responses.len());
    }
    Ok(())
}

async fn futures_join(
    client: &ThetaClient,
    request: &Wire,
    n: usize,
) -> Vec<Result<(Drain, Vec<wire::ResponseData>), Stop>> {
    let calls: Vec<_> = (0..n).map(|_| drain(client, request)).collect();
    let mut out = Vec::new();
    // join_all without a dependency: poll the futures concurrently on this task.
    let mut pinned: Vec<_> = calls.into_iter().map(Box::pin).collect();
    let mut done: Vec<Option<_>> = (0..n).map(|_| None).collect();
    std::future::poll_fn(|cx| {
        let mut pending = false;
        for (slot, future) in done.iter_mut().zip(pinned.iter_mut()) {
            if slot.is_none() {
                match future.as_mut().poll(cx) {
                    std::task::Poll::Ready(value) => *slot = Some(value),
                    std::task::Poll::Pending => pending = true,
                }
            }
        }
        if pending {
            std::task::Poll::Pending
        } else {
            std::task::Poll::Ready(())
        }
    })
    .await;
    for slot in done {
        out.push(slot.expect("all futures completed"));
    }
    out
}

/// One entitlement probe: status and transport shape only, never row values.
#[derive(Serialize, Debug)]
struct Probe {
    rpc: &'static str,
    python_name: Option<&'static str>,
    stability: &'static str,
    /// `OK`, a gRPC code name, or `ClientTimeout`.
    status: String,
    /// The server's status text, printable ASCII, at most 160 characters.
    /// Private report only: classified by a person, never committed or sent.
    status_text: Option<String>,
    headers_ns: Option<u128>,
    first_message_ns: Option<u128>,
    total_ns: u128,
    messages: u32,
    bytes: usize,
    /// The stream ended within `max_messages`.
    complete: bool,
}

/// Service-wide signals: anything else is a per-endpoint answer to record.
fn ends_entitlement_run(status: &str) -> bool {
    matches!(status, "ResourceExhausted" | "Unavailable")
}

fn status_text(status: &tonic::Status) -> Option<String> {
    let text: String = status
        .message()
        .chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .take(160)
        .collect();
    (!text.is_empty()).then_some(text)
}

async fn probe<E: Endpoint>(client: &ThetaClient, request: &E, max_messages: u32) -> Probe {
    let mut out = Probe {
        rpc: E::RPC,
        python_name: E::PYTHON_NAME,
        stability: match E::STABILITY {
            typed::Stability::VendorWrapper => "vendor_wrapper",
            typed::Stability::Unstable => "unstable",
        },
        status: String::new(),
        status_text: None,
        headers_ns: None,
        first_message_ns: None,
        total_ns: 0,
        messages: 0,
        bytes: 0,
        complete: false,
    };
    let start = Instant::now();
    let body = async {
        let mut stream = client.stream_raw(request).await?;
        out.headers_ns = Some(start.elapsed().as_nanos());
        while out.messages < max_messages {
            match stream.message().await? {
                Some(message) => {
                    if out.messages == 0 {
                        out.first_message_ns = Some(start.elapsed().as_nanos());
                    }
                    out.messages += 1;
                    out.bytes += message.encoded_len();
                }
                None => {
                    out.complete = true;
                    break;
                }
            }
        }
        Ok::<(), tonic::Status>(())
        // Dropping the stream here cancels the rest of the response.
    };
    match tokio::time::timeout(Duration::from_secs(30), body).await {
        Ok(Ok(())) => out.status = "OK".into(),
        Ok(Err(status)) => {
            out.status = status_name(status.code());
            out.status_text = status_text(&status);
        }
        Err(_) => out.status = "ClientTimeout".into(),
    }
    out.total_ns = start.elapsed().as_nanos();
    out
}

struct EntitlementRun<'a> {
    client: &'a ThetaClient,
    report: &'a mut Report,
    pacer: Pacer,
    only: &'a [String],
    max_messages: u32,
    /// Flat-file probes: a first message over the probe's limit is recorded
    /// as `OverProbeLimit` (the request was accepted; nothing bulk was read).
    flat_files: bool,
}
impl EntitlementRun<'_> {
    async fn run<E: Endpoint>(&mut self, request: E) -> Result<(), Stop> {
        if !self.only.is_empty() && !self.only.iter().any(|rpc| rpc == E::RPC) {
            return Ok(());
        }
        self.pacer.wait().await;
        let mut result = probe(self.client, &request, self.max_messages).await;
        if self.flat_files && result.status == "OutOfRange" && result.messages == 0 {
            result.status = "OverProbeLimit".into();
            result.status_text = None;
        }
        eprintln!(
            "{:<45} {:<18} messages {}",
            result.rpc, result.status, result.messages
        );
        let stop = ends_entitlement_run(&result.status).then(|| Stop(result.status.clone()));
        self.report
            .results
            .push(serde_json::to_value(&result).expect("probe serializes"));
        stop.map_or(Ok(()), Err)
    }
}

/// The FREE entitlement map (P1 PR 2): about 35 single requests, one per
/// endpoint family, each with one symbol and one day, wildcards bounded by
/// `max_messages`. Index and future symbols are the vendor's examples.
async fn entitlements(
    options: &EntitlementsArgs,
    client: &ThetaClient,
    flat_files: Option<&ThetaClient>,
    report: &mut Report,
) -> Result<(), Stop> {
    let day = date(&options.date);
    let exp = date(&options.expiration);
    if options.set == "options-pro" {
        let flat = date(
            options
                .flat_file_date
                .as_deref()
                .ok_or_else(|| Stop("MissingFlatFileDate".into()))?,
        );
        let flat_files = flat_files.ok_or_else(|| Stop("MissingFlatFileClient".into()))?;
        return option_entitlements(options, client, flat_files, report, day, exp, flat).await;
    }
    let mut r = EntitlementRun {
        client,
        report,
        pacer: Pacer {
            interval: Duration::from_secs_f64(options.common.min_interval),
            last: None,
        },
        only: &options.only,
        max_messages: options.max_messages,
        flat_files: false,
    };
    // Controls first: both returned rows on FREE in P0.
    r.run(typed::StockHistoryEod::new("AAPL", day, day)).await?;
    r.run(typed::OptionHistoryEod::new(day, day, "SPY", exp))
        .await?;
    // Stock.
    r.run(typed::StockListSymbols::new()).await?;
    r.run(typed::StockListDates::new("trade", "AAPL")).await?;
    r.run(typed::StockSnapshotOhlc::new("AAPL")).await?;
    r.run(typed::StockSnapshotTrade::new("AAPL")).await?;
    r.run(typed::StockSnapshotQuote::new("AAPL")).await?;
    r.run(
        typed::StockHistoryOhlc::new("AAPL")
            .date(day)
            .interval("1m"),
    )
    .await?;
    r.run(typed::StockHistoryTrade::new("AAPL").date(day))
        .await?;
    r.run(
        typed::StockHistoryQuote::new("AAPL")
            .date(day)
            .interval("1m"),
    )
    .await?;
    r.run(typed::StockAtTimeQuote::new("AAPL", day, day, "10:00:00"))
        .await?;
    r.run(typed::StockFlatFileEod::new(day)).await?;
    // Option.
    r.run(typed::OptionListSymbols::new()).await?;
    r.run(typed::OptionListExpirations::new("SPY")).await?;
    r.run(typed::OptionListStrikes::new("SPY", exp)).await?;
    r.run(typed::OptionListContracts::new("trade", day).symbol("SPY"))
        .await?;
    r.run(typed::OptionSnapshotQuote::new("SPY", "*").right("call"))
        .await?;
    r.run(typed::OptionSnapshotOpenInterest::new("SPY", "*").right("call"))
        .await?;
    r.run(typed::OptionSnapshotGreeksFirstOrder::new("SPY", "*").right("call"))
        .await?;
    r.run(
        typed::OptionHistoryOhlc::new("SPY", exp)
            .date(day)
            .interval("1m")
            .right("call"),
    )
    .await?;
    r.run(
        typed::OptionHistoryTrade::new("SPY", exp)
            .date(day)
            .right("call"),
    )
    .await?;
    r.run(
        typed::OptionHistoryQuote::new("SPY", exp)
            .date(day)
            .interval("1m")
            .right("call"),
    )
    .await?;
    r.run(
        typed::OptionHistoryOpenInterest::new("SPY", exp)
            .date(day)
            .right("call"),
    )
    .await?;
    r.run(typed::OptionHistoryGreeksEod::new("SPY", exp, day, day).right("call"))
        .await?;
    r.run(
        typed::OptionHistoryGreeksFirstOrder::new("SPY", exp)
            .date(day)
            .interval("1m")
            .right("call"),
    )
    .await?;
    r.run(
        typed::OptionHistoryTradeGreeksFirstOrder::new("SPY", exp)
            .date(day)
            .right("call"),
    )
    .await?;
    r.run(
        typed::OptionHistoryGreeksImpliedVolatility::new("SPY", exp)
            .date(day)
            .interval("1m")
            .right("call"),
    )
    .await?;
    r.run(typed::OptionAtTimeQuote::new("SPY", day, day, "10:00:00", exp).right("call"))
        .await?;
    // Index.
    r.run(typed::IndexListSymbols::new()).await?;
    r.run(typed::IndexSnapshotPrice::new("SPX")).await?;
    r.run(typed::IndexHistoryEod::new("SPX", day, day)).await?;
    r.run(
        typed::IndexHistoryPrice::new("SPX")
            .date(day)
            .interval("1m"),
    )
    .await?;
    // Calendar and rates.
    r.run(typed::CalendarOnDate::new(day)).await?;
    r.run(typed::InterestRateHistoryEod::new("SOFR", day, day))
        .await?;
    // Unstable: in the descriptor, with no vendor wrapper.
    r.run(typed::FutureListSymbols::default()).await?;
    r.run(typed::CorporateActionSplit {
        symbol: "AAPL".into(),
        start_date: "2024-01-01".into(),
        end_date: "2024-12-31".into(),
    })
    .await?;
    Ok(())
}

/// The PRO option map (approved by the user on 2026-11-04 as one run): every
/// option RPC once, in the order of the coverage table, each as narrow as the
/// request allows (SPY calls, one expiration, one day; tick requests over one
/// minute, interval requests over five) and read for at most `max_messages`.
/// Flat files come last, through `flat_files`: the same session with a
/// [`FLAT_FILE_PROBE_LIMIT`] message limit, reading one message at most, so a
/// probe never pulls bulk data. Dropping the stream cancels the rest.
#[allow(clippy::too_many_arguments)]
async fn option_entitlements(
    options: &EntitlementsArgs,
    client: &ThetaClient,
    flat_files: &ThetaClient,
    report: &mut Report,
    day: NaiveDate,
    exp: NaiveDate,
    flat: NaiveDate,
) -> Result<(), Stop> {
    let mut r = EntitlementRun {
        client,
        report,
        pacer: Pacer {
            interval: Duration::from_secs_f64(options.common.min_interval),
            last: None,
        },
        only: &options.only,
        max_messages: options.max_messages,
        flat_files: false,
    };
    macro_rules! snapshot {
        ($($t:ident),+) => {$(
            r.run(typed::$t::new("SPY", exp).right("call")).await?;
        )+};
    }
    macro_rules! interval {
        ($($t:ident),+) => {$(
            r.run(
                typed::$t::new("SPY", exp)
                    .date(day)
                    .interval("1m")
                    .right("call")
                    .start_time("10:00:00")
                    .end_time("10:05:00"),
            )
            .await?;
        )+};
    }
    macro_rules! tick {
        ($($t:ident),+) => {$(
            r.run(
                typed::$t::new("SPY", exp)
                    .date(day)
                    .right("call")
                    .start_time("10:00:00")
                    .end_time("10:01:00"),
            )
            .await?;
        )+};
    }
    macro_rules! eod_greeks {
        ($($t:ident),+) => {$(
            r.run(typed::$t::new("SPY", exp, day, day).right("call")).await?;
        )+};
    }
    // Lists.
    r.run(typed::OptionListSymbols::new()).await?;
    r.run(typed::OptionListDates::new("trade", "SPY", exp).right("call"))
        .await?;
    r.run(typed::OptionListExpirations::new("SPY")).await?;
    r.run(typed::OptionListStrikes::new("SPY", exp)).await?;
    r.run(typed::OptionListContracts::new("trade", day).symbol("SPY"))
        .await?;
    // Snapshots.
    snapshot!(OptionSnapshotOhlc);
    r.run(typed::OptionSnapshotTrade::new("SPY", exp).right("call"))
        .await?;
    snapshot!(
        OptionSnapshotQuote,
        OptionSnapshotOpenInterest,
        OptionSnapshotMarketValue,
        OptionSnapshotGreeksImpliedVolatility,
        OptionSnapshotGreeksAll,
        OptionSnapshotBinomialGreeksAll,
        OptionSnapshotGreeksFirstOrder,
        OptionSnapshotBinomialGreeksFirstOrder,
        OptionSnapshotGreeksSecondOrder,
        OptionSnapshotBinomialGreeksSecondOrder,
        OptionSnapshotGreeksThirdOrder,
        OptionSnapshotBinomialGreeksThirdOrder
    );
    // History.
    r.run(typed::OptionHistoryEod::new(day, day, "SPY", exp).right("call"))
        .await?;
    interval!(OptionHistoryOhlc);
    tick!(OptionHistoryTrade);
    interval!(OptionHistoryQuote);
    tick!(OptionHistoryTradeQuote);
    r.run(
        typed::OptionHistoryOpenInterest::new("SPY", exp)
            .date(day)
            .right("call"),
    )
    .await?;
    eod_greeks!(OptionHistoryGreeksEod, OptionHistoryBinomialGreeksEod);
    interval!(OptionHistoryGreeksAll, OptionHistoryBinomialGreeksAll);
    tick!(
        OptionHistoryTradeGreeksAll,
        OptionHistoryBinomialTradeGreeksAll
    );
    interval!(
        OptionHistoryGreeksFirstOrder,
        OptionHistoryBinomialGreeksFirstOrder
    );
    tick!(
        OptionHistoryTradeGreeksFirstOrder,
        OptionHistoryBinomialTradeGreeksFirstOrder
    );
    interval!(
        OptionHistoryGreeksSecondOrder,
        OptionHistoryBinomialGreeksSecondOrder
    );
    tick!(
        OptionHistoryTradeGreeksSecondOrder,
        OptionHistoryBinomialTradeGreeksSecondOrder
    );
    interval!(
        OptionHistoryGreeksThirdOrder,
        OptionHistoryBinomialGreeksThirdOrder
    );
    tick!(
        OptionHistoryTradeGreeksThirdOrder,
        OptionHistoryBinomialTradeGreeksThirdOrder
    );
    interval!(OptionHistoryGreeksImpliedVolatility);
    tick!(OptionHistoryTradeGreeksImpliedVolatility);
    // At time.
    r.run(typed::OptionAtTimeTrade::new("SPY", day, day, "10:00:00", exp).right("call"))
        .await?;
    r.run(typed::OptionAtTimeQuote::new("SPY", day, day, "10:00:00", exp).right("call"))
        .await?;
    // Flat files: the smallest first; the option quote file (about 38 GB) last.
    r.client = flat_files;
    r.max_messages = 1;
    r.flat_files = true;
    r.run(typed::OptionFlatFileEod::new(flat)).await?;
    r.run(typed::OptionFlatFileOpenInterest::new(flat)).await?;
    r.run(typed::OptionFlatFileTradeQuote::new(flat)).await?;
    r.run(typed::OptionFlatFileQuote::new(flat)).await?;
    Ok(())
}

/// The session probe (approved 2026-11-04): the runner's session A is
/// already logged in; log in again as B, then send one small FREE-entitled
/// request on A and one on B, A first. Records each status and gRPC code, to
/// learn whether a new login ends the earlier session and with which code.
async fn session_probe(
    options: &SessionArgs,
    client_a: &ThetaClient,
    report: &mut Report,
    identity: &mut Vec<String>,
) -> Result<(), Stop> {
    let day = date(&options.date);
    let session_b = login()
        .await
        .map_err(|_| Stop("SecondLoginFailed".into()))?;
    identity.push(session_b.session_id().to_owned());
    let client_b = connect(session_b)
        .await
        .map_err(|_| Stop("SecondConnectionFailed".into()))?;
    let mut pacer = Pacer {
        interval: Duration::from_secs_f64(options.common.min_interval),
        last: None,
    };
    for (name, client) in [
        ("A (first login)", client_a),
        ("B (second login)", &client_b),
    ] {
        pacer.wait().await;
        let result = probe(client, &typed::StockHistoryEod::new("AAPL", day, day), 5).await;
        eprintln!("session {name:<17} {}", result.status);
        let mut value = serde_json::to_value(&result).expect("probe serializes");
        value["session"] = name.into();
        report.results.push(value);
    }
    Ok(())
}

/// A comparable value: prices as Decimal128(38, 10) unscaled, timestamps as
/// the legacy RFC 3339 text. Only equality is reported, never a value.
#[derive(Debug, PartialEq)]
enum Exact {
    Null,
    Text(String),
    Integer(i64),
    Price(i128),
    Boolean(bool),
    Timestamp(String),
}

fn legacy_rows(table: &praevis_thetadata_core::Table) -> Vec<Vec<Exact>> {
    use praevis_thetadata_core::Value;
    table
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
        .collect()
}

fn arrow_rows(batches: &[praevis_thetadata_client::arrow_array::RecordBatch]) -> Vec<Vec<Exact>> {
    use praevis_thetadata_client::arrow_array::Array;
    use praevis_thetadata_client::arrow_array::cast::AsArray;
    use praevis_thetadata_client::arrow_array::types::{
        Decimal128Type, Int64Type, TimestampMillisecondType,
    };
    use praevis_thetadata_client::arrow_schema::DataType;
    let mut out = Vec::new();
    for batch in batches {
        for row in 0..batch.num_rows() {
            out.push(
                batch
                    .columns()
                    .iter()
                    .map(|column| {
                        if column.data_type() == &DataType::Null || column.is_null(row) {
                            return Exact::Null;
                        }
                        match column.data_type() {
                            DataType::Utf8 => {
                                Exact::Text(column.as_string::<i32>().value(row).into())
                            }
                            DataType::Int64 => {
                                Exact::Integer(column.as_primitive::<Int64Type>().value(row))
                            }
                            DataType::Boolean => Exact::Boolean(column.as_boolean().value(row)),
                            DataType::Decimal128(_, _) => {
                                Exact::Price(column.as_primitive::<Decimal128Type>().value(row))
                            }
                            DataType::Timestamp(_, tz) => {
                                let ms =
                                    column.as_primitive::<TimestampMillisecondType>().value(row);
                                let zone = i32::from(tz.as_deref() == Some("UTC"));
                                Exact::Timestamp(
                                    praevis_thetadata_core::timestamp(ms as u64, zone)
                                        .unwrap_or_default(),
                                )
                            }
                            other => Exact::Text(format!("unexpected type {other:?}")),
                        }
                    })
                    .collect(),
            );
        }
    }
    out
}

/// One endpoint: query() (collected), then the raw stream through the legacy
/// decoder, compared exactly. Statuses, counts, column names and Arrow types
/// only; never a value.
async fn check<E>(client: &ThetaClient, request: E, pacer: &mut Pacer) -> serde_json::Value
where
    E: Endpoint + Clone + Send + Sync + 'static,
{
    pacer.wait().await;
    let started = Instant::now();
    let mut first_batch_ns = None;
    let collected = async {
        let mut stream = client.query(&request).await?;
        let mut batches = Vec::new();
        while let Some(batch) = stream.next_batch().await? {
            first_batch_ns.get_or_insert(started.elapsed().as_nanos());
            batches.push(batch);
        }
        Ok::<_, praevis_thetadata_client::QueryError>(praevis_thetadata_client::unify(batches))
    }
    .await;
    let total_ns = started.elapsed().as_nanos();
    pacer.wait().await;
    let legacy = async {
        let mut stream = client.stream_raw(&request).await?;
        let mut tables = Vec::new();
        while let Some(message) = stream.message().await? {
            tables.push(
                decode::decode(message, LIMIT)
                    .map_err(|_| tonic::Status::data_loss("legacy decode"))?,
            );
        }
        Ok::<_, tonic::Status>(tables)
    }
    .await;
    let mut out = serde_json::json!({
        "rpc": E::RPC,
        "python_name": E::PYTHON_NAME,
        "first_batch_ns": first_batch_ns,
        "total_ns": total_ns,
    });
    match &collected {
        Ok(batches) => {
            let schema = batches.first().map(|b| b.schema());
            out["query_status"] = "OK".into();
            out["rows"] = batches.iter().map(|b| b.num_rows()).sum::<usize>().into();
            out["columns"] = schema
                .as_ref()
                .map(|s| {
                    s.fields()
                        .iter()
                        .map(|f| serde_json::json!([f.name(), format!("{}", f.data_type())]))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
                .into();
        }
        Err(error) => out["query_status"] = format!("{error:?}").into(),
    }
    match (&collected, &legacy) {
        (Ok(batches), Ok(tables)) => {
            let ours = arrow_rows(batches);
            let theirs: Vec<Vec<Exact>> = tables.iter().flat_map(legacy_rows).collect();
            let differing = ours.iter().zip(&theirs).filter(|(a, b)| a != b).count();
            out["legacy_status"] = "OK".into();
            out["legacy_rows"] = theirs.len().into();
            out["values_equal"] = (ours == theirs).into();
            out["differing_rows"] = (differing + ours.len().abs_diff(theirs.len())).into();
        }
        (_, Err(status)) => out["legacy_status"] = status_name(status.code()).into(),
        (Err(_), Ok(_)) => out["legacy_status"] = "OK".into(),
    }
    let label = E::PYTHON_NAME.unwrap_or(E::RPC);
    eprintln!(
        "{label:<32} query {:<12} legacy {:<10} rows {:<6} equal {}",
        out["query_status"].as_str().unwrap_or("?"),
        out["legacy_status"].as_str().unwrap_or("?"),
        out["rows"],
        out["values_equal"]
    );
    out
}

/// The 10 endpoints the FREE entitlement map found entitled (p1-entitlements-001).
async fn checks(
    options: &ChecksArgs,
    client: &ThetaClient,
    report: &mut Report,
) -> Result<(), Stop> {
    let day = date(&options.date);
    let exp = date(&options.expiration);
    let mut pacer = Pacer {
        interval: Duration::from_secs_f64(options.common.min_interval),
        last: None,
    };
    let p = &mut pacer;
    // A limit or service-wide signal ends the run at once, like the other probes.
    let stop = |result: &serde_json::Value| -> Result<(), Stop> {
        for key in ["query_status", "legacy_status"] {
            if let Some(status) = result[key].as_str()
                && (status.contains("ResourceExhausted") || status.contains("Unavailable"))
            {
                return Err(Stop(status.to_owned()));
            }
        }
        Ok(())
    };
    macro_rules! run {
        ($request:expr) => {{
            let result = check(client, $request, p).await;
            let outcome = stop(&result);
            report.results.push(result);
            outcome?;
        }};
    }
    run!(typed::StockHistoryEod::new("AAPL", day, day));
    run!(typed::OptionHistoryEod::new(day, day, "SPY", exp));
    run!(typed::StockListSymbols::new());
    run!(typed::StockListDates::new("trade", "AAPL"));
    run!(typed::OptionListSymbols::new());
    run!(typed::OptionListExpirations::new("SPY"));
    run!(typed::OptionListStrikes::new("SPY", exp));
    run!(typed::IndexListSymbols::new());
    run!(typed::IndexHistoryEod::new("SPX", day, day));
    run!(typed::InterestRateHistoryEod::new("SOFR", day, day));
    Ok(())
}

fn output_path(run_id: &str) -> Result<PathBuf, &'static str> {
    if run_id.is_empty()
        || !run_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("run id must be ASCII letters, digits, '-' or '_'");
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../artifacts/p0")
        .join(format!("{run_id}.json"));
    if path.exists() {
        return Err("run id already used");
    }
    Ok(path)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let (common, name) = match &cli.command {
        Command::Breakdown(options) => (&options.common, "breakdown"),
        Command::Limits(options) => (&options.common, "limits"),
        Command::Entitlements(options) => (&options.common, "entitlements"),
        Command::Checks(options) => (&options.common, "checks"),
        Command::Session(options) => (&options.common, "session"),
    };
    if !common.confirm_live || !common.api_key_env || std::env::var_os("CI").is_some() {
        eprintln!("live execution requires --confirm-live and --api-key-env, outside CI");
        return std::process::ExitCode::FAILURE;
    }
    let path = match output_path(&common.run_id) {
        Ok(path) => path,
        Err(message) => {
            eprintln!("{message}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let session = match login().await {
        Ok(session) => session,
        Err(message) => {
            eprintln!("{message}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let mut report = Report {
        run_id: common.run_id.clone(),
        command: name.into(),
        host: Host {
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            client_version: env!("CARGO_PKG_VERSION"),
        },
        ..Default::default()
    };
    // The report must never contain account identity: checked before writing.
    let mut identity = vec![
        session.session_id().to_owned(),
        session.user().email.clone(),
    ];
    let client = match connect(session).await {
        Ok(client) => client,
        Err(_) => {
            eprintln!("connection failed");
            return std::process::ExitCode::FAILURE;
        }
    };
    let result = match &cli.command {
        Command::Breakdown(options) => breakdown(options, &client, &mut report).await,
        Command::Limits(options) => limits(options, &client, &mut report).await,
        Command::Entitlements(options) => {
            // The same session (no second login), with a small message limit.
            let flat_files = if options.set == "options-pro" {
                match connect_with_limit(client.session(), FLAT_FILE_PROBE_LIMIT).await {
                    Ok(flat_files) => Some(flat_files),
                    Err(_) => {
                        eprintln!("connection failed");
                        return std::process::ExitCode::FAILURE;
                    }
                }
            } else {
                None
            };
            entitlements(options, &client, flat_files.as_ref(), &mut report).await
        }
        Command::Checks(options) => checks(options, &client, &mut report).await,
        Command::Session(options) => {
            session_probe(options, &client, &mut report, &mut identity).await
        }
    };
    if let Err(Stop(reason)) = &result {
        report.stopped = Some(reason.clone());
        eprintln!("Stopped on {reason}: results so far are saved.");
    }
    let text = serde_json::to_vec_pretty(&report).unwrap();
    if contains_identity(&text, &identity) {
        eprintln!("Refusing to write the report: it would contain account identity.");
        return std::process::ExitCode::FAILURE;
    }
    std::fs::create_dir_all(path.parent().unwrap()).expect("artifacts directory");
    std::fs::write(&path, text).expect("write report");
    println!("Wrote {}", path.display());
    if result.is_ok() {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}

fn contains_identity(text: &[u8], identity: &[String]) -> bool {
    identity
        .iter()
        .filter(|value| !value.is_empty())
        .any(|value| {
            text.windows(value.len())
                .any(|window| window == value.as_bytes())
        })
}

async fn connect(session: Session) -> Result<ThetaClient, EodError> {
    connect_with_limit(session, LIMIT).await
}

async fn connect_with_limit(session: Session, limit: usize) -> Result<ThetaClient, EodError> {
    ThetaClient::with_eod_policy(
        ClientConfig {
            request_timeout: Duration::from_secs(120),
            max_batch_bytes: limit,
            ..Default::default()
        },
        session,
        EodPolicy::default(),
    )
    .await
}

async fn login() -> Result<Session, &'static str> {
    let key = std::env::var("THETADATA_API_KEY").map_err(|_| "THETADATA_API_KEY is missing")?;
    if key.trim().is_empty() || key.trim().chars().any(char::is_control) {
        return Err("THETADATA_API_KEY is invalid");
    }
    let auth = AuthClient::new(AuthConfig {
        environment: Environment::Prod,
        ..Default::default()
    })
    .map_err(|_| "authentication configuration")?;
    auth.authenticate(&Credentials::api_key(key.trim()))
        .await
        .map_err(|_| "authentication failed")
}

/// Offline check of the breakdown pipeline against the loopback fixture (stock
/// EOD only): `cargo test --features live-tests --example live_p0`.
#[cfg(test)]
#[allow(dead_code)]
#[path = "../tests/support/mod.rs"]
mod fixture;

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn breakdown_pipeline_runs_against_the_loopback_fixture() {
        let (wire_table, _) = fixture::table(fixture::Shape::Mixed, 50);
        let server = fixture::Fixture::start(fixture::Script {
            messages: vec![
                Ok(fixture::response(&wire_table, true)),
                Ok(fixture::response(&wire_table, false)),
            ],
            ..Default::default()
        });
        let client = ThetaClient::with_eod_policy(
            ClientConfig {
                endpoint: Some(server.endpoint.clone()),
                allow_insecure: true,
                ..Default::default()
            },
            fixture::session().await,
            EodPolicy::default(),
        )
        .await
        .unwrap();
        let request = serde_json::json!({"symbol": "AAPL", "start_date": "2024-01-02", "end_date": "2024-01-05"});
        let wire = wire_request("stock_history_eod", &request);
        let (summary, responses) = drain(&client, &wire).await.unwrap();
        assert_eq!((summary.status.as_str(), summary.messages), ("OK", 2));
        assert!(summary.first_message_ns <= summary.total_ns && summary.bytes > 0);
        let paths = e2e(&client, "stock_history_eod", &request, &wire)
            .await
            .unwrap();
        assert_eq!(paths.len(), 3);
        assert!(
            paths
                .iter()
                .all(|p| p["status"] == "OK" && p["rows"] == 100),
            "{paths:?}"
        );
        let decoded = cpu(&responses, 2);
        assert!(decoded.iter().all(|d| d["rows"] == 100), "{decoded:?}");
        let workloads: serde_json::Value = serde_json::from_str(WORKLOADS).unwrap();
        for id in ["W1", "W2", "W2b"] {
            let method = workloads[id]["method"].as_str().unwrap();
            for request in workloads[id]["requests"].as_array().unwrap() {
                let _ = wire_request(method, request);
            }
        }
    }

    #[tokio::test]
    async fn invalid_argument_surfaces_by_the_name_the_span_checks_match() {
        let server = fixture::Fixture::start(fixture::Script {
            initial_error: Some(tonic::Status::invalid_argument("synthetic")),
            ..Default::default()
        });
        let client = ThetaClient::with_eod_policy(
            ClientConfig {
                endpoint: Some(server.endpoint.clone()),
                allow_insecure: true,
                ..Default::default()
            },
            fixture::session().await,
            EodPolicy::default(),
        )
        .await
        .unwrap();
        match drain(&client, &stock("2023-06-01", "2026-10-30")).await {
            Err(Stop(name)) => assert_eq!(name, "InvalidArgument"),
            Ok(_) => panic!("expected a stop"),
        }
    }

    async fn fixture_client(server: &fixture::Fixture) -> ThetaClient {
        fixture_client_with_limit(server, ClientConfig::default().max_batch_bytes).await
    }

    async fn fixture_client_with_limit(server: &fixture::Fixture, limit: usize) -> ThetaClient {
        ThetaClient::with_eod_policy(
            ClientConfig {
                endpoint: Some(server.endpoint.clone()),
                allow_insecure: true,
                max_batch_bytes: limit,
                ..Default::default()
            },
            fixture::session().await,
            EodPolicy::default(),
        )
        .await
        .unwrap()
    }

    fn entitlement_args(only: &[&str], max_messages: u32) -> EntitlementsArgs {
        EntitlementsArgs {
            common: Common {
                confirm_live: true,
                api_key_env: true,
                run_id: "test".into(),
                min_interval: 0.0,
            },
            date: "2024-06-14".into(),
            expiration: "2024-06-21".into(),
            max_messages,
            only: only.iter().map(|rpc| (*rpc).to_owned()).collect(),
            set: "free".into(),
            flat_file_date: None,
        }
    }

    #[tokio::test]
    async fn the_pro_option_map_sends_every_option_rpc_once() {
        let server = fixture::Fixture::start(fixture::Script {
            initial_error: Some(tonic::Status::permission_denied("synthetic")),
            ..Default::default()
        });
        let client = fixture_client(&server).await;
        let mut args = entitlement_args(&[], 5);
        args.set = "options-pro".into();
        let mut report = Report::default();
        // Without a flat-file date nothing is sent.
        assert!(matches!(
            entitlements(&args, &client, Some(&client), &mut report).await,
            Err(Stop(name)) if name == "MissingFlatFileDate"
        ));
        assert!(report.results.is_empty());
        args.flat_file_date = Some("2026-11-04".into());
        entitlements(&args, &client, Some(&client), &mut report)
            .await
            .unwrap();
        let sent: Vec<_> = report
            .results
            .iter()
            .map(|r| r["rpc"].as_str().unwrap().to_owned())
            .collect();
        let mut expected: Vec<_> = typed::ALL
            .iter()
            .filter(|e| e.rpc.starts_with("GetOption"))
            .map(|e| e.rpc.to_owned())
            .collect();
        let mut unique = sent.clone();
        unique.sort();
        unique.dedup();
        expected.sort();
        assert_eq!((sent.len(), unique), (51, expected));
        assert_eq!(sent.last().unwrap(), "GetOptionFlatFileQuote");
    }

    #[tokio::test]
    async fn flat_file_probes_never_read_bulk_data() {
        // The fixture serves only stock EOD, so the flat-file run settings
        // (capped client, one message, OverProbeLimit) are driven with it.
        let message = |rows| {
            let (wire_table, _) = fixture::table(fixture::Shape::Mixed, rows);
            fixture::response(&wire_table, false)
        };
        let big = message(40_000);
        assert!(big.encoded_len() > FLAT_FILE_PROBE_LIMIT);
        for (first, expected) in [(big, ("OverProbeLimit", 0)), (message(3), ("OK", 1))] {
            let server = fixture::Fixture::start(fixture::Script {
                messages: vec![Ok(first); 50],
                ..Default::default()
            });
            let flat_files = fixture_client_with_limit(&server, FLAT_FILE_PROBE_LIMIT).await;
            let mut report = Report::default();
            let mut run = EntitlementRun {
                client: &flat_files,
                report: &mut report,
                pacer: Pacer {
                    interval: Duration::ZERO,
                    last: None,
                },
                only: &[],
                max_messages: 1,
                flat_files: true,
            };
            run.run(typed::StockHistoryEod::new(
                "AAPL",
                date("2024-06-14"),
                date("2024-06-14"),
            ))
            .await
            .unwrap();
            let result = &report.results[0];
            assert_eq!(
                (
                    result["status"].as_str().unwrap(),
                    result["messages"].as_u64().unwrap()
                ),
                expected
            );
            // No vendor text is kept for the limit, and nothing past one
            // message is read.
            assert!(result["status_text"].is_null());
            assert_eq!(result["complete"], false);
            assert!(result["bytes"].as_u64().unwrap() <= FLAT_FILE_PROBE_LIMIT as u64);
        }
    }

    #[tokio::test]
    async fn entitlement_map_records_refusals_and_continues() {
        let (wire_table, _) = fixture::table(fixture::Shape::Mixed, 3);
        let server = fixture::Fixture::start(fixture::Script {
            messages: vec![
                Ok(fixture::response(&wire_table, true)),
                Ok(fixture::response(&wire_table, false)),
            ],
            ..Default::default()
        });
        let client = fixture_client(&server).await;
        let mut report = Report::default();
        entitlements(&entitlement_args(&[], 5), &client, None, &mut report)
            .await
            .unwrap();
        // About 35 single requests, each RPC once (the approved budget).
        let rpcs: std::collections::BTreeSet<_> = report
            .results
            .iter()
            .map(|r| r["rpc"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!((report.results.len(), rpcs.len()), (36, 36));
        assert!(
            rpcs.iter()
                .all(|rpc| typed::ALL.iter().any(|e| e.rpc == rpc))
        );
        // The fixture serves stock EOD only; every other RPC is refused there.
        let first = &report.results[0];
        assert_eq!(first["rpc"], "GetStockHistoryEod");
        assert_eq!(
            (first["status"].as_str(), first["messages"].as_u64()),
            (Some("OK"), Some(2))
        );
        assert_eq!(first["complete"], true);
        assert!(
            report.results[1..]
                .iter()
                .all(|r| r["status"] == "Unimplemented")
        );
        assert_eq!(server.requests.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn entitlement_map_reads_a_bounded_prefix() {
        let (wire_table, _) = fixture::table(fixture::Shape::Mixed, 3);
        let server = fixture::Fixture::start(fixture::Script {
            messages: vec![Ok(fixture::response(&wire_table, true)); 10],
            ..Default::default()
        });
        let client = fixture_client(&server).await;
        let mut report = Report::default();
        entitlements(
            &entitlement_args(&["GetStockHistoryEod"], 3),
            &client,
            None,
            &mut report,
        )
        .await
        .unwrap();
        assert_eq!(report.results.len(), 1);
        assert_eq!(report.results[0]["messages"], 3);
        assert_eq!(report.results[0]["complete"], false);
    }

    #[tokio::test]
    async fn entitlement_map_stops_on_resource_exhausted() {
        let server = fixture::Fixture::start(fixture::Script {
            initial_error: Some(tonic::Status::resource_exhausted("synthetic")),
            ..Default::default()
        });
        let client = fixture_client(&server).await;
        let mut report = Report::default();
        match entitlements(&entitlement_args(&[], 5), &client, None, &mut report).await {
            Err(Stop(name)) => assert_eq!(name, "ResourceExhausted"),
            Ok(()) => panic!("expected a stop"),
        }
        assert_eq!(report.results.len(), 1);
        assert_eq!(report.results[0]["status_text"], "synthetic");
    }

    #[tokio::test]
    async fn checks_compare_query_with_the_legacy_decoder() {
        let (wire_table, _) = fixture::table(fixture::Shape::Prices, 3);
        let server = fixture::Fixture::start(fixture::Script {
            messages: vec![
                Ok(fixture::response(&wire_table, true)),
                Ok(fixture::response(&wire_table, false)),
            ],
            ..Default::default()
        });
        let client = fixture_client(&server).await;
        let mut report = Report::default();
        let options = ChecksArgs {
            common: entitlement_args(&[], 5).common,
            date: "2024-06-14".into(),
            expiration: "2024-06-21".into(),
        };
        checks(&options, &client, &mut report).await.unwrap();
        assert_eq!(report.results.len(), 10);
        // The fixture serves stock EOD only: equal exact values, 6 rows.
        let first = &report.results[0];
        assert_eq!(first["rpc"], "GetStockHistoryEod");
        assert_eq!(
            (
                first["query_status"].as_str(),
                first["legacy_status"].as_str()
            ),
            (Some("OK"), Some("OK"))
        );
        assert_eq!(
            (first["rows"].as_u64(), first["legacy_rows"].as_u64()),
            (Some(6), Some(6))
        );
        assert_eq!(first["values_equal"], true);
        assert_eq!(first["columns"][0][1], "Decimal128(38, 10)");
        // Everything else is refused by the fixture and recorded, not fatal.
        assert!(
            report.results[1..]
                .iter()
                .all(|r| r["legacy_status"] == "Unimplemented")
        );
    }

    #[test]
    fn reports_with_identity_are_refused() {
        let identity = [
            "token-123".to_owned(),
            "me@example.invalid".to_owned(),
            String::new(),
        ];
        assert!(contains_identity(br#"{"x":"token-123"}"#, &identity));
        assert!(contains_identity(b"me@example.invalid", &identity));
        assert!(!contains_identity(br#"{"rows":3}"#, &identity));
    }

    #[test]
    fn run_ids_are_restricted() {
        assert!(output_path("ok-run_1").is_ok());
        assert!(output_path("../escape").is_err());
        assert!(output_path("").is_err());
    }
}
