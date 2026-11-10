//! Real-time option events from an operator-run Theta Terminal (W10,
//! ADR-0018 design note of 2026-11-06).
//!
//! The Terminal re-serves the vendor's stream as JSON on one local
//! WebSocket (`ws://127.0.0.1:25520/v1/events`). [`Stream::connect`] opens
//! it on a dedicated reader thread, which:
//! 1. reads a frame and stamps [`Event::received`];
//! 2. parses it without allocation into a fixed-size [`Event`] (exact
//!    prices, never floats);
//! 3. hands it to a bounded queue that the caller drains.
//!
//! The reader never blocks on a full queue, because that would back up the
//! Terminal's only connection: by default it drops the newest event and
//! counts it ([`Stats::dropped`]). The stream has no sequence numbers, so
//! gaps in data cannot be detected; heartbeat gaps over 2 s are counted. A
//! closed socket ends the stream with [`Kind::Disconnected`]; there is no
//! automatic reconnect.
//!
//! [`Stream::split`] gives a clonable, `Send + Sync` [`StreamControl`]
//! (subscribe, unsubscribe, counters, close) and the single [`StreamEvents`]
//! receiver, so one thread can wait for events while another subscribes.
//!
//! Closing (from any handle), dropping a [`Stream`] or dropping its
//! [`StreamEvents`] first unsubscribes everything still subscribed, then
//! closes the socket. The Terminal's subscriptions are
//! shared by every client, and it does not document dropping them when a
//! socket closes, so without this they could keep arriving on the next
//! connection. The requests are sent before the close frame, without
//! waiting for the Terminal's responses. Closing then waits up to 2 s for the
//! Terminal's close frame, discarding data still arriving: a socket dropped
//! with data unread is reset rather than closed, and a reset can discard
//! the unsubscribes before the Terminal reads them.
//!
//! This crate never starts, stops or reconfigures the Terminal.
/// `with_<field>` setters for a `#[non_exhaustive]` configuration struct: build
/// one with `Default::default()` (or its constructor) and chain the setters.
macro_rules! with_setters {
    ($ty:ty { $($setter:ident => $field:ident: $t:ty),* $(,)? }) => {
        impl $ty {
            $(
                #[doc = concat!("Set `", stringify!($field), "`.")]
                #[must_use]
                pub fn $setter(mut self, value: $t) -> Self {
                    self.$field = value;
                    self
                }
            )*
        }
    };
}
mod event;
mod parse;

pub use event::{
    Contract, Event, Kind, Price, Quote, Response, Right, Root, Status, Trade, TypeName,
};
pub use parse::parse;

use std::collections::HashSet;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tungstenite::{Message, WebSocket};

/// The Terminal's documented event endpoint.
pub const DEFAULT_URL: &str = "ws://127.0.0.1:25520/v1/events";

/// What the reader does when the caller's queue is full.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Overflow {
    /// Drop the newest event and count it (the default: never back up the
    /// Terminal).
    DropNewest,
    /// Wait for room. For tests only: it stalls the Terminal's connection.
    Block,
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Options {
    /// A `ws://` URL on this machine (default [`DEFAULT_URL`]).
    pub url: String,
    /// Events the queue holds (default 65,536), preallocated.
    pub capacity: usize,
    pub overflow: Overflow,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            url: DEFAULT_URL.into(),
            capacity: 1 << 16,
            overflow: Overflow::DropNewest,
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StreamError {
    #[error("only a ws:// URL on 127.0.0.1 or localhost is allowed")]
    Url,
    #[error("invalid stream options")]
    Configuration,
    #[error("could not connect to the Terminal: {0:?}")]
    Connect(std::io::ErrorKind),
    #[error("WebSocket handshake with the Terminal failed")]
    Handshake,
    #[error("the stream is closed")]
    Closed,
}

/// Counters since the stream opened. Every measurement reports them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Stats {
    pub frames: u64,
    pub quotes: u64,
    pub trades: u64,
    pub heartbeats: u64,
    pub others: u64,
    /// Text frames this slice could not parse exactly.
    pub unparsed: u64,
    /// Events dropped because the queue was full.
    pub dropped: u64,
    /// Heartbeats that arrived more than 2 s after the previous one.
    pub heartbeat_gaps: u64,
    /// The longest interval between two heartbeats, in milliseconds.
    pub heartbeat_max_interval_ms: u64,
}

