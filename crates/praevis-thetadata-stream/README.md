# praevis-thetadata-stream

Real-time option events from an **operator-run Theta Terminal v3**, through
its local WebSocket (`ws://127.0.0.1:25520/v1/events`). New in
`0.1.0-alpha.3`, and still **experimental** in `0.1.0`: it is outside the
supported surface (W10; see the ADR-0018 design note), so pin it exactly. It is
an independent, unofficial project, not affiliated with Theta Data Inc.

```toml
praevis-thetadata-stream = "=0.1.0"
```

- **Official path only:** it connects to the Terminal you run, on this
  machine, and never to the vendor's feed servers directly. It never starts,
  stops, updates or reconfigures the Terminal.
- **One reader thread:** it reads a frame, stamps its arrival, parses it
  without allocation into a fixed-size event, and hands it to a bounded,
  preallocated queue. Prices are exact (`mantissa × 10^-scale`, from the
  JSON digits), and strikes stay in tenths of a cent, as sent.
- **A slow consumer never backs up the Terminal:** when the queue is full,
  the newest event is dropped and counted (`Stats::dropped`).
- **Gaps:** the stream has no sequence numbers, so gaps in data cannot be
  detected. Heartbeat gaps over 2 s are counted.
- **Disconnects:** a disconnect ends the stream with `Kind::Disconnected`.
  There is no automatic reconnect.
- **One owner, or two handles:** `Stream` does everything from one thread.
  `stream.split()` gives two handles:
  - a `StreamControl` (`Clone + Send + Sync`) to subscribe, unsubscribe,
    read the counters and close from any thread;
  - the single `StreamEvents` receiver.

  So one thread can wait for events while another subscribes.
- **Closing cleans up:** `close()` (on the stream or any control handle),
  dropping the stream, or dropping the `StreamEvents` first unsubscribes
  everything still subscribed, then closes the socket. The Terminal's
  subscriptions are shared by all its clients, so this keeps a finished
  program's subscriptions from reaching the next one.
  - **The close waits for the Terminal's answer, up to 2 s.** A socket
    dropped while data is still arriving is reset rather than closed, and a
    reset can lose the unsubscribes. With the full trade stream, that's
    nearly every close.
- **Scope of this slice:**
  - option quotes per contract;
  - the full trade stream (`subscribe_full_trades`, options PRO);
  - the Terminal's answers to stream requests (`REQ_RESPONSE`).

  Other message types (OHLC, STATE) are counted with their type name but not
  decoded.
- **Contract discovery:** the `discover_contracts` example reads the full
  trade stream for a while and writes the most-traded contracts in the
  latency harness's format, with the trade date seen. The dev feed replays
  a past day the vendor does not name, so this is how you find contracts
  that trade on it.
- **Full-stream load:** the `latency` example's `--full-trades` mode
  subscribes to the full trade stream instead of a contract list. It
  reports:
  - per-event latency (quotes and trades) on the same histogram;
  - drops, and heartbeat gaps with the longest interval;
  - the Rust heap at the start and its peak (not process RSS);
  - CPU, as a JSON report under `artifacts/w10/`.

```rust,no_run
use praevis_thetadata_stream::{Contract, Kind, Options, Right, Root, Stream};
use std::time::Duration;

let stream = Stream::connect(Options::default())?;
stream.subscribe_quotes(&Contract {
    root: Root::new("SPY").unwrap(),
    expiration: 20261106,
    strike: 570_000, // $570.00, in tenths of a cent
    right: Right::Call,
})?;
while let Some(event) = stream.recv_timeout(Duration::from_secs(5)) {
    if let Kind::Quote { quote, .. } = event.kind {
        let _ = (quote.bid, quote.ask, event.received);
    }
}
# Ok::<(), praevis_thetadata_stream::StreamError>(())
```
