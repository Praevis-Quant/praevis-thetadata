#![doc = include_str!("../README.md")]
//!
//! The protobuf reading reproduces prost's `DataValue` semantics exactly (the
//! last oneof member wins, a repeated price or timestamp merges into the
//! previous one, unknown fields are skipped), like the client's bounded
//! decoder. The client's differential corpus checks both against the legacy
//! decoder.
/// The arrow-rs crates whose types this API returns.
pub use arrow_array;
pub use arrow_schema;

use arrow_array::builder::{
    BooleanBuilder, Decimal128Builder, Int64Builder, StringBuilder, TimestampMillisecondBuilder,
};
use arrow_array::{Array, ArrayRef, RecordBatch, new_null_array};
use arrow_schema::{DataType, Field, Schema, SchemaRef, TimeUnit};
use praevis_thetadata_core::{MAX_EPOCH_MS, MILLISECOND_PRECISION};
use praevis_thetadata_proto::endpoints::ResponseData;
use prost::encoding::{DecodeContext, WireType, decode_key, decode_varint, skip_field};
use std::sync::Arc;
use zstd::zstd_safe::{DCtx, DParameter, InBuffer, OutBuffer, ResetDirective};

/// Arrow precision of price columns: every wire price fits (at most
/// `2^31 × 10^19 ≈ 2.1 × 10^28 < 10^38`).
pub const PRICE_PRECISION: u8 = 38;
/// Arrow scale of price columns: wire scale `s` means `value × 10^(s − 10)`.
pub const PRICE_SCALE: i8 = 10;
/// Arrow time zone of wire zone 0.
pub const NEW_YORK: &str = "America/New_York";
/// Arrow time zone of wire zone 1.
pub const UTC: &str = "UTC";