#[derive(Default)]
struct Counters {
    frames: AtomicU64,
    quotes: AtomicU64,
    trades: AtomicU64,
    heartbeats: AtomicU64,
    others: AtomicU64,
    unparsed: AtomicU64,
    dropped: AtomicU64,
    heartbeat_gaps: AtomicU64,
    heartbeat_max_interval_ms: AtomicU64,
}
impl Counters {
    fn snapshot(&self) -> Stats {
        let get = |c: &AtomicU64| c.load(Ordering::Relaxed);
        Stats {
            frames: get(&self.frames),
            quotes: get(&self.quotes),
            trades: get(&self.trades),
            heartbeats: get(&self.heartbeats),
            others: get(&self.others),
            unparsed: get(&self.unparsed),
            dropped: get(&self.dropped),
            heartbeat_gaps: get(&self.heartbeat_gaps),
            heartbeat_max_interval_ms: get(&self.heartbeat_max_interval_ms),
        }
    }
}

/// An open stream: subscribe, then receive events. One owner for simple use;
/// [`Stream::split`] gives a clonable [`StreamControl`] for any thread and the
/// single [`StreamEvents`] receiver.
pub struct Stream {
    control: StreamControl,
    events: StreamEvents,
}

/// Subscribes, unsubscribes, reads the counters and closes the stream, from
/// any thread: `Clone + Send + Sync`. Requests are queued for the reader
/// thread, so a subscription made while another thread waits in
/// [`StreamEvents::recv_timeout`] takes effect at once.
#[derive(Clone)]
pub struct StreamControl {
    shared: Arc<Shared>,
}

/// The stream's events, for exactly one consumer (`Send`, not `Sync`).
/// Dropping it closes the stream, as [`StreamControl::close`] does.
pub struct StreamEvents {
    events: Receiver<Event>,
    shared: Arc<Shared>,
}

/// What the handles share: requests for the reader to send, the record of
/// what is subscribed, the counters and the reader thread.
struct Shared {
    outgoing: Arc<Mutex<Vec<String>>>,
    next_id: AtomicU64,
    /// Contracts with quotes subscribed, to unsubscribe on close.
    quotes: Mutex<HashSet<Contract>>,
    full_trades: AtomicBool,
    counters: Arc<Counters>,
    stop: Arc<AtomicBool>,
    reader: Mutex<Option<JoinHandle<()>>>,
}

const HEARTBEAT_GAP: Duration = Duration::from_secs(2);
/// How long a quiet read waits before the reader checks for requests to
/// send or a stop. Data wakes it at once.
const POLL: Duration = Duration::from_millis(20);
/// How long closing waits for the Terminal's close frame.
const CLOSE_WAIT: Duration = Duration::from_secs(2);

