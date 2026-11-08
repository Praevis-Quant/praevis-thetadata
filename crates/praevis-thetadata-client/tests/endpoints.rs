//! Generated typed requests (P1 PR 1): registry, vendor-exact wire queries and
//! raw dispatch. Expected queries follow the 1.0.17 catalogue's per-wrapper
//! assignment tables (docs/research/python-1.0.17-catalog.md).
#[allow(dead_code)]
mod support;

use chrono::{NaiveDate, NaiveTime};
use praevis_thetadata_client::endpoints::{self, ALL, Endpoint, Stability, TimeOfDay};
use praevis_thetadata_client::{ClientConfig, ThetaClient, queries as api};
use praevis_thetadata_proto::endpoints as wire;
use std::collections::BTreeSet;
use support::{Fixture, Script};

fn date(text: &str) -> NaiveDate {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
}

#[test]
fn registry_covers_every_descriptor_rpc_once() {
    assert_eq!(ALL.len(), 100);
    let rpcs: BTreeSet<_> = ALL.iter().map(|e| e.rpc).collect();
    assert_eq!(rpcs.len(), 100, "duplicate RPC");
    let wrappers: Vec<_> = ALL.iter().filter_map(|e| e.python_name).collect();
    assert_eq!(wrappers.len(), 81);
    assert_eq!(
        wrappers.iter().collect::<BTreeSet<_>>().len(),
        81,
        "duplicate wrapper"
    );
    let unstable: Vec<_> = ALL
        .iter()
        .filter(|e| e.stability == Stability::Unstable)
        .collect();
    assert_eq!(unstable.len(), 19);
    assert!(unstable.iter().all(|e| e.python_name.is_none()
        && (e.rpc.starts_with("GetFuture") || e.rpc.starts_with("GetCorporateAction"))));
}

#[test]
fn stock_eod_formats_dates_like_the_wrapper() {
    let request = endpoints::StockHistoryEod::new("AAPL", date("2024-01-02"), date("2024-12-31"));
    assert_eq!(
        request.to_query(),
        api::StockHistoryEodRequestQuery {
            symbol: "AAPL".into(),
            start_date: "2024-01-02".into(),
            end_date: "2024-12-31".into(),
        }
    );
    assert_eq!(endpoints::StockHistoryEod::RPC, "GetStockHistoryEod");
    assert_eq!(
        endpoints::StockHistoryEod::PYTHON_NAME,
        Some("stock_history_eod")
    );
}

#[test]
fn option_eod_builds_the_contract_spec_and_applies_truthiness_guards() {
    let base = endpoints::OptionHistoryEod::new(date("2024-06-14"), date("2024-06-14"), "SPY", "*");
    let query = base.to_query();
    assert_eq!(
        query.contract_spec,
        Some(wire::ContractSpec {
            symbol: "SPY".into(),
            expiration: "*".into(), // _fmt_if_date passes strings through
            strike: Some("*".into()),
            right: Some("both".into()),
        })
    );
    // The wrapper never sets the query-level expiration field (#4).
    assert_eq!(query.expiration, "");
    assert_eq!((query.max_dte, query.strike_range), (None, None));
    // An explicit zero is falsy in Python and is therefore not sent.
    assert_eq!(base.clone().max_dte(0).to_query().max_dte, None);
    assert_eq!(base.clone().max_dte(5).to_query().max_dte, Some(5));
    let dated = endpoints::OptionHistoryEod::new(
        date("2024-06-14"),
        date("2024-06-14"),
        "SPY",
        date("2024-06-21"),
    );
    assert_eq!(
        dated.to_query().contract_spec.unwrap().expiration,
        "2024-06-21"
    );
}

