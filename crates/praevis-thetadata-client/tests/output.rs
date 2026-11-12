//! Parquet output from query and job streams (P1 PR 5): the file appears only
//! when the whole stream is written.
#![cfg(feature = "parquet")]
#[allow(dead_code)]
mod support;

use chrono::NaiveDate;
use praevis_thetadata_client::{
    ClientConfig, ParquetOptions, ParquetOutputError, QueryError, SinkError, ThetaClient, endpoints,
};
use praevis_thetadata_proto::endpoints::{
    self as wire, DataTable, DataValue, DataValueList, data_value::DataType as Wire,
};
use std::path::PathBuf;
use support::{Fixture, Script};

fn two_rows() -> wire::ResponseData {
    let table = DataTable {
        headers: vec!["n".into(), "p".into()],
        data_table: (0..2)
            .map(|i| DataValueList {
                values: vec![
                    DataValue {
                        data_type: Some(Wire::Number(i)),
                    },
                    DataValue {
                        data_type: Some(Wire::Price(wire::Price {
                            value: 12345,
                            r#type: 8,
                        })),
                    },
                ],
            })
            .collect(),
    };
    support::response(&table, true)
}
async fn connect(fixture: &Fixture) -> ThetaClient {
    ThetaClient::with_session(
        ClientConfig::default()
            .with_endpoint(Some(fixture.endpoint.clone()))
            .with_allow_insecure(true),
        support::session().await,
    )
    .await
    .unwrap()
}
fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("praevis-output-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir(&dir).unwrap();
    dir
}
fn day(text: &str) -> NaiveDate {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
}

#[tokio::test]
async fn query_and_job_streams_write_whole_files() {
    let fixture = Fixture::start(Script {
        messages: vec![Ok(two_rows()), Ok(two_rows())],
        ..Default::default()
    });
    let client = connect(&fixture).await;
    let out = dir("whole");
    let one = endpoints::StockHistoryEod::new("AAPL", day("2024-06-14"), day("2024-06-14"));
    let summary = client
        .query(&one)
        .await
        .unwrap()
        .to_parquet(out.join("q.parquet"), ParquetOptions::default())
        .await
        .unwrap();
    assert_eq!(summary.rows, 4);
    // Three 366-day parts, merged in order into one file.
    let span = endpoints::StockHistoryEod::new("AAPL", day("2022-01-01"), day("2024-03-01"));
    let summary = client
        .job(&span)
        .await
        .unwrap()
        .to_parquet(out.join("j.parquet"), ParquetOptions::default())
        .await
        .unwrap();
    assert_eq!(summary.rows, 12);
    assert!(out.join("j.parquet").metadata().unwrap().len() > 0);
    // An existing target is refused before any request is read.
    let again = client
        .query(&one)
        .await
        .unwrap()
        .to_parquet(out.join("q.parquet"), ParquetOptions::default())
        .await;
    assert_eq!(again, Err(ParquetOutputError::Sink(SinkError::Exists)));
    std::fs::remove_dir_all(&out).unwrap();
}

#[tokio::test]
async fn a_failed_stream_leaves_no_file() {
    let fixture = Fixture::start(Script {
        messages: vec![Ok(two_rows()), Err(tonic::Status::unavailable("synthetic"))],
        ..Default::default()
    });
    let client = connect(&fixture).await;
    let out = dir("failed");
    let one = endpoints::StockHistoryEod::new("AAPL", day("2024-06-14"), day("2024-06-14"));
    let result = client
        .query(&one)
        .await
        .unwrap()
        .to_parquet(out.join("q.parquet"), ParquetOptions::default())
        .await;
    assert_eq!(
        result,
        Err(ParquetOutputError::Query(QueryError::Remote(
            tonic::Code::Unavailable
        )))
    );
    assert_eq!(
        std::fs::read_dir(&out).unwrap().count(),
        0,
        "no partial file remains"
    );
    std::fs::remove_dir_all(&out).unwrap();
}
