//! The login-conflict probe (W10, approved by the user on 2026-11-05).
//! Manual only: `--features live-tests`, `--confirm-live`, `--api-key-env`,
//! with the operator's Terminal already running. Never CI.
//!
//! The account keeps one historical session at a time: a new login ends the
//! earlier one (`opt-session-001`). The Terminal logs in with the same
//! account. This probe finds out what our login does to the Terminal's
//! stream and its own historical session. Steps:
//! 1. One Terminal REST request (a small list); record the HTTP status only.
//! 2. Subscribe to quotes for one contract, and record quotes, heartbeats
//!    and heartbeat states in 10 s buckets for `--before-s`.
//! 3. Log our gRPC client in and send one small request (stock EOD, one
//!    day); record its status.
//! 4. Keep recording for `--after-s`, then repeat the REST request and send
//!    a second gRPC request.
//!
//! Counts, codes and states only: never a value, message text, email or
//! session id (the report is refused if it would contain the last two).
use clap::Parser;
use praevis_thetadata_client::endpoints::StockHistoryEod;
use praevis_thetadata_client::{
    AuthClient, AuthConfig, ClientConfig, Credentials, Environment, NaiveDate, ThetaClient,
};
use praevis_thetadata_stream::{Contract, Kind, Options, Response, Right, Root, Status, Stream};
use std::io::{Read, Write};
use std::time::{Duration, Instant};

#[derive(Parser)]
struct Cli {
    #[arg(long, required = true)]
    confirm_live: bool,
    /// Read THETADATA_API_KEY for our login (not needed with --no-login).
    #[arg(long, required_unless_present = "no_login")]
    api_key_env: bool,
    #[arg(long)]
    run_id: String,
    /// A liquid contract listed today, so quotes flow during market hours.
    #[arg(long, default_value = "SPY")]
    root: String,
    /// `yyyymmdd`.
    #[arg(long)]
    expiration: u32,
    /// Dollars, for example 570 (sent in tenths of a cent).
    #[arg(long)]
    strike: f64,
    #[arg(long, default_value = "C", value_parser = ["C", "P"])]
    right: String,
    #[arg(long, default_value_t = 60)]
    before_s: u64,
    #[arg(long, default_value_t = 120)]
    after_s: u64,
    /// The Terminal's REST address and a small request on it.
    #[arg(long, default_value = "127.0.0.1:25503")]
    terminal_rest: String,
    #[arg(long, default_value = "/v3/option/list/expirations?symbol=SPY")]
    rest_path: String,
    /// Skip our gRPC login: watch the subscription alone (time to the
    /// first quote, and every non-quote message with its time).
    #[arg(long)]
    no_login: bool,
}

/// Non-quote messages and the first quote, with times since the start.
#[derive(Default)]
struct Log {
    messages: Vec<serde_json::Value>,
    first_quote_ms: Option<u128>,
}

/// The HTTP status code of one GET, or the I/O error kind. Reads only the
/// status line; the body is never read or kept.
fn rest_status(address: &str, path: &str) -> String {
    let attempt = || -> std::io::Result<String> {
        let mut tcp = std::net::TcpStream::connect(address)?;
        tcp.set_read_timeout(Some(Duration::from_secs(60)))?;
        write!(
            tcp,
            "GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
        )?;
        let mut head = [0u8; 32];
        let n = tcp.read(&mut head)?;
        let line = String::from_utf8_lossy(&head[..n]);
        Ok(line
            .split_whitespace()
            .nth(1)
            .unwrap_or("no-status")
            .to_owned())
    };
    attempt().unwrap_or_else(|e| format!("{:?}", e.kind()))
}

/// One 10 s window of the stream.
#[derive(Default)]
struct Bucket {
    quotes: u64,
    heartbeats: u64,
    connected: u64,
    disconnected: u64,
    other_status: u64,
    others: u64,
}

