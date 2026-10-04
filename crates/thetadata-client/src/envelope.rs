//! Keep the outer protobuf response bounded before constructing vendor types.
use crate::{api, bounded::DecodeLimits, eod::EodError, wire};
use prost::{
    Message,
    bytes::Buf,
    encoding::{DecodeContext, WireType, decode_key, decode_varint, skip_field},
};
use tonic::{
    Status,
    codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder},
};

pub(crate) struct EnvelopeCodec;
pub(crate) struct RequestEncoder;
pub(crate) struct FrameDecoder;
impl Codec for EnvelopeCodec {
    type Encode = api::StockHistoryEodRequest;
    type Decode = Vec<u8>;
    type Encoder = RequestEncoder;
    type Decoder = FrameDecoder;
    fn encoder(&mut self) -> RequestEncoder {
        RequestEncoder
    }
    fn decoder(&mut self) -> FrameDecoder {
        FrameDecoder
    }
}
impl Encoder for RequestEncoder {
    type Item = api::StockHistoryEodRequest;
    type Error = Status;
    fn encode(&mut self, item: Self::Item, dst: &mut EncodeBuf<'_>) -> Result<(), Status> {
        item.encode(dst)
            .map_err(|_| Status::internal("request encoding failed"))
    }
}
impl Decoder for FrameDecoder {
    type Item = Vec<u8>;
    type Error = Status;
    fn decode(&mut self, src: &mut DecodeBuf<'_>) -> Result<Option<Vec<u8>>, Status> {
        // Tonic checks the configured frame ceiling before calling this codec.
        // One exact-capacity owned frame; no outer prost tree or retained slice
        // of an unknown-capacity transport allocation escapes into the worker.
        let mut bytes = vec![0; src.remaining()];
        src.copy_to_slice(&mut bytes);
        Ok(Some(bytes))
    }
}
fn field<'a>(bytes: &mut &'a [u8], ty: WireType) -> Result<&'a [u8], EodError> {
    if ty != WireType::LengthDelimited {
        return Err(EodError::Decode);
    }
    let n = usize::try_from(decode_varint(bytes).map_err(|_| EodError::Decode)?)
        .map_err(|_| EodError::Decode)?;
    if n > bytes.len() {
        return Err(EodError::Decode);
    }
    let (value, rest) = bytes.split_at(n);
    *bytes = rest;
    Ok(value)
}
pub(crate) fn parse(bytes: Vec<u8>, limits: &DecodeLimits) -> Result<wire::ResponseData, EodError> {
    if bytes.len() > limits.encoded_bytes + 1024 || bytes.capacity() > limits.allocated_bytes {
        return Err(EodError::Resource("envelope bytes"));
    }
    let mut cursor = bytes.as_slice();
    let mut payload = &[][..];
    let mut compression = None;
    let mut original_size = 0;
    while !cursor.is_empty() {
        let (tag, ty) = decode_key(&mut cursor).map_err(|_| EodError::Decode)?;
        match tag {
            1 => {
                payload = field(&mut cursor, ty)?;
                if payload.len() > limits.encoded_bytes {
                    return Err(EodError::Resource("encoded bytes"));
                }
            }
            2 => {
                let description =
                    compression.get_or_insert_with(wire::CompressionDescription::default);
                // Fixed scalar fields only. Repeated submessages merge, just as
                // prost does; an absent scalar must not reset an earlier value.
                description
                    .merge(field(&mut cursor, ty)?)
                    .map_err(|_| EodError::Decode)?;
            }
            3 => prost::encoding::int32::merge(
                ty,
                &mut original_size,
                &mut cursor,
                DecodeContext::default(),
            )
            .map_err(|_| EodError::Decode)?,
            4 => {
                field(&mut cursor, ty)?;
                return Err(EodError::Unsupported);
            }
            _ => skip_field(ty, tag, &mut cursor, DecodeContext::default())
                .map_err(|_| EodError::Decode)?,
        }
    }
    if bytes.len() - payload.len() > 1024 {
        return Err(EodError::Resource("envelope overhead"));
    }
    if original_size > 0 && original_size as usize > limits.decompressed_bytes {
        return Err(EodError::Resource("declared bytes"));
    }
    if bytes
        .capacity()
        .checked_add(payload.len())
        .is_none_or(|n| n > limits.allocated_bytes)
    {
        return Err(EodError::Resource("allocation bytes"));
    }
    Ok(wire::ResponseData {
        compressed_data: payload.to_vec(),
        compression_description: compression,
        original_size,
        flat_file_manifest: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_payloads_and_compression_merge_match_prost() {
        let bytes = vec![10, 1, 0, 10, 2, 1, 2, 18, 2, 8, 1, 18, 2, 16, 3, 24, 9];
        assert_eq!(
            parse(bytes.clone(), &DecodeLimits::default()).unwrap(),
            wire::ResponseData::decode(bytes.as_slice()).unwrap()
        );
    }
    #[test]
    fn rejects_manifest_amplification_and_metadata_without_building_trees() {
        // A large manifest consisting of empty repeated fields is never decoded.
        let mut bytes = vec![34];
        prost::encoding::encode_varint(20000, &mut bytes);
        bytes.extend(std::iter::repeat_n([10, 0], 10000).flatten());
        assert_eq!(
            parse(bytes, &DecodeLimits::default()).unwrap_err(),
            EodError::Unsupported
        );
        let mut bytes = vec![40, 0];
        bytes.extend(std::iter::repeat_n([40, 0], 512).flatten());
        assert_eq!(
            parse(bytes, &DecodeLimits::default()).unwrap_err(),
            EodError::Resource("envelope overhead")
        );
        for bytes in [vec![0], vec![10, 255], vec![10, 2, 1], vec![34, 255]] {
            assert_eq!(
                parse(bytes, &DecodeLimits::default()).unwrap_err(),
                EodError::Decode
            );
        }
    }
    #[test]
    fn bounds_copy_peak_and_overwritten_fields() {
        let limits = DecodeLimits {
            allocated_bytes: 7,
            ..Default::default()
        };
        assert_eq!(
            parse(vec![10, 4, 0, 0, 0, 0], &limits).unwrap_err(),
            EodError::Resource("allocation bytes")
        );
        let limits = DecodeLimits {
            encoded_bytes: 1,
            ..Default::default()
        };
        assert_eq!(
            parse(vec![10, 2, 0, 0, 10, 0], &limits).unwrap_err(),
            EodError::Resource("encoded bytes")
        );
    }
}
