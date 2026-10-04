//! Preflight before materialization; never builds an unrestricted wire DataTable.
use crate::eod::EodError;
use prost::{
    Message,
    encoding::{DecodeContext, WireType, decode_key, decode_varint, skip_field},
};
use std::{io::Read, mem::size_of, sync::Arc};
use thetadata_core::{BatchValue, DataBatch, Price, Timestamp, Value};
use thetadata_proto::endpoints::{DataValue, ResponseData, data_value::DataType};

/// Finite per-batch limits. The accounted budget is requested storage, not RSS.
#[derive(Debug, Clone)]
pub struct DecodeLimits {
    pub encoded_bytes: usize,
    pub decompressed_bytes: usize,
    pub headers: usize,
    pub rows: usize,
    pub cells: usize,
    pub text_bytes: usize,
    pub header_bytes: usize,
    pub allocated_bytes: usize,
    pub zstd_window_log: u32,
}
impl Default for DecodeLimits {
    fn default() -> Self {
        Self {
            encoded_bytes: 64 << 20,
            decompressed_bytes: 64 << 20,
            headers: 128,
            rows: 100_000,
            cells: 1_000_000,
            text_bytes: 1 << 20,
            header_bytes: 1024,
            allocated_bytes: 128 << 20,
            zstd_window_log: 23,
        }
    }
}
impl DecodeLimits {
    pub(crate) fn validate(&self) -> Result<(), EodError> {
        if [
            self.encoded_bytes,
            self.decompressed_bytes,
            self.headers,
            self.rows,
            self.cells,
            self.text_bytes,
            self.header_bytes,
            self.allocated_bytes,
        ]
        .contains(&0)
            || self.encoded_bytes > i32::MAX as usize - 1024
            || self.decompressed_bytes > i32::MAX as usize
            || !(10..=27).contains(&self.zstd_window_log)
        {
            return Err(EodError::Configuration);
        }
        if self
            .schema_bytes()?
            .checked_add(self.encoded_bytes)
            .is_none()
            || self.allocated_bytes < self.encoded_bytes + 1024
            || self.allocated_bytes < self.decompressed_bytes
            || self.zstd_workspace()? > self.allocated_bytes
        {
            return Err(EodError::Configuration);
        }
        Ok(())
    }
    pub(crate) fn schema_bytes(&self) -> Result<usize, EodError> {
        add(mul(self.headers, add(self.header_bytes, 64)?)?, 64)
    }
    // Conservative libzstd workspace allowance beyond the explicit window.
    // This is not an allocator-level guarantee for libzstd or the transport.
    fn zstd_workspace(&self) -> Result<usize, EodError> {
        add(1usize << self.zstd_window_log, 16 << 20)
    }
}
fn add(a: usize, b: usize) -> Result<usize, EodError> {
    a.checked_add(b).ok_or(EodError::Resource("arithmetic"))
}
fn mul(a: usize, b: usize) -> Result<usize, EodError> {
    a.checked_mul(b).ok_or(EodError::Resource("arithmetic"))
}
fn bound(value: usize, limit: usize, name: &'static str) -> Result<(), EodError> {
    if value > limit {
        Err(EodError::Resource(name))
    } else {
        Ok(())
    }
}
fn key(bytes: &mut &[u8]) -> Result<(u32, WireType), EodError> {
    decode_key(bytes).map_err(|_| EodError::Decode)
}
fn skip(bytes: &mut &[u8], tag: u32, ty: WireType) -> Result<(), EodError> {
    skip_field(ty, tag, bytes, DecodeContext::default()).map_err(|_| EodError::Decode)
}
fn message<'a>(bytes: &mut &'a [u8], ty: WireType) -> Result<&'a [u8], EodError> {
    if ty != WireType::LengthDelimited {
        return Err(EodError::Decode);
    }
    let n = usize::try_from(decode_varint(bytes).map_err(|_| EodError::Decode)?)
        .map_err(|_| EodError::Decode)?;
    if n > bytes.len() {
        return Err(EodError::Decode);
    }
    let (head, tail) = bytes.split_at(n);
    *bytes = tail;
    Ok(head)
}
#[derive(Default)]
struct Shape {
    headers: usize,
    rows: usize,
    cells: usize,
    max_cell: usize,
}
fn cell_text(mut bytes: &[u8], limits: &DecodeLimits) -> Result<(), EodError> {
    while !bytes.is_empty() {
        let (tag, ty) = key(&mut bytes)?;
        if tag == 1 {
            let text = message(&mut bytes, ty)?;
            bound(text.len(), limits.text_bytes, "text bytes")?;
            std::str::from_utf8(text).map_err(|_| EodError::Decode)?;
        } else {
            skip(&mut bytes, tag, ty)?;
        }
    }
    Ok(())
}
fn preflight(mut bytes: &[u8], limits: &DecodeLimits) -> Result<Shape, EodError> {
    let mut shape = Shape::default();
    // Header and row order on the wire is unrestricted. Compare all widths after
    // counting headers, using min/max instead of a per-row allocation.
    let (mut min_width, mut max_width) = (usize::MAX, 0);
    while !bytes.is_empty() {
        let (tag, ty) = key(&mut bytes)?;
        match tag {
            1 => {
                let header = message(&mut bytes, ty)?;
                bound(header.len(), limits.header_bytes, "header bytes")?;
                std::str::from_utf8(header).map_err(|_| EodError::Decode)?;
                shape.headers = add(shape.headers, 1)?;
                bound(shape.headers, limits.headers, "headers")?;
            }
            2 => {
                shape.rows = add(shape.rows, 1)?;
                bound(shape.rows, limits.rows, "rows")?;
                let mut row = message(&mut bytes, ty)?;
                let mut width = 0;
                while !row.is_empty() {
                    let (tag, ty) = key(&mut row)?;
                    if tag == 1 {
                        let cell = message(&mut row, ty)?;
                        shape.cells = add(shape.cells, 1)?;
                        bound(shape.cells, limits.cells, "cells")?;
                        width = add(width, 1)?;
                        shape.max_cell = shape.max_cell.max(cell.len());
                        cell_text(cell, limits)?;
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
    if shape.rows > 0
        && (shape.headers == 0 || min_width != shape.headers || max_width != shape.headers)
    {
        return Err(EodError::Schema);
    }
    Ok(shape)
}

pub(crate) fn decode(
    response: ResponseData,
    limits: &DecodeLimits,
    schema: Option<Arc<[String]>>,
    table: bool,
) -> Result<DataBatch, EodError> {
    if response.flat_file_manifest.is_some() {
        return Err(EodError::Unsupported);
    }
    bound(
        response.compressed_data.len(),
        limits.encoded_bytes,
        "encoded bytes",
    )?;
    if response.original_size > 0 {
        bound(
            response.original_size as usize,
            limits.decompressed_bytes,
            "declared bytes",
        )?;
    }
    let algo = response
        .compression_description
        .map(|d| d.algo)
        .unwrap_or(0);
    let input_bytes = response.compressed_data;
    let bytes = match algo {
        0 => {
            bound(
                input_bytes.len(),
                limits.decompressed_bytes,
                "decompressed bytes",
            )?;
            input_bytes
        }
        1 => {
            let workspace = limits.zstd_workspace()?;
            let input = input_bytes.capacity();
            bound(
                add(input, workspace)?,
                limits.allocated_bytes,
                "allocation bytes",
            )?;
            let mut decoder = zstd::stream::read::Decoder::with_buffer(input_bytes.as_slice())
                .map_err(|_| EodError::Decode)?;
            decoder
                .window_log_max(limits.zstd_window_log)
                .map_err(|_| EodError::Decode)?;
            let mut output = Vec::new();
            let mut chunk = [0; 64 * 1024];
            loop {
                let n = decoder.read(&mut chunk).map_err(|_| EodError::Decode)?;
                if n == 0 {
                    break;
                }
                let length = add(output.len(), n)?;
                bound(length, limits.decompressed_bytes, "decompressed bytes")?;
                if length > output.capacity() {
                    let capacity = length
                        .max(output.capacity().saturating_mul(2))
                        .min(limits.decompressed_bytes);
                    // Include both old and replacement allocations during growth.
                    bound(
                        add(add(input, workspace)?, add(output.capacity(), capacity)?)?,
                        limits.allocated_bytes,
                        "allocation bytes",
                    )?;
                    output
                        .try_reserve_exact(capacity - output.len())
                        .map_err(|_| EodError::Resource("allocation"))?;
                }
                output.extend_from_slice(&chunk[..n]);
            }
            drop(decoder);
            drop(input_bytes);
            output
        }
        _ => return Err(EodError::Unsupported),
    };
    let shape = preflight(&bytes, limits)?;
    // Text retained in output cannot exceed wire length. Prost decodes only one
    // bounded cell at a time. Header Arc conversion/duplicate sorting and Table
    // conversion get conservative simultaneous-live allowances.
    let mut memory = add(bytes.capacity(), mul(bytes.len(), 2)?)?;
    // Prost's String backing Vec grows geometrically with a minimum allocation
    // of eight bytes, even for one-byte strings. Length alone undercounts it.
    memory = add(memory, mul(shape.cells, add(size_of::<BatchValue>(), 8)?)?)?;
    memory = add(memory, mul(shape.headers, 80)?)?;
    memory = add(memory, add(mul(shape.max_cell, 4)?, 16)?)?;
    if table {
        memory = add(memory, mul(shape.cells, add(size_of::<Value>(), 64)?)?)?;
        memory = add(memory, mul(shape.rows, size_of::<Vec<Value>>())?)?;
        memory = add(
            memory,
            add(bytes.len(), mul(shape.headers, size_of::<String>())?)?,
        )?;
    }
    bound(memory, limits.allocated_bytes, "allocation bytes")?;
    let mut headers = Vec::with_capacity(shape.headers);
    let mut cells = Vec::with_capacity(shape.cells);
    let mut cursor = bytes.as_slice();
    while !cursor.is_empty() {
        let (tag, ty) = key(&mut cursor)?;
        match tag {
            1 => headers.push(
                std::str::from_utf8(message(&mut cursor, ty)?)
                    .map_err(|_| EodError::Decode)?
                    .to_owned(),
            ),
            2 => {
                let mut row = message(&mut cursor, ty)?;
                while !row.is_empty() {
                    let (tag, ty) = key(&mut row)?;
                    if tag != 1 {
                        skip(&mut row, tag, ty)?;
                        continue;
                    }
                    let value =
                        DataValue::decode(message(&mut row, ty)?).map_err(|_| EodError::Decode)?;
                    cells.push(match value.data_type {
                        None | Some(DataType::NullValue(_)) => BatchValue::Null,
                        Some(DataType::Text(v)) => BatchValue::Text(v),
                        Some(DataType::Number(v)) => BatchValue::Integer(v),
                        Some(DataType::Boolean(v)) => BatchValue::Boolean(v),
                        Some(DataType::Price(v)) => Price::from_wire(v.value, v.r#type)
                            .map_err(|_| EodError::Decode)?
                            .map(BatchValue::Price)
                            .unwrap_or(BatchValue::Null),
                        Some(DataType::Timestamp(v)) => BatchValue::Timestamp(
                            Timestamp::from_wire(v.epoch_ms, v.zone)
                                .map_err(|_| EodError::Decode)?,
                        ),
                    });
                }
            }
            _ => skip(&mut cursor, tag, ty)?,
        }
    }
    let headers = if let Some(schema) = schema.filter(|_| !headers.is_empty()) {
        if schema.as_ref() != headers.as_slice() {
            return Err(EodError::Schema);
        }
        schema
    } else {
        Arc::from(headers)
    };
    DataBatch::new(headers, cells, shape.rows).map_err(|_| EodError::Schema)
}

#[cfg(test)]
mod tests {
    use super::*;
    use thetadata_proto::endpoints::{CompressionDescription, DataTable, DataValueList};
    fn response(bytes: Vec<u8>, compressed: bool) -> ResponseData {
        ResponseData {
            compressed_data: if compressed {
                zstd::stream::encode_all(bytes.as_slice(), 1).unwrap()
            } else {
                bytes
            },
            compression_description: Some(CompressionDescription {
                algo: i32::from(compressed),
                level: 1,
            }),
            ..Default::default()
        }
    }
    fn wire() -> DataTable {
        DataTable {
            headers: vec!["text".into()],
            data_table: vec![DataValueList {
                values: vec![DataValue {
                    data_type: Some(DataType::Text("é".into())),
                }],
            }],
        }
    }
    #[test]
    fn exact_and_over_count_string_and_byte_boundaries() {
        let bytes = wire().encode_to_vec();
        for compressed in [false, true] {
            let response = response(bytes.clone(), compressed);
            let limits = DecodeLimits {
                headers: 1,
                rows: 1,
                cells: 1,
                text_bytes: 2,
                header_bytes: 4,
                encoded_bytes: response.compressed_data.len(),
                decompressed_bytes: bytes.len(),
                ..Default::default()
            };
            assert!(decode(response.clone(), &limits, None, false).is_ok());
            for (field, name) in [
                (0, "headers"),
                (1, "rows"),
                (2, "cells"),
                (3, "text bytes"),
                (4, "header bytes"),
                (5, "encoded bytes"),
                (6, "decompressed bytes"),
            ] {
                let mut smaller = limits.clone();
                match field {
                    0 => smaller.headers -= 1,
                    1 => smaller.rows -= 1,
                    2 => smaller.cells -= 1,
                    3 => smaller.text_bytes -= 1,
                    4 => smaller.header_bytes -= 1,
                    5 => smaller.encoded_bytes -= 1,
                    _ => smaller.decompressed_bytes -= 1,
                }
                assert_eq!(
                    decode(response.clone(), &smaller, None, false).unwrap_err(),
                    EodError::Resource(name)
                );
            }
        }
    }
    #[test]
    fn rejects_tiny_wire_expansion_forged_hints_and_allocation_overflow() {
        let mut wire = wire();
        wire.data_table[0].values = vec![DataValue::default(); 1000];
        let limits = DecodeLimits {
            cells: 10,
            ..Default::default()
        };
        assert_eq!(
            decode(response(wire.encode_to_vec(), false), &limits, None, false).unwrap_err(),
            EodError::Resource("cells")
        );
        let mut data = response(vec![0; 10000], true);
        data.original_size = 1;
        let limits = DecodeLimits {
            decompressed_bytes: 100,
            ..Default::default()
        };
        assert_eq!(
            decode(data.clone(), &limits, None, false).unwrap_err(),
            EodError::Resource("decompressed bytes")
        );
        data.original_size = 101;
        assert_eq!(
            decode(data, &limits, None, false).unwrap_err(),
            EodError::Resource("declared bytes")
        );
        let limits = DecodeLimits {
            headers: usize::MAX,
            ..Default::default()
        };
        assert!(limits.validate().is_err());
        let limits = DecodeLimits {
            allocated_bytes: 1,
            ..Default::default()
        };
        assert_eq!(
            decode(
                response(self::wire().encode_to_vec(), false),
                &limits,
                None,
                false
            )
            .unwrap_err(),
            EodError::Resource("allocation bytes")
        );
    }
    #[test]
    fn unknown_and_repeated_fields_keep_prost_semantics_without_unbounded_table() {
        // One cell with two text occurrences: protobuf last value wins. An
        // oversized overwritten value must still meet the per-text bound.
        let bytes = vec![10, 1, b'h', 18, 8, 10, 6, 10, 1, b'a', 10, 1, b'b'];
        let expected = crate::decode::decode(response(bytes.clone(), false), 4096).unwrap();
        assert_eq!(
            decode(
                response(bytes, false),
                &DecodeLimits::default(),
                None,
                false
            )
            .unwrap()
            .into_table(),
            expected
        );
        // Unknown-only oneof field => null, same as the retained decoder.
        let bytes = vec![10, 1, b'h', 18, 4, 10, 2, 64, 1];
        assert_eq!(
            decode(
                response(bytes.clone(), false),
                &DecodeLimits::default(),
                None,
                false
            )
            .unwrap()
            .into_table(),
            crate::decode::decode(response(bytes, false), 4096).unwrap()
        );
        for bytes in [
            vec![10, 255],
            vec![0],
            vec![10, 1, 255],
            vec![18, 0],
            vec![11, 12, 12],
        ] {
            assert!(
                decode(
                    response(bytes, false),
                    &DecodeLimits::default(),
                    None,
                    false
                )
                .is_err()
            );
        }
        let mut duplicate = wire();
        duplicate.headers.push("text".into());
        duplicate.data_table.clear();
        assert_eq!(
            decode(
                response(duplicate.encode_to_vec(), false),
                &DecodeLimits::default(),
                None,
                false
            )
            .unwrap_err(),
            EodError::Schema
        );
    }
    #[test]
    fn rejects_zstd_window_and_unsupported_envelopes() {
        let input = vec![42; 1 << 20];
        let limits = DecodeLimits {
            zstd_window_log: 10,
            ..Default::default()
        };
        assert_eq!(
            decode(response(input, true), &limits, None, false).unwrap_err(),
            EodError::Decode
        );
        let mut data = response(Vec::new(), false);
        data.flat_file_manifest = Some(Default::default());
        assert_eq!(
            decode(data, &DecodeLimits::default(), None, false).unwrap_err(),
            EodError::Unsupported
        );
        let mut data = response(Vec::new(), false);
        data.compression_description.as_mut().unwrap().algo = 99;
        assert_eq!(
            decode(data, &DecodeLimits::default(), None, false).unwrap_err(),
            EodError::Unsupported
        );
    }
}
