//! Planned jobs: date splitting, ordered parts, empty parts, errors, the
//! per-job concurrency, and no retry, scheduler or default deadline
//! (ADR-0021), over the loopback fixture.
#[allow(dead_code)]
mod support;

use chrono::NaiveDate;
use praevis_thetadata_client::endpoints::{self, Endpoint};
use praevis_thetadata_client::{ClientConfig, QueryError, RequestOptions, ThetaClient, Tier, plan};
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
fn config(fixture: &Fixture) -> ClientConfig {
    ClientConfig::default()
        .with_endpoint(Some(fixture.endpoint.clone()))
        .with_allow_insecure(true)
}
/// A client of an account with no recognised subscription: FREE, so a job
/// runs one part at a time by default.
async fn connect(fixture: &Fixture) -> ThetaClient {
    ThetaClient::with_session(config(fixture), support::session().await)
        .await
        .unwrap()
}
/// A client of the observed PRO account (options 3, stocks and indices 0):
/// a job runs up to 8 parts at a time by default.
async fn connect_pro(fixture: &Fixture) -> ThetaClient {
    let pro = support::session_with_user(
        r#"{"email":"x","stockSubscription":0,"optionsSubscription":3,"indicesSubscription":0}"#,
    )
    .await
    .0;
    ThetaClient::with_session(config(fixture), pro)
        .await
        .unwrap()
}
fn concurrency(n: usize) -> RequestOptions {
    RequestOptions::default().with_concurrency(Some(n))
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

#[test]
fn greeks_eod_over_every_expiration_is_planned_one_day_at_a_time() {
    // The service refused expiration `*` over 2 and 5 days with
    // InvalidArgument and answered one day (2026-11-13, praevis-data's first
    // ingest), while option EOD over the same 5 days answered whole.
    let week = (day("2026-11-02"), day("2026-11-06"));
    let wildcard = endpoints::OptionHistoryGreeksEod::new("SPY", "*", week.0, week.1);
    let days: Vec<_> = plan(&wildcard)
        .iter()
        .map(|p| p.date_range().unwrap())
        .collect();
    assert_eq!(days.len(), 5);
    assert!(days.iter().all(|(s, e)| s == e));
    assert_eq!(days[0].0, week.0);
    assert_eq!(days[4].1, week.1);
    // One expiration keeps the 366-day split: no refusal was seen there.
    let one = endpoints::OptionHistoryGreeksEod::new("SPY", day("2026-11-21"), week.0, week.1);
    assert_eq!(plan(&one), std::slice::from_ref(&one));
    // Option EOD over every expiration answered 5 days whole: unchanged.
    let eod = endpoints::OptionHistoryEod::new(week.0, week.1, "SPY", "*");
    assert_eq!(plan(&eod), std::slice::from_ref(&eod));
}

#[tokio::test]
async fn a_job_runs_its_parts_in_order_and_merges_their_rows() {
    let fixture = Fixture::start(Script {
        messages: vec![Ok(two_rows())],
        ..Default::default()
    });
    let client = connect(&fixture).await;
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
    let client = connect(&fixture).await;
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
    let client = connect(&fixture).await;
    let batches = client.job(&request).await.unwrap().collect().await.unwrap();
    assert_eq!(batches.iter().map(|b| b.num_rows()).sum::<usize>(), 2);
}

#[tokio::test]
async fn a_busy_answer_is_busy_and_never_retried() {
    for (status, expected) in [
        (
            tonic::Status::resource_exhausted("synthetic"),
            QueryError::Busy,
        ),
        (
            tonic::Status::unavailable("synthetic"),
            QueryError::Remote(tonic::Code::Unavailable),
        ),
    ] {
        let fixture = Fixture::start(Script {
            messages: vec![Ok(two_rows())],
            initial_error: Some(status),
            fail_first: 1,
            ..Default::default()
        });
        let client = connect(&fixture).await;
        let request = endpoints::StockHistoryEod::new("AAPL", day("2024-06-14"), day("2024-06-14"));
        assert_eq!(client.query(&request).await.err(), Some(expected));
        // One request: no retry, although the second would succeed.
        assert_eq!(fixture.requests.lock().unwrap().len(), 1, "{expected:?}");
        // A job ends with the same error, also without retrying.
        let fixture = Fixture::start(Script {
            messages: vec![Ok(two_rows())],
            initial_error: Some(tonic::Status::resource_exhausted("synthetic")),
            ..Default::default()
        });
        let client = connect(&fixture).await;
        let mut job = client.job(&request).await.unwrap();
        assert_eq!(job.next_batch().await, Err(QueryError::Busy));
        assert_eq!(fixture.requests.lock().unwrap().len(), 1);
    }
}

#[tokio::test]
async fn queries_are_not_coordinated_and_a_dropped_job_cancels_its_part() {
    let fixture = Fixture::start(Script {
        messages: vec![Ok(two_rows()), Ok(two_rows())],
        pause_before: Some((1, Duration::from_secs(5))),
        ..Default::default()
    });
    // A FREE account: the library still sends a second query while the first
    // is open. Staying within the plan across calls is the caller's job.
    let client = connect(&fixture).await;
    let request = endpoints::StockHistoryEod::new("AAPL", day("2024-06-14"), day("2024-06-14"));
    let mut first = client.query(&request).await.unwrap();
    assert!(first.next_batch().await.unwrap().is_some());
    let second = tokio::time::timeout(Duration::from_millis(500), client.query(&request)).await;
    assert!(matches!(second, Ok(Ok(_))), "the second query waited");
    assert_eq!(fixture.requests.lock().unwrap().len(), 2);
    drop(first);
    // A dropped job cancels its running part.
    let fixture = Fixture::start(Script {
        messages: vec![Ok(two_rows()), Ok(two_rows())],
        pause_before: Some((1, Duration::from_secs(30))),
        ..Default::default()
    });
    let client = connect(&fixture).await;
    let long = endpoints::StockHistoryEod::new("AAPL", day("2015-01-01"), day("2024-12-31"));
    let mut job = client.job(&long).await.unwrap();
    assert!(job.next_batch().await.unwrap().is_some());
    drop(job);
    tokio::time::sleep(Duration::from_millis(300)).await;
    // FREE runs one part at a time, and the dropped job started no other.
    assert_eq!(fixture.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn the_default_job_concurrency_follows_the_account_tier() {
    // Every request takes 400 ms; ten yearly parts.
    let fixture = Fixture::start(Script {
        messages: vec![Ok(two_rows())],
        header_delay: Duration::from_millis(400),
        ..Default::default()
    });
    let long = endpoints::StockHistoryEod::new("AAPL", day("2015-01-01"), day("2024-12-31"));
    for (client, tier, parallel) in [
        (connect_pro(&fixture).await, Tier::Pro, 8),
        (connect(&fixture).await, Tier::Free, 1),
    ] {
        assert_eq!(client.tier(), tier);
        assert_eq!(tier.concurrency(), parallel);
        let before = fixture.requests.lock().unwrap().len();
        let job = client.job(&long).await.unwrap();
        let reader = tokio::spawn(job.collect());
        tokio::time::sleep(Duration::from_millis(200)).await;
        // The tier's documented limit is the job's default concurrency.
        let sent = fixture.requests.lock().unwrap().len() - before;
        assert_eq!(sent, parallel, "{tier:?}");
        reader.abort();
    }
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
    let client = connect_pro(&fixture).await;
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
    let client = connect_pro(&fixture).await;
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
    let serial = client.job_with(&base(), concurrency(1)).await.unwrap();
    assert_eq!(serial.parts(), 1);
    // No strikes listed: the job sends the request unsplit.
    let (fixture, _) = strike_fixture(&[], "", 2);
    let client = connect_pro(&fixture).await;
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
    let client = connect(&fixture).await;
    let eod = endpoints::StockHistoryEod::new("AAPL", day("2015-01-01"), day("2024-12-31"));
    let job = client.job_with(&eod, concurrency(2)).await.unwrap();
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
async fn no_deadline_applies_unless_one_is_set() {
    // The response takes 1.5 s; the client sets no timeout, as the vendor's.
    let fixture = Fixture::start(Script {
        messages: vec![Ok(two_rows())],
        header_delay: Duration::from_millis(1_500),
        ..Default::default()
    });
    let client = connect(&fixture).await;
    let batches = client
        .query(&one_day())
        .await
        .unwrap()
        .collect()
        .await
        .unwrap();
    assert_eq!(batches.iter().map(|b| b.num_rows()).sum::<usize>(), 2);
    // No grpc-timeout is sent unless the caller sets one.
    client
        .query_with(
            &one_day(),
            RequestOptions::default().with_timeout(Some(Duration::from_secs(20))),
        )
        .await
        .unwrap()
        .collect()
        .await
        .unwrap();
    let sent = fixture.timeouts.lock().unwrap().clone();
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0], None);
    assert!(sent[1].is_some(), "{sent:?}");
}

#[tokio::test]
async fn a_per_call_timeout_ends_a_slow_request_and_nothing_queues() {
    // Every response takes 3 s to start.
    let fixture = Fixture::start(Script {
        messages: vec![Ok(two_rows())],
        header_delay: Duration::from_secs(3),
        ..Default::default()
    });
    let client = connect(&fixture).await;
    // A slow query is open; another is not queued behind it.
    let holder = {
        let client = client.clone();
        tokio::spawn(async move { client.query(&one_day()).await.map(drop) })
    };
    tokio::time::sleep(Duration::from_millis(200)).await;
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
    // It reached the server at once, beside the open one: no queue.
    assert_eq!(fixture.requests.lock().unwrap().len(), 2);
    holder.abort();
}

#[tokio::test]
async fn a_per_call_idle_timeout_overrides_the_clients() {
    let fixture = Fixture::start(Script {
        messages: vec![Ok(two_rows())],
        message_delay: Duration::from_millis(500),
        ..Default::default()
    });
    // The client sets no idle limit; this call allows 100 ms.
    let client = connect_pro(&fixture).await;
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
async fn a_jobs_concurrency_is_checked_but_not_tied_to_the_tier() {
    let fixture = Fixture::start(Script {
        messages: vec![Ok(two_rows())],
        ..Default::default()
    });
    // A FREE account (default 1): the caller may choose more, up to 64.
    let client = connect(&fixture).await;
    for wanted in [0, 65] {
        let result = client.job_with(&one_day(), concurrency(wanted)).await;
        assert!(
            matches!(result, Err(QueryError::Configuration)),
            "{wanted}: {}",
            result.is_ok()
        );
    }
    for wanted in [9, 64] {
        let job = client
            .job_with(&one_day(), concurrency(wanted))
            .await
            .unwrap();
        assert_eq!(job.collect().await.unwrap().len(), 1, "{wanted}");
    }
}

#[tokio::test]
async fn a_jobs_concurrency_caps_its_parts_in_flight() {
    // Every request takes 300 ms to answer. Eight yearly parts at two at a
    // time need at least four rounds, although the PRO default would be 8.
    let fixture = Fixture::start(Script {
        messages: vec![Ok(two_rows())],
        header_delay: Duration::from_millis(300),
        ..Default::default()
    });
    let client = connect_pro(&fixture).await;
    let request = endpoints::StockHistoryEod::new("AAPL", day("2017-01-01"), day("2024-12-31"));
    let started = std::time::Instant::now();
    let job = client.job_with(&request, concurrency(2)).await.unwrap();
    assert_eq!(job.parts(), 8);
    let batches = job.collect().await.unwrap();
    let elapsed = started.elapsed();
    assert_eq!(batches.iter().map(|b| b.num_rows()).sum::<usize>(), 16);
    assert!(elapsed >= Duration::from_millis(1_150), "{elapsed:?}");
}
