mod support;
use praevis_thetadata_client::{ClientConfig, Error, ThetaClient};
use std::time::Duration;
use support::{Fixture, Script, Shape};

async fn client(fixture: &Fixture) -> ThetaClient {
    ThetaClient::with_session(
        ClientConfig {
            endpoint: Some(fixture.endpoint.clone()),
            allow_insecure: true,
            idle_timeout: Duration::from_secs(5),
            ..Default::default()
        },
        support::session().await,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn exact_values_compression_multiple_batches_and_wire_request() {
    for shape in [Shape::Mixed, Shape::Nulls, Shape::Timestamps, Shape::Prices] {
        let (table, expected) = support::table(shape, 4);
        let fixture = Fixture::start(Script {
            messages: vec![
                Ok(support::response(&table, false)),
                Ok(support::response(&table, true)),
            ],
            ..Default::default()
        });
        let client = client(&fixture).await;
        let mut stream = client.stock_history_eod(support::query()).await.unwrap();
        assert_eq!(stream.next_batch().await.unwrap().unwrap(), expected);
        assert_eq!(stream.next_batch().await.unwrap().unwrap(), expected);
        assert!(stream.next_batch().await.unwrap().is_none());
        let requests = fixture.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        let request = &requests[0];
        assert_eq!(request.params.as_ref().unwrap(), &support::query());
        assert_eq!(request.query_info.as_ref().unwrap(), &client.query_info());
        let info = request.query_info.as_ref().unwrap();
        assert_eq!(
            info.auth_token.as_ref().unwrap().session_uuid,
            support::TOKEN
        );
        assert_eq!(info.email_hint, support::EMAIL);
        assert_eq!(info.query_parameters.len(), 1);
        assert_eq!(info.query_parameters["client"], "rust");
    }
}

#[tokio::test]
async fn clean_empty_and_schema_bearing_empty_are_not_not_found() {
    for messages in [
        vec![],
        vec![Ok(support::response(
            &support::table(Shape::Mixed, 0).0,
            false,
        ))],
    ] {
        let fixture = Fixture::start(Script {
            messages,
            ..Default::default()
        });
        let mut stream = client(&fixture)
            .await
            .stock_history_eod(support::query())
            .await
            .unwrap();
        if let Some(batch) = stream.next_batch().await.unwrap() {
            assert_eq!(batch.headers.len(), 8);
            assert!(batch.rows.is_empty());
            assert!(stream.next_batch().await.unwrap().is_none());
        }
    }
    let fixture = Fixture::start(Script {
        initial_error: Some(tonic::Status::not_found("synthetic")),
        ..Default::default()
    });
    assert!(matches!(
        client(&fixture)
            .await
            .stock_history_eod(support::query())
            .await,
        Err(Error::NoData)
    ));
}

#[tokio::test]
async fn partial_delivery_then_remote_failure_is_not_complete_history() {
    let (table, expected) = support::table(Shape::Mixed, 2);
    let fixture = Fixture::start(Script {
        messages: vec![
            Ok(support::response(&table, true)),
            Err(tonic::Status::permission_denied("synthetic")),
        ],
        // Separate frames so the valid batch is delivered before terminal trailers.
        message_delay: Duration::from_millis(10),
        ..Default::default()
    });
    let mut stream = client(&fixture)
        .await
        .stock_history_eod(support::query())
        .await
        .unwrap();
    assert_eq!(stream.next_batch().await.unwrap().unwrap(), expected);
    assert!(
        matches!(stream.next_batch().await, Err(Error::Rpc(status)) if status.code() == tonic::Code::PermissionDenied)
    );
}

#[tokio::test]
async fn corrupted_unsupported_and_shape_errors_survive_the_transport() {
    let (mut ragged, _) = support::table(Shape::Mixed, 1);
    ragged.headers.pop();
    let mut corrupt = support::response(&ragged, false);
    corrupt.compressed_data = vec![255];
    let mut unsupported = support::response(&ragged, false);
    unsupported.compression_description.as_mut().unwrap().algo = 99;
    for (response, kind) in [
        (corrupt, 0),
        (unsupported, 1),
        (support::response(&ragged, true), 2),
    ] {
        let fixture = Fixture::start(Script {
            messages: vec![Ok(response)],
            ..Default::default()
        });
        let mut stream = client(&fixture)
            .await
            .stock_history_eod(support::query())
            .await
            .unwrap();
        let error = stream.next_batch().await.unwrap_err();
        assert!(matches!(
            (kind, error),
            (0, Error::Protobuf(_)) | (1, Error::UnsupportedCompression(99)) | (2, Error::Data(_))
        ));
    }
}

#[tokio::test]
async fn idle_timeout_and_explicit_insecure_opt_in() {
    let fixture = Fixture::start(Script {
        messages: vec![Ok(support::response(
            &support::table(Shape::Mixed, 1).0,
            false,
        ))],
        message_delay: Duration::from_millis(500),
        ..Default::default()
    });
    let session = support::session().await;
    assert!(matches!(
        ThetaClient::with_session(
            ClientConfig {
                endpoint: Some(fixture.endpoint.clone()),
                ..Default::default()
            },
            session
        )
        .await,
        Err(Error::Config(_))
    ));
    let client = ThetaClient::with_session(
        ClientConfig {
            endpoint: Some(fixture.endpoint.clone()),
            allow_insecure: true,
            idle_timeout: Duration::from_millis(20),
            ..Default::default()
        },
        support::session().await,
    )
    .await
    .unwrap();
    let mut stream = client.stock_history_eod(support::query()).await.unwrap();
    assert!(matches!(stream.next_batch().await, Err(Error::IdleTimeout)));
    // Drop aborts the local fixture without waiting for the delayed response.
}
