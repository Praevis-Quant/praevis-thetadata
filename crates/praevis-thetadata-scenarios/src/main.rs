//! End-to-end scenarios against `--target emulator` or `--target live`
//! (ADR-0022). They assert properties, never exact values: rows arrive,
//! schemas equal the registry, errors have the expected kind, streams do
//! not drop.
//!
//! ```text
//! scenarios --target emulator                     # CI, every pull request
//! scenarios --target live --confirm-live --part anytime|market|all --run-id ID
//! ```
//!
//! **Live runs are manual or Coordinator-run, never CI,** under the approval
//! rules in AGENTS.md. The key comes from `THETADATA_API_KEY` only and is
//! never printed. The live target never sends more requests at once than
//! the tier allows, never logs in twice, and never touches a gated flat
//! file: those scenarios run against the emulator only.
//!
//! The report (`--report`, default under the ignored `artifacts/`) holds
//! scenario names, outcomes, gRPC codes, column kinds and timings: never
//! values. The drift check (ADR-0022 part 3) reads it.
use clap::{Parser, ValueEnum};
use praevis_thetadata_client::arrow_schema::{DataType, Schema};
use praevis_thetadata_client::{
    AuthClient, AuthConfig, ClientConfig, Credentials, NaiveDate, QueryError, RequestOptions,
    ThetaClient, endpoints,
};
use praevis_thetadata_emulator::{Config, Emulator, schema};
use praevis_thetadata_stream::{Kind, Options, Stream};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Target {
    Emulator,
    Live,
}

/// `anytime`: historical requests. `market`: the stream (live market
/// hours, or the Terminal's dev replay feed off-hours).
#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Part {
    Anytime,
    Market,
    All,
}

#[derive(Parser)]
struct Cli {
    #[arg(long, value_enum)]
    target: Target,
    #[arg(long, value_enum, default_value = "all")]
    part: Part,
    /// Required for `--target live`.
    #[arg(long)]
    confirm_live: bool,
    #[arg(long, default_value = "scenarios")]
    run_id: String,
    /// The report; default `artifacts/scenarios/<run-id>.json`.
    #[arg(long)]
    report: Option<PathBuf>,
    /// Live only: the Terminal's WebSocket, default the documented one.
    #[arg(long)]
    terminal_url: Option<String>,
    /// Only check whether US equity markets are open now, from the
    /// service's own `calendar_open_today`: exit 0 if open, 3 if closed,
    /// 4 if it cannot tell. Runs no scenario and writes no report.
    #[arg(long)]
    market_open_check: bool,
}

/// Whether the market is open now: today's `open` and `close` times are
/// both present and New York's time is between them. A closed day has
/// null times (or no data at all); the `type` text is not relied on.
async fn market_open(client: &ThetaClient) -> Result<bool, String> {
    use praevis_thetadata_client::arrow_array::{Array, StringArray};
    let batches = match client.query(&endpoints::CalendarOpenToday::new()).await {
        Ok(stream) => stream.collect().await,
        Err(e) => Err(e),
    };
    let batches = match batches {
        Ok(batches) => batches,
        Err(QueryError::NoData) => return Ok(false),
        Err(e) => return Err(format!("calendar_open_today failed: {e:?}")),
    };
    let batch = batches.first().ok_or("calendar_open_today: no rows")?;
    let column = |name: &str| -> Result<Option<chrono::NaiveTime>, String> {
        let index = batch
            .schema()
            .index_of(name)
            .map_err(|_| format!("no {name} column"))?;
        let values = batch
            .column(index)
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or(format!("{name} is not text"))?;
        if values.is_null(0) {
            return Ok(None);
        }
        chrono::NaiveTime::parse_from_str(values.value(0), "%H:%M:%S")
            .map(Some)
            .map_err(|_| format!("{name} is not a time"))
    };
    let (Some(open), Some(close)) = (column("open")?, column("close")?) else {
        return Ok(false);
    };
    let now = chrono::Utc::now()
        .with_timezone(&chrono_tz::America::New_York)
        .time();
    Ok(open <= now && now < close)
}

/// One scenario's result, for the report.
struct Outcome {
    name: &'static str,
    status: &'static str,
    detail: String,
    observed: Value,
}

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).expect("date")
}

