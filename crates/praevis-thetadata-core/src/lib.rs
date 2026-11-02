#![doc = include_str!("../README.md")]
//!
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
    #[error("unsupported timestamp precision {precision} with {nanos} additional nanoseconds")]
    TimestampPrecision { precision: i32, nanos: u32 },
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
///
/// The scale is **per value**: the service can encode prices in one column with
/// different scales in different rows, so never infer or reuse a column scale.
/// Equality is **representational**. `PartialEq`/`Eq` compare `mantissa` and
/// `exponent`, so the same number written with different scales compares unequal:
///
/// ```
/// use praevis_thetadata_core::Price;
/// let a = Price::from_wire(12345, 8).unwrap().unwrap(); // 123.45
/// let b = Price::from_wire(1234500, 6).unwrap().unwrap(); // 123.4500
/// assert_eq!(a.to_string(), "123.45");
/// assert_eq!(b.to_string(), "123.4500");
/// assert_ne!(a, b);
/// ```
///
/// Normalize both values to a common exponent before comparing or ordering them
/// numerically. A numeric comparison helper is deferred to the beta.1 API review.
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

/// Wire precision value for millisecond timestamps (`Endpoints.TimestampPrecision.MILLISECOND`).
pub const MILLISECOND_PRECISION: i32 = 0;

/// ADR-0019: accept only millisecond timestamps. Any `NANOSECOND` or unknown precision,
/// or non-zero `nanos`, is unsupported rather than silently truncated, because the
/// vendor has not defined how `nanos` relates to `epoch_ms`.
pub fn check_timestamp_precision(nanos: u32, precision: i32) -> Result<(), DataError> {
    if precision == MILLISECOND_PRECISION && nanos == 0 {
        Ok(())
    } else {
        Err(DataError::TimestampPrecision { precision, nanos })
    }
}

/// [`timestamp`] after the ADR-0019 precision check, for wire values carrying 1.0.16+ fields.
pub fn timestamp_with_precision(
    epoch_ms: u64,
    zone: i32,
    nanos: u32,
    precision: i32,
) -> Result<String, DataError> {
    check_timestamp_precision(nanos, precision)?;
    timestamp(epoch_ms, zone)
}

/// Latest supported instant, `9999-12-31T23:59:59.999Z` (EOD-L3-009). RFC 3339 has
/// four-digit years only, and later instants would format as `+10000-...`. Epoch
/// milliseconds are unsigned, so the earliest instant is the Unix epoch. New York local
/// time is behind UTC, so the same bound keeps both zones within year 9999.
pub const MAX_EPOCH_MS: u64 = 253_402_300_799_999;

/// Reject instants outside the RFC 3339-representable domain shared by the Table
/// formatter and numeric [`Timestamp`].
pub fn check_timestamp_range(epoch_ms: u64) -> Result<(), DataError> {
    if epoch_ms > MAX_EPOCH_MS {
        return Err(DataError::Timestamp(epoch_ms));
    }
    Ok(())
}

/// Millisecond RFC 3339 representation, including the source timezone's offset.
pub fn timestamp(epoch_ms: u64, zone: i32) -> Result<String, DataError> {
    check_timestamp_range(epoch_ms)?;
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

    /// EOD-L3-009 boundary catalogue. chrono and chrono-tz are pure Rust with an embedded
    /// tz database, so these strings are platform-independent. The numeric Timestamp must
    /// accept exactly the same domain and render identically.
    #[test]
    fn timestamp_boundary_catalogue_matches_table_and_numeric() {
        use crate::batch::Timestamp;
        let accepted: &[(u64, &str, &str)] = &[
            (
                0,
                "1969-12-31T19:00:00.000-05:00",
                "1970-01-01T00:00:00.000Z",
            ),
            (
                1,
                "1969-12-31T19:00:00.001-05:00",
                "1970-01-01T00:00:00.001Z",
            ),
            // New York spring-forward (2026-03-08): 01:59:59.999 EST, then 03:00 EDT.
            (
                1_772_953_199_999,
                "2026-03-08T01:59:59.999-05:00",
                "2026-03-08T06:59:59.999Z",
            ),
            (
                1_772_953_200_000,
                "2026-03-08T03:00:00.000-04:00",
                "2026-03-08T07:00:00.000Z",
            ),
            // Fall-back (2026-11-01): 01:59:59.999 EDT, then the repeated 01:00 EST hour.
            (
                1_793_512_799_999,
                "2026-11-01T01:59:59.999-04:00",
                "2026-11-01T05:59:59.999Z",
            ),
            (
                1_793_512_800_000,
                "2026-11-01T01:00:00.000-05:00",
                "2026-11-01T06:00:00.000Z",
            ),
            (
                MAX_EPOCH_MS,
                "9999-12-31T18:59:59.999-05:00",
                "9999-12-31T23:59:59.999Z",
            ),
        ];
        for &(ms, new_york, utc) in accepted {
            for (zone, text) in [(0, new_york), (1, utc)] {
                assert_eq!(timestamp(ms, zone).unwrap(), text, "{ms} zone {zone}");
                assert_eq!(Timestamp::from_wire(ms, zone).unwrap().to_rfc3339(), text);
            }
        }
        // Year 10000 and beyond, chrono's own limit, the i64 limit and unsigned values
        // beyond it are all unrepresentable in RFC 3339, for both zones and both representations.
        for ms in [
            MAX_EPOCH_MS + 1,
            8_210_266_876_799_999,
            i64::MAX as u64,
            i64::MAX as u64 + 1,
            u64::MAX,
        ] {
            for zone in [0, 1] {
                assert!(matches!(timestamp(ms, zone), Err(DataError::Timestamp(v)) if v == ms));
                assert!(
                    matches!(Timestamp::from_wire(ms, zone), Err(DataError::Timestamp(v)) if v == ms)
                );
            }
        }
        // Unknown zones fail for every instant, including valid ones.
        for zone in [-1, 2, i32::MAX] {
            assert!(matches!(timestamp(0, zone), Err(DataError::TimeZone(z)) if z == zone));
            assert!(
                matches!(Timestamp::from_wire(0, zone), Err(DataError::TimeZone(z)) if z == zone)
            );
        }
    }

    #[test]
    fn only_millisecond_precision_without_nanos_is_supported() {
        assert!(check_timestamp_precision(0, MILLISECOND_PRECISION).is_ok());
        assert_eq!(
            timestamp_with_precision(0, 1, 0, 0).unwrap(),
            timestamp(0, 1).unwrap()
        );
        for (nanos, precision) in [(1, 0), (999_999, 0), (0, 1), (500, 1), (0, 2), (0, -1)] {
            assert!(matches!(
                check_timestamp_precision(nanos, precision),
                Err(DataError::TimestampPrecision { precision: p, nanos: n }) if p == precision && n == nanos
            ));
            assert!(timestamp_with_precision(0, 1, nanos, precision).is_err());
        }
        // Precision is checked before the instant/zone, so the unsupported category wins.
        assert!(matches!(
            timestamp_with_precision(u64::MAX, 42, 1, 1),
            Err(DataError::TimestampPrecision { .. })
        ));
    }
}
