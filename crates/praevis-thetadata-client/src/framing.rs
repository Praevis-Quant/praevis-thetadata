//! Identify local gRPC framing failures before tonic reserves the message buffer.
//! Remote status text/metadata is never used to infer local error provenance.
use crate::eod::EodError;
use http_body::Frame;
use std::{
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    task::{Context, Poll, ready},
};
use tonic::{
    Status,
    body::Body,
    codegen::{Bytes, Service, http},
    transport::Channel,
};

/// PROTOTYPE (option 2a): hand-off slot between the body and the codec. The
/// body assembles each complete gRPC message into one exact-capacity Vec and
/// parks it here; tonic only sees an empty frame, so its decode buffer never
/// grows and nothing message-sized stays with an idle stream.
pub(crate) use crate::envelope::Slot;
const EMPTY_FRAME: [u8; 5] = [0; 5];

#[derive(Clone, Default)]
pub(crate) struct Failure(Arc<AtomicU8>);
impl Failure {
    pub(crate) fn classify(&self, status: Status) -> EodError {
        match self.0.load(Ordering::Relaxed) {
            1 => EodError::Resource("envelope bytes"),
            2 => EodError::Decode,
            3 => EodError::Unsupported,
            _ if status.code() == tonic::Code::NotFound => EodError::NoData,
            _ => EodError::Remote(status.code()),
        }
    }
}