/// The registry's kind for an Arrow column type.
fn kind(data_type: &DataType) -> String {
    match data_type {
        DataType::Utf8 | DataType::LargeUtf8 => "text".into(),
        DataType::Int64 => "number".into(),
        DataType::Decimal128(..) => "price".into(),
        DataType::Boolean => "boolean".into(),
        DataType::Timestamp(_, Some(zone)) if zone.as_ref() == "America/New_York" => {
            "timestamp-new-york".into()
        }
        DataType::Timestamp(_, Some(zone)) if zone.as_ref() == "UTC" => "timestamp-utc".into(),
        other => format!("{other:?}"),
    }
}

fn columns(schema: &Schema) -> Vec<(String, String)> {
    schema
        .fields()
        .iter()
        .map(|f| (f.name().clone(), kind(f.data_type())))
        .collect()
}

/// Rows and columns of a query, checked against the registry.
async fn rows_matching<E: endpoints::Endpoint>(
    client: &ThetaClient,
    rpc: &str,
    request: &E,
) -> Result<(usize, Value), String> {
    let started = Instant::now();
    let batches = client
        .query(request)
        .await
        .map_err(|e| format!("{e:?}"))?
        .collect()
        .await
        .map_err(|e| format!("{e:?}"))?;
    let rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    let seen = batches
        .first()
        .map(|b| columns(&b.schema()))
        .unwrap_or_default();
    let observed =
        json!({"rpc": rpc, "code": "OK", "columns": seen, "ms": started.elapsed().as_millis()});
    if rows == 0 {
        return Err("no rows".into());
    }
    if Some(&seen) != schema(rpc).as_ref() {
        return Err(format!("columns differ from schemas.json: {seen:?}"));
    }
    Ok((rows, observed))
}

fn code_of(error: &QueryError) -> String {
    match error {
        QueryError::Remote(code) => format!("{code:?}"),
        other => format!("{other:?}"),
    }
}

/// Runs one scenario and records its outcome.
async fn run(
    out: &mut Vec<Outcome>,
    name: &'static str,
    scenario: impl Future<Output = Result<(String, Value), String>>,
) {
    let (status, detail, observed) = match scenario.await {
        Ok((detail, observed)) => ("pass", detail, observed),
        Err(detail) => ("fail", detail, Value::Null),
    };
    eprintln!("{status:4} {name}: {detail}");
    out.push(Outcome {
        name,
        status,
        detail,
        observed,
    });
}

fn skip(out: &mut Vec<Outcome>, name: &'static str, why: &str) {
    eprintln!("skip {name}: {why}");
    out.push(Outcome {
        name,
        status: "skip",
        detail: why.into(),
        observed: Value::Null,
    });
}

async fn login(auth_url: Option<&str>) -> Result<praevis_thetadata_client::Session, String> {
    let (config, credentials) = match auth_url {
        Some(url) => (
            AuthConfig::default()
                .with_auth_url(url.to_owned())
                .with_allow_insecure(true),
            Credentials::api_key("emulator-key"),
        ),
        None => {
            // The live key comes from THETADATA_API_KEY only; never printed.
            let key = std::env::var("THETADATA_API_KEY")
                .map_err(|_| "THETADATA_API_KEY is not set".to_owned())?;
            (AuthConfig::default(), Credentials::api_key(key))
        }
    };
    AuthClient::new(config)
        .map_err(|e| format!("{e:?}"))?
        .authenticate(&credentials)
        .await
        .map_err(|e| format!("login failed: {e:?}"))
}

