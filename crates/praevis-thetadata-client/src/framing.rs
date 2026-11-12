//! Identify local gRPC framing failures before any message buffer is reserved,
//! and assemble each response message ourselves (item 8, option 2a).
//!
//! `GuardedBody` parses every gRPC length prefix before tonic sees it. After
//! the limit check it allocates one exact-capacity `Vec`, appends the data
//! chunks into it, parks the complete message in the stream's [`ParkedFrame`]
//! and hands tonic only an **empty** 5-byte frame. The codec moves the parked
//! message out. So tonic's decode buffer stays at its initial size and nothing
//! message-sized stays with an idle stream; the copy from tonic's buffer into
//! an owned frame is gone too. **This body is the only size guard:** tonic
//! never sees a real message length.
//!
//! This relies on tonic decoding every buffered frame before it polls the body
//! again (`Streaming::poll_next`), so at most one message is ever parked. A
//! contract test checks it, and a second park is a framing error, never a
//! silent overwrite.
//!
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

/// The empty gRPC frame tonic receives in place of each assembled message.
const EMPTY_FRAME: [u8; 5] = [0; 5];

pub(crate) use crate::envelope::ParkedFrame;

#[derive(Clone, Default)]
pub(crate) struct Failure(Arc<AtomicU8>);
impl Failure {
    pub(crate) fn classify(&self, status: Status) -> EodError {
        match self.0.load(Ordering::Relaxed) {
            1 => EodError::Resource("envelope bytes"),
            2 => EodError::Decode,
            3 => EodError::Unsupported,
            _ if status.code() == tonic::Code::NotFound => EodError::NoData,
            _ if status.code() == tonic::Code::Unauthenticated => EodError::SessionEnded,
            _ if status.code() == tonic::Code::ResourceExhausted => EodError::Busy,
            _ => EodError::Remote(status.code()),
        }
    }
}