/// Drain the stream for `length`, in 10 s buckets. Returns true if the
/// stream ended (a disconnect).
fn watch(
    stream: &Stream,
    length: Duration,
    start: Instant,
    label: &str,
    out: &mut Vec<serde_json::Value>,
    log: &mut Log,
) -> bool {
    let end = Instant::now() + length;
    while Instant::now() < end {
        let bucket_end = (Instant::now() + Duration::from_secs(10)).min(end);
        let mut bucket = Bucket::default();
        let mut ended = false;
        while Instant::now() < bucket_end {
            let Some(event) =
                stream.recv_timeout(bucket_end.saturating_duration_since(Instant::now()))
            else {
                continue;
            };
            match event.kind {
                Kind::Quote { .. } => {
                    bucket.quotes += 1;
                    log.first_quote_ms
                        .get_or_insert_with(|| event.received.duration_since(start).as_millis());
                }
                Kind::Heartbeat { status } => {
                    bucket.heartbeats += 1;
                    match status {
                        Status::Connected => bucket.connected += 1,
                        Status::Disconnected => bucket.disconnected += 1,
                        Status::Other => bucket.other_status += 1,
                    }
                }
                Kind::Response { id, response } => {
                    bucket.others += 1;
                    let code = match response {
                        Response::Subscribed => "SUBSCRIBED",
                        Response::MaxStreamsReached => "MAX_STREAMS_REACHED",
                        Response::InvalidPerms => "INVALID_PERMS",
                        Response::Error => "ERROR",
                        Response::Other => "OTHER",
                    };
                    log.messages.push(serde_json::json!({
                        "t_ms": event.received.duration_since(start).as_millis(),
                        "type": "REQ_RESPONSE", "req_id": id, "response": code,
                    }));
                }
                // Trades only arrive on the full trade stream, which this
                // probe does not subscribe to.
                Kind::Trade { .. } => bucket.others += 1,
                Kind::Other { kind, .. } => {
                    bucket.others += 1;
                    log.messages.push(serde_json::json!({
                        "t_ms": event.received.duration_since(start).as_millis(),
                        "type": kind.as_str(),
                    }));
                }
                Kind::Disconnected => {
                    ended = true;
                    break;
                }
            }
        }
        out.push(serde_json::json!({
            "phase": label,
            "t_s": start.elapsed().as_secs(),
            "quotes": bucket.quotes,
            "heartbeats": bucket.heartbeats,
            "heartbeat_connected": bucket.connected,
            "heartbeat_disconnected": bucket.disconnected,
            "heartbeat_other": bucket.other_status,
            "other_messages": bucket.others,
            "stream_ended": ended,
        }));
        eprintln!(
            "{label:<6} t={:>4}s quotes {:>6} heartbeats {:>3} (connected {}, disconnected {}){}",
            start.elapsed().as_secs(),
            bucket.quotes,
            bucket.heartbeats,
            bucket.connected,
            bucket.disconnected,
            if ended { " STREAM ENDED" } else { "" }
        );
        if ended {
            return true;
        }
    }
    false
}

