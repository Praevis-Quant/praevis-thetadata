//! Transport-independent market data values. No network or CLI dependencies.

use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;
mod batch;
pub use batch::{BatchValue, DataBatch, TimeZone, Timestamp};

#[derive(Debug, thiserror::Error)]
pub enum DataError {
    #[error("invalid price scale {0}; expected 0..=19")]
    PriceScale(i32),
    #[error("timestamp is outside the supported range: {0}")]
    Timestamp(u64),
    #[error("unknown timezone {0}")]
    TimeZone(i32),
    #[error("row {row} has {actual} values, expected {expected}")]
    RowWidth {
        row: usize,
        actual: usize,
        expected: usize,
    },
    #[error("duplicate column name: {0}")]
    DuplicateColumn(String),
}

/// Preserve the vendor's integer price and scale, avoiding early float rounding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Price {
    pub mantissa: i32,
    pub exponent: i32,
}

impl Price {
    pub fn from_wire(value: i32, scale: i32) -> Result<Option<Self>, DataError> {
        match scale {
            0 => Ok(None),
            1..=19 => Ok(Some(Self {
                mantissa: value,
                exponent: scale - 10,
            })),
            _ => Err(DataError::PriceScale(scale)),
        }
    }
}

impl std::fmt::Display for Price {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // i64 permits abs(i32::MIN) without overflow.
        let magnitude = i64::from(self.mantissa).abs();
        let sign = if self.mantissa < 0 { "-" } else { "" };
        if self.exponent >= 0 {
            write!(f, "{sign}{magnitude}{}", "0".repeat(self.exponent as usize))
        } else {
            let digits = (-self.exponent) as usize;
            let factor = 10_i64.pow(digits as u32);
            write!(
                f,
                "{sign}{}.{:0digits$}",
                magnitude / factor,
                magnitude % factor
            )
        }
    }
}

/// Millisecond RFC 3339 representation, including the source timezone's offset.
pub fn timestamp(epoch_ms: u64, zone: i32) -> Result<String, DataError> {
    let millis = i64::try_from(epoch_ms).map_err(|_| DataError::Timestamp(epoch_ms))?;
    let utc =
        DateTime::<Utc>::from_timestamp_millis(millis).ok_or(DataError::Timestamp(epoch_ms))?;
    match zone {
        0 => Ok(utc
            .with_timezone(&chrono_tz::America::New_York)
            .to_rfc3339_opts(SecondsFormat::Millis, true)),
        1 => Ok(utc.to_rfc3339_opts(SecondsFormat::Millis, true)),
        _ => Err(DataError::TimeZone(zone)),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Value {
    Null,
    Text(String),
    Integer(i64),
    Price(Price),
    Boolean(bool),
    Timestamp(String),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

impl Table {
    pub fn validate(&self) -> Result<(), DataError> {
        let mut names = std::collections::HashSet::new();
        for name in &self.headers {
            if !names.insert(name) {
                return Err(DataError::DuplicateColumn(name.clone()));
            }
        }
        for (row, values) in self.rows.iter().enumerate() {
            if values.len() != self.headers.len() {
                return Err(DataError::RowWidth {
                    row,
                    actual: values.len(),
                    expected: self.headers.len(),
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn price_scales_are_exact_and_missing_is_null() {
        assert_eq!(
            Price::from_wire(12345, 8).unwrap().unwrap().to_string(),
            "123.45"
        );
        assert_eq!(
            Price::from_wire(-1, 1).unwrap().unwrap().to_string(),
            "-0.000000001"
        );
        assert_eq!(
            Price::from_wire(i32::MIN, 10).unwrap().unwrap().to_string(),
            "-2147483648"
        );
        assert_eq!(
            Price::from_wire(1, 19).unwrap().unwrap().to_string(),
            "1000000000"
        );
        assert_eq!(Price::from_wire(42, 0).unwrap(), None);
        assert!(Price::from_wire(1, 20).is_err());
    }

    #[test]
    fn new_york_timestamps_follow_daylight_saving() {
        for (input, expected) in [
            ("2026-01-15T12:00:00Z", "2026-01-15T07:00:00.000-05:00"),
            ("2026-07-15T12:00:00Z", "2026-07-15T08:00:00.000-04:00"),
        ] {
            let ms = DateTime::parse_from_rfc3339(input)
                .unwrap()
                .timestamp_millis() as u64;
            assert_eq!(timestamp(ms, 0).unwrap(), expected);
        }
        assert!(timestamp(u64::MAX, 0).is_err());
        assert!(timestamp(0, 2).is_err());
    }
}