/// Per-message limits, plus a bound on what one decoder buffers.
#[derive(Debug, Clone)]
pub struct Limits {
    pub encoded_bytes: usize,
    pub decompressed_bytes: usize,
    pub headers: usize,
    pub header_bytes: usize,
    pub text_bytes: usize,
    pub rows_per_message: usize,
    /// Accounted upper bound of the builders' contents between flushes.
    pub buffered_bytes: usize,
    pub zstd_window_log: u32,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            encoded_bytes: 64 << 20,
            decompressed_bytes: 64 << 20,
            headers: 128,
            header_bytes: 1024,
            text_bytes: 1 << 20,
            rows_per_message: 100_000,
            buffered_bytes: 256 << 20,
            zstd_window_log: 23,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    /// Malformed protobuf, ZSTD or UTF-8, or an invalid price scale, time
    /// zone or out-of-range timestamp.
    #[error("malformed response")]
    Decode,
    /// A flat-file manifest, an unknown compression or a sub-millisecond
    /// timestamp (ADR-0019).
    #[error("unsupported response")]
    Unsupported,
    /// Headers changed, duplicated or missing for rows, or a ragged row.
    #[error("response schema mismatch")]
    Schema,
    /// A value of another kind than its column's type, or a timestamp in
    /// another zone. Arrow columns have one type; the legacy `Table` allows
    /// mixed kinds, so this is the one input class only the legacy accepts.
    #[error("column {column} mixes value kinds")]
    ColumnType { column: usize },
    #[error("limit exceeded: {0}")]
    Resource(&'static str),
    /// Appending the message could exceed `buffered_bytes`. Nothing was
    /// appended: flush, then push the same message again.
    #[error("buffer full; flush first")]
    BufferFull,
    #[error("invalid limits")]
    Configuration,
    /// An earlier message failed; only the rows before it can be flushed.
    #[error("an earlier message failed")]
    Poisoned,
}

/// One decoded cell, borrowing text from the payload.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Cell<'a> {
    Null,
    Text(&'a str),
    Number(i64),
    Price { value: i32, scale: i32 },
    Timestamp { epoch_ms: u64, zone: i32 },
    Boolean(bool),
}

#[derive(Debug)]
enum Builder {
    /// No non-null value yet: only a null count.
    Pending(usize),
    Utf8(StringBuilder),
    Int64(Int64Builder),
    Decimal(Decimal128Builder),
    Timestamp(TimestampMillisecondBuilder, i32),
    Boolean(BooleanBuilder),
}

#[derive(Debug)]
struct Column {
    builder: Builder,
    /// Buffered row at which the type was fixed (0 if fixed in an earlier batch).
    typed_at: usize,
}

const POW10: [i128; 20] = {
    let mut table = [1i128; 20];
    let mut i = 1;
    while i < 20 {
        table[i] = table[i - 1] * 10;
        i += 1;
    }
    table
};

impl Column {
    fn append(&mut self, cell: Cell<'_>, index: usize, row: usize) -> Result<(), DecodeError> {
        let mismatch = DecodeError::ColumnType { column: index };
        if let Builder::Pending(nulls) = self.builder {
            if cell == Cell::Null {
                self.builder = Builder::Pending(nulls + 1);
                return Ok(());
            }
            self.builder = match cell {
                Cell::Null => unreachable!(),
                Cell::Text(_) => Builder::Utf8(StringBuilder::new()),
                Cell::Number(_) => Builder::Int64(Int64Builder::new()),
                Cell::Price { .. } => Builder::Decimal(
                    Decimal128Builder::new()
                        .with_precision_and_scale(PRICE_PRECISION, PRICE_SCALE)
                        .expect("valid decimal type"),
                ),
                Cell::Timestamp { zone, .. } => Builder::Timestamp(
                    TimestampMillisecondBuilder::new().with_timezone(zone_name(zone)?),
                    zone,
                ),
                Cell::Boolean(_) => Builder::Boolean(BooleanBuilder::new()),
            };
            self.typed_at = row;
            self.append_nulls(nulls);
        }
        match (&mut self.builder, cell) {
            (_, Cell::Null) => self.append_nulls(1),
            (Builder::Utf8(b), Cell::Text(v)) => b.append_value(v),
            (Builder::Int64(b), Cell::Number(v)) => b.append_value(v),
            (Builder::Boolean(b), Cell::Boolean(v)) => b.append_value(v),
            (Builder::Decimal(b), Cell::Price { value, scale }) => {
                // Scale 0 is null and other values are rejected when parsed.
                b.append_value(i128::from(value) * POW10[scale as usize])
            }
            (Builder::Timestamp(b, column_zone), Cell::Timestamp { epoch_ms, zone, .. }) => {
                if zone != *column_zone {
                    return Err(mismatch);
                }
                // Bounded by MAX_EPOCH_MS when parsed.
                b.append_value(epoch_ms as i64)
            }
            _ => return Err(mismatch),
        }
        Ok(())
    }

    fn append_nulls(&mut self, n: usize) {
        match &mut self.builder {
            Builder::Pending(nulls) => *nulls += n,
            Builder::Utf8(b) => b.append_nulls(n),
            Builder::Int64(b) => b.append_nulls(n),
            Builder::Decimal(b) => b.append_nulls(n),
            Builder::Timestamp(b, _) => b.append_nulls(n),
            Builder::Boolean(b) => b.append_nulls(n),
        }
    }

    /// Finish the buffered values, keeping the type for the next batch.
    fn finish(&mut self) -> ArrayRef {
        match &mut self.builder {
            Builder::Pending(nulls) => {
                let array = new_null_array(&DataType::Null, *nulls);
                *nulls = 0;
                array
            }
            Builder::Utf8(b) => Arc::new(b.finish()),
            Builder::Int64(b) => Arc::new(b.finish()),
            Builder::Decimal(b) => Arc::new(b.finish()),
            Builder::Timestamp(b, _) => Arc::new(b.finish()),
            Builder::Boolean(b) => Arc::new(b.finish()),
        }
    }
}

fn zone_name(zone: i32) -> Result<&'static str, DecodeError> {
    match zone {
        0 => Ok(NEW_YORK),
        1 => Ok(UTC),
        _ => Err(DecodeError::Decode),
    }
}

