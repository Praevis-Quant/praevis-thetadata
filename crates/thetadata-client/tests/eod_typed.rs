mod support;
use std::time::Duration;
use support::{Fixture, Script, Shape};
use thetadata_client::{
    ClientConfig, EodError, EodPolicy, NaiveDate, StockEodRequest, ThetaClient,
};

fn request() -> StockEodRequest {
    StockEodRequest::new(
        "SYNTHETIC",
        NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
        NaiveDate::from_ymd_opt(2026, 1, 16).unwrap(),
    )
    .unwrap()
}
async fn client(fixture: &Fixture, policy: EodPolicy, timeout: Duration) -> ThetaClient {
    ThetaClient::with_eod_policy(
        ClientConfig {
            endpoint: Some(fixture.endpoint.clone()),
            allow_insecure: true,
            request_timeout: timeout,
            idle_timeout: Duration::from_secs(2),
            ..Default::default()
        },
        support::session().await,
        policy,
    )
    .await
    .unwrap()
}

#[test]
fn validates_symbols_and_calendar_ranges() {
    let leap = NaiveDate::from_ymd_opt(2024, 2, 29).unwrap();
    for symbol in ["", " A", "A\u{2003}", "A\nB", "A,B", "*"] {
        assert!(matches!(
            StockEodRequest::new(symbol, leap, leap),
            Err(EodError::Input)
        ));
    }
    for symbol in ["a.B-ç", "A B"] {
        assert!(StockEodRequest::new(symbol, leap, leap).is_ok());
    }
    assert!(NaiveDate::from_ymd_opt(2023, 2, 29).is_none());
    assert!(StockEodRequest::new("A", leap, leap.pred_opt().unwrap()).is_err());
    for year in [0, 10000] {
        assert!(
            StockEodRequest::new("A", NaiveDate::from_ymd_opt(year, 1, 1).unwrap(), leap).is_err()
        );
    }
}

#[tokio::test]
async fn numeric_and_table_paths_preserve_values_schema_identity_and_request() {
    for shape in [Shape::Mixed, Shape::Nulls, Shape::Timestamps, Shape::Prices] {
        let (wire, expected) = support::table(shape, 5);
        let fixture = Fixture::start(Script {
            messages: vec![
                Ok(support::response(&wire, false)),
                Ok(support::response(&wire, true)),
            ],
            ..Default::default()
        });
        let client = client(&fixture, EodPolicy::default(), Duration::from_secs(10)).await;
        let mut stream = client.stock_eod_batches(request()).await.unwrap();
        let first = stream.next_batch().await.unwrap().unwrap();
        let second = stream.next_batch().await.unwrap().unwrap();
        assert!(std::sync::Arc::ptr_eq(first.headers(), second.headers()));
        assert_eq!(first.row_count(), 5);
        assert_eq!(first.into_table(), expected);
        assert_eq!(second.into_table(), expected);
        assert!(stream.next_batch().await.unwrap().is_none());
        assert!(stream.next_batch().await.unwrap().is_none());
        let mut table = client.stock_eod(request()).await.unwrap();
        assert_eq!(table.next_batch().await.unwrap().unwrap(), expected);
        let requests = fixture.requests.lock().unwrap();
        assert_eq!(requests[0].params.as_ref().unwrap(), &support::query());
        assert_eq!(
            requests[0].query_info.as_ref().unwrap(),
            &client.query_info()
        );
    }
}

#[tokio::test]
async fn empty_schema_change_and_errors_are_terminal() {
    let (wire, expected) = support::table(Shape::Mixed, 1);
    let mut changed = wire.clone();
    changed.headers.swap(0, 1);
    let fixture = Fixture::start(Script {
        messages: vec![
            Ok(support::response(&Default::default(), false)),
            Ok(support::response(&wire, false)),
            Ok(support::response(&Default::default(), true)),
            Ok(support::response(&changed, false)),
        ],
        ..Default::default()
    });
    let mut stream = client(&fixture, EodPolicy::default(), Duration::from_secs(10))
        .await
        .stock_eod(request())
        .await
        .unwrap();
    assert_eq!(stream.next_batch().await.unwrap().unwrap(), expected);
    assert_eq!(stream.next_batch().await.unwrap_err(), EodError::Schema);
    assert!(stream.next_batch().await.unwrap().is_none());
    let fixture = Fixture::start(Script {
        initial_error: Some(tonic::Status::permission_denied(
            "secret-token@example.invalid",
        )),
        ..Default::default()
    });
    let error = client(&fixture, EodPolicy::default(), Duration::from_secs(10))
        .await
        .stock_eod(request())
        .await
        .err()
        .unwrap();
    assert_eq!(error, EodError::Remote(tonic::Code::PermissionDenied));
    assert!(!format!("{error:?} {error}").contains("secret"));
    assert!(std::error::Error::source(&error).is_none());
}

