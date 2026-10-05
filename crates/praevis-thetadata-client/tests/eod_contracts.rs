//! Synthetic acceptance cases shared by the numeric and Table public APIs.
#[allow(dead_code)]
mod support;
use praevis_thetadata_client::{
    ClientConfig, EodBatchStream, EodError, EodPolicy, EodTableStream, NaiveDate, StockEodRequest,
    Table, ThetaClient,
};
use praevis_thetadata_proto::endpoints::{self as wire, data_value::DataType};
use std::{error::Error, time::Duration};
use support::{Fixture, Script};

enum Stream {
    Numeric(EodBatchStream),
    Table(EodTableStream),
}
impl Stream {
    async fn open(client: &ThetaClient, table: bool) -> Result<Self, EodError> {
        let date = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
        let request = StockEodRequest::new("SYNTHETIC", date, date).unwrap();
        if table {
            client.stock_eod(request).await.map(Self::Table)
        } else {
            client.stock_eod_batches(request).await.map(Self::Numeric)
        }
    }
    async fn next(&mut self) -> Result<Option<Table>, EodError> {
        match self {
            Self::Numeric(s) => s.next_batch().await.map(|b| b.map(|b| b.into_table())),
            Self::Table(s) => s.next_batch().await,
        }
    }
    fn cancel(&mut self) {
        match self {
            Self::Numeric(s) => s.cancel(),
            Self::Table(s) => s.cancel(),
        }
    }
}
fn table(value: DataType) -> wire::DataTable {
    wire::DataTable {
        headers: vec!["h".into()],
        data_table: vec![wire::DataValueList {
            values: vec![wire::DataValue {
                data_type: Some(value),
            }],
        }],
    }
}
async fn client(fixture: &Fixture, policy: EodPolicy) -> ThetaClient {
    ThetaClient::with_eod_policy(
        ClientConfig {
            endpoint: Some(fixture.endpoint.clone()),
            allow_insecure: true,
            request_timeout: Duration::from_secs(10),
            ..Default::default()
        },
        support::session().await,
        policy,
    )
    .await
    .unwrap()
}
fn safe(error: EodError) {
    assert!(!format!("{error:?} {error}").contains("SENTINEL"));
    assert!(error.source().is_none());
}

#[tokio::test]
async fn every_remote_code_stays_remote_except_not_found_even_with_spoofed_text() {
    use tonic::Code::*;
    for code in [
        Cancelled,
        Unknown,
        InvalidArgument,
        DeadlineExceeded,
        NotFound,
        AlreadyExists,
        PermissionDenied,
        ResourceExhausted,
        FailedPrecondition,
        Aborted,
        OutOfRange,
        Unimplemented,
        Internal,
        Unavailable,
        DataLoss,
        Unauthenticated,
    ] {
        let mut status = tonic::Status::new(
            code,
            "SENTINEL Error, decoded message length too large: found 999 bytes, the limit is: 1 bytes",
        );
        status
            .metadata_mut()
            .insert("secret", "SENTINEL".parse().unwrap());
        let expected = if code == NotFound {
            EodError::NoData
        } else {
            EodError::Remote(code)
        };
        for table_api in [false, true] {
            for partial in [false, true] {
                let good = table(DataType::Number(7));
                let fixture = Fixture::start(Script {
                    messages: if partial {
                        vec![
                            Result::Ok(support::response(&good, false)),
                            Err(status.clone()),
                        ]
                    } else {
                        vec![]
                    },
                    initial_error: (!partial).then(|| status.clone()),
                    ..Default::default()
                });
                let client = client(&fixture, EodPolicy::default()).await;
                if partial {
                    let mut stream = Stream::open(&client, table_api).await.unwrap();
                    assert!(stream.next().await.unwrap().is_some());
                    let error = stream.next().await.unwrap_err();
                    assert_eq!(error, expected);
                    safe(error);
                    for _ in 0..3 {
                        assert!(stream.next().await.unwrap().is_none());
                    }
                } else {
                    let error = Stream::open(&client, table_api).await.err().unwrap();
                    assert_eq!(error, expected);
                    safe(error);
                }
                assert_eq!(fixture.requests.lock().unwrap().len(), 1);
            }
        }
    }
}