/// Decodes vendor responses into exact, coalesced Arrow batches.
///
/// Push messages in stream order and flush whenever a batch is wanted; rows
/// from many messages share one batch. If a message fails, nothing after it
/// is accepted, and `flush` returns only the rows of the messages before it.
pub struct ColumnarDecoder {
    limits: Limits,
    zstd: DCtx<'static>,
    scratch: Vec<u8>,
    names: Option<Vec<String>>,
    columns: Vec<Column>,
    rows: usize,
    buffered: usize,
    /// Rows buffered before the message that failed.
    failed_at: Option<usize>,
}

impl std::fmt::Debug for ColumnarDecoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ColumnarDecoder")
            .field("limits", &self.limits)
            .field("columns", &self.columns.len())
            .field("rows", &self.rows)
            .field("buffered", &self.buffered)
            .field("failed", &self.failed_at.is_some())
            .finish_non_exhaustive()
    }
}

fn bound(value: usize, limit: usize, name: &'static str) -> Result<(), DecodeError> {
    if value > limit {
        Err(DecodeError::Resource(name))
    } else {
        Ok(())
    }
}

impl ColumnarDecoder {
    pub fn new(limits: Limits) -> Result<Self, DecodeError> {
        if [
            limits.encoded_bytes,
            limits.decompressed_bytes,
            limits.headers,
            limits.header_bytes,
            limits.text_bytes,
            limits.rows_per_message,
            limits.buffered_bytes,
        ]
        .contains(&0)
            || !(10..=27).contains(&limits.zstd_window_log)
        {
            return Err(DecodeError::Configuration);
        }
        let mut zstd = DCtx::try_create().ok_or(DecodeError::Resource("allocation"))?;
        zstd.set_parameter(DParameter::WindowLogMax(limits.zstd_window_log))
            .map_err(zstd_error)?;
        Ok(Self {
            limits,
            zstd,
            scratch: Vec::new(),
            names: None,
            columns: Vec::new(),
            rows: 0,
            buffered: 0,
            failed_at: None,
        })
    }

    /// Column names, once a message has carried headers.
    pub fn headers(&self) -> Option<&[String]> {
        self.names.as_deref()
    }
    /// Rows buffered since the last flush.
    pub fn buffered_rows(&self) -> usize {
        self.failed_at.unwrap_or(self.rows)
    }
    /// Accounted upper bound of the buffered contents, in bytes.
    pub fn buffered_bytes(&self) -> usize {
        self.buffered
    }

    /// Append one message's rows; returns how many were appended.
    pub fn push(&mut self, response: &ResponseData) -> Result<usize, DecodeError> {
        if self.failed_at.is_some() {
            return Err(DecodeError::Poisoned);
        }
        if response.flat_file_manifest.is_some() {
            return Err(DecodeError::Unsupported);
        }
        let algo = response
            .compression_description
            .as_ref()
            .map_or(0, |d| d.algo);
        self.push_payload(&response.compressed_data, algo, response.original_size)
    }

    /// Append one message from its encoded `ResponseData` bytes, as the gRPC
    /// framing delivers them, reading the envelope in place: no `ResponseData`
    /// or payload copy. Envelope rules match the client's bounded envelope
    /// parser: at most `encoded_bytes + 1024` bytes, at most 1024 bytes
    /// besides the payload, the last payload wins, and a flat-file manifest
    /// is unsupported.
    pub fn push_encoded(&mut self, envelope: &[u8]) -> Result<usize, DecodeError> {
        if self.failed_at.is_some() {
            return Err(DecodeError::Poisoned);
        }
        let (payload, algo, original_size) = parse_envelope(envelope, &self.limits)?;
        self.push_payload(payload, algo, original_size)
    }

    fn push_payload(
        &mut self,
        payload: &[u8],
        algo: i32,
        original_size: i32,
    ) -> Result<usize, DecodeError> {
        bound(payload.len(), self.limits.encoded_bytes, "encoded bytes")?;
        if original_size > 0 {
            bound(
                original_size as usize,
                self.limits.decompressed_bytes,
                "declared bytes",
            )?;
        }
        let mut scratch = std::mem::take(&mut self.scratch);
        let result = match algo {
            0 => bound(
                payload.len(),
                self.limits.decompressed_bytes,
                "decompressed bytes",
            )
            .and_then(|()| self.append(payload)),
            1 => self
                .decompress(payload, original_size, &mut scratch)
                .and_then(|()| self.append(&scratch)),
            _ => Err(DecodeError::Unsupported),
        };
        self.scratch = scratch;
        result
    }

