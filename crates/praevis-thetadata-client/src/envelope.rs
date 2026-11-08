//! Keep the outer protobuf response bounded before constructing vendor types.
use crate::{bounded::DecodeLimits, eod::EodError, wire};
use prost::{
    Message,
    bytes::Buf,
    encoding::{DecodeContext, WireType, decode_key, decode_varint, skip_field},
};
use std::sync::{Arc, Mutex};
use tonic::{
    Status,
    codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder},
};

/// One assembled response message, handed from `framing::GuardedBody` to the codec.
/// It holds at most one message: tonic decodes each frame before polling the
/// body again.
#[derive(Clone, Default)]
pub(crate) struct ParkedFrame(Arc<Mutex<Option<Vec<u8>>>>);
impl ParkedFrame {
    /// Park a complete message. `false` if one is already parked, which would
    /// mean tonic polled the body again before decoding the previous frame.
    pub(crate) fn park(&self, message: Vec<u8>) -> bool {
        let mut slot = self.0.lock().unwrap_or_else(|e| e.into_inner());
        debug_assert!(
            slot.is_none(),
            "a message was parked before the previous one was decoded"
        );
        if slot.is_some() {
            return false;
        }
        *slot = Some(message);
        true
    }
    /// Move the parked message out, if any.
    pub(crate) fn take(&self) -> Option<Vec<u8>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).take()
    }
}

/// Encodes any request message with prost and hands each response message
/// over as owned bytes, so no prost response tree is built by the transport.
/// The messages are assembled by `framing::GuardedBody` (item 8, option 2a);
/// tonic only carries an empty frame for each.
pub(crate) struct EnvelopeCodec<T>(std::marker::PhantomData<fn(T)>, ParkedFrame);
impl<T> EnvelopeCodec<T> {
    /// A codec reading the messages this call's `GuardedChannel` parks.
    pub(crate) fn new(parked: ParkedFrame) -> Self {
        Self(std::marker::PhantomData, parked)
    }
}
pub(crate) struct RequestEncoder<T>(std::marker::PhantomData<fn(T)>);
pub(crate) struct FrameDecoder(pub(crate) ParkedFrame);
impl<T: Message + Send + 'static> Codec for EnvelopeCodec<T> {
    type Encode = T;
    type Decode = Vec<u8>;
    type Encoder = RequestEncoder<T>;
    type Decoder = FrameDecoder;
    fn encoder(&mut self) -> RequestEncoder<T> {
        RequestEncoder(std::marker::PhantomData)
    }
    fn decoder(&mut self) -> FrameDecoder {
        FrameDecoder(self.1.clone())
    }
}
impl<T: Message> Encoder for RequestEncoder<T> {
    type Item = T;
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
        // The body parked the whole message (one exact-capacity Vec, after its
        // limit check) and tonic carried an empty frame for it. Move the
        // message out: no copy, and nothing retained by the transport.
        debug_assert_eq!(src.remaining(), 0, "tonic carries only empty frames");
        src.advance(src.remaining());
        let message = self.0.take();
        debug_assert!(message.is_some(), "exactly one message is parked per frame");
        message
            .map(Some)
            .ok_or_else(|| Status::internal("local response framing failure"))
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