async fn anytime(out: &mut Vec<Outcome>, client: &ThetaClient, emulator: Option<&Emulator>) {
    let week = (day(2024, 6, 3), day(2024, 6, 7));
    run(out, "login-tier", async {
        let tier = client.tier();
        Ok((
            format!(
                "{} (default concurrency {})",
                tier.name(),
                tier.concurrency()
            ),
            json!({"tier": tier.name(), "default_concurrency": tier.concurrency()}),
        ))
    })
    .await;
    run(out, "query-stock-eod", async {
        let request = endpoints::StockHistoryEod::new("AAPL", week.0, week.1);
        let (rows, observed) = rows_matching(client, "GetStockHistoryEod", &request).await?;
        Ok((format!("{rows} rows, schema as registered"), observed))
    })
    .await;
    run(out, "job-option-eod", async {
        let request = endpoints::OptionHistoryEod::new(week.0, week.1, "SPY", "*");
        let mut options = RequestOptions::default();
        options.concurrency = Some(2);
        let started = Instant::now();
        let batches = client
            .job_with(&request, options)
            .await
            .map_err(|e| format!("{e:?}"))?
            .collect()
            .await
            .map_err(|e| format!("{e:?}"))?;
        let rows: usize = batches.iter().map(|b| b.num_rows()).sum();
        let seen = batches.first().map(|b| columns(&b.schema())).unwrap_or_default();
        if rows == 0 {
            return Err("no rows".into());
        }
        if Some(&seen) != schema("GetOptionHistoryEod").as_ref() {
            return Err(format!("columns differ from schemas.json: {seen:?}"));
        }
        Ok((
            format!("{rows} rows from a job at concurrency 2"),
            json!({"rpc": "GetOptionHistoryEod", "code": "OK", "columns": seen, "ms": started.elapsed().as_millis()}),
        ))
    })
    .await;
    run(out, "job-to-parquet", async {
        let request = endpoints::StockHistoryEod::new("AAPL", week.0, week.1);
        let path =
            std::env::temp_dir().join(format!("praevis-scenario-{}.parquet", std::process::id()));
        let summary = client
            .job(&request)
            .await
            .map_err(|e| format!("{e:?}"))?
            .to_parquet(&path, Default::default())
            .await
            .map_err(|e| format!("{e:?}"));
        let _ = std::fs::remove_file(&path);
        let summary = summary?;
        if summary.rows == 0 {
            return Err("no rows written".into());
        }
        Ok((
            format!("{} rows, {} row groups", summary.rows, summary.row_groups),
            json!({"rows_written": summary.rows > 0}),
        ))
    })
    .await;
    run(out, "tier-refusal", async {
        // Stocks FREE on both targets: a snapshot names a higher tier.
        match client
            .query(&endpoints::StockSnapshotOhlc::new("AAPL"))
            .await
        {
            Err(e) if code_of(&e) == "PermissionDenied" => Ok((
                "PermissionDenied".into(),
                json!({"rpc": "GetStockSnapshotOhlc", "code": "PermissionDenied"}),
            )),
            Err(e) => Err(format!("expected PermissionDenied, got {}", code_of(&e))),
            Ok(_) => Err("expected a refusal, got rows".into()),
        }
    })
    .await;
    run(out, "not-found-is-identical", async {
        let unknown = endpoints::StockHistoryEod::new("NOSUCHSYMBOLX", week.0, week.1);
        // 2024-06-08 and 09: a Saturday and a Sunday.
        let weekend = endpoints::StockHistoryEod::new("AAPL", day(2024, 6, 8), day(2024, 6, 9));
        let a = client.query(&unknown).await.err();
        let b = client.query(&weekend).await.err();
        match (&a, &b) {
            (Some(QueryError::NoData), Some(QueryError::NoData)) => Ok((
                "both NoData".into(),
                json!({"rpc": "GetStockHistoryEod", "code": "NotFound"}),
            )),
            _ => Err(format!("expected NoData twice, got {a:?} and {b:?}")),
        }
    })
    .await;
    run(out, "cancel-midway", async {
        // Two years of EOD: the planner splits it; read one batch, drop.
        let request = endpoints::StockHistoryEod::new("AAPL", day(2022, 1, 3), day(2023, 12, 29));
        let mut job = client.job(&request).await.map_err(|e| format!("{e:?}"))?;
        let parts = job.parts();
        let first = job
            .next_batch()
            .await
            .map_err(|e| format!("{e:?}"))?
            .map_or(0, |b| b.num_rows());
        job.cancel();
        drop(job);
        if first == 0 {
            return Err("no first batch".into());
        }
        Ok((
            format!("{first} rows of the first of {parts} parts, then cancelled"),
            json!({"parts": parts}),
        ))
    })
    .await;
    let Some(emulator) = emulator else {
        skip(
            out,
            "busy-above-limit",
            "emulator only: no over-limit requests live",
        );
        skip(
            out,
            "second-login-ends-session",
            "emulator only: never log in twice live",
        );
        return;
    };
    run(out, "busy-above-limit", async {
        // Hold the account's limit with an all-strike quote each (the
        // recorded first-row delay), then one more is busy.
        let limit = client.tier().concurrency();
        let held: Vec<_> = (0..limit)
            .map(|_| {
                let client = client.clone();
                tokio::spawn(async move {
                    let all = endpoints::OptionHistoryQuote::new("SPY", day(2024, 6, 21));
                    let _ = client.query(&all).await;
                })
            })
            .collect();
        let before = emulator.requests().len();
        let deadline = Instant::now() + Duration::from_secs(5);
        while emulator.requests().len() < before + limit && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let extra = client
            .query(&endpoints::StockHistoryEod::new("AAPL", week.0, week.1))
            .await
            .err();
        for task in held {
            task.abort();
        }
        match extra {
            Some(QueryError::Busy) => Ok((
                format!("busy above {limit}"),
                json!({"code": "ResourceExhausted"}),
            )),
            other => Err(format!("expected Busy, got {other:?}")),
        }
    })
    .await;
    run(out, "second-login-ends-session", async {
        let other = login(Some(&emulator.auth_url)).await?;
        let _ = other;
        match client
            .query(&endpoints::StockHistoryEod::new("AAPL", week.0, week.1))
            .await
        {
            Err(QueryError::SessionEnded) => {
                Ok(("SessionEnded".into(), json!({"code": "Unauthenticated"})))
            }
            other => Err(format!("expected SessionEnded, got {:?}", other.err())),
        }
    })
    .await;
}