    fn decompress(
        &mut self,
        input: &[u8],
        declared: i32,
        out: &mut Vec<u8>,
    ) -> Result<(), DecodeError> {
        let limit = self.limits.decompressed_bytes;
        out.clear();
        if declared > 0 && out.capacity() < declared as usize {
            out.try_reserve_exact(declared as usize)
                .map_err(|_| DecodeError::Resource("allocation"))?;
        }
        self.zstd
            .reset(ResetDirective::SessionOnly)
            .map_err(zstd_error)?;
        let mut source = InBuffer::around(input);
        loop {
            if out.len() == out.capacity() {
                // One byte over the limit detects expansion beyond it.
                let target = (out.capacity().max(4096) * 2).min(limit + 1);
                if target <= out.len() {
                    return Err(DecodeError::Resource("decompressed bytes"));
                }
                out.try_reserve_exact(target - out.len())
                    .map_err(|_| DecodeError::Resource("allocation"))?;
            }
            let (before_in, before_out) = (source.pos(), out.len());
            // The output buffer sets the vector's length as it writes.
            let remaining = self
                .zstd
                .decompress_stream(&mut OutBuffer::around_pos(out, before_out), &mut source)
                .map_err(zstd_error)?;
            bound(out.len(), limit, "decompressed bytes")?;
            if remaining == 0 && source.pos() == input.len() {
                return Ok(());
            }
            // Truncated input or no progress; concatenated and skippable
            // frames are consumed like the bounded decoder does.
            if source.pos() == before_in && out.len() == before_out && out.len() < out.capacity() {
                return Err(DecodeError::Decode);
            }
        }
    }

    /// Check headers in place (pass 1), then append rows (pass 2).
    fn append(&mut self, payload: &[u8]) -> Result<usize, DecodeError> {
        let (headers, rows) = self.check_headers(payload)?;
        if rows == 0 {
            return Ok(0);
        }
        if headers == 0 {
            return Err(DecodeError::Schema);
        }
        // Text cannot exceed the payload; each cell is at most 16 bytes of
        // fixed width plus offsets and validity.
        let added = payload
            .len()
            .saturating_add(rows.saturating_mul(headers).saturating_mul(17));
        let total = self.buffered.saturating_add(added);
        if total > self.limits.buffered_bytes {
            return Err(if self.rows == 0 {
                DecodeError::Resource("buffered bytes")
            } else {
                DecodeError::BufferFull
            });
        }
        let start = self.rows;
        match self.append_rows(payload, headers) {
            Ok(()) => {
                self.buffered = total;
                Ok(self.rows - start)
            }
            Err(error) => {
                self.failed_at = Some(start);
                Err(error)
            }
        }
    }