pub(crate) struct GuardedChannel {
    pub channel: Channel,
    pub limit: usize,
    pub failure: Failure,
    pub slot: Slot,
}
impl Service<http::Request<Body>> for GuardedChannel {
    type Response = http::Response<GuardedBody>;
    type Error = <Channel as Service<http::Request<Body>>>::Error;
    type Future = tonic::codegen::BoxFuture<Self::Response, Self::Error>;
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.channel.poll_ready(cx)
    }
    fn call(&mut self, request: http::Request<Body>) -> Self::Future {
        let future = self.channel.call(request);
        let limit = self.limit;
        let failure = self.failure.clone();
        let slot = self.slot.clone();
        Box::pin(async move {
            Ok(future.await?.map(|inner| GuardedBody {
                inner,
                limit,
                failure,
                pending: Bytes::new(),
                header: [0; 5],
                header_len: 0,
                remaining: 0,
                ended: false,
                slot,
                message: None,
            }))
        })
    }
}
pub(crate) struct GuardedBody {
    inner: Body,
    limit: usize,
    failure: Failure,
    pending: Bytes,
    header: [u8; 5],
    header_len: usize,
    remaining: usize,
    ended: bool,
    slot: Slot,
    message: Option<Vec<u8>>,
}
impl GuardedBody {
    /// Park the completed message and give tonic an empty frame in its place.
    fn complete(&mut self) -> Poll<Option<Result<Frame<Bytes>, Status>>> {
        let message = self.message.take().unwrap_or_default();
        self.slot.lock().unwrap().push_back(message);
        Poll::Ready(Some(Ok(Frame::data(Bytes::from_static(&EMPTY_FRAME)))))
    }
    fn reject(&mut self, kind: u8) -> Poll<Option<Result<Frame<Bytes>, Status>>> {
        self.failure.0.store(kind, Ordering::Relaxed);
        self.ended = true;
        self.pending = Bytes::new();
        self.message = None;
        Poll::Ready(Some(Err(Status::internal(
            "local response framing failure",
        ))))
    }
}
impl http_body::Body for GuardedBody {
    type Data = Bytes;
    type Error = Status;
    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Status>>> {
        if self.ended {
            return Poll::Ready(None);
        }
        loop {
            if !self.pending.is_empty() {
                if self.remaining != 0 {
                    let n = self.remaining.min(self.pending.len());
                    self.remaining -= n;
                    let chunk = self.pending.split_to(n);
                    self.message.as_mut().unwrap().extend_from_slice(&chunk);
                    if self.remaining == 0 {
                        return self.complete();
                    }
                    continue;
                }
                // Forward partial headers without allocation, but never forward
                // the final header bytes until the complete length is checked.
                let n = (5 - self.header_len).min(self.pending.len());
                let start = self.header_len;
                let bytes = self.pending.split_to(n);
                self.header[start..start + n].copy_from_slice(&bytes);
                self.header_len += n;
                if self.header_len == 5 {
                    if self.header[0] > 1 {
                        return self.reject(2);
                    }
                    // This API accepts envelope ZSTD, not gRPC compression.
                    if self.header[0] == 1 {
                        return self.reject(3);
                    }
                    let length = u32::from_be_bytes(self.header[1..].try_into().unwrap()) as usize;
                    if length > self.limit {
                        return self.reject(1);
                    }
                    self.remaining = length;
                    self.header_len = 0;
                    // One exact allocation per message, after the limit check.
                    self.message = Some(Vec::with_capacity(length));
                    if length == 0 {
                        return self.complete();
                    }
                }
                let _ = bytes;
                continue;
            }
            match ready!(Pin::new(&mut self.inner).poll_frame(cx)) {
                Some(Ok(frame)) => match frame.into_data() {
                    Ok(data) if data.is_empty() => return Poll::Ready(Some(Ok(Frame::data(data)))),
                    Ok(data) => self.pending = data,
                    Err(frame) => {
                        if self.header_len != 0 || self.remaining != 0 {
                            return self.reject(2);
                        }
                        self.ended = true;
                        return Poll::Ready(Some(Ok(frame)));
                    }
                },
                Some(Err(status)) => {
                    self.ended = true;
                    return Poll::Ready(Some(Err(status)));
                }
                None => {
                    if self.header_len != 0 || self.remaining != 0 {
                        return self.reject(2);
                    }
                    self.ended = true;
                    return Poll::Ready(None);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body::Body as _;
    use std::{collections::VecDeque, future::poll_fn};
    struct Chunks(VecDeque<Bytes>);
    impl http_body::Body for Chunks {
        type Data = Bytes;
        type Error = Status;
        fn poll_frame(
            mut self: Pin<&mut Self>,
            _: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Bytes>, Status>>> {
            Poll::Ready(self.0.pop_front().map(|b| Ok(Frame::data(b))))
        }
    }
    fn guard(chunks: Vec<Bytes>, limit: usize) -> GuardedBody {
        GuardedBody {
            inner: Body::new(Chunks(chunks.into())),
            limit,
            failure: Failure::default(),
            pending: Bytes::new(),
            header: [0; 5],
            header_len: 0,
            remaining: 0,
            ended: false,
            slot: Slot::default(),
            message: None,
        }
    }
    #[tokio::test]
    async fn fragmented_coalesced_and_zero_length_frames_preserve_bytes() {
        let data = [0, 0, 0, 0, 3, 1, 2, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 42];
        for split in 0..=data.len() {
            let mut body = guard(
                vec![
                    Bytes::copy_from_slice(&data[..split]),
                    Bytes::copy_from_slice(&data[split..]),
                ],
                3,
            );
            let mut result = Vec::new();
            while let Some(frame) = poll_fn(|cx| Pin::new(&mut body).poll_frame(cx)).await {
                result.extend_from_slice(&frame.unwrap().into_data().unwrap());
            }
            // PROTOTYPE contract: tonic sees one empty frame per message and
            // the exact messages are parked in order.
            assert_eq!(result, [0u8; 15]);
            let parked: Vec<Vec<u8>> = body.slot.lock().unwrap().drain(..).collect();
            assert_eq!(parked, vec![vec![1, 2, 3], vec![], vec![42]]);
            assert!(parked.iter().all(|m| m.capacity() == m.len()));
            assert_eq!(body.failure.0.load(Ordering::Relaxed), 0);
        }
    }
    #[tokio::test]
    async fn huge_headers_reject_without_payload_and_without_discarding_prior_message() {
        // A valid message and a u32::MAX announcement coalesced in one transport
        // chunk. The valid message remains deliverable before the terminal error.
        let data = [0, 0, 0, 0, 1, 42, 0, 255, 255, 255, 255];
        for split in 0..=data.len() {
            let mut body = guard(
                vec![
                    Bytes::copy_from_slice(&data[..split]),
                    Bytes::copy_from_slice(&data[split..]),
                ],
                1,
            );
            let mut result = Vec::new();
            loop {
                let frame = poll_fn(|cx| Pin::new(&mut body).poll_frame(cx))
                    .await
                    .unwrap();
                match frame {
                    Ok(frame) => result.extend_from_slice(&frame.into_data().unwrap()),
                    Err(status) => {
                        assert_eq!(
                            body.failure.classify(status),
                            EodError::Resource("envelope bytes")
                        );
                        break;
                    }
                }
            }
            assert_eq!(result, [0u8; 5]);
            assert_eq!(body.slot.lock().unwrap().pop_front(), Some(vec![42]));
            assert!(
                poll_fn(|cx| Pin::new(&mut body).poll_frame(cx))
                    .await
                    .is_none()
            );
        }
    }
    #[tokio::test]
    async fn truncated_headers_bodies_and_flags_have_local_provenance() {
        for (data, expected) in [
            (vec![0, 0], EodError::Decode),
            (vec![0, 0, 0, 0, 2, 42], EodError::Decode),
            (vec![2, 0, 0, 0, 0], EodError::Decode),
            (vec![1, 0, 0, 0, 0], EodError::Unsupported),
        ] {
            let mut body = guard(vec![Bytes::from(data)], 64);
            loop {
                if let Err(status) = poll_fn(|cx| Pin::new(&mut body).poll_frame(cx))
                    .await
                    .unwrap()
                {
                    assert_eq!(body.failure.classify(status), expected);
                    break;
                }
            }
        }
        let failure = Failure::default();
        assert_eq!(
            failure.classify(Status::out_of_range("local response framing failure")),
            EodError::Remote(tonic::Code::OutOfRange)
        );
    }
}