async fn grpc_request(client: &ThetaClient) -> String {
    let day = NaiveDate::from_ymd_opt(2024, 6, 14).expect("date");
    match client.query(&StockHistoryEod::new("AAPL", day, day)).await {
        Ok(stream) => match stream.collect().await {
            Ok(_) => "OK".into(),
            Err(error) => format!("{error:?}"),
        },
        Err(error) => format!("{error:?}"),
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    if !cli.confirm_live || !(cli.api_key_env || cli.no_login) || std::env::var_os("CI").is_some() {
        eprintln!("live execution requires --confirm-live and --api-key-env, outside CI");
        return std::process::ExitCode::FAILURE;
    }
    if !cli
        .run_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        || cli.run_id.is_empty()
    {
        eprintln!("run id must be ASCII letters, digits, '-' or '_'");
        return std::process::ExitCode::FAILURE;
    }
    let report_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../artifacts/w10")
        .join(format!("{}.json", cli.run_id));
    if report_path.exists() {
        eprintln!("run id already used");
        return std::process::ExitCode::FAILURE;
    }
    let contract = Contract {
        root: match Root::new(&cli.root) {
            Some(root) => root,
            None => {
                eprintln!("root: 1-8 ASCII letters or digits");
                return std::process::ExitCode::FAILURE;
            }
        },
        expiration: cli.expiration,
        strike: (cli.strike * 1000.0).round() as i64,
        right: if cli.right == "C" {
            Right::Call
        } else {
            Right::Put
        },
    };
    let start = Instant::now();
    let mut timeline = Vec::new();
    let mut log = Log::default();
    let rest_before = rest_status(&cli.terminal_rest, &cli.rest_path);
    eprintln!("Terminal REST before: {rest_before}");
    let stream = match Stream::connect(Options::default()) {
        Ok(stream) => stream,
        Err(error) => {
            eprintln!("stream: {error}");
            return std::process::ExitCode::FAILURE;
        }
    };
    if let Err(error) = stream.subscribe_quotes(&contract) {
        eprintln!("subscribe: {error}");
        return std::process::ExitCode::FAILURE;
    }
    let ended_before = watch(
        &stream,
        Duration::from_secs(cli.before_s),
        start,
        "before",
        &mut timeline,
        &mut log,
    );
    // Our login: the step under test (skipped with --no-login).
    let login_at_s = start.elapsed().as_secs();
    let key = if cli.no_login {
        String::new()
    } else {
        std::env::var("THETADATA_API_KEY").unwrap_or_default()
    };
    let session = match AuthClient::new(AuthConfig {
        environment: Environment::Prod,
        ..Default::default()
    }) {
        Ok(_) if cli.no_login => None,
        Ok(auth) => auth
            .authenticate(&Credentials::api_key(key.trim()))
            .await
            .ok(),
        Err(_) => None,
    };
    drop(key);
    let (identity, grpc_first, client) = match session {
        Some(session) => {
            let identity = [
                session.session_id().to_owned(),
                session.user().email.clone(),
            ];
            match ThetaClient::with_session(ClientConfig::default(), session).await {
                Ok(client) => {
                    let status = grpc_request(&client).await;
                    (identity, status, Some(client))
                }
                Err(_) => (identity, "ConnectionFailed".into(), None),
            }
        }
        None if cli.no_login => ([String::new(), String::new()], "NotRun".into(), None),
        None => ([String::new(), String::new()], "LoginFailed".into(), None),
    };
    eprintln!("our login at {login_at_s}s; first gRPC request: {grpc_first}");
    let ended_after = !ended_before
        && watch(
            &stream,
            Duration::from_secs(cli.after_s),
            start,
            "after",
            &mut timeline,
            &mut log,
        );
    let rest_after = rest_status(&cli.terminal_rest, &cli.rest_path);
    let grpc_second = match &client {
        Some(client) => grpc_request(client).await,
        None => "NotRun".into(),
    };
    eprintln!("Terminal REST after: {rest_after}; second gRPC request: {grpc_second}");
    let stats = stream.close();
    let report = serde_json::json!({
        "run_id": cli.run_id,
        "command": "login-conflict-probe",
        "contract": {"root": contract.root.as_str(), "expiration": contract.expiration, "strike": contract.strike, "right": cli.right},
        "terminal_rest_before": rest_before,
        "terminal_rest_after": rest_after,
        "our_login_at_s": login_at_s,
        "grpc_first": grpc_first,
        "grpc_second": grpc_second,
        "stream_ended_before_login": ended_before,
        "stream_ended_after_login": ended_after,
        "timeline": timeline,
        "no_login": cli.no_login,
        "first_quote_ms": log.first_quote_ms,
        "messages": log.messages,
        "stats": {
            "frames": stats.frames, "quotes": stats.quotes, "heartbeats": stats.heartbeats,
            "others": stats.others, "unparsed": stats.unparsed, "dropped": stats.dropped,
            "heartbeat_gaps": stats.heartbeat_gaps,
        },
    });
    let text = serde_json::to_string_pretty(&report).expect("report");
    if identity
        .iter()
        .any(|v| !v.is_empty() && text.contains(v.as_str()))
    {
        eprintln!("Refusing to write the report: it would contain account identity.");
        return std::process::ExitCode::FAILURE;
    }
    std::fs::create_dir_all(report_path.parent().expect("parent")).expect("artifacts directory");
    std::fs::write(&report_path, text).expect("write report");
    println!("Wrote {}", report_path.display());
    std::process::ExitCode::SUCCESS
}