    /// Preflight, in the bounded decoder's order: structure, header and text
    /// validity and limits over the whole message, then row widths. Returns
    /// (headers, rows) of this message. The first message with headers fixes
    /// the names; later ones must match byte for byte.
    fn check_headers(&mut self, mut bytes: &[u8]) -> Result<(usize, usize), DecodeError> {
        let (mut headers, mut rows) = (0, 0);
        let (mut min_width, mut max_width) = (usize::MAX, 0);
        let mut first = self.names.is_none().then(Vec::new);
        while !bytes.is_empty() {
            let (tag, ty) = key(&mut bytes)?;
            match tag {
                1 => {
                    let header = message(&mut bytes, ty)?;
                    bound(header.len(), self.limits.header_bytes, "header bytes")?;
                    let name = utf8(header)?;
                    headers += 1;
                    bound(headers, self.limits.headers, "headers")?;
                    match (&mut first, &self.names) {
                        (Some(names), _) => names.push(name.to_owned()),
                        (None, Some(names)) => {
                            if names.get(headers - 1).map(String::as_bytes) != Some(header) {
                                return Err(DecodeError::Schema);
                            }
                        }
                        (None, None) => unreachable!(),
                    }
                }
                2 => {
                    rows += 1;
                    bound(rows, self.limits.rows_per_message, "rows")?;
                    let mut row = message(&mut bytes, ty)?;
                    let mut width = 0;
                    while !row.is_empty() {
                        let (tag, ty) = key(&mut row)?;
                        if tag == 1 {
                            cell_text(message(&mut row, ty)?, self.limits.text_bytes)?;
                            width += 1;
                        } else {
                            skip(&mut row, tag, ty)?;
                        }
                    }
                    min_width = min_width.min(width);
                    max_width = max_width.max(width);
                }
                _ => skip(&mut bytes, tag, ty)?,
            }
        }
        if rows > 0 && (headers == 0 || min_width != headers || max_width != headers) {
            return Err(DecodeError::Schema);
        }
        match first {
            Some(names) if !names.is_empty() => {
                let mut sorted: Vec<_> = names.iter().collect();
                sorted.sort_unstable();
                if sorted.windows(2).any(|pair| pair[0] == pair[1]) {
                    return Err(DecodeError::Schema);
                }
                self.columns = (0..names.len())
                    .map(|_| Column {
                        builder: Builder::Pending(self.rows),
                        typed_at: 0,
                    })
                    .collect();
                self.names = Some(names);
            }
            _ => {
                if headers > 0 && self.names.as_ref().map(Vec::len) != Some(headers) {
                    return Err(DecodeError::Schema);
                }
            }
        }
        Ok((headers, rows))
    }

    fn append_rows(&mut self, mut bytes: &[u8], width: usize) -> Result<(), DecodeError> {
        while !bytes.is_empty() {
            let (tag, ty) = key(&mut bytes)?;
            if tag != 2 {
                skip(&mut bytes, tag, ty)?;
                continue;
            }
            let mut row = message(&mut bytes, ty)?;
            let mut index = 0;
            while !row.is_empty() {
                let (tag, ty) = key(&mut row)?;
                if tag != 1 {
                    skip(&mut row, tag, ty)?;
                    continue;
                }
                let cell = data_value(message(&mut row, ty)?, self.limits.text_bytes)?;
                let column = self.columns.get_mut(index).ok_or(DecodeError::Schema)?;
                column.append(cell, index, self.rows)?;
                index += 1;
            }
            if index != width {
                return Err(DecodeError::Schema);
            }
            self.rows += 1;
        }
        Ok(())
    }

    /// The buffered rows as one batch, or `None` if there are none. Column
    /// types persist to the next batch; a column with no value yet is `Null`.
    pub fn flush(&mut self) -> Option<RecordBatch> {
        let names = self.names.as_ref()?;
        let rows = self.failed_at.unwrap_or(self.rows);
        if rows == 0 {
            return None;
        }
        let mut fields = Vec::with_capacity(names.len());
        let mut arrays = Vec::with_capacity(names.len());
        for (name, column) in names.iter().zip(&mut self.columns) {
            // A type first seen in the failed message is not reported.
            let typed_late = column.typed_at >= rows;
            let mut array = column.finish();
            if typed_late {
                array = new_null_array(&DataType::Null, rows);
            } else if array.len() != rows {
                array = array.slice(0, rows);
            }
            column.typed_at = 0;
            fields.push(Field::new(name, array.data_type().clone(), true));
            arrays.push(array);
        }
        self.rows = 0;
        self.buffered = 0;
        if self.failed_at.is_some() {
            self.failed_at = Some(0);
        }
        let schema: SchemaRef = Arc::new(Schema::new(fields));
        Some(RecordBatch::try_new(schema, arrays).expect("columns match the schema"))
    }
}