#[tokio::test]
async fn local_faults_before_and_after_delivery_fuse_and_release_single_job() {
    let good = table(DataType::Number(7));
    let mut cases = Vec::new();
    let response = |t: &wire::DataTable| support::response(t, false);
    let default = EodPolicy::default();
    let mut corrupt = response(&good);
    corrupt.compressed_data = b"SENTINEL".to_vec();
    cases.push((
        "protobuf",
        corrupt.clone(),
        default.clone(),
        EodError::Decode,
    ));
    corrupt.compression_description.as_mut().unwrap().algo = 1;
    cases.push((
        "zstd corruption",
        corrupt,
        default.clone(),
        EodError::Decode,
    ));
    let mut unsupported = response(&good);
    unsupported.compression_description.as_mut().unwrap().algo = 42;
    cases.push((
        "compression",
        unsupported,
        default.clone(),
        EodError::Unsupported,
    ));
    let mut manifest = response(&good);
    manifest.flat_file_manifest = Some(Default::default());
    cases.push(("manifest", manifest, default.clone(), EodError::Unsupported));
    let mut duplicate = good.clone();
    duplicate.headers.push("h".into());
    duplicate.data_table.clear();
    cases.push((
        "duplicate",
        response(&duplicate),
        default.clone(),
        EodError::Schema,
    ));
    let mut ragged = good.clone();
    ragged.data_table[0].values.clear();
    cases.push((
        "ragged",
        response(&ragged),
        default.clone(),
        EodError::Schema,
    ));
    ragged.headers.clear();
    cases.push((
        "zero width",
        response(&ragged),
        default.clone(),
        EodError::Schema,
    ));
    for value in [
        DataType::Price(wire::Price {
            value: 1,
            r#type: 20,
        }),
        DataType::Timestamp(wire::ZonedDateTime {
            epoch_ms: u64::MAX,
            zone: 1,
        }),
        DataType::Timestamp(wire::ZonedDateTime {
            epoch_ms: 0,
            zone: 42,
        }),
    ] {
        cases.push((
            "invalid value",
            response(&table(value)),
            default.clone(),
            EodError::Decode,
        ));
    }
    let mut declared = response(&good);
    declared.original_size = i32::MAX;
    cases.push((
        "declared",
        declared,
        default.clone(),
        EodError::Resource("declared bytes"),
    ));
    for (name, bytes, expected) in [
        ("encoded", 100, "encoded bytes"),
        ("frame", 2048, "envelope bytes"),
    ] {
        let mut policy = default.clone();
        policy.decode.encoded_bytes = 64;
        let mut data = response(&good);
        data.compressed_data = vec![0; bytes];
        cases.push((name, data, policy, EodError::Resource(expected)));
    }
    let large = table(DataType::Text("SENTINEL".repeat(1024)));
    let mut policy = default.clone();
    policy.decode.zstd_window_log = 10;
    cases.push((
        "window",
        support::response(&large, true),
        policy,
        EodError::Resource("zstd window"),
    ));
    let mut policy = default.clone();
    policy.decode.decompressed_bytes = 100;
    let mut bomb = support::response(&large, true);
    bomb.original_size = 1;
    cases.push((
        "expansion",
        bomb,
        policy,
        EodError::Resource("decompressed bytes"),
    ));
    for (kind, name) in [
        (0, "headers"),
        (1, "rows"),
        (2, "cells"),
        (3, "text bytes"),
        (4, "header bytes"),
    ] {
        let mut policy = default.clone();
        let mut data = good.clone();
        match kind {
            0 => {
                policy.decode.headers = 1;
                data.headers.push("other".into());
            }
            1 => {
                policy.decode.rows = 1;
                data.data_table.push(data.data_table[0].clone());
            }
            2 => {
                policy.decode.cells = 1;
                data.data_table.push(data.data_table[0].clone());
            }
            3 => {
                policy.decode.text_bytes = 1;
                data = table(DataType::Text("SENTINEL".into()));
            }
            _ => {
                policy.decode.header_bytes = 1;
                data.headers[0] = "SENTINEL".into();
            }
        }
        cases.push((name, response(&data), policy, EodError::Resource(name)));
    }
    for (name, fault, mut policy, expected) in cases {
        policy.concurrent_jobs = 1;
        policy.concurrent_streams = 1;
        for table_api in [false, true] {
            for partial in [false, true] {
                let mut messages = Vec::new();
                if partial {
                    messages.push(Ok(response(&good)));
                }
                messages.push(Ok(fault.clone()));
                messages.push(Ok(response(&good))); // Must never escape after the error.
                let fixture = Fixture::start(Script {
                    messages,
                    ..Default::default()
                });
                let client = client(&fixture, policy.clone()).await;
                // Repeating on a clone detects leaked single-job reservations.
                for _ in 0..2 {
                    let mut stream = Stream::open(&client.clone(), table_api).await.unwrap();
                    if partial {
                        assert!(stream.next().await.unwrap().is_some(), "{name}");
                    }
                    let error = stream.next().await.unwrap_err();
                    assert_eq!(
                        error, expected,
                        "{name}, table={table_api}, partial={partial}"
                    );
                    safe(error);
                    for _ in 0..3 {
                        assert!(stream.next().await.unwrap().is_none());
                    }
                }
                assert_eq!(fixture.requests.lock().unwrap().len(), 2);
            }
        }
    }
}

