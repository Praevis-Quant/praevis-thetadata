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
//! Closing or dropping a [`Stream`] first unsubscribes everything it still
//! has subscribed, then closes the socket. The Terminal's subscriptions are
//! shared by every client, and it does not document dropping them when a
//! socket closes, so without this they could keep arriving on the next
//! connection. The requests are sent before the close frame, without
//! waiting for the Terminal's responses.
//!
//! This crate never starts, stops or reconfigures the Terminal.
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
pub enum Overflow {
    /// Drop the newest event and count it (the default: never back up the
    /// Terminal).
    DropNewest,
    /// Wait for room. For tests only: it stalls the Terminal's connection.
    Block,
}

#[derive(Clone, Debug)]
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

/// An open stream: subscribe, then receive events.
pub struct Stream {
    events: Receiver<Event>,
    outgoing: Arc<Mutex<Vec<String>>>,
    next_id: AtomicU64,
    /// Contracts with quotes subscribed, to unsubscribe on close.
    quotes: Mutex<HashSet<Contract>>,
    full_trades: AtomicBool,
    counters: Arc<Counters>,
    stop: Arc<AtomicBool>,
    reader: Option<JoinHandle<()>>,
}

const HEARTBEAT_GAP: Duration = Duration::from_secs(2);
/// How long a quiet read waits before the reader checks for requests to
/// send or a stop. Data wakes it at once.
const POLL: Duration = Duration::from_millis(20);

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
        Ok(Self {
            events,
            outgoing,
            next_id: AtomicU64::new(0),
            quotes: Mutex::new(HashSet::new()),
            full_trades: AtomicBool::new(false),
            counters,
            stop,
            reader: Some(reader),
        })
    }

    fn request(&self, contract: &Contract, add: bool) -> Result<u64, StreamError> {
        if self.reader.as_ref().is_none_or(JoinHandle::is_finished) {
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

    /// Start (`add`) or stop the full trade stream: every US option trade
    /// on OPRA, each preceded by a quote and an OHLC message for its
    /// contract (options PRO only, as the vendor documents). Returns the
    /// request id.
    fn full_trades(&self, add: bool) -> Result<u64, StreamError> {
        if self.reader.as_ref().is_none_or(JoinHandle::is_finished) {
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

    /// Start the full trade stream; returns the request id.
    pub fn subscribe_full_trades(&self) -> Result<u64, StreamError> {
        self.full_trades(true)
    }

    /// Stop the full trade stream; returns the request id.
    pub fn unsubscribe_full_trades(&self) -> Result<u64, StreamError> {
        self.full_trades(false)
    }

    /// Start quote events for `contract`; returns the request id.
    pub fn subscribe_quotes(&self, contract: &Contract) -> Result<u64, StreamError> {
        self.request(contract, true)
    }

    /// Stop quote events for `contract`; returns the request id.
    pub fn unsubscribe_quotes(&self, contract: &Contract) -> Result<u64, StreamError> {
        self.request(contract, false)
    }

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
        self.counters.snapshot()
    }

    /// Unsubscribe everything still subscribed, close the socket and wait
    /// for the reader thread.
    pub fn close(mut self) -> Stats {
        self.shutdown();
        self.counters.snapshot()
    }

    fn shutdown(&mut self) {
        if self.reader.as_ref().is_some_and(|r| !r.is_finished()) {
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
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

impl Drop for Stream {
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
            let _ = socket.close(None);
            let _ = socket.flush();
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

fn disconnected() -> Event {
    Event {
        kind: Kind::Disconnected,
        received: Instant::now(),
    }
}