pub(crate) struct GuardedChannel {
    pub channel: Channel,
    pub limit: usize,
    pub failure: Failure,
    /// Shared with this call's codec (`EnvelopeCodec::new`).
    pub parked: ParkedFrame,
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
        let parked = self.parked.clone();
        Box::pin(async move {
            Ok(future
                .await?
                .map(|inner| GuardedBody::new(inner, limit, failure, parked)))
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
    parked: ParkedFrame,
    /// The message being assembled: allocated only after its length passed
    /// the limit check.
    message: Option<Vec<u8>>,
}
impl GuardedBody {
    fn new(inner: Body, limit: usize, failure: Failure, parked: ParkedFrame) -> Self {
        Self {
            inner,
            limit,
            failure,
            pending: Bytes::new(),
            header: [0; 5],
            header_len: 0,
            remaining: 0,
            ended: false,
            parked,
            message: None,
        }
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
    /// Park the complete message and give tonic an empty frame in its place.
    fn complete(&mut self) -> Poll<Option<Result<Frame<Bytes>, Status>>> {
        let message = self.message.take().unwrap_or_default();
        if !self.parked.park(message) {
            return self.reject(2);
        }
        Poll::Ready(Some(Ok(Frame::data(Bytes::from_static(&EMPTY_FRAME)))))
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
                    match self.message.as_mut() {
                        Some(message) => message.extend_from_slice(&chunk),
                        None => return self.reject(2),
                    }
                    if self.remaining == 0 {
                        return self.complete();
                    }
                    continue;
                }
                // Collect the 5 header bytes, possibly across chunks, without
                // allocating; nothing is forwarded until the message is whole.
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
                    // The limit is checked before anything is allocated.
                    if length > self.limit {
                        return self.reject(1);
                    }
                    self.remaining = length;
                    self.header_len = 0;
                    // One exact allocation per message.
                    self.message = Some(Vec::with_capacity(length));
                    if length == 0 {
                        return self.complete();
                    }
                }
                continue;
            }
            match ready!(Pin::new(&mut self.inner).poll_frame(cx)) {
                Some(Ok(frame)) => match frame.into_data() {
                    Ok(data) if data.is_empty() => continue,
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

    /// A transport body: data chunks, then optional trailers.
    enum Item {
        Data(Bytes),
        Trailers(http::HeaderMap),
    }
    struct Chunks(VecDeque<Item>);
    impl http_body::Body for Chunks {
        type Data = Bytes;
        type Error = Status;
        fn poll_frame(
            mut self: Pin<&mut Self>,
            _: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Bytes>, Status>>> {
            Poll::Ready(self.0.pop_front().map(|item| {
                Ok(match item {
                    Item::Data(bytes) => Frame::data(bytes),
                    Item::Trailers(map) => Frame::trailers(map),
                })
            }))
        }
    }
    fn body_of(items: Vec<Item>, limit: usize) -> GuardedBody {
        GuardedBody::new(
            Body::new(Chunks(items.into())),
            limit,
            Failure::default(),
            ParkedFrame::default(),
        )
    }
    fn guard(chunks: Vec<Bytes>, limit: usize) -> GuardedBody {
        body_of(chunks.into_iter().map(Item::Data).collect(), limit)
    }
    fn split(data: &[u8], at: usize) -> Vec<Bytes> {
        vec![
            Bytes::copy_from_slice(&data[..at]),
            Bytes::copy_from_slice(&data[at..]),
        ]
    }
    /// Poll like tonic: after every frame, take the parked message (the
    /// codec's job) before polling again. Returns the messages and the
    /// terminal error, if any.
    async fn read(body: &mut GuardedBody) -> (Vec<Vec<u8>>, Option<Status>) {
        let mut messages = Vec::new();
        while let Some(frame) = poll_fn(|cx| Pin::new(&mut *body).poll_frame(cx)).await {
            match frame {
                Ok(frame) => {
                    if let Ok(data) = frame.into_data() {
                        assert_eq!(&data[..], &EMPTY_FRAME, "tonic sees only empty frames");
                        messages.push(body.parked.take().expect("one parked message"));
                    }
                }
                Err(status) => return (messages, Some(status)),
            }
        }
        (messages, None)
    }

    #[tokio::test]
    async fn fragmented_coalesced_and_zero_length_messages_are_parked_exactly() {
        // Three messages (3 bytes, empty, 1 byte) split at every position.
        let data = [0, 0, 0, 0, 3, 1, 2, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 42];
        for at in 0..=data.len() {
            let mut body = guard(split(&data, at), 3);
            let (messages, error) = read(&mut body).await;
            assert!(error.is_none(), "split at {at}");
            assert_eq!(
                messages,
                vec![vec![1, 2, 3], vec![], vec![42]],
                "split at {at}"
            );
            // Exact allocations: capacity equals length.
            assert!(messages.iter().all(|m| m.capacity() == m.len()));
            assert_eq!(body.failure.0.load(Ordering::Relaxed), 0);
        }
        // One byte per transport chunk: headers split across five chunks.
        let mut body = guard(
            data.iter().map(|b| Bytes::copy_from_slice(&[*b])).collect(),
            3,
        );
        assert_eq!(
            read(&mut body).await.0,
            vec![vec![1, 2, 3], vec![], vec![42]]
        );
    }

    #[tokio::test]
    async fn the_limit_is_checked_before_anything_is_allocated() {
        // Exactly the limit: one exact allocation.
        let mut body = guard(vec![Bytes::from_static(&[0, 0, 0, 0, 2, 7, 8])], 2);
        assert_eq!(read(&mut body).await.0, vec![vec![7, 8]]);
        // One over: refused from the header alone, nothing allocated.
        let mut body = guard(vec![Bytes::from_static(&[0, 0, 0, 0, 3, 7, 8, 9])], 2);
        let (messages, error) = read(&mut body).await;
        assert!(messages.is_empty());
        assert_eq!(
            body.failure.classify(error.unwrap()),
            EodError::Resource("envelope bytes")
        );
        assert!(body.message.is_none());
    }

    #[tokio::test]
    async fn huge_headers_reject_without_payload_and_without_discarding_prior_message() {
        // A valid message and a u32::MAX announcement coalesced in one transport
        // chunk. The valid message remains deliverable before the terminal error.
        let data = [0, 0, 0, 0, 1, 42, 0, 255, 255, 255, 255];
        for at in 0..=data.len() {
            let mut body = guard(split(&data, at), 1);
            let (messages, error) = read(&mut body).await;
            assert_eq!(messages, vec![vec![42]], "split at {at}");
            assert_eq!(
                body.failure.classify(error.unwrap()),
                EodError::Resource("envelope bytes")
            );
            assert!(body.message.is_none());
            assert!(
                poll_fn(|cx| Pin::new(&mut body).poll_frame(cx))
                    .await
                    .is_none()
            );
        }
    }

    #[tokio::test]
    async fn trailers_or_the_end_mid_message_are_local_decode_errors() {
        let trailers = || Item::Trailers(http::HeaderMap::new());
        for items in [
            // Trailers after part of a message's payload.
            vec![
                Item::Data(Bytes::from_static(&[0, 0, 0, 0, 3, 1])),
                trailers(),
            ],
            // Trailers in the middle of a header.
            vec![Item::Data(Bytes::from_static(&[0, 0])), trailers()],
            // The body ends mid-payload.
            vec![Item::Data(Bytes::from_static(&[0, 0, 0, 0, 3, 1, 2]))],
        ] {
            let mut body = body_of(items, 64);
            let (messages, error) = read(&mut body).await;
            assert!(messages.is_empty());
            assert_eq!(body.failure.classify(error.unwrap()), EodError::Decode);
            // The partial message is dropped, and nothing was parked.
            assert!(body.message.is_none());
            assert!(body.parked.take().is_none());
        }
    }

    #[tokio::test]
    async fn dropping_a_stream_mid_message_parks_nothing() {
        // A transport that delivers part of a message and then stays pending.
        struct Stalled(Option<Bytes>);
        impl http_body::Body for Stalled {
            type Data = Bytes;
            type Error = Status;
            fn poll_frame(
                mut self: Pin<&mut Self>,
                _: &mut Context<'_>,
            ) -> Poll<Option<Result<Frame<Bytes>, Status>>> {
                match self.0.take() {
                    Some(bytes) => Poll::Ready(Some(Ok(Frame::data(bytes)))),
                    None => Poll::Pending,
                }
            }
        }
        let parked = ParkedFrame::default();
        let mut body = GuardedBody::new(
            Body::new(Stalled(Some(Bytes::from_static(&[0, 0, 0, 0, 4, 1, 2])))),
            64,
            Failure::default(),
            parked.clone(),
        );
        let mut cx = Context::from_waker(std::task::Waker::noop());
        assert!(Pin::new(&mut body).poll_frame(&mut cx).is_pending());
        assert_eq!(body.message.as_ref().map(Vec::len), Some(2));
        drop(body);
        assert!(parked.take().is_none());
    }

    #[tokio::test]
    #[should_panic(expected = "parked before the previous one was decoded")]
    async fn parking_twice_without_decoding_is_caught() {
        // Two messages, polled without taking the first: in debug builds the
        // assertion fires; in release builds it is a framing error.
        let mut body = guard(
            vec![Bytes::from_static(&[0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 1, 2])],
            8,
        );
        for _ in 0..2 {
            let _ = poll_fn(|cx| Pin::new(&mut body).poll_frame(cx)).await;
        }
    }

    /// The contract 2a depends on: tonic decodes every buffered frame before
    /// polling the body again. If a tonic upgrade changed that, a second
    /// message would be parked before the first is taken, and this test
    /// fails (the debug assertion, or a framing error).
    #[tokio::test]
    async fn tonic_decodes_each_frame_before_polling_the_body_again() {
        let mut data = Vec::new();
        let mut expected = Vec::new();
        for n in 0..200u8 {
            let message: Vec<u8> = (0..n).collect();
            data.extend_from_slice(&[0, 0, 0, 0, n]);
            data.extend_from_slice(&message);
            expected.push(message);
        }
        let mut trailers = http::HeaderMap::new();
        trailers.insert("grpc-status", http::HeaderValue::from_static("0"));
        // All messages coalesced in one chunk, and split at several points.
        for at in [0, 1, 7, data.len() / 2, data.len()] {
            let mut items: Vec<Item> = split(&data, at).into_iter().map(Item::Data).collect();
            items.push(Item::Trailers(trailers.clone()));
            let parked = ParkedFrame::default();
            let body = GuardedBody::new(
                Body::new(Chunks(items.into())),
                1 << 20,
                Failure::default(),
                parked.clone(),
            );
            let mut streaming = tonic::codec::Streaming::new_response(
                crate::envelope::FrameDecoder(parked.clone()),
                body,
                http::StatusCode::OK,
                None,
                None,
            );
            let mut messages = Vec::new();
            while let Some(message) = streaming.message().await.unwrap() {
                messages.push(message);
            }
            assert_eq!(messages, expected, "split at {at}");
            assert!(parked.take().is_none());
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
            let (_, error) = read(&mut body).await;
            assert_eq!(body.failure.classify(error.unwrap()), expected);
        }
        let failure = Failure::default();
        assert_eq!(
            failure.classify(Status::out_of_range("local response framing failure")),
            EodError::Remote(tonic::Code::OutOfRange)
        );
    }
}
