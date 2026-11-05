//! Planned jobs: date splitting, ordered scheduled parts, empty parts, errors
//! and retry at open (P1 PR 4b), over the loopback fixture.
#[allow(dead_code)]
mod support;

use chrono::NaiveDate;
use praevis_thetadata_client::endpoints::{self, Endpoint};
use praevis_thetadata_client::{ClientConfig, QueryError, SchedulerPolicy, ThetaClient, plan};
use praevis_thetadata_proto::endpoints::{
    self as wire, DataTable, DataValue, DataValueList, data_value::DataType as Wire,
};
use std::time::Duration;
use support::{Fixture, Script};

fn day(text: &str) -> NaiveDate {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
}
fn two_rows() -> wire::ResponseData {
    let table = DataTable {
        headers: vec!["n".into()],
        data_table: (0..2)
            .map(|i| DataValueList {
                values: vec![DataValue {
                    data_type: Some(Wire::Number(i)),
                }],
            })
            .collect(),
    };
    support::response(&table, true)
}
async fn connect(fixture: &Fixture, policy: SchedulerPolicy) -> ThetaClient {
    ThetaClient::with_session(
        ClientConfig {
            endpoint: Some(fixture.endpoint.clone()),
            allow_insecure: true,
            ..Default::default()
        },
        support::session().await,
    )
    .await
    .unwrap()
    .with_scheduler(policy)
    .unwrap()
}
fn fast() -> SchedulerPolicy {
    SchedulerPolicy {
        backoff: Duration::from_millis(5),
        max_backoff: Duration::from_millis(20),
        ..Default::default()
    }
}

#[test]
fn plans_split_by_the_endpoint_rule_in_date_order() {
    let eod = endpoints::StockHistoryEod::new("AAPL", day("2023-01-01"), day("2024-12-31"));
    let ranges: Vec<_> = plan(&eod).iter().map(|p| p.date_range().unwrap()).collect();
    assert_eq!(
        ranges,
        [
            (day("2023-01-01"), day("2024-01-01")), // 366 days inclusive
            (day("2024-01-02"), day("2024-12-31")),
        ]
    );
    let one = endpoints::StockHistoryEod::new("AAPL", day("2024-06-14"), day("2024-06-14"));
    assert_eq!(plan(&one), std::slice::from_ref(&one));
    let ticks = endpoints::OptionHistoryTrade::new("SPY", day("2024-06-21"))
        .start_date(day("2024-06-12"))
        .end_date(day("2024-06-14"));
    let days: Vec<_> = plan(&ticks)
        .iter()
        .map(|p| (p.start_date.unwrap(), p.end_date.unwrap()))
        .collect();
    assert_eq!(days.len(), 3);
    assert!(days.iter().all(|(s, e)| s == e));
    // A request with `date` set, or without a range, is sent whole.
    let dated = ticks.clone().date(day("2024-06-13"));
    assert_eq!(plan(&dated), std::slice::from_ref(&dated));
    assert_eq!(plan(&endpoints::StockListSymbols::new()).len(), 1);
    let inverted = endpoints::StockHistoryEod::new("AAPL", day("2024-02-01"), day("2024-01-01"));
    assert_eq!(plan(&inverted).len(), 1);
}

#[tokio::test]
async fn a_job_runs_its_parts_in_order_and_merges_their_rows() {
    let fixture = Fixture::start(Script {
        messages: vec![Ok(two_rows())],
        ..Default::default()
    });
    let client = connect(&fixture, fast()).await;
    let request = endpoints::StockHistoryEod::new("AAPL", day("2022-01-01"), day("2024-03-01"));
    let job = client.job(&request).await.unwrap();
    assert_eq!(job.parts(), 3);
    let batches = job.collect().await.unwrap();
    assert_eq!(batches.iter().map(|b| b.num_rows()).sum::<usize>(), 6);
    let sent: Vec<_> = fixture
        .requests
        .lock()
        .unwrap()
        .iter()
        .map(|r| {
            let q = r.params.clone().unwrap();
            (q.start_date, q.end_date)
        })
        .collect();
    let expected: Vec<_> = plan(&request)
        .iter()
        .map(|p| {
            let q = p.to_query();
            (q.start_date, q.end_date)
        })
        .collect();
    assert_eq!(sent, expected);
}