#[tokio::test]
async fn empty_completion_schema_changes_and_cancellation_match_both_apis() {
    let good = table(DataType::Number(7));
    let mut empty = good.clone();
    empty.data_table.clear();
    for table_api in [false, true] {
        for messages in [
            vec![],
            vec![Ok(support::response(&Default::default(), false))],
            vec![Ok(support::response(&empty, true))],
        ] {
            let schema_empty = messages.first().is_some_and(|r| {
                r.as_ref().unwrap().original_size > 0
                    && r.as_ref()
                        .unwrap()
                        .compression_description
                        .as_ref()
                        .unwrap()
                        .algo
                        == 1
            });
            let fixture = Fixture::start(Script {
                messages,
                ..Default::default()
            });
            let client = client(&fixture, EodPolicy::default()).await;
            let mut stream = Stream::open(&client, table_api).await.unwrap();
            if schema_empty {
                let batch = stream.next().await.unwrap().unwrap();
                assert!(batch.rows.is_empty());
                assert_eq!(batch.headers, vec!["h"]);
            }
            for _ in 0..3 {
                assert!(stream.next().await.unwrap().is_none());
            }
        }
        let fixture = Fixture::start(Script {
            messages: vec![Ok(support::response(&good, false)); 2],
            ..Default::default()
        });
        let client = client(&fixture, EodPolicy::default()).await;
        for partial in [false, true] {
            let mut stream = Stream::open(&client, table_api).await.unwrap();
            if partial {
                assert!(stream.next().await.unwrap().is_some());
            }
            stream.cancel();
            assert_eq!(stream.next().await.unwrap_err(), EodError::Cancelled);
            for _ in 0..3 {
                assert!(stream.next().await.unwrap().is_none());
            }
        }
    }
}