impl Stream {
    /// Connect to the Terminal and start the reader thread.
    pub fn connect(options: Options) -> Result<Self, StreamError> {
        if options.capacity == 0 {
            return Err(StreamError::Configuration);
        }
        let authority = options
            .url
            .strip_prefix("ws://")
            .ok_or(StreamError::Url)?
            .split('/')
            .next()
            .unwrap_or_default();
        let host = authority.rsplit_once(':').map_or(authority, |(h, _)| h);
        if host != "127.0.0.1" && host != "localhost" {
            return Err(StreamError::Url);
        }
        let tcp = TcpStream::connect(authority).map_err(|e| StreamError::Connect(e.kind()))?;
        let _ = tcp.set_nodelay(true);
        let (socket, _) =
            tungstenite::client(options.url.as_str(), tcp).map_err(|_| StreamError::Handshake)?;
        socket
            .get_ref()
            .set_read_timeout(Some(POLL))
            .map_err(|e| StreamError::Connect(e.kind()))?;
        let (sender, events) = mpsc::sync_channel(options.capacity);
        let outgoing = Arc::new(Mutex::new(Vec::new()));
        let counters = Arc::new(Counters::default());
        let stop = Arc::new(AtomicBool::new(false));
        let reader = {
            let (outgoing, counters, stop) = (outgoing.clone(), counters.clone(), stop.clone());
            std::thread::Builder::new()
                .name("praevis-thetadata-stream".into())
                .spawn(move || {
                    read(
                        socket,
                        sender,
                        options.overflow,
                        &outgoing,
                        &counters,
                        &stop,
                    )
                })
                .map_err(|e| StreamError::Connect(e.kind()))?
        };
        let shared = Arc::new(Shared {
            outgoing,
            next_id: AtomicU64::new(0),
            quotes: Mutex::new(HashSet::new()),
            full_trades: AtomicBool::new(false),
            counters,
            stop,
            reader: Mutex::new(Some(reader)),
        });
        Ok(Self {
            control: StreamControl {
                shared: shared.clone(),
            },
            events: StreamEvents { events, shared },
        })
    }

    /// The control handle (clonable, any thread) and the single receiver.
    pub fn split(self) -> (StreamControl, StreamEvents) {
        (self.control, self.events)
    }

    /// Start the full trade stream; returns the request id.
    pub fn subscribe_full_trades(&self) -> Result<u64, StreamError> {
        self.control.subscribe_full_trades()
    }

    /// Stop the full trade stream; returns the request id.
    pub fn unsubscribe_full_trades(&self) -> Result<u64, StreamError> {
        self.control.unsubscribe_full_trades()
    }

    /// Start quote events for `contract`; returns the request id.
    pub fn subscribe_quotes(&self, contract: &Contract) -> Result<u64, StreamError> {
        self.control.subscribe_quotes(contract)
    }

    /// Stop quote events for `contract`; returns the request id.
    pub fn unsubscribe_quotes(&self, contract: &Contract) -> Result<u64, StreamError> {
        self.control.unsubscribe_quotes(contract)
    }

    /// The next event, waiting at most `timeout`. `None` on timeout or after
    /// the stream ended (its last event is [`Kind::Disconnected`]).
    pub fn recv_timeout(&self, timeout: Duration) -> Option<Event> {
        self.events.recv_timeout(timeout)
    }

    /// The next event if one is waiting.
    pub fn try_recv(&self) -> Option<Event> {
        self.events.try_recv()
    }

    pub fn stats(&self) -> Stats {
        self.control.stats()
    }

    /// Unsubscribe everything still subscribed, close the socket and wait
    /// for the reader thread.
    pub fn close(self) -> Stats {
        self.control.close()
    }
}

impl StreamControl {
    /// Start the full trade stream: every US option trade on OPRA, each
    /// preceded by a quote and an OHLC message for its contract (options PRO
    /// only, as the vendor documents). Returns the request id.
    pub fn subscribe_full_trades(&self) -> Result<u64, StreamError> {
        self.shared.full_trades(true)
    }

    /// Stop the full trade stream; returns the request id.
    pub fn unsubscribe_full_trades(&self) -> Result<u64, StreamError> {
        self.shared.full_trades(false)
    }

    /// Start quote events for `contract`; returns the request id.
    pub fn subscribe_quotes(&self, contract: &Contract) -> Result<u64, StreamError> {
        self.shared.request(contract, true)
    }

    /// Stop quote events for `contract`; returns the request id.
    pub fn unsubscribe_quotes(&self, contract: &Contract) -> Result<u64, StreamError> {
        self.shared.request(contract, false)
    }

