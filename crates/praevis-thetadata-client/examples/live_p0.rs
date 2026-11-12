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
//!   user-approved exception to "never sweep endpoints" (AGENTS.md); re-running
//!   it needs approval again. The private report keeps truncated vendor status
//!   text for a person to classify locally; the code never interprets it, and
//!   only classifications and gRPC codes are committed. `--set options-pro`
//!   is the options PRO map (one approved run, 2026-11-04): every option RPC
//!   once, each read for at most `--max-messages`. Flat files come last,
//!   through a second client on the same session with a 1 MiB message limit,
//!   reading one message: a larger first message is refused from its length
//!   prefix (`OverProbeLimit`), so a probe never pulls bulk data.
//!   `--set stock-index-rate` (one run approved by the user on 2026-11-07;
//!   re-running needs approval) covers every stock, index, calendar and
//!   interest-rate RPC with a vendor wrapper (30): each once, paced and
//!   sequential, through ONE response decoded both ways (the columnar and
//!   legacy decoders, compared value by value), read for at most
//!   `--max-messages`. Calendar requests run for several dates and years.
//!   The flat files are bounded probes, as above; the stock quote flat file
//!   only with `--include-stock-quote-flat-file` (gated, off by default).
//!   The private report keeps
//!   the server's status text, for a person to classify the tier, and the
//!   calendar requests' decoded rows (calendar data, not market prices).
//! * `sizes`: the options workloads (W4-W6 in `workloads.json`), one paced
//!   request at a time through the typed requests, counting rows (decoded)
//!   and bytes, and recording each request's largest encoded message and a
//!   histogram of message sizes (item 8: how large the messages that
//!   `query()`/`job()` streams assemble really are). A request is dropped as soon as it passes `--max-rows`
//!   (default 1M, the vendor's sizing guide), so a run never pulls more than
//!   about that much per request. Sizes the workloads before any baseline.
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
    /// Rows and bytes per request of the options workloads, capped.
    Sizes(SizesArgs),
    /// Time to complete whole options workloads through `query()`.
    Workload(WorkloadArgs),
    /// One all-strike request against the same request split per strike.
    SplitProbe(SplitProbeArgs),
    /// Step F: every option RPC through `query()`, values compared with the
    /// legacy decoder; flat files probed, bounded.
    OptionChecks(OptionChecksArgs),
    /// One login, then the same small request after growing idle gaps:
    /// does an idle connection fail the next request, and with which code?
    IdleReconnect(IdleReconnectArgs),
    /// Beta.1 error review: an unknown symbol against a range with no
    /// trading days. Two stock EOD requests, each through `query()` and the
    /// raw stub: are they told apart by anything but the vendor's text?
    NotFound(NotFoundArgs),
}

#[derive(Args)]
struct NotFoundArgs {
    #[command(flatten)]
    common: Common,
}

#[derive(Args)]
struct IdleReconnectArgs {
    #[command(flatten)]
    common: Common,
    /// Idle gaps in seconds before each further request.
    #[arg(long, default_value = "30,60,120,180", value_delimiter = ',')]
    gaps: Vec<u64>,
}

#[derive(Args)]
struct OptionChecksArgs {
    #[command(flatten)]
    common: Common,
    /// Historical trading day for history, at-time and list requests.
    #[arg(long, default_value = "2024-06-14")]
    date: String,
    /// An option expiration listed on `--date`.
    #[arg(long, default_value = "2024-06-21")]
    expiration: String,
    /// A SPY expiration listed today, for the snapshots (run during US
    /// market hours).
    #[arg(long)]
    snapshot_expiration: String,
    /// A past trading day within the vendor's 7 most recent calendar days,
    /// for the flat files (bounded probes: one message of at most 1 MiB).
    #[arg(long)]
    flat_file_date: String,
    /// Also probe the option quote flat file (about 38 GB, about 30 minutes
    /// to generate). Needs the user's explicit go-ahead.
    #[arg(long)]
    include_option_quote_flat_file: bool,
    /// Only these RPCs, comma-separated.
    #[arg(long, value_delimiter = ',')]
    only: Vec<String>,
    /// Decode ONE response both ways (columnar and legacy) instead of two
    /// separate requests, so a moving market cannot cause differences.
    /// Flat files: the first message only (at most 1 MiB), both ways.
    #[arg(long)]
    same_response: bool,
}

#[derive(Args)]
struct SplitProbeArgs {
    #[command(flatten)]
    common: Common,
    /// A one-request options workload with every strike (default: W5b,
    /// tick quotes for SPY calls, 10:00-10:15, about 939k rows).
    #[arg(long, default_value = "W5b")]
    workload: String,
    /// Concurrency of the hand-made per-strike runs, each run in turn (`job()`
    /// runs once more at the tier's concurrency).
    #[arg(long, default_value = "1,8")]
    concurrency: String,
    /// Skip the hand-made per-strike runs: only the references and `job()`.
    /// (Use this rather than an empty `--concurrency`, which Windows
    /// PowerShell 5.1 drops.)
    #[arg(long)]
    no_splits: bool,
}

#[derive(Args)]
struct WorkloadArgs {
    #[command(flatten)]
    common: Common,
    /// Comma-separated workload IDs from `workloads.json`.
    #[arg(long)]
    workloads: String,
    /// Requests in flight at once, at most 8 (PRO's documented limit; the
    /// client itself no longer limits them).
    #[arg(long, default_value_t = 8, value_parser = clap::value_parser!(u32).range(1..=8))]
    concurrency: u32,
    #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u32).range(1..=10))]
    rounds: u32,
}