#[tokio::test]
async fn cancellation_pending_future_and_consumer_pause_deadline() {
    let wire = support::table(Shape::Mixed, 1).0;
    let fixture = Fixture::start(Script {
        messages: vec![Ok(support::response(&wire, false)); 2],
        message_delay: Duration::from_millis(100),
        ..Default::default()
    });
    let client = client(&fixture, EodPolicy::default(), Duration::from_secs(2)).await;
    let mut stream = client.stock_eod_batches(request()).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(10), stream.next_batch())
            .await
            .is_err()
    );
    assert_eq!(stream.next_batch().await.unwrap_err(), EodError::Cancelled);
    assert!(stream.next_batch().await.unwrap().is_none());
    let mut stream = client.stock_eod_batches(request()).await.unwrap();
    stream.cancel();
    assert_eq!(stream.next_batch().await.unwrap_err(), EodError::Cancelled);
    assert!(stream.next_batch().await.unwrap().is_none());
    let fixture = Fixture::start(Script {
        messages: vec![Ok(support::response(&wire, false)); 2],
        ..Default::default()
    });
    let mut stream = self::client(&fixture, EodPolicy::default(), Duration::from_millis(100))
        .await
        .stock_eod_batches(request())
        .await
        .unwrap();
    assert!(stream.next_batch().await.unwrap().is_some());
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(stream.next_batch().await.unwrap_err(), EodError::Deadline);
    assert!(stream.next_batch().await.unwrap().is_none());
}

#[tokio::test]
async fn allocation_and_count_limits_reject_before_delivery_and_release_admission() {
    let wire = support::table(Shape::Nulls, 100).0;
    let fixture = Fixture::start(Script {
        messages: vec![Ok(support::response(&wire, true))],
        ..Default::default()
    });
    let mut policy = EodPolicy::default();
    policy.decode.cells = 799;
    policy.concurrent_jobs = 1;
    let client = client(&fixture, policy, Duration::from_secs(5)).await;
    for _ in 0..3 {
        let mut stream = client.clone().stock_eod_batches(request()).await.unwrap();
        assert_eq!(
            stream.next_batch().await.unwrap_err(),
            EodError::Resource("cells")
        );
        assert!(stream.next_batch().await.unwrap().is_none());
    }
}

#[tokio::test]
async fn schema_empty_partial_not_found_and_idle_classification() {
    let wire = support::table(Shape::Mixed, 0).0;
    let fixture = Fixture::start(Script {
        messages: vec![
            Ok(support::response(&wire, false)),
            Err(tonic::Status::not_found("secret")),
        ],
        message_delay: Duration::from_millis(20),
        ..Default::default()
    });
    let mut stream = client(&fixture, EodPolicy::default(), Duration::from_secs(5))
        .await
        .stock_eod_batches(request())
        .await
        .unwrap();
    let batch = stream.next_batch().await.unwrap().unwrap();
    assert_eq!(batch.row_count(), 0);
    assert_eq!(batch.headers().len(), 8);
    assert_eq!(stream.next_batch().await.unwrap_err(), EodError::NoData);
    assert!(stream.next_batch().await.unwrap().is_none());
    let fixture = Fixture::start(Script {
        messages: vec![Ok(support::response(&wire, false))],
        message_delay: Duration::from_secs(1),
        ..Default::default()
    });
    let client = ThetaClient::with_eod_policy(
        ClientConfig {
            endpoint: Some(fixture.endpoint.clone()),
            allow_insecure: true,
            idle_timeout: Duration::from_millis(20),
            ..Default::default()
        },
        support::session().await,
        EodPolicy::default(),
    )
    .await
    .unwrap();
    let mut stream = client.stock_eod_batches(request()).await.unwrap();
    assert_eq!(stream.next_batch().await.unwrap_err(), EodError::Idle);
    assert!(stream.next_batch().await.unwrap().is_none());
}
