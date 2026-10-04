use crate::Error;
use prost::Message;
use std::io::Read;
use thetadata_core::{Price, Table, Value, timestamp};
use thetadata_proto::endpoints::{DataTable, ResponseData, data_value::DataType};

pub(crate) fn decode(response: ResponseData, limit: usize) -> Result<Table, Error> {
    if response.flat_file_manifest.is_some() {
        return Err(Error::FlatFileUnsupported);
    }
    if response.compressed_data.len() > limit || i64::from(response.original_size) > limit as i64 {
        return Err(Error::BatchLimit(limit));
    }
    let algo = response
        .compression_description
        .map(|d| d.algo)
        .unwrap_or(0);
    let bytes = match algo {
        0 => response.compressed_data,
        1 => {
            let decoder = zstd::stream::read::Decoder::new(response.compressed_data.as_slice())?;
            let mut bytes = Vec::new();
            decoder.take(limit as u64 + 1).read_to_end(&mut bytes)?;
            if bytes.len() > limit {
                return Err(Error::BatchLimit(limit));
            }
            bytes
        }
        n => return Err(Error::UnsupportedCompression(n)),
    };
    let table = DataTable::decode(bytes.as_slice())?;
    let rows = table
        .data_table
        .into_iter()
        .map(|row| {
            row.values
                .into_iter()
                .map(|cell| {
                    Ok(match cell.data_type {
                        None | Some(DataType::NullValue(_)) => Value::Null,
                        Some(DataType::Text(value)) => Value::Text(value),
                        Some(DataType::Number(value)) => Value::Integer(value),
                        Some(DataType::Boolean(value)) => Value::Boolean(value),
                        Some(DataType::Price(value)) => {
                            Price::from_wire(value.value, value.r#type)?
                                .map(Value::Price)
                                .unwrap_or(Value::Null)
                        }
                        Some(DataType::Timestamp(value)) => {
                            Value::Timestamp(timestamp(value.epoch_ms, value.zone)?)
                        }
                    })
                })
                .collect::<Result<Vec<_>, Error>>()
        })
        .collect::<Result<Vec<_>, _>>()?;
    let result = Table {
        headers: table.headers,
        rows,
    };
    result.validate()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use thetadata_proto::endpoints::{CompressionDescription, DataValue, DataValueList};

    fn table_bytes() -> Vec<u8> {
        DataTable {
            headers: vec!["symbol".into(), "size".into()],
            data_table: vec![DataValueList {
                values: vec![
                    DataValue {
                        data_type: Some(DataType::Text("AAPL".into())),
                    },
                    DataValue {
                        data_type: Some(DataType::Number(42)),
                    },
                ],
            }],
        }
        .encode_to_vec()
    }

    fn response(bytes: Vec<u8>, algo: i32) -> ResponseData {
        ResponseData {
            compressed_data: bytes,
            compression_description: Some(CompressionDescription { algo, level: 0 }),
            ..Default::default()
        }
    }

    #[test]
    fn compressed_and_uncompressed_batches_match() {
        let bytes = table_bytes();
        let plain = decode(response(bytes.clone(), 0), 4096).unwrap();
        let compressed = zstd::stream::encode_all(bytes.as_slice(), 1).unwrap();
        assert_eq!(decode(response(compressed, 1), 4096).unwrap(), plain);
        assert_eq!(plain.rows[0][1], Value::Integer(42));
    }

    #[test]
    fn rejects_corruption_unknown_compression_and_expansion() {
        assert!(matches!(
            decode(response(vec![], 99), 1024),
            Err(Error::UnsupportedCompression(99))
        ));
        assert!(decode(response(vec![255], 0), 1024).is_err());
        assert!(decode(response(vec![255], 1), 1024).is_err());
        let bomb = zstd::stream::encode_all(vec![0; 10000].as_slice(), 1).unwrap();
        assert!(matches!(
            decode(response(bomb, 1), 100),
            Err(Error::BatchLimit(100))
        ));
    }

    #[test]
    fn rejects_ragged_rows_and_manifests() {
        let mut table = DataTable::decode(table_bytes().as_slice()).unwrap();
        table.headers.pop();
        assert!(matches!(
            decode(response(table.encode_to_vec(), 0), 1024),
            Err(Error::Data(_))
        ));
        let manifest = ResponseData {
            flat_file_manifest: Some(Default::default()),
            ..Default::default()
        };
        assert!(matches!(
            decode(manifest, 1024),
            Err(Error::FlatFileUnsupported)
        ));
    }
}
