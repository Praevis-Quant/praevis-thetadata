//! Generic query streams over the loopback fixture: coalescing and flush
//! thresholds, rows before an error, error classes, offload, cancellation and
//! schema unification (P1 PR 4a).
#[allow(dead_code)]
mod support;

use praevis_thetadata_client::arrow_array::RecordBatch;
use praevis_thetadata_client::arrow_schema::DataType;
use praevis_thetadata_client::{BatchPolicy, ClientConfig, QueryError, ThetaClient, endpoints};
use praevis_thetadata_proto::endpoints::{
    self as wire, DataTable, DataValue, DataValueList, data_value::DataType as Wire,
};
use std::time::Duration;
use support::{Fixture, Script};

fn rows(headers: &[&str], values: Vec<Vec<Option<Wire>>>) -> wire::ResponseData {
    let table = DataTable {
        headers: headers.iter().map(|h| (*h).into()).collect(),
        data_table: values
            .into_iter()
            .map(|row| DataValueList {
                values: row
                    .into_iter()
                    .map(|data_type| DataValue { data_type })
                    .collect(),
            })
            .collect(),
    };
    support::response(&table, true)
}
fn two_rows(start: i64) -> wire::ResponseData {
    rows(
        &["n", "p"],
        vec![
            vec![
                Some(Wire::Number(start)),
                Some(Wire::Price(wire::Price {
                    value: 1,
                    r#type: 10,
                })),
            ],
            vec![
                Some(Wire::Number(start + 1)),
                Some(Wire::Price(wire::Price {
                    value: 2,
                    r#type: 10,
                })),
            ],
        ],
    )
}

async fn connect(fixture: &Fixture, config: ClientConfig) -> ThetaClient {
    ThetaClient::with_session(
        config
            .with_endpoint(Some(fixture.endpoint.clone()))
            .with_allow_insecure(true),
        support::session().await,
    )
    .await
    .unwrap()
}
fn request() -> endpoints::StockHistoryEod {
    let day = chrono::NaiveDate::from_ymd_opt(2024, 6, 14).unwrap();
    endpoints::StockHistoryEod::new("AAPL", day, day)
}
async fn sizes(
    stream: &mut praevis_thetadata_client::QueryStream,
) -> (Vec<usize>, Result<(), QueryError>) {
    let mut out = Vec::new();
    loop {
        match stream.next_batch().await {
            Ok(Some(batch)) => out.push(batch.num_rows()),
            Ok(None) => return (out, Ok(())),
            Err(error) => return (out, Err(error)),
        }
    }
}
fn script(messages: Vec<Result<wire::ResponseData, tonic::Status>>) -> Script {
    Script {
        messages,
        ..Default::default()
    }
}

#[tokio::test]
async fn first_message_is_emitted_at_once_and_the_rest_coalesce() {
    let fixture = Fixture::start(script((0..5).map(|i| Ok(two_rows(i * 2))).collect()));
    let client = connect(&fixture, ClientConfig::default()).await;
    let mut stream = client.query(&request()).await.unwrap();
    assert_eq!(sizes(&mut stream).await, (vec![2, 8], Ok(())));
    // The request went to the endpoint's RPC with the generated query.
    let sent = fixture.requests.lock().unwrap()[0].params.clone();
    assert_eq!(sent, Some(endpoints::Endpoint::to_query(&request())));
    // Fused after the end.
    assert_eq!(stream.next_batch().await, Ok(None));
}

#[tokio::test]
async fn row_threshold_and_offloaded_decoding_split_batches_identically() {
    for inline_bytes in [64 << 10, 0] {
        let fixture = Fixture::start(script((0..4).map(|i| Ok(two_rows(i * 2))).collect()));
        let client = connect(&fixture, ClientConfig::default()).await;
        let policy = BatchPolicy::default()
            .with_rows(3)
            .with_inline_bytes(inline_bytes);
        let mut stream = client.query_with(&request(), policy).await.unwrap();
        assert_eq!(
            sizes(&mut stream).await,
            (vec![2, 4, 2], Ok(())),
            "{inline_bytes}"
        );
    }
}

#[tokio::test]
async fn buffered_rows_are_emitted_after_the_latency_while_the_stream_waits() {
    let fixture = Fixture::start(Script {
        messages: (0..3).map(|i| Ok(two_rows(i * 2))).collect(),
        pause_before: Some((2, Duration::from_millis(400))),
        ..Default::default()
    });
    let client = connect(&fixture, ClientConfig::default()).await;
    let mut stream = client.query(&request()).await.unwrap();
    assert_eq!(sizes(&mut stream).await, (vec![2, 2, 2], Ok(())));
}

#[tokio::test]
async fn rows_before_a_remote_error_are_delivered_then_the_error_once() {
    let fixture = Fixture::start(script(vec![
        Ok(two_rows(0)),
        Ok(two_rows(2)),
        Err(tonic::Status::unavailable("synthetic")),
    ]));
    let client = connect(&fixture, ClientConfig::default()).await;
    let mut stream = client.query(&request()).await.unwrap();
    assert_eq!(
        sizes(&mut stream).await,
        (
            vec![2, 2],
            Err(QueryError::Remote(tonic::Code::Unavailable))
        )
    );
    assert_eq!(stream.next_batch().await, Ok(None));
}