    pub fn stats(&self) -> Stats {
        self.shared.counters.snapshot()
    }

    /// Unsubscribe everything still subscribed, close the socket and wait
    /// for the reader thread. Idempotent; every handle sees the stream end.
    pub fn close(&self) -> Stats {
        self.shared.shutdown();
        self.shared.counters.snapshot()
    }
}

impl StreamEvents {
    /// The next event, waiting at most `timeout`. `None` on timeout or after
    /// the stream ended (its last event is [`Kind::Disconnected`]).
    pub fn recv_timeout(&self, timeout: Duration) -> Option<Event> {
        self.events.recv_timeout(timeout).ok()
    }

    /// The next event if one is waiting.
    pub fn try_recv(&self) -> Option<Event> {
        self.events.try_recv().ok()
    }

    pub fn stats(&self) -> Stats {
        self.shared.counters.snapshot()
    }

    /// Close the stream (see [`StreamControl::close`]).
    pub fn close(self) -> Stats {
        self.shared.shutdown();
        self.shared.counters.snapshot()
    }
}

impl Drop for StreamEvents {
    /// The consumer is gone: close the stream, unsubscribing first.
    fn drop(&mut self) {
        self.shared.shutdown();
    }
}

impl Shared {
    fn open(&self) -> bool {
        self.reader
            .lock()
            .map(|r| r.as_ref().is_some_and(|r| !r.is_finished()))
            .unwrap_or(false)
    }

    fn request(&self, contract: &Contract, add: bool) -> Result<u64, StreamError> {
        if !self.open() {
            return Err(StreamError::Closed);
        }
        let id = self.send_quotes(contract, add)?;
        if let Ok(mut quotes) = self.quotes.lock() {
            if add {
                quotes.insert(*contract);
            } else {
                quotes.remove(contract);
            }
        }
        Ok(id)
    }

    /// Queue a quote request for the reader to send.
    fn send_quotes(&self, contract: &Contract, add: bool) -> Result<u64, StreamError> {
        // A new id per request, as the vendor asks.
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let right = match contract.right {
            Right::Call => "C",
            Right::Put => "P",
        };
        let text = format!(
            r#"{{"msg_type":"STREAM","sec_type":"OPTION","req_type":"QUOTE","add":{add},"id":{id},"contract":{{"root":"{}","expiration":{},"strike":{},"right":"{right}"}}}}"#,
            contract.root.as_str(),
            contract.expiration,
            contract.strike
        );
        self.outgoing
            .lock()
            .map_err(|_| StreamError::Closed)?
            .push(text);
        Ok(id)
    }

    fn full_trades(&self, add: bool) -> Result<u64, StreamError> {
        if !self.open() {
            return Err(StreamError::Closed);
        }
        let id = self.send_full_trades(add)?;
        self.full_trades.store(add, Ordering::Relaxed);
        Ok(id)
    }

