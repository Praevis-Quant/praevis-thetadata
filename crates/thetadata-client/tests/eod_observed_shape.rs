//! Constructed data using only the column order/type mix from the reviewed live
//! observation. No captured prices, timestamps, rows, identity or credentials.
//! This is a regression fixture, not a guaranteed vendor schema.
#[allow(dead_code)]
mod support;

use support::{Fixture, Script};
use thetadata_client::{ClientConfig, NaiveDate, StockEodRequest, ThetaClient};
use thetadata_core::{BatchValue, Price, Table, TimeZone, Value};
use thetadata_proto::endpoints::{self as wire, data_value::DataType};

const HEADERS: [&str; 16] = [
    "created",
    "last_trade",
    "open",
    "high",
    "low",
    "close",
    "volume",
    "count",
    "bid_size",
    "bid_exchange",
    "bid",
    "bid_condition",
    "ask_size",
    "ask_exchange",
    "ask",
    "ask_condition",
];

fn constructed_row() -> (wire::DataTable, Table) {
    // Intentionally artificial, distinct values make swaps/type coercion visible.
    let cells = vec![
        DataType::Timestamp(wire::ZonedDateTime {
            epoch_ms: 0,
            zone: 1,
        }),
        DataType::Timestamp(wire::ZonedDateTime {
            epoch_ms: 0,
            zone: 0,
        }),
        DataType::Price(wire::Price {
            value: 10101,
            r#type: 8,
        }),
        DataType::Price(wire::Price {
            value: 20202,
            r#type: 8,
        }),
        DataType::Price(wire::Price {
            value: 30303,
            r#type: 8,
        }),
        DataType::Price(wire::Price {
            value: 40404,
            r#type: 8,
        }),
        DataType::Number(9007199254740993), // Must not round through f64.
        DataType::Number(17),
        DataType::Number(23),
        DataType::Number(29),
        DataType::Price(wire::Price {
            value: 50505,
            r#type: 8,
        }),
        DataType::Number(31),
        DataType::Number(37),
        DataType::Number(41),
        DataType::Price(wire::Price {
            value: 60606,
            r#type: 8,
        }),
        DataType::Number(43),
    ];
    let price = |mantissa| {
        Value::Price(Price {
            mantissa,
            exponent: -2,
        })
    };
    let expected = vec![
        Value::Timestamp("1970-01-01T00:00:00.000Z".into()),
        Value::Timestamp("1969-12-31T19:00:00.000-05:00".into()),
        price(10101),
        price(20202),
        price(30303),
        price(40404),
        Value::Integer(9007199254740993),
        Value::Integer(17),
        Value::Integer(23),
        Value::Integer(29),
        price(50505),
        Value::Integer(31),
        Value::Integer(37),
        Value::Integer(41),
        price(60606),
        Value::Integer(43),
    ];
    let headers: Vec<String> = HEADERS.iter().map(|s| (*s).into()).collect();
    (
        wire::DataTable {
            headers: headers.clone(),
            data_table: vec![wire::DataValueList {
                values: cells
                    .into_iter()
                    .map(|data_type| wire::DataValue {
                        data_type: Some(data_type),
                    })
                    .collect(),
            }],
        },
        Table {
            headers,
            rows: vec![expected],
        },
    )
}

fn request() -> StockEodRequest {
    let date = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
    StockEodRequest::new("SYNTHETIC", date, date).unwrap()
}

#[tokio::test]
async fn observed_column_shape_preserves_exact_values_across_three_interfaces() {
    let (wire, expected) = constructed_row();
    let fixture = Fixture::start(Script {
        messages: vec![
            Ok(support::response(&wire, false)),
            Ok(support::response(&wire, true)),
        ],
        ..Default::default()
    });
    let client = ThetaClient::with_session(
        ClientConfig {
            endpoint: Some(fixture.endpoint.clone()),
            allow_insecure: true,
            ..Default::default()
        },
        support::session().await,
    )
    .await
    .unwrap();

    let mut raw_query = support::query();
    raw_query.end_date = raw_query.start_date.clone();
    let mut raw = client.stock_history_eod(raw_query.clone()).await.unwrap();
    for _ in 0..2 {
        assert_eq!(raw.next_batch().await.unwrap().unwrap(), expected);
    }
    assert!(raw.next_batch().await.unwrap().is_none());

    let mut numeric = client.stock_eod_batches(request()).await.unwrap();
    let mut previous_headers = None;
    for _ in 0..2 {
        let batch = numeric.next_batch().await.unwrap().unwrap();
        assert_eq!(batch.row_count(), 1);
        assert_eq!(batch.headers().as_ref(), expected.headers);
        if let Some(previous) = previous_headers.replace(batch.headers().clone()) {
            assert!(std::sync::Arc::ptr_eq(&previous, batch.headers()));
        }
        // Independent numeric checks, before invoking the presentation adapter.
        for (index, zone) in [(0, TimeZone::Utc), (1, TimeZone::NewYork)] {
            let BatchValue::Timestamp(value) = batch.cells()[index] else {
                panic!("expected numeric timestamp")
            };
            assert_eq!(value.epoch_ms(), 0);
            assert_eq!(value.zone(), zone);
        }
        for (actual, expected) in batch.cells()[2..].iter().zip(&expected.rows[0][2..]) {
            match (actual, expected) {
                (BatchValue::Integer(a), Value::Integer(e)) => assert_eq!(a, e),
                (BatchValue::Price(a), Value::Price(e)) => assert_eq!(a, e),
                _ => panic!("numeric type changed"),
            }
        }
        assert_eq!(batch.into_table(), expected);
    }
    assert!(numeric.next_batch().await.unwrap().is_none());
    assert!(numeric.next_batch().await.unwrap().is_none());

    let mut table = client.stock_eod(request()).await.unwrap();
    for _ in 0..2 {
        assert_eq!(table.next_batch().await.unwrap().unwrap(), expected);
    }
    assert!(table.next_batch().await.unwrap().is_none());
    assert!(table.next_batch().await.unwrap().is_none());
    let requests = fixture.requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    for request in requests.iter() {
        assert_eq!(request.params.as_ref().unwrap(), &raw_query);
        assert_eq!(request.query_info.as_ref().unwrap(), &client.query_info());
    }
}
