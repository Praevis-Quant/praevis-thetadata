//! Numeric values and flat, immutable-schema batches for incremental consumers.
use crate::{DataError, Price, Table, Value, timestamp};
use chrono::{DateTime, Utc};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeZone {
    NewYork,
    Utc,
}

/// Validated instant, retaining the source zone without allocating display text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timestamp {
    epoch_ms: u64,
    zone: TimeZone,
}

impl Timestamp {
    pub fn from_wire(epoch_ms: u64, zone: i32) -> Result<Self, DataError> {
        let millis = i64::try_from(epoch_ms).map_err(|_| DataError::Timestamp(epoch_ms))?;
        DateTime::<Utc>::from_timestamp_millis(millis).ok_or(DataError::Timestamp(epoch_ms))?;
        let zone = match zone {
            0 => TimeZone::NewYork,
            1 => TimeZone::Utc,
            n => return Err(DataError::TimeZone(n)),
        };
        Ok(Self { epoch_ms, zone })
    }
    pub fn epoch_ms(self) -> u64 {
        self.epoch_ms
    }
    pub fn zone(self) -> TimeZone {
        self.zone
    }
    pub fn to_rfc3339(self) -> String {
        timestamp(
            self.epoch_ms,
            match self.zone {
                TimeZone::NewYork => 0,
                TimeZone::Utc => 1,
            },
        )
        .expect("Timestamp construction validates the formatting domain")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchValue {
    Null,
    Text(String),
    Integer(i64),
    Price(Price),
    Boolean(bool),
    Timestamp(Timestamp),
}

/// Flat row-major storage. Headers and cells cannot be mutated after validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataBatch {
    headers: Arc<[String]>,
    cells: Vec<BatchValue>,
    rows: usize,
}

impl DataBatch {
    /// Headers must be unique; cells must contain exactly rows * headers.len().
    pub fn new(
        headers: Arc<[String]>,
        cells: Vec<BatchValue>,
        rows: usize,
    ) -> Result<Self, DataError> {
        let mut names: Vec<&str> = headers.iter().map(String::as_str).collect();
        names.sort_unstable();
        if let Some(pair) = names.windows(2).find(|pair| pair[0] == pair[1]) {
            return Err(DataError::DuplicateColumn(pair[0].into()));
        }
        if rows.checked_mul(headers.len()) != Some(cells.len()) || (rows > 0 && headers.is_empty())
        {
            return Err(DataError::RowWidth {
                row: 0,
                actual: cells.len(),
                expected: rows.saturating_mul(headers.len()),
            });
        }
        Ok(Self {
            headers,
            cells,
            rows,
        })
    }
    pub fn headers(&self) -> &Arc<[String]> {
        &self.headers
    }
    pub fn cells(&self) -> &[BatchValue] {
        &self.cells
    }
    pub fn row_count(&self) -> usize {
        self.rows
    }
    pub fn rows(&self) -> impl ExactSizeIterator<Item = &[BatchValue]> {
        self.cells.chunks(self.headers.len().max(1))
    }
    /// Consume numeric data and format timestamps only at this adapter boundary.
    pub fn into_table(self) -> Table {
        let width = self.headers.len();
        let mut cells = self.cells.into_iter();
        let rows = (0..self.rows)
            .map(|_| {
                (0..width)
                    .map(|_| match cells.next().unwrap() {
                        BatchValue::Null => Value::Null,
                        BatchValue::Text(v) => Value::Text(v),
                        BatchValue::Integer(v) => Value::Integer(v),
                        BatchValue::Price(v) => Value::Price(v),
                        BatchValue::Boolean(v) => Value::Boolean(v),
                        BatchValue::Timestamp(v) => Value::Timestamp(v.to_rfc3339()),
                    })
                    .collect()
            })
            .collect();
        Table {
            headers: self.headers.to_vec(),
            rows,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numeric_instants_preserve_dst_offsets_and_invalid_domain() {
        for (instant, expected) in [
            ("2026-03-08T06:59:59Z", "2026-03-08T01:59:59.000-05:00"),
            ("2026-03-08T07:00:00Z", "2026-03-08T03:00:00.000-04:00"),
            ("2026-11-01T05:59:59Z", "2026-11-01T01:59:59.000-04:00"),
            ("2026-11-01T06:00:00Z", "2026-11-01T01:00:00.000-05:00"),
        ] {
            let ms = DateTime::parse_from_rfc3339(instant)
                .unwrap()
                .timestamp_millis() as u64;
            let numeric = Timestamp::from_wire(ms, 0).unwrap();
            assert_eq!(numeric.epoch_ms(), ms);
            assert_eq!(numeric.zone(), TimeZone::NewYork);
            assert_eq!(numeric.to_rfc3339(), expected);
        }
        assert!(Timestamp::from_wire(u64::MAX, 0).is_err());
        assert!(Timestamp::from_wire(i64::MAX as u64, 0).is_err());
        assert!(Timestamp::from_wire(0, 2).is_err());
    }
}