/// `ResponseData` fields 1 (payload), 2 (compression), 3 (original size) and
/// 4 (flat-file manifest), with prost's merge semantics for what is kept.
fn parse_envelope<'a>(
    bytes: &'a [u8],
    limits: &Limits,
) -> Result<(&'a [u8], i32, i32), DecodeError> {
    if bytes.len() > limits.encoded_bytes.saturating_add(1024) {
        return Err(DecodeError::Resource("envelope bytes"));
    }
    let mut cursor = bytes;
    let (mut payload, mut algo, mut original_size) = (&[][..], 0, 0);
    while !cursor.is_empty() {
        let (tag, ty) = key(&mut cursor)?;
        match tag {
            1 => {
                payload = message(&mut cursor, ty)?;
                bound(payload.len(), limits.encoded_bytes, "encoded bytes")?;
            }
            2 => {
                // A repeated description merges; an absent field keeps its value.
                let mut description = message(&mut cursor, ty)?;
                while !description.is_empty() {
                    let (tag, ty) = key(&mut description)?;
                    match tag {
                        1 => algo = varint(&mut description, ty)? as i32,
                        _ => skip(&mut description, tag, ty)?,
                    }
                }
            }
            3 => original_size = varint(&mut cursor, ty)? as i32,
            4 => {
                message(&mut cursor, ty)?;
                return Err(DecodeError::Unsupported);
            }
            _ => skip(&mut cursor, tag, ty)?,
        }
    }
    if bytes.len() - payload.len() > 1024 {
        return Err(DecodeError::Resource("envelope overhead"));
    }
    Ok((payload, algo, original_size))
}

/// The Arrow type of a timestamp column in wire zone `zone`.
pub fn timestamp_type(zone: i32) -> Option<DataType> {
    zone_name(zone)
        .ok()
        .map(|tz| DataType::Timestamp(TimeUnit::Millisecond, Some(tz.into())))
}

fn zstd_error(code: usize) -> DecodeError {
    use zstd::zstd_safe::zstd_sys::{ZSTD_ErrorCode, ZSTD_getErrorCode};
    // SAFETY: a pure function of a numeric result code; no pointers. The typed
    // code is kept; no library or remote error strings are parsed.
    match unsafe { ZSTD_getErrorCode(code) } {
        ZSTD_ErrorCode::ZSTD_error_frameParameter_windowTooLarge => {
            DecodeError::Resource("zstd window")
        }
        ZSTD_ErrorCode::ZSTD_error_memory_allocation => DecodeError::Resource("allocation"),
        _ => DecodeError::Decode,
    }
}

/// Validate every text member of a cell (an overwritten one too, as prost does).
fn cell_text(mut bytes: &[u8], limit: usize) -> Result<(), DecodeError> {
    while !bytes.is_empty() {
        let (tag, ty) = key(&mut bytes)?;
        if tag == 1 {
            let text = message(&mut bytes, ty)?;
            bound(text.len(), limit, "text bytes")?;
            utf8(text)?;
        } else {
            skip(&mut bytes, tag, ty)?;
        }
    }
    Ok(())
}
fn key(bytes: &mut &[u8]) -> Result<(u32, WireType), DecodeError> {
    decode_key(bytes).map_err(|_| DecodeError::Decode)
}
fn skip(bytes: &mut &[u8], tag: u32, ty: WireType) -> Result<(), DecodeError> {
    skip_field(ty, tag, bytes, DecodeContext::default()).map_err(|_| DecodeError::Decode)
}
fn message<'a>(bytes: &mut &'a [u8], ty: WireType) -> Result<&'a [u8], DecodeError> {
    if ty != WireType::LengthDelimited {
        return Err(DecodeError::Decode);
    }
    let n = usize::try_from(decode_varint(bytes).map_err(|_| DecodeError::Decode)?)
        .map_err(|_| DecodeError::Decode)?;
    if n > bytes.len() {
        return Err(DecodeError::Decode);
    }
    let (head, tail) = bytes.split_at(n);
    *bytes = tail;
    Ok(head)
}
fn varint(bytes: &mut &[u8], ty: WireType) -> Result<u64, DecodeError> {
    if ty != WireType::Varint {
        return Err(DecodeError::Decode);
    }
    decode_varint(bytes).map_err(|_| DecodeError::Decode)
}
fn utf8(bytes: &[u8]) -> Result<&str, DecodeError> {
    std::str::from_utf8(bytes).map_err(|_| DecodeError::Decode)
}

