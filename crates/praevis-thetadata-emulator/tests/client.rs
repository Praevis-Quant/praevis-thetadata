//! The real client against the emulator: login, rows with the recorded
//! schema, a recorded tier refusal and an evidence gap (ADR-0022).
use praevis_thetadata_client::{
    AuthClient, AuthConfig, ClientConfig, Credentials, NaiveDate, QueryError, ThetaClient, Tier,
    endpoints,
};
use praevis_thetadata_emulator::{Config, Emulator, Profile, schema};

async fn connect(emulator: &Emulator) -> ThetaClient {
    let session = AuthClient::new(
        AuthConfig::default()
            .with_auth_url(emulator.auth_url.clone())
            .with_allow_insecure(true),
    )
    .unwrap()
    .authenticate(&Credentials::api_key("emulator-key"))
    .await
    .unwrap();
    ThetaClient::with_session(
        ClientConfig::default()
            .with_endpoint(Some(emulator.endpoint.clone()))
            .with_allow_insecure(true),
        session,
    )
    .await
    .unwrap()
}

fn day(d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2024, 6, d).unwrap()
}

#[tokio::test]
async fn an_entitled_rpc_streams_rows_with_its_recorded_schema() {
    let emulator = Emulator::start(Config::default());
    let client = connect(&emulator).await;
    assert_eq!(client.tier(), Tier::Pro);
    let batches = client
        .query(&endpoints::StockHistoryEod::new("EMU", day(3), day(7)))
        .await
        .unwrap()
        .collect()
        .await
        .unwrap();
    let rows: usize = batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(rows, 6, "two messages of three rows (model.json)");
    let names: Vec<_> = batches[0]
        .schema()
        .fields()
        .iter()
        .map(|f| f.name().clone())
        .collect();
    let recorded: Vec<_> = schema("GetStockHistoryEod")
        .unwrap()
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(names, recorded);
    assert_eq!(emulator.requests(), ["GetStockHistoryEod"]);
    assert_eq!(emulator.logins(), 1);
}

#[tokio::test]
async fn a_recorded_tier_refusal_and_an_evidence_gap_are_errors() {
    let emulator = Emulator::start(Config {
        profile: Profile::Free,
        ..Config::default()
    });
    let client = connect(&emulator).await;
    assert_eq!(client.tier(), Tier::Free);
    let denied = client
        .query(&endpoints::StockSnapshotOhlc::new("EMU"))
        .await
        .err()
        .expect("refused");
    assert!(
        matches!(denied, QueryError::Remote(tonic::Code::PermissionDenied)),
        "{denied:?}"
    );
    let emulator = Emulator::start(Config::default());
    let client = connect(&emulator).await;
    let gap = client
        .query(&endpoints::OptionFlatFileQuote::new(day(3)))
        .await
        .err()
        .expect("not emulated");
    assert!(
        matches!(
            gap,
            QueryError::Remote(tonic::Code::InvalidArgument | tonic::Code::Unimplemented)
        ),
        "{gap:?}"
    );
}
