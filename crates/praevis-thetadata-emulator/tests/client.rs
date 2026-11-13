//! The real client against the emulator: login, rows with the recorded
//! schema, a recorded tier refusal and an evidence gap (ADR-0022).
use praevis_thetadata_client::{
    AuthClient, AuthConfig, ClientConfig, Credentials, NaiveDate, QueryError, ThetaClient, Tier,
    endpoints,
};
use praevis_thetadata_emulator::{Concurrency, Config, Emulator, Profile, schema};

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

async fn rows<E: endpoints::Endpoint>(
    client: &ThetaClient,
    request: &E,
) -> Result<usize, QueryError> {
    let batches = client.query(request).await?.collect().await?;
    Ok(batches.iter().map(|b| b.num_rows()).sum())
}

#[tokio::test]
async fn a_second_login_ends_the_first_session() {
    let emulator = Emulator::start(Config::default());
    let first = connect(&emulator).await;
    let eod = endpoints::StockHistoryEod::new("EMU", day(3), day(7));
    assert_eq!(rows(&first, &eod).await.unwrap(), 6);
    let second = connect(&emulator).await;
    assert!(matches!(
        rows(&first, &eod).await,
        Err(QueryError::SessionEnded)
    ));
    assert_eq!(rows(&second, &eod).await.unwrap(), 6);
}

/// Starts `n` requests that the emulator holds open (`Config::hold`), and
/// waits until it has them all.
async fn hold(
    emulator: &Emulator,
    client: &ThetaClient,
    n: usize,
) -> Vec<tokio::task::JoinHandle<()>> {
    let before = emulator.requests().len();
    let held: Vec<_> = (0..n)
        .map(|_| {
            let client = client.clone();
            tokio::spawn(async move {
                let eod = endpoints::StockHistoryEod::new("EMU", day(3), day(7));
                let _ = rows(&client, &eod).await;
            })
        })
        .collect();
    while emulator.requests().len() < before + n {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    held
}

#[tokio::test]
async fn more_requests_at_once_than_the_tier_allows_are_busy() {
    let emulator = Emulator::start(Config {
        profile: Profile::Free,
        hold: std::time::Duration::from_millis(500),
        ..Config::default()
    });
    let client = connect(&emulator).await;
    let eod = endpoints::StockHistoryEod::new("EMU", day(3), day(7));
    // FREE's documented limit is one request at once.
    let held = hold(&emulator, &client, 1).await;
    assert!(matches!(rows(&client, &eod).await, Err(QueryError::Busy)));
    // Its place is freed once the held request ends.
    for task in held {
        task.await.unwrap();
    }
    assert_eq!(rows(&client, &eod).await.unwrap(), 6);
}

#[tokio::test]
async fn an_unknown_symbol_and_an_empty_range_are_the_same_not_found() {
    let emulator = Emulator::start(Config::default());
    let client = connect(&emulator).await;
    let unknown = endpoints::StockHistoryEod::new("NOSUCHSYMBOL", day(3), day(7));
    // 2024-06-08 and 09 are a Saturday and a Sunday.
    let weekend = endpoints::StockHistoryEod::new("EMU", day(8), day(9));
    assert!(matches!(
        rows(&client, &unknown).await,
        Err(QueryError::NoData)
    ));
    assert!(matches!(
        rows(&client, &weekend).await,
        Err(QueryError::NoData)
    ));
}

#[tokio::test]
async fn all_strike_fine_quotes_wait_the_recorded_delay_scaled() {
    let emulator = Emulator::start(Config {
        // 60 s recorded, so 60 ms here.
        time_scale: 0.001,
        ..Config::default()
    });
    let client = connect(&emulator).await;
    let all_strikes = endpoints::OptionHistoryQuote::new("EMU", day(21));
    let started = std::time::Instant::now();
    assert_eq!(rows(&client, &all_strikes).await.unwrap(), 6);
    assert!(started.elapsed() >= std::time::Duration::from_millis(55));
    let mut one = endpoints::OptionHistoryQuote::new("EMU", day(21));
    one.strike = Some("542".into());
    let started = std::time::Instant::now();
    assert_eq!(rows(&client, &one).await.unwrap(), 6);
    assert!(started.elapsed() < std::time::Duration::from_millis(55));
}

#[tokio::test]
async fn a_flat_file_streams_many_messages_of_data() {
    let emulator = Emulator::start(Config::default());
    let client = connect(&emulator).await;
    let rows = rows(&client, &endpoints::OptionFlatFileEod::new(day(3)))
        .await
        .unwrap();
    assert_eq!(rows, 20 * 50, "model.json flat_file");
}

#[tokio::test]
async fn recorded_sizes_follow_the_histogram() {
    let emulator = Emulator::start(Config {
        recorded_sizes: true,
        ..Config::default()
    });
    let client = connect(&emulator).await;
    let eod = endpoints::StockHistoryEod::new("EMU", day(3), day(7));
    assert!(rows(&client, &eod).await.unwrap() > 1_000);
}

#[tokio::test]
async fn a_test_can_opt_into_the_observed_free_limit_of_three() {
    let emulator = Emulator::start(Config {
        profile: Profile::Free,
        concurrency: Concurrency::Observed,
        hold: std::time::Duration::from_millis(500),
        ..Config::default()
    });
    let client = connect(&emulator).await;
    let eod = endpoints::StockHistoryEod::new("EMU", day(3), day(7));
    let held = hold(&emulator, &client, 3).await;
    assert!(matches!(rows(&client, &eod).await, Err(QueryError::Busy)));
    for task in held {
        task.await.unwrap();
    }
}

#[tokio::test]
async fn greeks_eod_over_every_expiration_is_refused_whole_and_answered_per_day() {
    // Recorded live 2026-11-13 (model.json refusals_by_shape).
    let emulator = Emulator::start(Config::default());
    let client = connect(&emulator).await;
    let week = endpoints::OptionHistoryGreeksEod::new(
        "SPY",
        "*",
        NaiveDate::from_ymd_opt(2026, 11, 2).unwrap(),
        NaiveDate::from_ymd_opt(2026, 11, 6).unwrap(),
    );
    let whole = rows(&client, &week).await;
    assert!(
        matches!(whole, Err(QueryError::Remote(tonic::Code::InvalidArgument))),
        "{whole:?}"
    );
    // The planner splits it per day, so a job answers.
    let batches = client.job(&week).await.unwrap().collect().await.unwrap();
    let total: usize = batches.iter().map(|b| b.num_rows()).sum();
    assert_eq!(total, 5 * 6, "five one-day parts");
    // One expiration over the week is not refused.
    let one = endpoints::OptionHistoryGreeksEod::new(
        "SPY",
        NaiveDate::from_ymd_opt(2026, 11, 21).unwrap(),
        NaiveDate::from_ymd_opt(2026, 11, 2).unwrap(),
        NaiveDate::from_ymd_opt(2026, 11, 6).unwrap(),
    );
    assert!(rows(&client, &one).await.is_ok());
}