#[tokio::test]
async fn a_bad_message_ends_the_stream_after_the_good_rows() {
    let changed = rows(&["other", "p"], vec![vec![Some(Wire::Number(1)), None]]);
    let mixed = rows(&["n", "p"], vec![vec![Some(Wire::Text("x".into())), None]]);
    for (bad, expected) in [(changed, QueryError::Schema), (mixed, QueryError::Schema)] {
        let fixture = Fixture::start(script(vec![Ok(two_rows(0)), Ok(two_rows(2)), Ok(bad)]));
        let client = connect(&fixture, ClientConfig::default()).await;
        let mut stream = client.query(&request()).await.unwrap();
        assert_eq!(sizes(&mut stream).await, (vec![2, 2], Err(expected)));
    }
}

#[tokio::test]
async fn opening_errors_keep_the_stock_eod_classes() {
    for (status, expected) in [
        (tonic::Status::not_found("synthetic"), QueryError::NoData),
        (
            tonic::Status::permission_denied("synthetic"),
            QueryError::Remote(tonic::Code::PermissionDenied),
        ),
        // An ended session (a later login for the account): a typed error,
        // sent once, with no retry and no new login.
        (
            tonic::Status::unauthenticated("synthetic"),
            QueryError::SessionEnded,
        ),
    ] {
        let fixture = Fixture::start(Script {
            initial_error: Some(status),
            ..Default::default()
        });
        let client = connect(&fixture, ClientConfig::default()).await;
        assert_eq!(client.query(&request()).await.err(), Some(expected));
        assert_eq!(fixture.requests.lock().unwrap().len(), 1);
    }
    let fixture = Fixture::start(script(vec![]));
    let client = connect(&fixture, ClientConfig::default()).await;
    let invalid = BatchPolicy::default().with_rows(0);
    assert_eq!(
        client.query_with(&request(), invalid).await.err(),
        Some(QueryError::Configuration)
    );
}

#[tokio::test]
async fn idle_timeout_and_cancellation_are_terminal() {
    let fixture = Fixture::start(Script {
        messages: vec![Ok(two_rows(0)), Ok(two_rows(2))],
        pause_before: Some((1, Duration::from_secs(5))),
        ..Default::default()
    });
    let config = ClientConfig::default().with_idle_timeout(Duration::from_millis(200));
    let client = connect(&fixture, config).await;
    let mut stream = client.query(&request()).await.unwrap();
    assert_eq!(sizes(&mut stream).await, (vec![2], Err(QueryError::Idle)));

    let fixture = Fixture::start(Script {
        messages: vec![Ok(two_rows(0)), Ok(two_rows(2))],
        pause_before: Some((1, Duration::from_secs(5))),
        ..Default::default()
    });
    let client = connect(&fixture, ClientConfig::default()).await;
    let mut stream = client.query(&request()).await.unwrap();
    assert_eq!(stream.next_batch().await.unwrap().unwrap().num_rows(), 2);
    // Dropping a pending call cancels the stream.
    assert!(
        tokio::time::timeout(Duration::from_millis(100), stream.next_batch())
            .await
            .is_err()
    );
    assert_eq!(stream.next_batch().await, Err(QueryError::Cancelled));
    assert_eq!(stream.next_batch().await, Ok(None));
}

#[tokio::test]
async fn collect_gives_null_columns_their_final_type() {
    let fixture = Fixture::start(script(vec![
        Ok(rows(
            &["n", "late"],
            vec![vec![Some(Wire::Number(1)), None]],
        )),
        Ok(rows(
            &["n", "late"],
            vec![vec![Some(Wire::Number(2)), Some(Wire::Number(7))]],
        )),
    ]));
    let client = connect(&fixture, ClientConfig::default()).await;
    let batches: Vec<RecordBatch> = client
        .query(&request())
        .await
        .unwrap()
        .collect()
        .await
        .unwrap();
    assert_eq!(batches.len(), 2);
    for batch in &batches {
        assert_eq!(batch.schema(), batches[1].schema());
        assert_eq!(batch.column(1).data_type(), &DataType::Int64);
    }
    assert!(batches[0].column(1).is_null(0));
}

#[tokio::test]
async fn calendar_results_share_one_schema_whether_the_day_is_open_or_closed() {
    // A closed day's open and close are null_value on the wire; an open day's
    // are text. Separate requests must still give one schema (cal-local-001).
    let handler: support::Handler = std::sync::Arc::new(|rpc: &str, request: Vec<u8>| {
        assert_eq!(rpc, "GetCalendarOnDate");
        let text = |v: &str| Some(Wire::Text(v.into()));
        let closed = request.windows(10).any(|w| w == b"2025-12-25");
        let row = if closed {
            vec![
                text("full_close"),
                Some(Wire::NullValue(0)),
                Some(Wire::NullValue(0)),
            ]
        } else {
            vec![text("open"), text("09:30:00"), text("16:00:00")]
        };
        Ok(vec![rows(&["type", "open", "close"], vec![row])])
    });
    let fixture = Fixture::start(Script {
        other: Some(handler),
        ..Default::default()
    });
    let client = connect(&fixture, ClientConfig::default()).await;
    let day = |d| chrono::NaiveDate::from_ymd_opt(2025, 12, d).unwrap();
    let mut schemas = Vec::new();
    for date in [day(25), day(26)] {
        let batches = client
            .query(&endpoints::CalendarOnDate::new(date))
            .await
            .unwrap()
            .collect()
            .await
            .unwrap();
        assert_eq!(batches.len(), 1);
        // The 25th really is the closed day: its open and close are null.
        let nulls = usize::from(date == day(25));
        assert_eq!(batches[0].column(1).null_count(), nulls);
        schemas.push(batches[0].schema());
    }
    assert_eq!(schemas[0], schemas[1]);
    for field in schemas[0].fields() {
        assert_eq!(field.data_type(), &DataType::Utf8, "{}", field.name());
    }
}