    fn send_full_trades(&self, add: bool) -> Result<u64, StreamError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let text = format!(
            r#"{{"msg_type":"STREAM_BULK","sec_type":"OPTION","req_type":"TRADE","add":{add},"id":{id}}}"#
        );
        self.outgoing
            .lock()
            .map_err(|_| StreamError::Closed)?
            .push(text);
        Ok(id)
    }

    /// Unsubscribe what is still subscribed, then stop the reader, which
    /// sends the queued requests before the close frame, and wait for it.
    /// Runs once; later calls find no reader and return.
    fn shutdown(&self) {
        let reader = self.reader.lock().ok().and_then(|mut r| r.take());
        let Some(reader) = reader else {
            return;
        };
        if !reader.is_finished() {
            if self.full_trades.swap(false, Ordering::Relaxed) {
                let _ = self.send_full_trades(false);
            }
            let quotes = self
                .quotes
                .lock()
                .map(|mut q| q.drain().collect::<Vec<_>>())
                .unwrap_or_default();
            for contract in &quotes {
                let _ = self.send_quotes(contract, false);
            }
        }
        // The reader sends queued requests before it closes the socket.
        self.stop.store(true, Ordering::Relaxed);
        let _ = reader.join();
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// The reader thread. Owns the socket.
fn read(
    mut socket: WebSocket<TcpStream>,
    sender: SyncSender<Event>,
    overflow: Overflow,
    outgoing: &Mutex<Vec<String>>,
    counters: &Counters,
    stop: &AtomicBool,
) {
    let mut last_heartbeat: Option<Instant> = None;
    let mut pending: Vec<String> = Vec::new();
    loop {
        // Read the flag before draining, so requests queued before a stop
        // (the unsubscribes on close) are sent before the close frame.
        let stopping = stop.load(Ordering::Relaxed);
        if let Ok(mut queue) = outgoing.lock() {
            std::mem::swap(&mut *queue, &mut pending);
        }
        for text in pending.drain(..) {
            if socket.send(Message::text(text)).is_err() {
                let _ = sender.try_send(disconnected());
                return;
            }
        }
        if stopping {
            finish_close(&mut socket);
            return;
        }
        let frame = match socket.read() {
            Ok(frame) => frame,
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(_) => {
                let _ = sender.send(disconnected());
                return;
            }
        };
        let received = Instant::now();
        let Message::Text(text) = frame else {
            // Pings are answered by tungstenite; nothing else is expected.
            if matches!(frame, Message::Close(_)) {
                let _ = sender.send(disconnected());
                return;
            }
            continue;
        };
        counters.frames.fetch_add(1, Ordering::Relaxed);
        let Some(kind) = parse(text.as_str()) else {
            counters.unparsed.fetch_add(1, Ordering::Relaxed);
            continue;
        };
        match kind {
            Kind::Quote { .. } => &counters.quotes,
            Kind::Trade { .. } => &counters.trades,
            Kind::Heartbeat { .. } => {
                if let Some(last) = last_heartbeat {
                    let interval = received.duration_since(last);
                    if interval > HEARTBEAT_GAP {
                        counters.heartbeat_gaps.fetch_add(1, Ordering::Relaxed);
                    }
                    counters
                        .heartbeat_max_interval_ms
                        .fetch_max(interval.as_millis() as u64, Ordering::Relaxed);
                }
                last_heartbeat = Some(received);
                &counters.heartbeats
            }
            _ => &counters.others,
        }
        .fetch_add(1, Ordering::Relaxed);
        let event = Event { kind, received };
        match overflow {
            Overflow::DropNewest => match sender.try_send(event) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => {
                    counters.dropped.fetch_add(1, Ordering::Relaxed);
                }
                // The caller is gone.
                Err(TrySendError::Disconnected(_)) => return,
            },
            Overflow::Block => {
                if sender.send(event).is_err() {
                    return;
                }
            }
        }
    }
}

/// Send the close frame, then read and discard until the Terminal answers
/// with its own (bounded by [`CLOSE_WAIT`]). Dropping the socket while data
/// is still arriving makes the OS reset the connection instead of closing
/// it, and a reset can discard requests the Terminal has received but not
/// yet read: the unsubscribes sent just before. With the full trade stream
/// flowing, that is nearly every close.
fn finish_close(socket: &mut WebSocket<TcpStream>) {
    let _ = socket.close(None);
    let _ = socket.flush();
    let deadline = Instant::now() + CLOSE_WAIT;
    while Instant::now() < deadline {
        match socket.read() {
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                let _ = socket.flush();
            }
            // The handshake finished (`ConnectionClosed`) or the socket failed.
            Err(_) => return,
        }
    }
}

fn disconnected() -> Event {
    Event {
        kind: Kind::Disconnected,
        received: Instant::now(),
    }
}

with_setters!(Options {
    with_url => url: String,
    with_capacity => capacity: usize,
    with_overflow => overflow: Overflow,
});
