//! Runs the README example against the staged archive.
use praevis_thetadata_core::{Price, Timestamp, check_timestamp_precision};

fn main() {
    let price = Price::from_wire(12345, 8).unwrap().unwrap();
    assert_eq!(price.to_string(), "123.45");
    let utc = Timestamp::from_wire(0, 1).unwrap();
    assert_eq!(utc.to_rfc3339(), "1970-01-01T00:00:00.000Z");
    assert!(check_timestamp_precision(1, 0).is_err());
    println!("core-consumer: exact price {price}, timestamp {}", utc.to_rfc3339());
}