#[tokio::test]
async fn empty_parts_are_skipped_and_an_empty_job_has_no_data() {
    let fixture = Fixture::start(Script {
        initial_error: Some(tonic::Status::not_found("synthetic")),
        ..Default::default()
    });
    let client = connect(&fixture, fast()).await;
    let request = endpoints::StockHistoryEod::new("AAPL", day("2023-01-01"), day("2024-12-31"));
    let mut job = client.job(&request).await.unwrap();
    assert_eq!(job.next_batch().await, Err(QueryError::NoData));
    assert_eq!(job.next_batch().await, Ok(None));
    assert_eq!(fixture.requests.lock().unwrap().len(), 2);
    // One empty part among others is skipped.
    let fixture = Fixture::start(Script {
        messages: vec![Ok(two_rows())],
        initial_error: Some(tonic::Status::not_found("synthetic")),
        fail_first: 1,
        ..Default::default()
    });
    let client = connect(&fixture, fast()).await;
    let batches = client.job(&request).await.unwrap().collect().await.unwrap();
    assert_eq!(batches.iter().map(|b| b.num_rows()).sum::<usize>(), 2);
}

#[tokio::test]
async fn only_resource_exhausted_and_unavailable_are_retried_at_open() {
    for (status, fails, ok) in [
        (tonic::Status::unavailable("synthetic"), 2, true),
        (tonic::Status::resource_exhausted("synthetic"), 3, true),
        (tonic::Status::resource_exhausted("synthetic"), 4, false), // retries exhausted
        (tonic::Status::permission_denied("synthetic"), 1, false),
        (tonic::Status::invalid_argument("synthetic"), 1, false),
    ] {
        let code = status.code();
        let fixture = Fixture::start(Script {
            messages: vec![Ok(two_rows())],
            initial_error: Some(status),
            fail_first: fails,
            ..Default::default()
        });
        let client = connect(&fixture, fast()).await;
        let request = endpoints::StockHistoryEod::new("AAPL", day("2024-06-14"), day("2024-06-14"));
        let result = client.query(&request).await;
        let calls = fixture.requests.lock().unwrap().len();
        if ok {
            assert!(result.is_ok(), "{code:?}");
            assert_eq!(calls, fails + 1, "{code:?}");
        } else {
            assert_eq!(result.err(), Some(QueryError::Remote(code)));
            assert_eq!(calls, fails.min(4), "{code:?}");
        }
    }
}

#[tokio::test]
async fn one_slot_holds_until_the_stream_ends_and_dropping_a_job_releases_it() {
    let fixture = Fixture::start(Script {
        messages: vec![Ok(two_rows()), Ok(two_rows())],
        pause_before: Some((1, Duration::from_secs(5))),
        ..Default::default()
    });
    let client = connect(&fixture, fast()).await;
    let request = endpoints::StockHistoryEod::new("AAPL", day("2024-06-14"), day("2024-06-14"));
    let mut first = client.query(&request).await.unwrap();
    assert!(first.next_batch().await.unwrap().is_some());
    // FREE's default is one request at a time: a second waits.
    assert!(
        tokio::time::timeout(Duration::from_millis(200), client.query(&request))
            .await
            .is_err()
    );
    first.cancel();
    assert!(client.query(&request).await.is_ok());
    // A dropped job cancels its running part and frees the slot.
    let client = connect(&fixture, fast()).await;
    let mut job = client.job(&request).await.unwrap();
    assert!(job.next_batch().await.unwrap().is_some());
    drop(job);
    assert!(
        tokio::time::timeout(Duration::from_secs(2), client.query(&request))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn the_default_scheduler_follows_the_account_tier() {
    let fixture = Fixture::start(Script::default());
    let config = || ClientConfig {
        endpoint: Some(fixture.endpoint.clone()),
        allow_insecure: true,
        ..Default::default()
    };
    // The observed PRO account: options 3, stocks and indices 0.
    let pro = support::session_with_user(
        r#"{"email":"x","stockSubscription":0,"optionsSubscription":3,"indicesSubscription":0}"#,
    )
    .await
    .0;
    let client = ThetaClient::with_session(config(), pro).await.unwrap();
    assert_eq!(client.tier(), praevis_thetadata_client::Tier::Pro);
    let policy = client.scheduler_policy();
    assert_eq!((policy.concurrency, policy.per_minute), (8, None));
    // An explicit policy overrides the tier.
    let client = client.with_scheduler(SchedulerPolicy::default()).unwrap();
    let policy = client.scheduler_policy();
    assert_eq!((policy.concurrency, policy.per_minute), (1, Some(20)));
    // No recognised subscription: FREE's documented limits.
    let client = ThetaClient::with_session(config(), support::session().await)
        .await
        .unwrap();
    assert_eq!(client.tier(), praevis_thetadata_client::Tier::Free);
    let policy = client.scheduler_policy();
    assert_eq!((policy.concurrency, policy.per_minute), (1, Some(20)));
}