fn market(out: &mut Vec<Outcome>, url: &str) {
    let started = Instant::now();
    let result = (|| -> Result<(String, Value), String> {
        let stream = Stream::connect(Options::default().with_url(url.to_owned()))
            .map_err(|e| format!("{e:?}"))?;
        stream
            .subscribe_full_trades()
            .map_err(|e| format!("{e:?}"))?;
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut first_trade_ms = None;
        while Instant::now() < deadline {
            if let Some(event) = stream.recv_timeout(Duration::from_millis(100))
                && matches!(event.kind, Kind::Trade { .. })
            {
                first_trade_ms = Some(started.elapsed().as_millis());
                break;
            }
        }
        stream
            .unsubscribe_full_trades()
            .map_err(|e| format!("{e:?}"))?;
        let stats = stream.close();
        let Some(first) = first_trade_ms else {
            return Err("no trade within 60 s".into());
        };
        if stats.dropped > 0 || stats.unparsed > 0 {
            return Err(format!(
                "{} dropped, {} unparsed",
                stats.dropped, stats.unparsed
            ));
        }
        Ok((
            format!("first trade after {first} ms; 0 dropped, 0 unparsed"),
            json!({"first_trade_ms": first, "heartbeats": stats.heartbeats}),
        ))
    })();
    let (status, detail, observed) = match result {
        Ok((detail, observed)) => ("pass", detail, observed),
        Err(detail) => ("fail", detail, Value::Null),
    };
    eprintln!("{status:4} stream-full-trades: {detail}");
    out.push(Outcome {
        name: "stream-full-trades",
        status,
        detail,
        observed,
    });
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    if cli.target == Target::Live && !cli.confirm_live {
        eprintln!(
            "--target live needs --confirm-live (manual runs only; see docs/live-runbook.md)"
        );
        return std::process::ExitCode::FAILURE;
    }
    let emulator = (cli.target == Target::Emulator).then(|| {
        Emulator::start(Config {
            // The recorded delays in hundredths, so CI runs in seconds.
            time_scale: 0.01,
            ..Config::default()
        })
    });
    if cli.market_open_check {
        let session = match login(emulator.as_ref().map(|e| e.auth_url.as_str())).await {
            Ok(session) => session,
            Err(e) => {
                eprintln!("{e}");
                return std::process::ExitCode::from(4);
            }
        };
        let mut config = ClientConfig::default();
        if let Some(emulator) = &emulator {
            config = config
                .with_endpoint(Some(emulator.endpoint.clone()))
                .with_allow_insecure(true);
        }
        let client = match ThetaClient::with_session(config, session).await {
            Ok(client) => client,
            Err(e) => {
                eprintln!("connect failed: {e:?}");
                return std::process::ExitCode::from(4);
            }
        };
        return match market_open(&client).await {
            Ok(true) => {
                eprintln!("market open now (calendar_open_today)");
                std::process::ExitCode::SUCCESS
            }
            Ok(false) => {
                eprintln!("market closed now (calendar_open_today)");
                std::process::ExitCode::from(3)
            }
            Err(e) => {
                eprintln!("cannot tell whether the market is open: {e}");
                std::process::ExitCode::from(4)
            }
        };
    }
    let mut out = Vec::new();
    if cli.part != Part::Market {
        let session = match login(emulator.as_ref().map(|e| e.auth_url.as_str())).await {
            Ok(session) => session,
            Err(e) => {
                eprintln!("{e}");
                return std::process::ExitCode::FAILURE;
            }
        };
        let mut config = ClientConfig::default();
        if let Some(emulator) = &emulator {
            config = config
                .with_endpoint(Some(emulator.endpoint.clone()))
                .with_allow_insecure(true);
        }
        let client = match ThetaClient::with_session(config, session).await {
            Ok(client) => client,
            Err(e) => {
                eprintln!("connect failed: {e:?}");
                return std::process::ExitCode::FAILURE;
            }
        };
        anytime(&mut out, &client, emulator.as_ref()).await;
    }
    if cli.part != Part::Anytime {
        let url = match &emulator {
            Some(emulator) => emulator.terminal.url.clone(),
            None => cli
                .terminal_url
                .clone()
                .unwrap_or_else(|| praevis_thetadata_stream::DEFAULT_URL.to_owned()),
        };
        tokio::task::block_in_place(|| market(&mut out, &url));
    }
    let failed = out.iter().filter(|o| o.status == "fail").count();
    let report = json!({
        "run_id": cli.run_id,
        "target": if emulator.is_some() { "emulator" } else { "live" },
        "scenarios": out.iter().map(|o| json!({
            "name": o.name, "status": o.status, "detail": o.detail, "observed": o.observed,
        })).collect::<Vec<_>>(),
    });
    let path = cli
        .report
        .unwrap_or_else(|| PathBuf::from(format!("artifacts/scenarios/{}.json", cli.run_id)));
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&path, serde_json::to_string_pretty(&report).expect("json")) {
        eprintln!("report not written: {e}");
        return std::process::ExitCode::FAILURE;
    }
    eprintln!(
        "{} scenarios, {failed} failed; report {}",
        out.len(),
        path.display()
    );
    if failed == 0 {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use praevis_thetadata_emulator::fixture::{self, Fixture, Script};
    use praevis_thetadata_proto::endpoints::{
        DataTable, DataValue, DataValueList, data_value::DataType as Wire,
    };

    /// A calendar answer: `open` and `close` as text, or null.
    fn calendar(times: Option<(&'static str, &'static str)>) -> Fixture {
        let handler: fixture::Handler = std::sync::Arc::new(move |rpc: &str, _| {
            assert_eq!(rpc, "GetCalendarOpenToday");
            let text = |v: &str| DataValue {
                data_type: Some(Wire::Text(v.into())),
            };
            let null = DataValue {
                data_type: Some(Wire::NullValue(0)),
            };
            let values = match times {
                Some((open, close)) => vec![text("open"), text(open), text(close)],
                None => vec![text("full_close"), null.clone(), null],
            };
            let table = DataTable {
                headers: vec!["type".into(), "open".into(), "close".into()],
                data_table: vec![DataValueList { values }],
            };
            Ok(vec![fixture::response(&table, false)])
        });
        Fixture::start(Script {
            other: Some(handler),
            ..Default::default()
        })
    }

    async fn client(fixture: &Fixture) -> ThetaClient {
        ThetaClient::with_session(
            ClientConfig::default()
                .with_endpoint(Some(fixture.endpoint.clone()))
                .with_allow_insecure(true),
            fixture::session().await,
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn open_only_between_todays_open_and_close() {
        let all_day = calendar(Some(("00:00:00", "23:59:59")));
        assert_eq!(market_open(&client(&all_day).await).await, Ok(true));
        let never = calendar(Some(("00:00:00", "00:00:00")));
        assert_eq!(market_open(&client(&never).await).await, Ok(false));
        let closed = calendar(None);
        assert_eq!(market_open(&client(&closed).await).await, Ok(false));
    }

    #[tokio::test]
    async fn no_data_is_closed() {
        let handler: fixture::Handler =
            std::sync::Arc::new(|_: &str, _| Err(tonic::Status::not_found("no data")));
        let fixture = Fixture::start(Script {
            other: Some(handler),
            ..Default::default()
        });
        assert_eq!(market_open(&client(&fixture).await).await, Ok(false));
    }
}
