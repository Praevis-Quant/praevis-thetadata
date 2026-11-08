//! Planned jobs: date splitting, ordered scheduled parts, empty parts, errors
//! and retry at open (P1 PR 4b), over the loopback fixture.
#[allow(dead_code)]
mod support;

use chrono::NaiveDate;
use praevis_thetadata_client::endpoints::{self, Endpoint};
use praevis_thetadata_client::{
    ClientConfig, QueryError, RequestOptions, SchedulerPolicy, ThetaClient, plan,
};
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
        ClientConfig::default()
            .with_endpoint(Some(fixture.endpoint.clone()))
            .with_allow_insecure(true),
        support::session().await,
    )
    .await
    .unwrap()
    .with_scheduler(policy)
    .unwrap()
}
/// 8 at a time with no per-minute limit, like PRO.
fn parallel() -> SchedulerPolicy {
    fast().with_concurrency(8).with_per_minute(None)
}
fn fast() -> SchedulerPolicy {
    SchedulerPolicy::default()
        .with_backoff(Duration::from_millis(5))
        .with_max_backoff(Duration::from_millis(20))
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
    let config = || {
        ClientConfig::default()
            .with_endpoint(Some(fixture.endpoint.clone()))
            .with_allow_insecure(true)
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

/// A fixture for the per-strike split: `option_list_strikes` lists the given
/// strikes (in dollars, 2 decimals); `option_history_quote` answers each
/// strike with `rows` rows naming it, or `NOT_FOUND` for `empty`, and the
/// all-strike request with one row named `*`. Every quote request's strike is
/// recorded in order of arrival.
fn strike_fixture(
    strikes: &'static [i32],
    empty: &'static str,
    rows: i64,
) -> (Fixture, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
    use praevis_thetadata_proto::beta_endpoints::OptionHistoryQuoteRequest;
    use prost::Message;
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let log = seen.clone();
    let handler: support::Handler = std::sync::Arc::new(move |rpc, request| match rpc {
        "GetOptionListStrikes" => {
            if strikes.is_empty() {
                return Err(tonic::Status::not_found("synthetic"));
            }
            let table = DataTable {
                headers: vec!["strike".into()],
                data_table: strikes
                    .iter()
                    .map(|cents| DataValueList {
                        values: vec![DataValue {
                            data_type: Some(Wire::Price(wire::Price {
                                value: *cents,
                                r#type: 8,
                            })),
                        }],
                    })
                    .collect(),
            };
            Ok(vec![support::response(&table, false)])
        }
        "GetOptionHistoryQuote" => {
            let query = OptionHistoryQuoteRequest::decode(request.as_slice())
                .unwrap()
                .params
                .unwrap();
            let strike = query
                .contract_spec
                .and_then(|c| c.strike)
                .unwrap_or_else(|| "*".into());
            log.lock().unwrap().push(strike.clone());
            if strike == empty {
                return Err(tonic::Status::not_found("synthetic"));
            }
            let count = if strike == "*" { 1 } else { rows };
            let table = DataTable {
                headers: vec!["s".into(), "n".into()],
                data_table: (0..count)
                    .map(|i| DataValueList {
                        values: vec![
                            DataValue {
                                data_type: Some(Wire::Text(strike.clone())),
                            },
                            DataValue {
                                data_type: Some(Wire::Number(i)),
                            },
                        ],
                    })
                    .collect(),
            };
            Ok(vec![support::response(&table, true)])
        }
        _ => Err(tonic::Status::unimplemented("synthetic")),
    });
    let fixture = Fixture::start(Script {
        other: Some(handler),
        ..Default::default()
    });
    (fixture, seen)
}

async fn strike_texts(
    client: &ThetaClient,
    request: &endpoints::OptionHistoryQuote,
) -> Vec<String> {
    use praevis_thetadata_client::arrow_array::Array;
    use praevis_thetadata_client::arrow_array::cast::AsArray;
    let batches = client.job(request).await.unwrap().collect().await.unwrap();
    batches
        .iter()
        .flat_map(|b| {
            let column = b.column_by_name("s").unwrap().as_string::<i32>().clone();
            (0..column.len()).map(move |i| column.value(i).to_owned())
        })
        .collect()
}

#[tokio::test]
async fn all_strike_fine_quotes_split_per_listed_strike_in_strike_order() {
    // 99.00 sorts before 540.00 by value, after it as text.
    let (fixture, seen) = strike_fixture(&[54250, 9900, 54000], "540.00", 2);
    let client = connect(&fixture, parallel()).await;
    let request = endpoints::OptionHistoryQuote::new("SPY", day("2024-06-21"))
        .date(day("2024-06-14"))
        .interval("tick");
    let mut job = client.job(&request).await.unwrap();
    // One list request was made while planning; 3 strike parts follow.
    assert_eq!(job.parts(), 3);
    let mut rows = Vec::new();
    while let Some(batch) = job.next_batch().await.unwrap() {
        rows.push(batch.num_rows());
    }
    // The strike without quotes (NOT_FOUND) is skipped, not an error.
    let texts = strike_texts(&client, &request).await;
    assert_eq!(texts, ["99.00", "99.00", "542.50", "542.50"]);
    let mut sent = seen.lock().unwrap().clone();
    sent.sort();
    assert_eq!(
        sent,
        ["540.00", "540.00", "542.50", "542.50", "99.00", "99.00"]
    );
    // The default interval (1 s) splits too.
    let default_interval =
        endpoints::OptionHistoryQuote::new("SPY", "2024-06-21").date(day("2024-06-14"));
    assert_eq!(client.job(&default_interval).await.unwrap().parts(), 3);
}

#[tokio::test]
async fn other_quote_shapes_and_query_are_never_split() {
    let (fixture, seen) = strike_fixture(&[54000, 54250], "", 2);
    let client = connect(&fixture, parallel()).await;
    let base = || {
        endpoints::OptionHistoryQuote::new("SPY", day("2024-06-21"))
            .date(day("2024-06-14"))
            .interval("tick")
    };
    for unsplit in [
        base().interval("1m"),
        base().interval("100ms"),
        base().strike("540.00"),
        base().strike_range(5),
        base().max_dte(7),
        endpoints::OptionHistoryQuote::new("SPY", "*")
            .date(day("2024-06-14"))
            .interval("tick"),
    ] {
        let job = client.job(&unsplit).await.unwrap();
        assert_eq!(job.parts(), 1, "{unsplit:?}");
    }
    // query() sends the request as given.
    let batches = client
        .query(&base())
        .await
        .unwrap()
        .collect()
        .await
        .unwrap();
    assert_eq!(batches.iter().map(|b| b.num_rows()).sum::<usize>(), 1);
    assert_eq!(seen.lock().unwrap().last().map(String::as_str), Some("*"));
    // One request at a time: never split (slower than the single request).
    let serial = client
        .clone()
        .with_scheduler(fast().with_concurrency(1))
        .unwrap();
    assert_eq!(serial.job(&base()).await.unwrap().parts(), 1);
    // No strikes listed: the job sends the request unsplit.
    let (fixture, _) = strike_fixture(&[], "", 2);
    let client = connect(&fixture, parallel()).await;
    let job = client.job(&base()).await.unwrap();
    assert_eq!(job.parts(), 1);
    assert_eq!(strike_texts(&client, &base()).await, ["*"]);
}

#[tokio::test]
async fn a_slow_first_part_does_not_hold_the_other_slots() {
    // 10 yearly parts; the first request waits 2 s, the rest answer at once.
    let fixture = Fixture::start(Script {
        messages: vec![Ok(two_rows())],
        header_delay: Duration::from_secs(2),
        delay_requests: vec![1],
        ..Default::default()
    });
    let client = connect(&fixture, fast().with_concurrency(2).with_per_minute(None)).await;
    let eod = endpoints::StockHistoryEod::new("AAPL", day("2015-01-01"), day("2024-12-31"));
    let job = client.job(&eod).await.unwrap();
    assert_eq!(job.parts(), 10);
    let reader = tokio::spawn(job.collect());
    tokio::time::sleep(Duration::from_millis(1000)).await;
    // While the head waits, the second slot works through the read-ahead
    // window (4 x 2 = 8 open parts), not just the second part.
    let sent = fixture.requests.lock().unwrap().len();
    assert_eq!(sent, 8, "parts sent while the head was slow");
    let batches = reader.await.unwrap().unwrap();
    assert_eq!(batches.iter().map(|b| b.num_rows()).sum::<usize>(), 20);
    // Order is unchanged: the parts' date ranges were requested in order.
    let starts: Vec<String> = fixture
        .requests
        .lock()
        .unwrap()
        .iter()
        .map(|r| r.params.as_ref().unwrap().start_date.clone())
        .collect();
    let mut sorted = starts.clone();
    sorted.sort();
    assert_eq!(starts, sorted);
    assert_eq!(fixture.requests.lock().unwrap().len(), 10);
}

fn one_day() -> endpoints::StockHistoryEod {
    endpoints::StockHistoryEod::new("AAPL", day("2024-01-02"), day("2024-01-02"))
}

#[tokio::test]
async fn a_per_call_timeout_covers_time_queued_for_a_slot() {
    // The first request holds the only slot while its headers are delayed.
    let fixture = Fixture::start(Script {
        messages: vec![Ok(two_rows())],
        header_delay: Duration::from_secs(3),
        delay_requests: vec![1],
        ..Default::default()
    });
    let client = connect(&fixture, fast().with_concurrency(1).with_per_minute(None)).await;
    let holder = {
        let client = client.clone();
        tokio::spawn(async move { client.query(&one_day()).await.map(drop) })
    };
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(fixture.requests.lock().unwrap().len(), 1);
    // The client's own request timeout is the default 120 s; this call asks
    // for 300 ms, all of it spent waiting for the slot.
    let started = std::time::Instant::now();
    let result = client
        .query_with(
            &one_day(),
            RequestOptions::default().with_timeout(Some(Duration::from_millis(300))),
        )
        .await;
    let waited = started.elapsed();
    assert!(
        matches!(result, Err(QueryError::Deadline)),
        "{:?}",
        result.err()
    );
    assert!(
        waited >= Duration::from_millis(250) && waited < Duration::from_secs(2),
        "{waited:?}"
    );
    // It never reached the server: its budget ran out in the queue.
    assert_eq!(fixture.requests.lock().unwrap().len(), 1);
    holder.abort();
}

#[tokio::test]
async fn a_per_call_idle_timeout_overrides_the_clients() {
    let fixture = Fixture::start(Script {
        messages: vec![Ok(two_rows())],
        message_delay: Duration::from_millis(500),
        ..Default::default()
    });
    // The client allows 30 s between messages; this call allows 100 ms.
    let client = connect(&fixture, parallel()).await;
    let stream = client
        .query_with(
            &one_day(),
            RequestOptions::default().with_idle_timeout(Some(Duration::from_millis(100))),
        )
        .await
        .unwrap();
    assert!(matches!(stream.collect().await, Err(QueryError::Idle)));
}

#[tokio::test]
async fn a_jobs_concurrency_is_checked_against_the_scheduler() {
    let fixture = Fixture::start(Script {
        messages: vec![Ok(two_rows())],
        ..Default::default()
    });
    let client = connect(&fixture, parallel()).await;
    for wanted in [0, 9] {
        let result = client
            .job_with(
                &one_day(),
                RequestOptions::default().with_concurrency(Some(wanted)),
            )
            .await;
        assert!(
            matches!(result, Err(QueryError::Configuration)),
            "{wanted}: {}",
            result.is_ok()
        );
    }
    // At the scheduler's own limit it runs.
    let job = client
        .job_with(
            &one_day(),
            RequestOptions::default().with_concurrency(Some(8)),
        )
        .await
        .unwrap();
    assert_eq!(job.collect().await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_jobs_concurrency_caps_its_parts_in_flight() {
    // Every request takes 300 ms to answer. Eight yearly parts at two at a
    // time need at least four rounds, however many slots the scheduler has.
    let fixture = Fixture::start(Script {
        messages: vec![Ok(two_rows())],
        header_delay: Duration::from_millis(300),
        ..Default::default()
    });
    let client = connect(&fixture, parallel()).await;
    let request = endpoints::StockHistoryEod::new("AAPL", day("2017-01-01"), day("2024-12-31"));
    let started = std::time::Instant::now();
    let job = client
        .job_with(
            &request,
            RequestOptions::default().with_concurrency(Some(2)),
        )
        .await
        .unwrap();
    assert_eq!(job.parts(), 8);
    let batches = job.collect().await.unwrap();
    let elapsed = started.elapsed();
    assert_eq!(batches.iter().map(|b| b.num_rows()).sum::<usize>(), 16);
    assert!(elapsed >= Duration::from_millis(1_150), "{elapsed:?}");
}