#[derive(Args)]
struct SizesArgs {
    #[command(flatten)]
    common: Common,
    #[arg(long, default_value = "W4a,W4b,W5a,W5b,W6a,W6b")]
    workloads: String,
    /// Drop a request once it has returned more rows than this.
    #[arg(long, default_value_t = 1_000_000, value_parser = clap::value_parser!(u64).range(1..=1_000_000))]
    max_rows: u64,
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
    /// `stock-index-rate`: every stock, index, calendar and interest-rate
    /// RPC with a vendor wrapper, values compared on the same response
    /// (approved 2026-11-07).
    #[arg(long, default_value = "free", value_parser = ["free", "options-pro", "stock-index-rate"])]
    set: String,
    /// A date among the 7 most recent calendar days, for the flat files
    /// (`options-pro` and `stock-index-rate`).
    #[arg(long)]
    flat_file_date: Option<String>,
    /// `stock-index-rate` only: also probe the stock quote flat file. Off by
    /// default: if entitled, even a probe can start the vendor's generation
    /// of a very large file (like the about-38 GB option quote file), so it
    /// needs explicit approval.
    #[arg(long)]
    include_stock_quote_flat_file: bool,
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
    /// Whether the first message carried a flat-file manifest (not data).
    manifest: Option<bool>,
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
            _ => "other",
        },
        status: String::new(),
        status_text: None,
        headers_ns: None,
        first_message_ns: None,
        total_ns: 0,
        messages: 0,
        bytes: 0,
        complete: false,
        manifest: None,
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
                        out.manifest = Some(message.flat_file_manifest.is_some());
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
    /// Live checks (step F): run each request through `query()` and the
    /// legacy decoder and compare values, instead of a bounded probe. Flat
    /// files are always probed.
    check: bool,
    /// With `check`: decode ONE response both ways (`check_same`). Flat
    /// files then read at most `max_messages` through the capped client.
    same_response: bool,
    /// With `same_response`: also cap the other requests at this many
    /// messages (`None`: the whole response).
    cap: Option<u32>,
    /// A label for the next result (for example, a calendar date), so
    /// repeated requests to one RPC can be told apart.
    label: Option<String>,
}
impl EntitlementRun<'_> {
    async fn run<E: Endpoint + Clone + Send + Sync + 'static>(
        &mut self,
        request: E,
    ) -> Result<(), Stop> {
        if !self.only.is_empty() && !self.only.iter().any(|rpc| rpc == E::RPC) {
            return Ok(());
        }
        if self.check && self.same_response {
            let limit = if self.flat_files {
                Some(self.max_messages)
            } else {
                self.cap
            };
            let mut result = check_same(self.client, request, &mut self.pacer, limit).await;
            // A first flat-file message over the probe's limit is refused
            // from its length prefix: accepted, but nothing bulk was read.
            if self.flat_files && result["status"] == "OutOfRange" && result["messages"].is_null() {
                result["status"] = "OverProbeLimit".into();
                result["status_text"] = serde_json::Value::Null;
            }
            if let Some(label) = self.label.take() {
                result["label"] = label.into();
            }
            let status = result["status"].as_str().unwrap_or("").to_owned();
            let stop = ends_entitlement_run(&status).then_some(Stop(status));
            self.report.results.push(result);
            return stop.map_or(Ok(()), Err);
        }
        if self.check && !self.flat_files {
            let result = check(self.client, request, &mut self.pacer).await;
            let status = result["query_status"].as_str().unwrap_or("").to_owned();
            let stop = (status.contains("ResourceExhausted") || status.contains("Unavailable"))
                .then_some(Stop(status));
            self.report.results.push(result);
            return stop.map_or(Ok(()), Err);
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
        let set = OptionSet {
            min_interval: options.common.min_interval,
            only: &options.only,
            max_messages: options.max_messages,
            day,
            exp,
            snapshot_exp: exp,
            flat,
            include_quote_flat_file: true,
            check: false,
            same_response: false,
        };
        return option_entitlements(&set, client, flat_files, report).await;
    }
    if options.set == "stock-index-rate" {
        let flat = date(
            options
                .flat_file_date
                .as_deref()
                .ok_or_else(|| Stop("MissingFlatFileDate".into()))?,
        );
        let flat_files = flat_files.ok_or_else(|| Stop("MissingFlatFileClient".into()))?;
        let set = StockIndexRateSet {
            min_interval: options.common.min_interval,
            only: &options.only,
            max_messages: options.max_messages,
            day,
            flat,
            include_quote_flat_file: options.include_stock_quote_flat_file,
        };
        return stock_index_rate(&set, client, flat_files, report).await;
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
        check: false,
        same_response: false,
        cap: None,
        label: None,
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
    r.run(
        typed::CorporateActionSplit::default()
            .symbol("AAPL".into())
            .start_date("2024-01-01".into())
            .end_date("2024-12-31".into()),
    )
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
/// The option request set, shared by the PRO map (bounded probes) and the
/// step F live checks (values compared).
struct OptionSet<'a> {
    min_interval: f64,
    only: &'a [String],
    max_messages: u32,
    /// History, at-time and list requests: this trading day and expiration.
    day: NaiveDate,
    exp: NaiveDate,
    /// Snapshots: an expiration listed now (the map used `exp`).
    snapshot_exp: NaiveDate,
    /// Flat files: a past day within the vendor's 7-day window.
    flat: NaiveDate,
    /// Probe the about-38 GB option quote flat file too (needs the user's
    /// go-ahead outside the one map run).
    include_quote_flat_file: bool,
    check: bool,
    same_response: bool,
}

async fn option_entitlements(
    set: &OptionSet<'_>,
    client: &ThetaClient,
    flat_files: &ThetaClient,
    report: &mut Report,
) -> Result<(), Stop> {
    let (day, exp, snapshot_exp, flat) = (set.day, set.exp, set.snapshot_exp, set.flat);
    let mut r = EntitlementRun {
        client,
        report,
        pacer: Pacer {
            interval: Duration::from_secs_f64(set.min_interval),
            last: None,
        },
        only: set.only,
        max_messages: set.max_messages,
        flat_files: false,
        check: set.check,
        same_response: set.same_response,
        cap: None,
        label: None,
    };
    macro_rules! snapshot {
        ($($t:ident),+) => {$(
            r.run(typed::$t::new("SPY", snapshot_exp).right("call")).await?;
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
    r.run(typed::OptionSnapshotTrade::new("SPY", snapshot_exp).right("call"))
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
    if set.include_quote_flat_file {
        r.run(typed::OptionFlatFileQuote::new(flat)).await?;
    }
    Ok(())
}

/// The stock, index, calendar and interest-rate set (one run approved by the
/// user on 2026-11-07): every RPC of those products with a vendor wrapper.
struct StockIndexRateSet<'a> {
    min_interval: f64,
    only: &'a [String],
    /// Messages read per request before the stream is dropped.
    max_messages: u32,
    /// History and at-time requests: this trading day.
    day: NaiveDate,
    /// Flat files: a past day within the vendor's 7-day window.
    flat: NaiveDate,
    /// Probe the stock quote flat file too (gated: needs explicit approval).
    include_quote_flat_file: bool,
}

/// The calendar requests: a normal weekday, a weekend day, a full holiday and
/// an early close, then two whole years.
const CALENDAR_DATES: [(&str, &str); 4] = [
    ("2025-11-05", "weekday"),
    ("2025-11-08", "weekend"),
    ("2025-12-25", "holiday"),
    ("2025-11-28", "early close"),
];
const CALENDAR_YEARS: [&str; 2] = ["2025", "2026"];

/// Each RPC once (calendar requests per date and year), as narrow as the
/// request allows: AAPL and SPX, one day, tick requests over one minute and
/// interval requests over five, every response capped at `max_messages`.
/// ONE response per request is decoded both ways and compared, so a refusal
/// is recorded with its status text for a person to classify, and an
/// entitled response is value-checked. Flat files come last, through
/// `flat_files` (the 1 MiB message limit), one message each.
async fn stock_index_rate(
    set: &StockIndexRateSet<'_>,
    client: &ThetaClient,
    flat_files: &ThetaClient,
    report: &mut Report,
) -> Result<(), Stop> {
    let (day, flat) = (set.day, set.flat);
    let mut r = EntitlementRun {
        client,
        report,
        pacer: Pacer {
            interval: Duration::from_secs_f64(set.min_interval),
            last: None,
        },
        only: set.only,
        max_messages: set.max_messages,
        flat_files: false,
        check: true,
        same_response: true,
        cap: Some(set.max_messages),
        label: None,
    };
    // Stock.
    r.run(typed::StockListSymbols::new()).await?;
    r.run(typed::StockListDates::new("trade", "AAPL")).await?;
    r.run(typed::StockSnapshotOhlc::new("AAPL")).await?;
    r.run(typed::StockSnapshotTrade::new("AAPL")).await?;
    r.run(typed::StockSnapshotQuote::new("AAPL")).await?;
    r.run(typed::StockSnapshotMarketValue::new("AAPL")).await?;
    r.run(typed::StockHistoryEod::new("AAPL", day, day)).await?;
    r.run(
        typed::StockHistoryOhlc::new("AAPL")
            .date(day)
            .interval("1m")
            .start_time("10:00:00")
            .end_time("10:05:00"),
    )
    .await?;
    r.run(
        typed::StockHistoryTrade::new("AAPL")
            .date(day)
            .start_time("10:00:00")
            .end_time("10:01:00"),
    )
    .await?;
    r.run(
        typed::StockHistoryQuote::new("AAPL")
            .date(day)
            .interval("1m")
            .start_time("10:00:00")
            .end_time("10:05:00"),
    )
    .await?;
    r.run(
        typed::StockHistoryTradeQuote::new("AAPL")
            .date(day)
            .start_time("10:00:00")
            .end_time("10:01:00"),
    )
    .await?;
    r.run(typed::StockAtTimeTrade::new("AAPL", day, day, "10:00:00"))
        .await?;
    r.run(typed::StockAtTimeQuote::new("AAPL", day, day, "10:00:00"))
        .await?;
    // Index.
    r.run(typed::IndexListSymbols::new()).await?;
    r.run(typed::IndexListDates::new("SPX")).await?;
    r.run(typed::IndexSnapshotOhlc::new("SPX")).await?;
    r.run(typed::IndexSnapshotPrice::new("SPX")).await?;
    r.run(typed::IndexSnapshotMarketValue::new("SPX")).await?;
    r.run(typed::IndexHistoryEod::new("SPX", day, day)).await?;
    r.run(
        typed::IndexHistoryOhlc::new("SPX", day, day)
            .interval("1m")
            .start_time("10:00:00")
            .end_time("10:05:00"),
    )
    .await?;
    r.run(
        typed::IndexHistoryPrice::new("SPX")
            .date(day)
            .interval("1m")
            .start_time("10:00:00")
            .end_time("10:05:00"),
    )
    .await?;
    r.run(typed::IndexAtTimePrice::new("SPX", day, day, "10:00:00"))
        .await?;
    // Calendar.
    r.run(typed::CalendarOpenToday::new()).await?;
    for (text, kind) in CALENDAR_DATES {
        r.label = Some(format!("{text} ({kind})"));
        r.run(typed::CalendarOnDate::new(date(text))).await?;
    }
    for year in CALENDAR_YEARS {
        r.label = Some(year.into());
        r.run(typed::CalendarYear::new(year)).await?;
    }
    // Interest rate.
    r.run(typed::InterestRateHistoryEod::new("SOFR", day, day))
        .await?;
    // Flat files: bounded probes, one message of at most 1 MiB, values
    // compared on that message.
    r.client = flat_files;
    r.max_messages = 1;
    r.flat_files = true;
    r.run(typed::IndexFlatFileEod::new(flat)).await?;
    r.run(typed::StockFlatFileEod::new(flat)).await?;
    r.run(typed::StockFlatFileTradeQuote::new(flat)).await?;
    if set.include_quote_flat_file {
        r.run(typed::StockFlatFileQuote::new(flat)).await?;
    }
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

/// Type an options workload request from its `workloads.json` entry (the
/// vendor's keyword names) and evaluate `$body` with it bound to `$r`, as
/// `Ok($body)`. Unknown methods or fields give `Err(Stop("BadWorkload"))`
/// before anything is sent. A macro, because each method is a different type.
macro_rules! option_dispatch {
    ($method:expr, $request:expr, $r:ident => $body:expr) => {{
        let method: &str = $method;
        let request: &serde_json::Value = $request;
        let bad = || Stop("BadWorkload".into());
        let known = [
            "symbol",
            "expiration",
            "date",
            "interval",
            "strike",
            "right",
            "start_time",
            "end_time",
        ];
        match request.as_object() {
            Some(fields)
                if fields.keys().all(|k| known.contains(&k.as_str()))
                    && fields.values().all(|v| v.is_string()) =>
            {
                let text = |name: &str| request[name].as_str().map(str::to_owned);
                match (text("symbol"), text("expiration"), text("date")) {
                    (Some(symbol), Some(expiration), Some(day)) => {
                        let expiration = expiration.as_str();
                        let day = date(&day);
                        let interval = text("interval");
                        macro_rules! common {
                            ($e:expr) => {{
                                let mut r = $e.date(day);
                                if let Some(v) = text("strike") {
                                    r = r.strike(v);
                                }
                                if let Some(v) = text("right") {
                                    r = r.right(v);
                                }
                                if let Some(v) = text("start_time") {
                                    r = r.start_time(v);
                                }
                                if let Some(v) = text("end_time") {
                                    r = r.end_time(v);
                                }
                                r
                            }};
                        }
                        macro_rules! with_interval {
                            ($e:expr) => {{
                                let r = common!($e);
                                match interval.clone() {
                                    Some(v) => r.interval(v),
                                    None => r,
                                }
                            }};
                        }
                        match (method, interval.is_some()) {
                            ("option_history_quote", _) => {
                                let $r = with_interval!(typed::OptionHistoryQuote::new(
                                    symbol, expiration
                                ));
                                Ok($body)
                            }
                            ("option_history_greeks_all", _) => {
                                let $r = with_interval!(typed::OptionHistoryGreeksAll::new(
                                    symbol, expiration
                                ));
                                Ok($body)
                            }
                            ("option_history_trade", false) => {
                                let $r =
                                    common!(typed::OptionHistoryTrade::new(symbol, expiration));
                                Ok($body)
                            }
                            ("option_history_trade_quote", false) => {
                                let $r = common!(typed::OptionHistoryTradeQuote::new(
                                    symbol, expiration
                                ));
                                Ok($body)
                            }
                            ("option_history_trade_greeks_all", false) => {
                                let $r = common!(typed::OptionHistoryTradeGreeksAll::new(
                                    symbol, expiration
                                ));
                                Ok($body)
                            }
                            _ => Err(bad()),
                        }
                    }
                    _ => Err(bad()),
                }
            }
            _ => Err(bad()),
        }
    }};
}

/// One options workload request, sized with [`size`].
async fn size_request(
    client: &ThetaClient,
    method: &str,
    request: &serde_json::Value,
    max_rows: u64,
) -> Result<Size, Stop> {
    option_dispatch!(method, request, r => size(client, r, max_rows).await)
}

/// Rows, bytes and timing of one capped request; never a value.
#[derive(Serialize, Debug, Default)]
struct Size {
    status: String,
    first_message_ns: Option<u128>,
    total_ns: u128,
    messages: u64,
    bytes: u64,
    rows: u64,
    /// Dropped after passing `max_rows`: the request is larger than that.
    over_max_rows: bool,
    /// The largest encoded gRPC message (`ResponseData`), in bytes: what a
    /// stream assembles in full before it can be decoded (item 8).
    max_message_bytes: u64,
    /// Messages by encoded size, upper bounds in `MESSAGE_SIZE_BUCKETS`.
    message_size_histogram: [u64; MESSAGE_SIZE_BUCKETS.len() + 1],
}

/// Upper bounds of the message-size histogram: 64 KiB, 256 KiB, 1, 4, 16 and
/// 64 MiB (the default `encoded_bytes`); the last bucket is anything larger.
const MESSAGE_SIZE_BUCKETS: [u64; 6] = [64 << 10, 256 << 10, 1 << 20, 4 << 20, 16 << 20, 64 << 20];

/// The histogram bucket of a message of `bytes` encoded bytes.
fn message_size_bucket(bytes: u64) -> usize {
    MESSAGE_SIZE_BUCKETS
        .iter()
        .position(|&limit| bytes <= limit)
        .unwrap_or(MESSAGE_SIZE_BUCKETS.len())
}

async fn size<E: Endpoint>(client: &ThetaClient, request: E, max_rows: u64) -> Size {
    let mut out = Size::default();
    let start = Instant::now();
    let mut decoder = match praevis_thetadata_arrow::ColumnarDecoder::new(
        praevis_thetadata_arrow::Limits::default(),
    ) {
        Ok(decoder) => decoder,
        Err(_) => {
            out.status = "DecoderConfiguration".into();
            return out;
        }
    };
    let body = async {
        let mut stream = client.stream_raw(&request).await?;
        while let Some(message) = stream.message().await? {
            if out.messages == 0 {
                out.first_message_ns = Some(start.elapsed().as_nanos());
            }
            out.messages += 1;
            let encoded = message.encoded_len() as u64;
            out.bytes += encoded;
            out.max_message_bytes = out.max_message_bytes.max(encoded);
            out.message_size_histogram[message_size_bucket(encoded)] += 1;
            match decoder.push(&message) {
                Ok(rows) => out.rows += rows as u64,
                Err(_) => return Ok(Some("DecodeError")),
            }
            // Counting only: release the decoded rows at once.
            let _ = decoder.flush();
            if out.rows > max_rows {
                out.over_max_rows = true;
                return Ok(None);
            }
        }
        Ok::<_, tonic::Status>(None)
        // Dropping the stream cancels the rest of the response.
    };
    match tokio::time::timeout(Duration::from_secs(600), body).await {
        Ok(Ok(None)) => out.status = "OK".into(),
        Ok(Ok(Some(local))) => out.status = local.into(),
        Ok(Err(status)) => out.status = status_name(status.code()),
        Err(_) => out.status = "ClientTimeout".into(),
    }
    out.total_ns = start.elapsed().as_nanos();
    out
}

/// One request through `query()`: rows counted and dropped, never kept.
#[derive(Serialize, Debug, Default)]
struct Run {
    request: usize,
    status: String,
    first_batch_ns: Option<u128>,
    total_ns: u128,
    rows: u64,
}

async fn run_query<E: Endpoint>(client: &ThetaClient, request: E, index: usize) -> Run {
    let start = Instant::now();
    let mut out = Run {
        request: index,
        ..Default::default()
    };
    let result = async {
        let mut stream = client.query(&request).await?;
        while let Some(batch) = stream.next_batch().await? {
            out.first_batch_ns
                .get_or_insert_with(|| start.elapsed().as_nanos());
            out.rows += batch.num_rows() as u64;
        }
        Ok::<(), EodError>(())
    }
    .await;
    out.status = match result {
        Ok(()) => "OK".into(),
        Err(error) => format!("{error:?}"),
    };
    out.total_ns = start.elapsed().as_nanos();
    out
}

/// Whole options workloads through `query()`, `--concurrency` requests at a
/// time, `--rounds` times, paced between workload runs (not within one). The
/// time to complete is the workload's wall time. Stops on `RESOURCE_EXHAUSTED`
/// or `UNAVAILABLE`.
async fn workload(
    options: &WorkloadArgs,
    client: &ThetaClient,
    report: &mut Report,
) -> Result<(), Stop> {
    let workloads: serde_json::Value =
        serde_json::from_str(WORKLOADS).expect("valid workloads.json");
    let mut pacer = Pacer {
        interval: Duration::from_secs_f64(options.common.min_interval),
        last: None,
    };
    let ids: Vec<&str> = options
        .workloads
        .split(',')
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .collect();
    for round in 0..options.rounds {
        for id in &ids {
            let spec = &workloads[*id];
            let method = spec["method"]
                .as_str()
                .ok_or_else(|| Stop(format!("UnknownWorkload {id}")))?;
            let requests = spec["requests"]
                .as_array()
                .ok_or_else(|| Stop(format!("UnknownWorkload {id}")))?;
            pacer.wait().await;
            let slots =
                std::sync::Arc::new(tokio::sync::Semaphore::new(options.concurrency as usize));
            let mut tasks = tokio::task::JoinSet::new();
            let start = Instant::now();
            for (index, request) in requests.iter().enumerate() {
                let client = client.clone();
                let slots = slots.clone();
                option_dispatch!(method, request, r => {
                    tasks.spawn(async move {
                        let _slot = slots.acquire_owned().await.expect("open semaphore");
                        run_query(&client, r, index).await
                    });
                })?;
            }
            let mut runs = Vec::new();
            while let Some(run) = tasks.join_next().await {
                runs.push(run.expect("request task"));
            }
            let wall_ns = start.elapsed().as_nanos();
            runs.sort_by_key(|r| r.request);
            let rows: u64 = runs.iter().map(|r| r.rows).sum();
            let ok = runs.iter().filter(|r| r.status == "OK").count();
            eprintln!(
                "round {round} {id:<5} x{} {:>8.2} s {rows:>9} rows, {ok}/{} OK",
                options.concurrency,
                wall_ns as f64 / 1e9,
                runs.len()
            );
            let stop = runs
                .iter()
                .find(|r| {
                    r.status.contains("ResourceExhausted") || r.status.contains("Unavailable")
                })
                .map(|r| r.status.clone());
            report.results.push(serde_json::json!({
                "round": round,
                "workload": id,
                "concurrency": options.concurrency,
                "wall_ns": wall_ns,
                "rows": rows,
                "requests": runs,
            }));
            if let Some(status) = stop {
                return Err(Stop(status));
            }
        }
    }
    Ok(())
}

/// One request's rows and an order-independent digest of its values: the sum
/// (wrapping) of a fixed hash of each row's (column name, exact value)
/// pairs. Two results with the same multiset of rows have the same digest
/// whatever the order or the request split. Only the digest is reported.
#[derive(Serialize, Debug, Default, Clone)]
struct Digested {
    request: usize,
    status: String,
    first_batch_ns: Option<u128>,
    total_ns: u128,
    rows: u64,
    #[serde(skip)]
    digest: u64,
    #[serde(skip)]
    columns: Vec<String>,
}

fn row_digest(batch: &praevis_thetadata_client::arrow_array::RecordBatch) -> u64 {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let schema = batch.schema();
    let mut names: Vec<(usize, &str)> = schema
        .fields()
        .iter()
        .enumerate()
        .map(|(i, f)| (i, f.name().as_str()))
        .collect();
    names.sort_by_key(|(_, name)| *name);
    let mut sum = 0u64;
    for row in arrow_rows(std::slice::from_ref(batch)) {
        // DefaultHasher::new() has fixed keys: the same row hashes the same.
        let mut hasher = DefaultHasher::new();
        for (index, name) in &names {
            name.hash(&mut hasher);
            row[*index].hash(&mut hasher);
        }
        sum = sum.wrapping_add(hasher.finish());
    }
    sum
}

/// How a digested request is sent: `query()` as given, or `job()` (which
/// plans the per-strike split for the all-strike fine quote shape).
#[derive(Clone, Copy)]
enum Via {
    Query,
    Job,
}

async fn digest_query<E: Endpoint + Clone + Send + Sync + 'static>(
    client: &ThetaClient,
    request: E,
    index: usize,
) -> Digested {
    digest_via(client, request, index, Via::Query).await
}

async fn digest_via<E: Endpoint + Clone + Send + Sync + 'static>(
    client: &ThetaClient,
    request: E,
    index: usize,
    via: Via,
) -> Digested {
    enum Stream {
        Query(Box<praevis_thetadata_client::QueryStream>),
        Job(praevis_thetadata_client::JobStream),
    }
    let start = Instant::now();
    let mut out = Digested {
        request: index,
        ..Default::default()
    };
    let result = async {
        let mut stream = match via {
            Via::Query => Stream::Query(Box::new(client.query(&request).await?)),
            Via::Job => Stream::Job(client.job(&request).await?),
        };
        loop {
            let next = match &mut stream {
                Stream::Query(s) => s.next_batch().await?,
                Stream::Job(s) => s.next_batch().await?,
            };
            let Some(batch) = next else { break };
            out.first_batch_ns
                .get_or_insert_with(|| start.elapsed().as_nanos());
            if out.columns.is_empty() {
                let mut names: Vec<String> = batch
                    .schema()
                    .fields()
                    .iter()
                    .map(|f| f.name().clone())
                    .collect();
                names.sort();
                out.columns = names;
            }
            out.rows += batch.num_rows() as u64;
            out.digest = out.digest.wrapping_add(row_digest(&batch));
        }
        Ok::<(), EodError>(())
    }
    .await;
    out.status = match result {
        Ok(()) => "OK".into(),
        Err(EodError::NoData) => "NoData".into(),
        Err(error) => format!("{error:?}"),
    };
    out.total_ns = start.elapsed().as_nanos();
    out
}

/// A decimal strike as request text in dollars, with at least two decimals
/// (`542.00`, `542.50`, `0.125`), as the vendor documents strikes.
fn strike_text(value: i128, scale: u32) -> String {
    let unit = 10i128.pow(scale);
    let (whole, mut frac) = (value / unit, (value % unit).abs());
    let mut digits = scale;
    while digits > 2 && frac % 10 == 0 {
        frac /= 10;
        digits -= 1;
    }
    if digits < 2 {
        frac *= 10i128.pow(2 - digits);
        digits = 2;
    }
    format!("{whole}.{frac:0width$}", width = digits as usize)
}

/// The strikes listed for the workload's symbol and expiration, as request
/// text in dollars (`542.00`, `542.50`). Decimal strikes only.
async fn listed_strikes(
    client: &ThetaClient,
    symbol: &str,
    expiration: &str,
) -> Result<Vec<String>, Stop> {
    use praevis_thetadata_client::arrow_array::cast::AsArray;
    use praevis_thetadata_client::arrow_array::types::Decimal128Type;
    use praevis_thetadata_client::arrow_schema::DataType;
    let expiration = date(expiration);
    let batches = client
        .query(&typed::OptionListStrikes::new(symbol, expiration))
        .await
        .map_err(|e| Stop(format!("ListStrikes {e:?}")))?
        .collect()
        .await
        .map_err(|e| Stop(format!("ListStrikes {e:?}")))?;
    let mut strikes = Vec::new();
    for batch in &batches {
        let column = batch
            .column_by_name("strike")
            .ok_or_else(|| Stop("ListStrikesNoStrikeColumn".into()))?;
        let DataType::Decimal128(_, scale) = column.data_type() else {
            return Err(Stop(format!("ListStrikesType {:?}", column.data_type())));
        };
        let scale = *scale as u32;
        for value in column.as_primitive::<Decimal128Type>().iter().flatten() {
            strikes.push(strike_text(value, scale));
        }
    }
    strikes.sort();
    strikes.dedup();
    Ok(strikes)
}

/// The per-strike split hypothesis: the workload's all-strike request (run
/// before and after, as the reference), then the same request once per
/// listed strike at each `--concurrency`. Reports wall times, row counts and
/// whether each split's digest equals the reference; never a value or a
/// strike. Stops on `RESOURCE_EXHAUSTED` or `UNAVAILABLE`.
async fn split_probe(
    options: &SplitProbeArgs,
    client: &ThetaClient,
    report: &mut Report,
) -> Result<(), Stop> {
    let workloads: serde_json::Value =
        serde_json::from_str(WORKLOADS).expect("valid workloads.json");
    let spec = &workloads[options.workload.as_str()];
    let method = spec["method"]
        .as_str()
        .ok_or_else(|| Stop("UnknownWorkload".into()))?;
    let request = match spec["requests"].as_array().map(Vec::as_slice) {
        Some([one]) if one.get("strike").is_none() => one.clone(),
        _ => return Err(Stop("NeedsOneAllStrikeRequest".into())),
    };
    let levels: Vec<u32> = options
        .concurrency
        .split(',')
        .filter(|c| !options.no_splits && !c.trim().is_empty())
        .map(|c| c.trim().parse().ok().filter(|c| (1..=8).contains(c)))
        .collect::<Option<_>>()
        .ok_or_else(|| Stop("ConcurrencyOneToEight".into()))?;
    let mut pacer = Pacer {
        interval: Duration::from_secs_f64(options.common.min_interval),
        last: None,
    };
    let stop_on =
        |status: &str| status.contains("ResourceExhausted") || status.contains("Unavailable");
    pacer.wait().await;
    let strikes = listed_strikes(
        client,
        request["symbol"].as_str().unwrap_or_default(),
        request["expiration"].as_str().unwrap_or_default(),
    )
    .await?;
    eprintln!("{} listed strikes", strikes.len());
    let mut reference = Vec::new();
    let run_reference = async |label: &str, pacer: &mut Pacer| -> Result<Digested, Stop> {
        pacer.wait().await;
        let run = option_dispatch!(method, &request, r => digest_query(client, r, 0).await)?;
        eprintln!(
            "reference ({label}) {} {} rows in {:.1} s (first {:.1} s)",
            run.status,
            run.rows,
            run.total_ns as f64 / 1e9,
            run.first_batch_ns.unwrap_or(0) as f64 / 1e9
        );
        if stop_on(&run.status) {
            return Err(Stop(run.status));
        }
        Ok(run)
    };
    reference.push(run_reference("before", &mut pacer).await?);
    let mut splits = Vec::new();
    for &level in &levels {
        pacer.wait().await;
        let slots = std::sync::Arc::new(tokio::sync::Semaphore::new(level as usize));
        let mut tasks = tokio::task::JoinSet::new();
        let start = Instant::now();
        for (index, strike) in strikes.iter().enumerate() {
            let mut one = request.clone();
            one["strike"] = strike.clone().into();
            let client = client.clone();
            let slots = slots.clone();
            option_dispatch!(method, &one, r => {
                tasks.spawn(async move {
                    let _slot = slots.acquire_owned().await.expect("open semaphore");
                    digest_query(&client, r, index).await
                });
            })?;
        }
        let mut runs = Vec::new();
        while let Some(run) = tasks.join_next().await {
            runs.push(run.expect("request task"));
        }
        let wall_ns = start.elapsed().as_nanos();
        runs.sort_by_key(|r| r.request);
        let rows: u64 = runs.iter().map(|r| r.rows).sum();
        let digest = runs.iter().fold(0u64, |d, r| d.wrapping_add(r.digest));
        let mut statuses = std::collections::BTreeMap::<String, usize>::new();
        for run in &runs {
            *statuses.entry(run.status.clone()).or_default() += 1;
        }
        let columns_match = runs
            .iter()
            .filter(|r| !r.columns.is_empty())
            .all(|r| r.columns == reference[0].columns);
        eprintln!(
            "split x{level}: {} requests, {rows} rows in {:.1} s, digest equal: {}, {statuses:?}",
            runs.len(),
            wall_ns as f64 / 1e9,
            digest == reference[0].digest
        );
        let stop = runs
            .iter()
            .find(|r| stop_on(&r.status))
            .map(|r| r.status.clone());
        splits.push(serde_json::json!({
            "concurrency": level,
            "requests": runs.len(),
            "wall_ns": wall_ns,
            "rows": rows,
            "statuses": statuses,
            "rows_equal_reference": rows == reference[0].rows,
            "digest_equal_reference": digest == reference[0].digest,
            "columns_equal_reference": columns_match,
            "runs": runs,
        }));
        if let Some(status) = stop {
            report.results.push(serde_json::json!({"workload": options.workload, "strikes": strikes.len(), "reference": reference, "splits": splits}));
            return Err(Stop(status));
        }
    }
    // The same request through job(), whose planner makes the per-strike
    // split itself (one extra strikes-list request), at the tier's
    // concurrency.
    pacer.wait().await;
    let job = option_dispatch!(method, &request, r => digest_via(client, r, 0, Via::Job).await)?;
    eprintln!(
        "job(): {} {} rows in {:.1} s, digest equal: {}",
        job.status,
        job.rows,
        job.total_ns as f64 / 1e9,
        job.digest == reference[0].digest
    );
    if stop_on(&job.status) {
        return Err(Stop(job.status));
    }
    let job = serde_json::json!({
        "status": job.status,
        "total_ns": job.total_ns,
        "first_batch_ns": job.first_batch_ns,
        "rows": job.rows,
        "rows_equal_reference": job.rows == reference[0].rows,
        "digest_equal_reference": job.digest == reference[0].digest,
        "columns_equal_reference": job.columns == reference[0].columns,
    });
    reference.push(run_reference("after", &mut pacer).await?);
    let stable =
        reference[0].rows == reference[1].rows && reference[0].digest == reference[1].digest;
    report.results.push(serde_json::json!({
        "workload": options.workload,
        "strikes": strikes.len(),
        "reference_stable": stable,
        "reference": reference,
        "splits": splits,
        "job": job,
    }));
    Ok(())
}

/// The options workloads, one paced request at a time, capped at `max_rows`.
/// Stops on `RESOURCE_EXHAUSTED`, `UNAVAILABLE` or a local failure; other
/// statuses are recorded and the run continues.
async fn sizes(options: &SizesArgs, client: &ThetaClient, report: &mut Report) -> Result<(), Stop> {
    let workloads: serde_json::Value =
        serde_json::from_str(WORKLOADS).expect("valid workloads.json");
    let mut pacer = Pacer {
        interval: Duration::from_secs_f64(options.common.min_interval),
        last: None,
    };
    for id in options
        .workloads
        .split(',')
        .map(str::trim)
        .filter(|w| !w.is_empty())
    {
        let spec = &workloads[id];
        let method = spec["method"]
            .as_str()
            .ok_or_else(|| Stop(format!("UnknownWorkload {id}")))?;
        let requests = spec["requests"]
            .as_array()
            .ok_or_else(|| Stop(format!("UnknownWorkload {id}")))?;
        for (index, request) in requests.iter().enumerate() {
            pacer.wait().await;
            let result = size_request(client, method, request, options.max_rows).await?;
            eprintln!(
                "{id}[{index}] {:<16} rows {:>9} messages {:>8} max message {:>6.2} MiB {:.1} s{}",
                result.status,
                result.rows,
                result.messages,
                result.max_message_bytes as f64 / (1 << 20) as f64,
                result.total_ns as f64 / 1e9,
                if result.over_max_rows {
                    " (over max rows)"
                } else {
                    ""
                }
            );
            let stop = ends_entitlement_run(&result.status)
                || matches!(
                    result.status.as_str(),
                    "DecodeError" | "DecoderConfiguration"
                );
            let status = result.status.clone();
            let mut value = serde_json::to_value(&result).expect("size serializes");
            value["workload"] = id.into();
            value["request"] = index.into();
            report.results.push(value);
            if stop {
                return Err(Stop(status));
            }
        }
    }
    Ok(())
}

/// A comparable value: prices as Decimal128(38, 10) unscaled, timestamps as
/// the legacy RFC 3339 text. Only equality is reported, never a value.
#[derive(Debug, PartialEq, Hash)]
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
                    other => Exact::Text(format!("unknown value kind {other:?}")),
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

/// One decoded value as JSON, for the calendar rows: prices as exact decimal
/// text (scale 10), never floats.
fn exact_json(value: &Exact) -> serde_json::Value {
    match value {
        Exact::Null => serde_json::Value::Null,
        Exact::Text(v) | Exact::Timestamp(v) => v.clone().into(),
        Exact::Integer(v) => (*v).into(),
        Exact::Boolean(v) => (*v).into(),
        Exact::Price(v) => {
            let sign = if *v < 0 { "-" } else { "" };
            let digits = v.unsigned_abs();
            let (whole, fraction) = (digits / 10u128.pow(10), digits % 10u128.pow(10));
            let fraction = format!("{fraction:010}");
            let fraction = fraction.trim_end_matches('0');
            if fraction.is_empty() {
                format!("{sign}{whole}").into()
            } else {
                format!("{sign}{whole}.{fraction}").into()
            }
        }
    }
}

/// The request through `query()` (the path that returned `Internal` in
/// `w10-conflict-001`): `OK` or our error variant.
async fn query_status(client: &ThetaClient) -> String {
    let day = date("2024-06-14");
    match client
        .query(&typed::StockHistoryEod::new("AAPL", day, day))
        .await
    {
        Ok(stream) => match stream.collect().await {
            Ok(_) => "OK".into(),
            Err(error) => format!("{error:?}"),
        },
        Err(error) => format!("{error:?}"),
    }
}

/// One login (this run's), then the same small request after each idle gap,
/// through `query()` and through the raw stub. The raw probe keeps the
/// server's status text in the private report only (as the entitlement
/// map does): a person reads it locally, and only codes are sent.
async fn idle_reconnect(
    options: &IdleReconnectArgs,
    client: &ThetaClient,
    report: &mut Report,
) -> Result<(), Stop> {
    let request = || typed::StockHistoryEod::new("AAPL", date("2024-06-14"), date("2024-06-14"));
    let start = Instant::now();
    for (index, gap) in std::iter::once(0)
        .chain(options.gaps.iter().copied())
        .enumerate()
    {
        tokio::time::sleep(Duration::from_secs(gap)).await;
        let at_s = start.elapsed().as_secs();
        let query = query_status(client).await;
        let raw = probe(client, &request(), 5).await;
        eprintln!(
            "request {index} after {gap:>3} s idle (t={at_s}s): query {query}, raw {}",
            raw.status
        );
        let mut value = serde_json::to_value(&raw).expect("probe serializes");
        value["idle_s"] = gap.into();
        value["at_s"] = at_s.into();
        value["query_status"] = query.into();
        report.results.push(value);
    }
    Ok(())
}

/// The beta.1 error review (item 6): `QueryError::NoData` covers both an
/// unknown symbol and a range with no trading days, because both answered
/// `NOT_FOUND` (2026-11-02). This asks whether anything other than the
/// vendor's text tells them apart.
///
/// Two stock EOD requests inside the FREE window, each sent through `query()`
/// (the category a caller sees) and through the raw stub (the full status).
/// Recorded per case:
/// - the gRPC code;
/// - the names of the status metadata keys (never their values);
/// - the length of the binary status details.
///
/// Then: whether the two messages and the two details are byte-identical.
/// The text itself goes to the private report only.
async fn not_found(client: &ThetaClient, report: &mut Report, pause: Duration) -> Result<(), Stop> {
    let cases = [
        ("unknown-symbol", "ZZZZNOTREAL", "2024-01-02", "2024-01-05"),
        ("no-trading-days", "AAPL", "2024-01-06", "2024-01-07"),
    ];
    let mut statuses = Vec::new();
    for (index, (case, symbol, start, end)) in cases.into_iter().enumerate() {
        if index > 0 {
            tokio::time::sleep(pause).await;
        }
        let request = typed::StockHistoryEod::new(symbol, date(start), date(end));
        let query = match client.query(&request).await {
            Ok(stream) => match stream.collect().await {
                Ok(batches) => format!(
                    "OK ({} rows)",
                    batches.iter().map(|b| b.num_rows()).sum::<usize>()
                ),
                Err(error) => format!("{error:?}"),
            },
            Err(error) => format!("{error:?}"),
        };
        tokio::time::sleep(pause).await;
        let raw = async {
            let mut stream = client.stream_raw(&request).await?;
            let mut messages = 0u32;
            while stream.message().await?.is_some() {
                messages += 1;
            }
            Ok::<_, tonic::Status>(messages)
        }
        .await;
        let mut out = serde_json::json!({"case": case, "query": query});
        match raw {
            Ok(messages) => {
                out["status"] = "OK".into();
                out["messages"] = messages.into();
            }
            Err(status) => {
                let mut keys: Vec<String> = status
                    .metadata()
                    .keys()
                    .map(|key| match key {
                        tonic::metadata::KeyRef::Ascii(k) => k.as_str().to_owned(),
                        tonic::metadata::KeyRef::Binary(k) => k.as_str().to_owned(),
                    })
                    .collect();
                keys.sort();
                out["status"] = status_name(status.code()).into();
                out["metadata_keys"] = keys.into();
                out["details_bytes"] = status.details().len().into();
                // Private report only, for a person to read; never committed.
                out["status_text"] = status_text(&status).into();
                statuses.push(status);
            }
        }
        eprintln!(
            "{case:<16} query {}, raw {}",
            out["query"].as_str().unwrap_or("?"),
            out["status"].as_str().unwrap_or("?")
        );
        report.results.push(out);
    }
    if let [a, b] = statuses.as_slice() {
        let same = serde_json::json!({
            "case": "comparison",
            "same_code": a.code() == b.code(),
            "same_message": a.message() == b.message(),
            "same_details": a.details() == b.details(),
        });
        eprintln!("comparison: {same}");
        report.results.push(same);
    }
    Ok(())
}

/// One endpoint, ONE request: every message of the same response decoded by
/// the columnar decoder (the one `query()` uses) and by the legacy decoder,
/// compared exactly. Unlike [`check`], the two sides see identical bytes, so
/// a moving market cannot make them differ. At most `max_messages` are read
/// (`None`: the whole response). Never a value in the report.
async fn check_same<E>(
    client: &ThetaClient,
    request: E,
    pacer: &mut Pacer,
    max_messages: Option<u32>,
) -> serde_json::Value
where
    E: Endpoint + Clone + Send + Sync + 'static,
{
    pacer.wait().await;
    let started = Instant::now();
    let captured = async {
        let mut stream = client.stream_raw(&request).await?;
        let mut messages = Vec::new();
        while max_messages.is_none_or(|m| (messages.len() as u32) < m) {
            match stream.message().await? {
                Some(message) => messages.push(message),
                None => return Ok::<_, tonic::Status>((messages, true)),
            }
        }
        Ok((messages, false))
    }
    .await;
    let mut out = serde_json::json!({
        "rpc": E::RPC,
        "python_name": E::PYTHON_NAME,
        "mode": "same-response",
        "total_ns": started.elapsed().as_nanos(),
    });
    let (messages, complete) = match captured {
        Ok(captured) => captured,
        Err(status) => {
            out["status"] = status_name(status.code()).into();
            // Private report only: a person classifies it (for example, the
            // tier a refusal names); never committed or interpreted here.
            out["status_text"] = status_text(&status).into();
            eprintln!("{:<45} {}", E::RPC, out["status"].as_str().unwrap_or("?"));
            return out;
        }
    };
    out["status"] = "OK".into();
    out["messages"] = messages.len().into();
    out["complete"] = complete.into();
    out["manifest"] = messages
        .iter()
        .any(|m| m.flat_file_manifest.is_some())
        .into();
    let columnar = (|| {
        let mut decoder = praevis_thetadata_arrow::ColumnarDecoder::new(
            praevis_thetadata_arrow::Limits::default(),
        )
        .map_err(|_| "configuration")?;
        let mut batches = Vec::new();
        for message in &messages {
            decoder.push(message).map_err(|_| "decode")?;
            batches.extend(decoder.flush());
        }
        Ok::<_, &'static str>(praevis_thetadata_client::unify(batches))
    })();
    let legacy: Result<Vec<_>, _> = messages
        .iter()
        .map(|m| decode::decode(m.clone(), LIMIT))
        .collect();
    match (&columnar, &legacy) {
        (Ok(batches), Ok(tables)) => {
            let ours = arrow_rows(batches);
            let theirs: Vec<Vec<Exact>> = tables.iter().flat_map(legacy_rows).collect();
            let differing = ours.iter().zip(&theirs).filter(|(a, b)| a != b).count();
            out["rows"] = ours.len().into();
            out["legacy_rows"] = theirs.len().into();
            out["values_equal"] = (ours == theirs).into();
            // Calendar data is not market prices, and the user asked to see
            // it: keep the decoded rows, in the private report only.
            if E::RPC.starts_with("GetCalendar") {
                out["calendar_rows"] = ours
                    .iter()
                    .map(|row| row.iter().map(exact_json).collect::<Vec<_>>())
                    .collect::<Vec<_>>()
                    .into();
            }
            out["differing_rows"] = (differing + ours.len().abs_diff(theirs.len())).into();
            out["columns"] = batches
                .first()
                .map(|b| {
                    b.schema()
                        .fields()
                        .iter()
                        .map(|f| serde_json::json!([f.name(), format!("{}", f.data_type())]))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
                .into();
        }
        (Err(error), _) => out["columnar_status"] = (*error).into(),
        (_, Err(_)) => out["legacy_status"] = "decode".into(),
    }
    eprintln!(
        "{:<45} same response: messages {:<5} rows {:<7} equal {}",
        E::RPC,
        out["messages"],
        out["rows"],
        out["values_equal"]
    );
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

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    // `workload` runs many queries at once: a multi-threaded runtime with the
    // Python package's worker count (at most 4), so decoding is not confined
    // to one thread. Every other command keeps the single-threaded runtime
    // its earlier results were measured with.
    let runtime = match cli.command {
        Command::Workload(_) | Command::SplitProbe(_) => {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(std::thread::available_parallelism().map_or(2, |n| n.get().min(4)))
                .enable_all()
                .build()
        }
        _ => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build(),
    }
    .expect("tokio runtime");
    runtime.block_on(run(cli))
}

async fn run(cli: Cli) -> std::process::ExitCode {
    let (common, name) = match &cli.command {
        Command::Breakdown(options) => (&options.common, "breakdown"),
        Command::Limits(options) => (&options.common, "limits"),
        Command::Entitlements(options) => (&options.common, "entitlements"),
        Command::Checks(options) => (&options.common, "checks"),
        Command::Session(options) => (&options.common, "session"),
        Command::Sizes(options) => (&options.common, "sizes"),
        Command::Workload(options) => (&options.common, "workload"),
        Command::SplitProbe(options) => (&options.common, "split-probe"),
        Command::OptionChecks(options) => (&options.common, "option-checks"),
        Command::IdleReconnect(options) => (&options.common, "idle-reconnect"),
        Command::NotFound(options) => (&options.common, "not-found"),
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
            let flat_files = if options.set != "free" {
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
        Command::Sizes(options) => sizes(options, &client, &mut report).await,
        Command::Workload(options) => workload(options, &client, &mut report).await,
        Command::SplitProbe(options) => split_probe(options, &client, &mut report).await,
        Command::IdleReconnect(options) => idle_reconnect(options, &client, &mut report).await,
        Command::NotFound(_) => not_found(&client, &mut report, Duration::from_secs(4)).await,
        Command::OptionChecks(options) => {
            // Flat files through the capped client, as in the PRO map.
            match connect_with_limit(client.session(), FLAT_FILE_PROBE_LIMIT).await {
                Ok(flat_files) => {
                    let set = OptionSet {
                        min_interval: options.common.min_interval,
                        only: &options.only,
                        max_messages: 1,
                        day: date(&options.date),
                        exp: date(&options.expiration),
                        snapshot_exp: date(&options.snapshot_expiration),
                        flat: date(&options.flat_file_date),
                        include_quote_flat_file: options.include_option_quote_flat_file,
                        check: true,
                        same_response: options.same_response,
                    };
                    option_entitlements(&set, &client, &flat_files, &mut report).await
                }
                Err(_) => {
                    eprintln!("connection failed");
                    return std::process::ExitCode::FAILURE;
                }
            }
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
        ClientConfig::default()
            .with_request_timeout(Some(Duration::from_secs(120)))
            .with_max_batch_bytes(limit),
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
    let auth = AuthClient::new(AuthConfig::default().with_environment(Environment::Prod))
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
            ClientConfig::default()
                .with_endpoint(Some(server.endpoint.clone()))
                .with_allow_insecure(true),
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
            ClientConfig::default()
                .with_endpoint(Some(server.endpoint.clone()))
                .with_allow_insecure(true),
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
            ClientConfig::default()
                .with_endpoint(Some(server.endpoint.clone()))
                .with_allow_insecure(true)
                .with_max_batch_bytes(limit),
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
            include_stock_quote_flat_file: false,
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
    async fn sizes_types_every_options_workload_and_records_each_request() {
        // The fixture serves only stock EOD: every option request reaches it
        // (no BadWorkload) and is answered Unimplemented.
        let server = fixture::Fixture::start(fixture::Script::default());
        let client = fixture_client(&server).await;
        let args = SizesArgs {
            common: entitlement_args(&[], 5).common,
            workloads: "W4a,W4b,W5a,W5b,W6a,W6b".into(),
            max_rows: 1_000_000,
        };
        let mut report = Report::default();
        sizes(&args, &client, &mut report).await.unwrap();
        assert_eq!(report.results.len(), 12);
        assert!(
            report
                .results
                .iter()
                .all(|r| r["status"] == "Unimplemented"),
            "{:?}",
            report.results
        );
        // Unknown fields, non-text values and tick-only methods with an
        // interval are refused before sending.
        for (method, request) in [
            ("option_history_quote", serde_json::json!({"symbol": "SPY"})),
            (
                "option_history_trade",
                serde_json::json!({"symbol": "SPY", "expiration": "2024-06-21", "date": "2024-06-14", "interval": "1m"}),
            ),
            (
                "option_history_quote",
                serde_json::json!({"symbol": "SPY", "expiration": "2024-06-21", "date": "2024-06-14", "strike": 540}),
            ),
            (
                "option_history_quote",
                serde_json::json!({"symbol": "SPY", "expiration": "2024-06-21", "date": "2024-06-14", "api_key": "x"}),
            ),
            (
                "stock_history_eod",
                serde_json::json!({"symbol": "SPY", "expiration": "2024-06-21", "date": "2024-06-14"}),
            ),
        ] {
            assert!(
                matches!(size_request(&client, method, &request, 10).await, Err(Stop(s)) if s == "BadWorkload"),
                "{method} {request}"
            );
        }
    }

    #[tokio::test]
    async fn sizes_counts_rows_and_drops_a_request_past_the_cap() {
        // Prices: one type per column, as the columnar decoder requires.
        let (wire_table, _) = fixture::table(fixture::Shape::Prices, 100);
        let server = fixture::Fixture::start(fixture::Script {
            messages: vec![Ok(fixture::response(&wire_table, true)); 30],
            ..Default::default()
        });
        let client = fixture_client(&server).await;
        let request =
            || typed::StockHistoryEod::new("AAPL", date("2024-06-14"), date("2024-06-14"));
        let whole = size(&client, request(), 1_000_000).await;
        assert_eq!(
            (
                whole.status.as_str(),
                whole.rows,
                whole.messages,
                whole.over_max_rows
            ),
            ("OK", 3000, 30, false)
        );
        let capped = size(&client, request(), 250).await;
        assert_eq!(
            (
                capped.status.as_str(),
                capped.rows,
                capped.messages,
                capped.over_max_rows
            ),
            ("OK", 300, 3, true)
        );
    }

    #[tokio::test]
    async fn workload_times_every_request_and_counts_rows() {
        // Option RPCs reach the stock-EOD-only fixture and are refused there;
        // every request of each workload is still sent and recorded once.
        let server = fixture::Fixture::start(fixture::Script::default());
        // No scheduler or rate limit (ADR-0021): 26 requests at once is fine.
        let client = fixture_client(&server).await;
        let args = WorkloadArgs {
            common: entitlement_args(&[], 5).common,
            workloads: "W5c,S2".into(),
            concurrency: 3,
            rounds: 2,
        };
        let mut report = Report::default();
        workload(&args, &client, &mut report).await.unwrap();
        assert_eq!(report.results.len(), 4);
        let first = &report.results[0];
        assert_eq!(
            (first["workload"].as_str(), first["round"].as_u64()),
            (Some("W5c"), Some(0))
        );
        let requests = first["requests"].as_array().unwrap();
        assert_eq!(requests.len(), 12);
        assert!(
            requests.iter().enumerate().all(|(i, r)| r["request"] == i
                && r["status"].as_str().unwrap().contains("Unimplemented"))
        );
        // Rows through query(), counted and dropped.
        let (wire_table, _) = fixture::table(fixture::Shape::Prices, 100);
        let server = fixture::Fixture::start(fixture::Script {
            messages: vec![Ok(fixture::response(&wire_table, true)); 5],
            ..Default::default()
        });
        let client = fixture_client(&server).await;
        let run = run_query(
            &client,
            typed::StockHistoryEod::new("AAPL", date("2024-06-14"), date("2024-06-14")),
            7,
        )
        .await;
        assert_eq!((run.status.as_str(), run.rows, run.request), ("OK", 500, 7));
        assert!(run.first_batch_ns.is_some());
    }

    #[test]
    fn strikes_are_dollar_text_and_the_digest_ignores_order_and_splits() {
        for (value, scale, text) in [
            (5_420_000_000_000i128, 10, "542.00"),
            (5_425_000_000_000, 10, "542.50"),
            (1_250_000_000, 10, "0.125"),
            (542, 0, "542.00"),
            (54_250, 2, "542.50"),
        ] {
            assert_eq!(strike_text(value, scale), text);
        }
        use praevis_thetadata_client::arrow_array::{
            ArrayRef, Int64Array, RecordBatch, StringArray,
        };
        let batch = |ids: &[i64], names: &[&str], flip: bool| {
            let a: ArrayRef = std::sync::Arc::new(Int64Array::from(ids.to_vec()));
            let b: ArrayRef = std::sync::Arc::new(StringArray::from(names.to_vec()));
            if flip {
                RecordBatch::try_from_iter([("b", b), ("a", a)]).unwrap()
            } else {
                RecordBatch::try_from_iter([("a", a), ("b", b)]).unwrap()
            }
        };
        let whole = row_digest(&batch(&[1, 2, 3], &["x", "y", "z"], false));
        // Split into two batches, rows reordered, columns reordered: equal.
        let split = row_digest(&batch(&[3, 1], &["z", "x"], true))
            .wrapping_add(row_digest(&batch(&[2], &["y"], false)));
        assert_eq!(whole, split);
        // A changed value, a missing row or a duplicate row: not equal.
        assert_ne!(
            whole,
            row_digest(&batch(&[1, 2, 4], &["x", "y", "z"], false))
        );
        assert_ne!(whole, row_digest(&batch(&[1, 2], &["x", "y"], false)));
        assert_ne!(
            whole,
            row_digest(&batch(&[1, 2, 3, 3], &["x", "y", "z", "z"], false))
        );
    }

    #[tokio::test]
    async fn split_probe_refuses_a_request_that_already_names_a_strike() {
        let server = fixture::Fixture::start(fixture::Script::default());
        let client = fixture_client(&server).await;
        for workload in ["S2", "W5c"] {
            let args = SplitProbeArgs {
                common: entitlement_args(&[], 5).common,
                workload: workload.into(),
                concurrency: "1,8".into(),
                no_splits: false,
            };
            let mut report = Report::default();
            assert!(matches!(
                split_probe(&args, &client, &mut report).await,
                Err(Stop(s)) if s == "NeedsOneAllStrikeRequest"
            ));
        }
        assert!(server.requests.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn option_checks_compare_values_and_only_probe_flat_files() {
        // The fixture serves stock EOD only: every option RPC is refused
        // there, which is enough to see which path each one takes.
        let server = fixture::Fixture::start(fixture::Script::default());
        // No scheduler or rate limit (ADR-0021).
        let client = fixture_client(&server).await;
        let flat_files = fixture_client_with_limit(&server, FLAT_FILE_PROBE_LIMIT).await;
        // 5 lists, 14 snapshots, 26 history and 2 at-time requests are value
        // checks; the 3 (or 4) flat files are probes.
        for (include_quote_flat_file, expected) in [(false, 50), (true, 51)] {
            let set = OptionSet {
                min_interval: 0.0,
                only: &[],
                max_messages: 5,
                day: date("2024-06-14"),
                exp: date("2024-06-21"),
                snapshot_exp: date("2026-11-06"),
                flat: date("2026-11-04"),
                include_quote_flat_file,
                check: true,
                same_response: false,
            };
            let mut report = Report::default();
            option_entitlements(&set, &client, &flat_files, &mut report)
                .await
                .unwrap();
            assert_eq!(report.results.len(), expected);
            let (checked, probed): (Vec<_>, Vec<_>) = report
                .results
                .iter()
                .partition(|r| r.get("query_status").is_some());
            // Every non-flat-file RPC is a value check; flat files are probes.
            assert_eq!(checked.len(), 47);
            assert!(probed.iter().all(|r| {
                r["rpc"].as_str().unwrap().starts_with("GetOptionFlatFile")
                    && r.get("status").is_some()
            }));
            let quote_file = probed.iter().any(|r| r["rpc"] == "GetOptionFlatFileQuote");
            assert_eq!(quote_file, include_quote_flat_file);
        }
    }

    #[tokio::test]
    async fn the_stock_index_rate_set_checks_every_wrapper_once_and_keeps_calendar_rows() {
        // Calendar RPCs answer with a small table; everything else is refused
        // with a text naming a tier, like a tier-denied endpoint.
        let (calendar, _) = fixture::table(fixture::Shape::Prices, 3);
        let handler: fixture::Handler = std::sync::Arc::new(move |rpc: &str, _| {
            if rpc.contains("Calendar") {
                Ok(vec![fixture::response(&calendar, true)])
            } else {
                Err(tonic::Status::permission_denied(
                    "synthetic: VALUE required",
                ))
            }
        });
        let server = fixture::Fixture::start(fixture::Script {
            other: Some(handler),
            ..Default::default()
        });
        let client = fixture_client(&server).await;
        let flat_files = fixture_client_with_limit(&server, FLAT_FILE_PROBE_LIMIT).await;
        // The gated stock quote flat file is only probed when asked for.
        let mut set = StockIndexRateSet {
            min_interval: 0.0,
            only: &[],
            max_messages: 5,
            day: date("2024-06-14"),
            flat: date("2026-11-04"),
            include_quote_flat_file: false,
        };
        let mut report = Report::default();
        stock_index_rate(&set, &client, &flat_files, &mut report)
            .await
            .unwrap();
        assert!(
            report
                .results
                .iter()
                .all(|r| r["rpc"] != "GetStockFlatFileQuote")
        );
        assert_eq!(report.results.len(), 29 + 3 + 1);
        set.include_quote_flat_file = true;
        let mut report = Report::default();
        stock_index_rate(&set, &client, &flat_files, &mut report)
            .await
            .unwrap();
        // Every stock, index, calendar and interest-rate RPC with a vendor
        // wrapper, and nothing else.
        let expected: std::collections::BTreeSet<&str> = typed::ALL
            .iter()
            .filter(|e| e.python_name.is_some() && !e.rpc.starts_with("GetOption"))
            .map(|e| e.rpc)
            .collect();
        assert_eq!(expected.len(), 30);
        let seen: std::collections::BTreeSet<&str> = report
            .results
            .iter()
            .map(|r| r["rpc"].as_str().unwrap())
            .collect();
        assert_eq!(seen, expected);
        // Once each, except the calendar's 4 dates and 2 years.
        assert_eq!(report.results.len(), 30 + 3 + 1);
        let count = |rpc: &str| report.results.iter().filter(|r| r["rpc"] == rpc).count();
        assert_eq!(
            (count("GetCalendarOnDate"), count("GetCalendarYear")),
            (4, 2)
        );
        for result in &report.results {
            let rpc = result["rpc"].as_str().unwrap();
            if rpc.starts_with("GetCalendar") {
                // Decoded both ways, equal, and the rows kept.
                assert_eq!(result["values_equal"], true, "{rpc}");
                assert_eq!(
                    result["calendar_rows"].as_array().unwrap().len(),
                    3,
                    "{rpc}"
                );
            } else if rpc == "GetStockHistoryEod" {
                // The fixture serves stock EOD itself: an entitled response
                // is value-checked, and its rows are not kept.
                assert_eq!(result["values_equal"], true);
                assert!(result.get("calendar_rows").is_none());
            } else {
                // A refusal keeps its text for a person to classify; no rows.
                assert_eq!(result["status"], "PermissionDenied", "{rpc}");
                assert_eq!(result["status_text"], "synthetic: VALUE required", "{rpc}");
                assert!(result.get("calendar_rows").is_none(), "{rpc}");
            }
        }
        // Repeated calendar requests are labelled.
        let labels: Vec<&str> = report
            .results
            .iter()
            .filter_map(|r| r["label"].as_str())
            .collect();
        assert_eq!(
            labels,
            [
                "2025-11-05 (weekday)",
                "2025-11-08 (weekend)",
                "2025-12-25 (holiday)",
                "2025-11-28 (early close)",
                "2025",
                "2026"
            ]
        );
        // Flat files come last.
        let last: Vec<&str> = report.results[report.results.len() - 4..]
            .iter()
            .map(|r| r["rpc"].as_str().unwrap())
            .collect();
        assert!(last.iter().all(|rpc| rpc.contains("FlatFile")), "{last:?}");
    }

    #[tokio::test]
    async fn sizes_record_the_largest_message_and_a_histogram() {
        // Three messages of different sizes: the maximum is the largest
        // encoded message, and each lands in its bucket.
        let (small, _) = fixture::table(fixture::Shape::Prices, 10);
        let (large, _) = fixture::table(fixture::Shape::Prices, 20_000);
        let messages = [
            fixture::response(&small, false),
            fixture::response(&large, false),
            fixture::response(&small, false),
        ];
        let largest = messages
            .iter()
            .map(|m| m.encoded_len() as u64)
            .max()
            .unwrap();
        let server = fixture::Fixture::start(fixture::Script {
            messages: messages.iter().cloned().map(Ok).collect(),
            ..Default::default()
        });
        let client = fixture_client(&server).await;
        let request = typed::StockHistoryEod::new("AAPL", date("2024-06-14"), date("2024-06-14"));
        let result = size(&client, request, 1_000_000).await;
        assert_eq!(result.status, "OK");
        assert_eq!(result.messages, 3);
        assert_eq!(result.max_message_bytes, largest);
        assert_eq!(result.message_size_histogram.iter().sum::<u64>(), 3);
        assert_eq!(
            result.message_size_histogram[message_size_bucket(largest)],
            1
        );
    }

    #[test]
    fn message_sizes_fall_in_the_right_bucket() {
        let bucket = message_size_bucket;
        assert_eq!(bucket(0), 0);
        assert_eq!(bucket(64 << 10), 0);
        assert_eq!(bucket((64 << 10) + 1), 1);
        assert_eq!(bucket(1 << 20), 2);
        assert_eq!(bucket(10 << 20), 4);
        assert_eq!(bucket(64 << 20), 5);
        assert_eq!(bucket((64 << 20) + 1), 6);
    }

    #[test]
    fn calendar_prices_are_exact_decimal_text() {
        assert_eq!(exact_json(&Exact::Price(12_500_000_000)), "1.25");
        assert_eq!(exact_json(&Exact::Price(-30_000_000_000)), "-3");
        assert_eq!(exact_json(&Exact::Price(1)), "0.0000000001");
        assert_eq!(exact_json(&Exact::Integer(7)), 7);
        assert_eq!(exact_json(&Exact::Null), serde_json::Value::Null);
    }

    #[tokio::test]
    async fn the_same_response_is_decoded_both_ways_and_compared() {
        let (wire_table, _) = fixture::table(fixture::Shape::Prices, 50);
        let server = fixture::Fixture::start(fixture::Script {
            messages: vec![Ok(fixture::response(&wire_table, true)); 3],
            ..Default::default()
        });
        let client = fixture_client(&server).await;
        let mut pacer = Pacer {
            interval: Duration::ZERO,
            last: None,
        };
        let request =
            || typed::StockHistoryEod::new("AAPL", date("2024-06-14"), date("2024-06-14"));
        let whole = check_same(&client, request(), &mut pacer, None).await;
        assert_eq!(
            (
                whole["status"].as_str(),
                whole["messages"].as_u64(),
                whole["rows"].as_u64(),
                whole["legacy_rows"].as_u64(),
                whole["values_equal"].as_bool(),
                whole["complete"].as_bool(),
                whole["manifest"].as_bool()
            ),
            (
                Some("OK"),
                Some(3),
                Some(150),
                Some(150),
                Some(true),
                Some(true),
                Some(false)
            )
        );
        // One request only: the comparison never sends a second one.
        assert_eq!(server.requests.lock().unwrap().len(), 1);
        // A bounded read (the flat files): the first message only.
        let first = check_same(&client, request(), &mut pacer, Some(1)).await;
        assert_eq!(
            (
                first["messages"].as_u64(),
                first["rows"].as_u64(),
                first["complete"].as_bool()
            ),
            (Some(1), Some(50), Some(false))
        );
        assert_eq!(first["values_equal"], true);
    }

    #[tokio::test]
    async fn idle_reconnect_records_both_paths_after_each_gap() {
        let (wire_table, _) = fixture::table(fixture::Shape::Prices, 3);
        let server = fixture::Fixture::start(fixture::Script {
            messages: vec![Ok(fixture::response(&wire_table, true))],
            ..Default::default()
        });
        let client = fixture_client(&server).await;
        let args = IdleReconnectArgs {
            common: entitlement_args(&[], 5).common,
            gaps: vec![0, 0],
        };
        let mut report = Report::default();
        idle_reconnect(&args, &client, &mut report).await.unwrap();
        // The first request plus one per gap.
        assert_eq!(report.results.len(), 3);
        assert!(
            report
                .results
                .iter()
                .all(|r| r["query_status"] == "OK" && r["status"] == "OK")
        );
        assert_eq!(report.results[2]["idle_s"], 0);
        // query() and the raw stub each sent every request.
        assert_eq!(server.requests.lock().unwrap().len(), 6);
    }

    #[tokio::test]
    async fn not_found_records_codes_and_compares_without_committing_text() {
        let server = fixture::Fixture::start(fixture::Script {
            initial_error: Some(tonic::Status::not_found("no data")),
            ..Default::default()
        });
        let client = fixture_client(&server).await;
        let mut report = Report::default();
        not_found(&client, &mut report, Duration::ZERO)
            .await
            .unwrap();
        assert_eq!(report.results.len(), 3);
        for case in &report.results[..2] {
            assert_eq!(case["query"], "NoData");
            assert_eq!(case["status"], "NotFound");
            assert_eq!(case["details_bytes"], 0);
            assert!(case["metadata_keys"].is_array());
        }
        assert_eq!(
            report.results[2],
            serde_json::json!({"case": "comparison", "same_code": true,
                "same_message": true, "same_details": true})
        );
        // Each case once through query() and once through the raw stub.
        assert_eq!(server.requests.lock().unwrap().len(), 4);
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
                check: false,
                same_response: false,
                cap: None,
                label: None,
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
