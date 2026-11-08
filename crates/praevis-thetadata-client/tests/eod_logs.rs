//! EOD-L3-017: dependency log/trace output must not contain credentials, session tokens,
//! account identity or remote payload/status text. Installs process-global `tracing` and
//! `log` collectors at TRACE level, so it lives in its own test binary with a single test.
#[path = "support/mod.rs"]
#[allow(dead_code)]
mod support;

use praevis_thetadata_client::{
    ClientConfig, EodError, EodPolicy, NaiveDate, StockEodRequest, ThetaClient,
};
use praevis_thetadata_proto::endpoints::{self as wire, data_value::DataType};
use std::sync::{Arc, Mutex, OnceLock};
use support::{Fixture, Script};

static CAPTURED: OnceLock<Arc<Mutex<Vec<String>>>> = OnceLock::new();
fn captured() -> Arc<Mutex<Vec<String>>> {
    CAPTURED.get_or_init(Default::default).clone()
}

/// Minimal TRACE-level `tracing` subscriber that renders every event and span field.
struct Collector;
struct Fields<'a>(&'a mut String);
impl tracing::field::Visit for Fields<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        use std::fmt::Write;
        let _ = write!(self.0, " {}={value:?}", field.name());
    }
}
impl tracing::Subscriber for Collector {
    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn max_level_hint(&self) -> Option<tracing::level_filters::LevelFilter> {
        Some(tracing::level_filters::LevelFilter::TRACE)
    }
    fn new_span(&self, span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        let mut text = format!("span {}", span.metadata().target());
        span.record(&mut Fields(&mut text));
        captured().lock().unwrap().push(text);
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _: &tracing::span::Id, values: &tracing::span::Record<'_>) {
        let mut text = String::from("record");
        values.record(&mut Fields(&mut text));
        captured().lock().unwrap().push(text);
    }
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        let mut text = format!("event {}", event.metadata().target());
        event.record(&mut Fields(&mut text));
        captured().lock().unwrap().push(text);
    }
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
}

/// `log` crate records (reqwest and others), also at TRACE.
struct Logger;
impl log::Log for Logger {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        captured()
            .lock()
            .unwrap()
            .push(format!("log {} {}", record.target(), record.args()));
    }
    fn flush(&self) {}
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dependency_logs_never_contain_secrets() {
    tracing::subscriber::set_global_default(Collector).unwrap();
    log::set_logger(&Logger).unwrap();
    log::set_max_level(log::LevelFilter::Trace);

    // Authentication (reqwest/hyper) with a synthetic API key, then typed queries: one
    // success, and one remote failure whose message and metadata carry SENTINEL text.
    let session = support::session().await;
    let table = wire::DataTable {
        headers: vec!["h".into()],
        data_table: vec![wire::DataValueList {
            values: vec![wire::DataValue {
                data_type: Some(DataType::Number(1)),
            }],
        }],
    };
    let mut status = tonic::Status::unavailable("SENTINEL remote message");
    status
        .metadata_mut()
        .insert("secret", "SENTINEL".parse().unwrap());
    let fixture = Fixture::start(Script {
        messages: vec![Ok(support::response(&table, true)), Err(status)],
        ..Default::default()
    });
    let client = ThetaClient::with_eod_policy(
        ClientConfig::default()
            .with_endpoint(Some(fixture.endpoint.clone()))
            .with_allow_insecure(true),
        session,
        EodPolicy::default(),
    )
    .await
    .unwrap();
    let day = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
    let mut stream = client
        .stock_eod_batches(StockEodRequest::new("SYNTHETIC", day, day).unwrap())
        .await
        .unwrap();
    assert!(stream.next_batch().await.unwrap().is_some());
    assert_eq!(
        stream.next_batch().await.unwrap_err(),
        EodError::Remote(tonic::Code::Unavailable)
    );
    drop(stream);
    drop(client);
    drop(fixture);

    let lines = captured().lock().unwrap().clone();
    // Non-vacuous: the dependencies really emitted trace output during these calls.
    assert!(lines.len() > 20, "too little captured output: {lines:?}");
    // ...including hyper connection events (hosts/ports) and h2 frame/HPACK decoding,
    // which processes the server trailer carrying the `secret: SENTINEL` header.
    assert!(
        lines
            .iter()
            .any(|line| line.contains("hyper_util") && line.contains("127.0.0.1"))
    );
    assert!(lines.iter().any(|line| line.contains("h2::hpack::decoder")));
    for secret in [support::TOKEN, support::EMAIL, "synthetic-key", "SENTINEL"] {
        let leaks: Vec<_> = lines.iter().filter(|line| line.contains(secret)).collect();
        assert!(
            leaks.is_empty(),
            "{secret} appeared in dependency output: {leaks:?}"
        );
    }
    eprintln!(
        "captured {} dependency log/trace lines without secrets",
        lines.len()
    );
}