/// prost `DataValue` semantics without building the message, then the
/// legacy decoder's value rules (price scale, timestamp precision and range).
fn data_value(mut bytes: &[u8], text_limit: usize) -> Result<Cell<'_>, DecodeError> {
    // Exact single-field encodings that dominate real tables.
    match bytes {
        [] => return Ok(Cell::Null),
        [0x28, n] if *n < 0x80 => return Ok(Cell::Null),
        [0x30, b] if *b < 0x80 => return Ok(Cell::Boolean(*b != 0)),
        [0x10, n] if *n < 0x80 => return Ok(Cell::Number(i64::from(*n))),
        _ => {}
    }
    enum Raw<'a> {
        Text(&'a str),
        Number(i64),
        Price(i32, i32),
        Timestamp(u64, i32, u32, i32),
        Null,
        Boolean(bool),
    }
    let mut value = None;
    while !bytes.is_empty() {
        let (tag, ty) = key(&mut bytes)?;
        value = Some(match tag {
            1 => {
                let text = message(&mut bytes, ty)?;
                bound(text.len(), text_limit, "text bytes")?;
                Raw::Text(utf8(text)?)
            }
            2 => Raw::Number(varint(&mut bytes, ty)? as i64),
            3 => {
                let (mut v, mut s) = match value.take() {
                    Some(Raw::Price(v, s)) => (v, s),
                    _ => (0, 0),
                };
                let mut body = message(&mut bytes, ty)?;
                while !body.is_empty() {
                    let (tag, ty) = key(&mut body)?;
                    match tag {
                        1 => v = varint(&mut body, ty)? as i32,
                        2 => s = varint(&mut body, ty)? as i32,
                        _ => skip(&mut body, tag, ty)?,
                    }
                }
                Raw::Price(v, s)
            }
            4 => {
                let (mut e, mut z, mut n, mut p) = match value.take() {
                    Some(Raw::Timestamp(e, z, n, p)) => (e, z, n, p),
                    _ => (0, 0, 0, 0),
                };
                let mut body = message(&mut bytes, ty)?;
                while !body.is_empty() {
                    let (tag, ty) = key(&mut body)?;
                    match tag {
                        1 => e = varint(&mut body, ty)?,
                        2 => z = varint(&mut body, ty)? as i32,
                        3 => n = varint(&mut body, ty)? as u32,
                        4 => p = varint(&mut body, ty)? as i32,
                        _ => skip(&mut body, tag, ty)?,
                    }
                }
                Raw::Timestamp(e, z, n, p)
            }
            5 => {
                varint(&mut bytes, ty)?;
                Raw::Null
            }
            6 => Raw::Boolean(varint(&mut bytes, ty)? != 0),
            _ => {
                skip(&mut bytes, tag, ty)?;
                continue;
            }
        });
    }
    Ok(match value {
        None | Some(Raw::Null) => Cell::Null,
        Some(Raw::Text(v)) => Cell::Text(v),
        Some(Raw::Number(v)) => Cell::Number(v),
        Some(Raw::Boolean(v)) => Cell::Boolean(v),
        Some(Raw::Price(_, 0)) => Cell::Null,
        Some(Raw::Price(value, scale @ 1..=19)) => Cell::Price { value, scale },
        Some(Raw::Price(..)) => return Err(DecodeError::Decode),
        Some(Raw::Timestamp(epoch_ms, zone, nanos, precision)) => {
            // ADR-0019 first (unsupported), then range and zone (malformed),
            // in the bounded decoder's order.
            if precision != MILLISECOND_PRECISION || nanos != 0 {
                return Err(DecodeError::Unsupported);
            }
            if epoch_ms > MAX_EPOCH_MS {
                return Err(DecodeError::Decode);
            }
            zone_name(zone)?;
            Cell::Timestamp { epoch_ms, zone }
        }
    })
}