#[test]
fn snapshot_defaults_and_false_values_follow_the_wrapper() {
    let query = endpoints::StockSnapshotOhlc::new(vec!["AAPL", "MSFT"]).to_query();
    assert_eq!(query.symbol, ["AAPL", "MSFT"]);
    assert_eq!(query.venue.as_deref(), Some("nqb")); // default, truthy
    assert_eq!(query.current_day_only, None); // default False is not sent
    assert_eq!(query.min_time, None);
    let query = endpoints::StockSnapshotOhlc::new("AAPL")
        .current_day_only(true)
        .venue("")
        .min_time(NaiveTime::from_hms_micro_opt(9, 30, 0, 123_456).unwrap())
        .to_query();
    assert_eq!(query.symbol, ["AAPL"]);
    assert_eq!(query.current_day_only, Some(true));
    assert_eq!(query.venue, None); // "" is falsy
    assert_eq!(query.min_time.as_deref(), Some("09:30:00.123")); // truncated to ms
}

#[test]
fn ohlc_interval_is_unconditional_and_time_defaults_pass_through() {
    let query = endpoints::OptionHistoryOhlc::new("SPY", date("2024-06-21")).to_query();
    assert_eq!(query.interval, "1s");
    assert_eq!(query.start_time.as_deref(), Some("09:30:00"));
    assert_eq!(query.end_time.as_deref(), Some("16:00:00"));
    assert_eq!(query.date, None);
    assert_eq!(query.contract_spec.unwrap().expiration, "2024-06-21");
    let text = endpoints::OptionHistoryOhlc::new("SPY", date("2024-06-21"))
        .start_time(TimeOfDay::from("10:00:00"))
        .date(date("2024-06-14"))
        .to_query();
    assert_eq!(text.start_time.as_deref(), Some("10:00:00"));
    assert_eq!(text.date.as_deref(), Some("2024-06-14"));
}

#[test]
fn greeks_defaults_send_truthy_values_only() {
    let query =
        endpoints::OptionHistoryGreeksEod::new("SPY", "*", date("2024-06-14"), date("2024-06-14"))
            .annual_dividend(0.0)
            .to_query();
    assert_eq!(query.rate_type.as_deref(), Some("sofr"));
    assert_eq!(query.version.as_deref(), Some("latest"));
    assert_eq!(query.option_use_nbbo, Some(true)); // default True, sent
    assert_eq!(query.underlyer_use_nbbo, None); // default False, not sent
    assert_eq!(query.annual_dividend, None); // 0.0 is falsy
}

#[test]
fn unstable_requests_copy_their_fields() {
    let request = endpoints::FutureHistoryEod::default()
        .symbol("ES".into())
        .start_date("2024-01-02".into())
        .end_date("2024-01-05".into());
    assert_eq!(
        <endpoints::FutureHistoryEod as Endpoint>::STABILITY,
        Stability::Unstable
    );
    let query = request.to_query();
    assert_eq!(
        (
            query.symbol.as_str(),
            query.start_date.as_str(),
            query.end_date.as_str()
        ),
        ("ES", "2024-01-02", "2024-01-05")
    );
}

#[tokio::test]
async fn stream_raw_dispatches_the_typed_request_to_its_rpc() {
    let (table, _) = support::table(support::Shape::Mixed, 2);
    let fixture = Fixture::start(Script {
        messages: vec![
            Ok(support::response(&table, true)),
            Ok(support::response(&table, false)),
        ],
        ..Default::default()
    });
    let client = ThetaClient::with_session(
        ClientConfig::default()
            .with_endpoint(Some(fixture.endpoint.clone()))
            .with_allow_insecure(true),
        support::session().await,
    )
    .await
    .unwrap();
    let request = endpoints::StockHistoryEod::new("AAPL", date("2024-01-02"), date("2024-01-05"));
    let mut stream = client.stream_raw(&request).await.unwrap();
    let mut messages = 0;
    while let Some(message) = stream.message().await.unwrap() {
        assert!(!message.compressed_data.is_empty());
        messages += 1;
    }
    assert_eq!(messages, 2);
    let sent = fixture.requests.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].params, Some(request.to_query()));
    assert_eq!(
        sent[0]
            .query_info
            .as_ref()
            .unwrap()
            .query_parameters
            .get("client")
            .map(String::as_str),
        Some("rust")
    );
}