#[tokio::test]
async fn both_apis_preserve_scalar_boundaries_order_and_exact_query_text() {
    use praevis_thetadata_core::{Price, Value};
    let mut wire = wire::DataTable {
        headers: vec![" unusual Unicode h é ".into()],
        data_table: vec![],
    };
    let mut expected = Table {
        headers: wire.headers.clone(),
        rows: vec![],
    };
    let mut pairs = vec![
        (Some(DataType::Text("".into())), Value::Text("".into())),
        (
            Some(DataType::Text("é\0測試".into())),
            Value::Text("é\0測試".into()),
        ),
        (Some(DataType::Number(i64::MIN)), Value::Integer(i64::MIN)),
        (Some(DataType::Number(i64::MAX)), Value::Integer(i64::MAX)),
        (Some(DataType::Boolean(false)), Value::Boolean(false)),
        (Some(DataType::Boolean(true)), Value::Boolean(true)),
        (Some(DataType::NullValue(0)), Value::Null),
        (None, Value::Null),
    ];
    for scale in 0..=19 {
        for mantissa in [i32::MIN, -1, 0, i32::MAX] {
            pairs.push((
                Some(DataType::Price(wire::Price {
                    value: mantissa,
                    r#type: scale,
                })),
                if scale == 0 {
                    Value::Null
                } else {
                    Value::Price(Price {
                        mantissa,
                        exponent: scale - 10,
                    })
                },
            ));
        }
    }
    // Repeated/out-of-order values must remain in source order.
    pairs.extend([
        (Some(DataType::Number(7)), Value::Integer(7)),
        (Some(DataType::Number(3)), Value::Integer(3)),
        (Some(DataType::Number(7)), Value::Integer(7)),
    ]);
    for (data_type, value) in pairs {
        wire.data_table.push(wire::DataValueList {
            values: vec![wire::DataValue { data_type }],
        });
        expected.rows.push(vec![value]);
    }
    for compressed in [false, true] {
        let fixture = Fixture::start(Script {
            messages: vec![Ok(support::response(&wire, compressed))],
            ..Default::default()
        });
        let client = client(&fixture, EodPolicy::default()).await;
        for table_api in [false, true] {
            let mut stream = Stream::open(&client, table_api).await.unwrap();
            assert_eq!(stream.next().await.unwrap().unwrap(), expected);
            assert!(stream.next().await.unwrap().is_none());
        }
        let start = NaiveDate::from_ymd_opt(1, 1, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(9999, 12, 31).unwrap();
        drop(
            client
                .stock_eod_batches(StockEodRequest::new("a.B-é", start, end).unwrap())
                .await
                .unwrap(),
        );
        let requests = fixture.requests.lock().unwrap();
        let query = requests.last().unwrap().params.as_ref().unwrap();
        assert_eq!(
            (&query.symbol, &query.start_date, &query.end_date),
            (
                &"a.B-é".to_owned(),
                &"0001-01-01".to_owned(),
                &"9999-12-31".to_owned()
            )
        );
    }
}

#[tokio::test]
async fn all_duration_and_policy_configuration_failures_precede_connection() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let session = support::session().await;
    let config = ClientConfig {
        endpoint: Some(format!("http://{}", listener.local_addr().unwrap())),
        allow_insecure: true,
        ..Default::default()
    };
    let mut policies = Vec::new();
    for field in 0..8 {
        let mut p = EodPolicy::default();
        match field {
            0 => p.decode.encoded_bytes = 0,
            1 => p.decode.decompressed_bytes = 0,
            2 => p.decode.headers = 0,
            3 => p.decode.rows = 0,
            4 => p.decode.cells = 0,
            5 => p.decode.text_bytes = 0,
            6 => p.decode.header_bytes = 0,
            _ => p.decode.allocated_bytes = 0,
        }
        policies.push(p);
    }
    for field in 0..7 {
        let mut p = EodPolicy::default();
        match field {
            0 => p.decode.encoded_bytes = usize::MAX,
            1 => p.decode.decompressed_bytes = usize::MAX,
            2 => p.decode.headers = usize::MAX,
            3 => p.decode.header_bytes = usize::MAX,
            4 => p.decode.zstd_window_log = 9,
            5 => p.decode.zstd_window_log = 28,
            _ => p.concurrent_jobs = 65,
        }
        policies.push(p);
    }
    for policy in policies {
        assert_eq!(
            ThetaClient::with_eod_policy(config.clone(), session.clone(), policy)
                .await
                .err()
                .unwrap(),
            EodError::Configuration
        );
    }
    for field in 0..3 {
        for value in [Duration::ZERO, Duration::MAX] {
            let mut c = config.clone();
            match field {
                0 => c.connect_timeout = value,
                1 => c.request_timeout = value,
                _ => c.idle_timeout = value,
            }
            assert_eq!(
                ThetaClient::with_eod_policy(c, session.clone(), EodPolicy::default())
                    .await
                    .err()
                    .unwrap(),
                EodError::Configuration
            );
        }
    }
    let mut insecure = config;
    insecure.allow_insecure = false;
    assert_eq!(
        ThetaClient::with_eod_policy(insecure, session, EodPolicy::default())
            .await
            .err()
            .unwrap(),
        EodError::Configuration
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(20), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn stalled_tls_connection_is_bounded_and_diagnostics_are_safe() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("https://{}", listener.local_addr().unwrap());
    let session = support::session().await;
    let server = tokio::spawn(async move {
        let (_socket, _) = listener.accept().await.unwrap();
        std::future::pending::<()>().await;
    });
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        ThetaClient::with_eod_policy(
            ClientConfig {
                endpoint: Some(endpoint),
                connect_timeout: Duration::from_millis(50),
                ..Default::default()
            },
            session,
            EodPolicy::default(),
        ),
    )
    .await
    .unwrap();
    server.abort();
    let error = result.err().unwrap();
    assert_eq!(error, EodError::Connection);
    safe(error);
}

#[tokio::test]
async fn idle_deadline_and_pending_cancellation_match_before_and_after_delivery() {
    let good = table(DataType::Number(7));
    for table_api in [false, true] {
        for partial in [false, true] {
            for mode in 0..3 {
                let fixture = Fixture::start(Script {
                    messages: vec![Ok(support::response(&good, false)); 2],
                    pause_before: Some((usize::from(partial), Duration::from_secs(2))),
                    ..Default::default()
                });
                let client = ThetaClient::with_eod_policy(
                    ClientConfig {
                        endpoint: Some(fixture.endpoint.clone()),
                        allow_insecure: true,
                        idle_timeout: if mode == 0 {
                            Duration::from_millis(100)
                        } else {
                            Duration::from_secs(5)
                        },
                        request_timeout: if mode == 1 {
                            Duration::from_millis(250)
                        } else {
                            Duration::from_secs(5)
                        },
                        ..Default::default()
                    },
                    support::session().await,
                    EodPolicy::default(),
                )
                .await
                .unwrap();
                let mut stream = Stream::open(&client, table_api).await.unwrap();
                if partial {
                    assert!(stream.next().await.unwrap().is_some());
                }
                let expected = match mode {
                    0 => EodError::Idle,
                    1 => EodError::Deadline,
                    _ => {
                        assert!(
                            tokio::time::timeout(Duration::from_millis(25), stream.next())
                                .await
                                .is_err()
                        );
                        EodError::Cancelled
                    }
                };
                assert_eq!(stream.next().await.unwrap_err(), expected);
                for _ in 0..3 {
                    assert!(stream.next().await.unwrap().is_none());
                }
                assert_eq!(fixture.requests.lock().unwrap().len(), 1);
            }
        }
    }
}
