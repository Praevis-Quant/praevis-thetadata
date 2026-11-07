# ADR-0018: Explicit direct and Terminal connection adapters

Status: Proposed, 2026-10-04. Origin: Rust enhancement.
Owner: workspace/client maintainers.
Requirements: [CONN contracts](../requirements/connections.md),
[LIVE subscription evidence](../requirements/live-verification.md).

## Context

The Python-derived Rust implementation authenticates over hosted HTTPS and queries
MDDS using direct gRPC. The vendor's Terminal diagram describes another path:
a Java gateway exposing local REST and WebSocket APIs. Similar endpoint names
do not make their transport, error, value-format or lifecycle contracts identical.
Subscription levels are also independent of connector selection.

## Proposed decision

Keep the current direct path self-contained and Java-free. Add an optional REST
adapter for an operator-managed Theta Terminal v3 first. Select connection mode
explicitly; never silently switch transports when authentication, permissions
or decoding fails. Do not mix HTTP status numbers with gRPC codes or assume that
JSON/CSV prices preserve the same scale and precision as numeric protobuf values.

Keep optional JAR process supervision in application/tooling code, separate from
the transport-independent core and auth. Require an explicitly supplied JAR/config,
record the effective version/hash, distinguish an attached existing process from
one started by this application, bound readiness and shutdown, and protect logs
and credentials. Do not silently download/update/redistribute the vendor JAR.
Choose module/crate packaging when the adapter requirements are implemented;
do not reserve a speculative crate or preemptively fragment the client workspace.

Add the WebSocket event adapter under its own subscription lifecycle. Finite
gRPC batch delivery does not implement reconnects, event gaps, subscription
acknowledgements, multiplexing or slow-consumer policy for real-time streams.

Maintain manual entitlement evidence per product/tier/connector/endpoint/query.
Expected permission rejection is a separate assertion from successful data
delivery. Missing account access remains not-run; mock status coverage cannot
establish paid-tier interoperability. This extends testing policy, not a local
entitlement engine or a promise to purchase accounts.

## W10 design note (2026-11-06)

The user decided on 2026-11-05: a WebSocket client is approved for the
real-time path only; the login-conflict probe is approved; and the dev
(replay) server comes first, with a small contract list, then scale, then
the live market. Option quotes for a contract list come first, and the full
trade stream later. This note fixes the design for that first slice. The
REST adapter and JAR supervision above stay unimplemented.

- **Path:** the operator's running Theta Terminal v3 only, at its local
  WebSocket `ws://127.0.0.1:25520/v1/events`, which allows one connection.
  No direct FPSS. We never start, stop, update or reconfigure the Terminal.
- **Packaging:** a new crate, `praevis-thetadata-stream` (`publish = false`
  for now), so auth and history users never pull a WebSocket dependency.
  - It uses `tungstenite`, the synchronous core of the approved
    `tokio-tungstenite`, without TLS (the connection is local). That is a
    narrower dependency set and needs no async runtime on the hot path.
- **Threads:**
  - One reader thread owns the socket. It reads a frame, stamps the receive
    time, parses it and hands the event to a bounded single-producer,
    single-consumer queue (a preallocated array channel).
  - The user's thread drains the queue.
  - Subscriptions go out on the same socket; the vendor asks for a new
    `id` per request.
- **Events:** fixed-size and `Copy`, parsed **without allocation** by a
  purpose-built parser for the vendor's flat JSON.
  - **Contract:** root (8 bytes), expiration `yyyymmdd`, strike in tenths
    of a cent (as sent) and right.
  - **Quote fields:** `ms_of_day`, sizes, exchanges and conditions as
    integers, and `date`.
  - **Prices are exact:** a mantissa and scale taken from the JSON number's
    digits, never a float.
  - **Status:** heartbeats (about one per second) are passed through as
    events.
  - `tungstenite` allocates one buffer per frame. That allocation is the
    library's, not the parse or the hand-off's; the measurement counts it
    separately.
- **Slow consumer:** the reader never blocks on the queue, because blocking
  would back up the Terminal's only connection. A full queue drops the
  newest event and counts it; the count is part of every measurement. A
  `Block` policy exists for tests only.
- **Gaps and reconnects:** the stream carries no sequence numbers, so gaps
  in data cannot be detected. Heartbeat gaps over 2 s are counted. A closed
  socket ends the stream with an explicit disconnect event; there is no
  automatic reconnect in this slice.
- **Measurement (W10):**
  - **What:** added latency per event, from the frame read to the event
    reaching the consumer, at p50, p99 and p99.9, against the **p99 ≤ 50 µs**
    target. Also drops, heartbeat gaps, allocations per event and CPU.
  - **What it cannot measure:** the Terminal's own delay. The events carry
    no send timestamp.
  - **Order:** the dev server first, with a small contract list (about 10),
    then 100 and 1,000, then the live market.
- **Login-conflict probe:**
  1. Hold a quote subscription and record heartbeats and events for 60 s.
  2. Log in our gRPC client and send one small request.
  3. Record the same for another 120 s.
  4. Read one Terminal REST status code before and after, a small list
     request, to see whether the Terminal's own historical session ended.

  It records only counts, codes and states. It runs during market hours, so
  quotes flow.

## W10 findings so far (2026-11-06)

**Login-conflict probe** (`w10-conflict-001`, main 7b5d7b2, Windows, options
PRO, live market, 15:02-15:06 UTC; SPY 20261106 742 C). The strike was taken
from the Terminal's own REST snapshot, so there was no extra login.
- **Our login ends the Terminal's historical session.** Its REST answered
  **200 before and 478 after**. It was still 478 at the end of the run and
  later (`w10-idle-001`). **A Terminal restart restores it (verified
  2026-11-07):** REST answered 200 again within 15 s of the restart. This
  matches the one-historical-session rule (`opt-session-001`).
- **The Terminal's live stream survives our login.** It delivered 24,135
  quotes after it, with no drops, no unparsed frames and no heartbeat gaps,
  and every heartbeat reported connected. **So running the Terminal and our
  historical client together breaks the Terminal's REST, but not its
  stream.**
- **Unexplained: our second gRPC request returned `Remote(Internal)`,** not
  the `Unauthenticated` an ended session gives (`SessionEnded`). That request
  came about 120 s after the first.
  - **Same RPC:** the second request was the same AAPL stock EOD request
    through `query()` as the first.
  - **Not an idle connection** (`w10-idle-001`, main c79fc0f, 15:29-15:36
    UTC, with the Terminal running and its REST at 478): one login, then the
    same request after 0, 30, 60, 120 and 180 s idle. All OK, through
    `query()` and the raw stub, with no status text.
  - **What differed in the failing run:** the Terminal's stream was
    actively delivering quotes on the same account.
  - **Status:** that, or a one-off server error, are both open. It stays
    unexplained, with no fix, until evidence appears.
- **Pending: quotes started about 150 s after the subscribe,** then ran at
  about 700-1,000 per second, so the contract was not quiet. The probe had
  discarded the types of the 3 non-quote messages, and has since been
  extended (#106). `login_conflict_probe --no-login` on a restarted Terminal
  will record the Terminal's answer to the subscribe and the time to the
  first quote.

**Dev feed observations (2026-11-07, Saturday).** The Terminal was
restarted with `fpss_region = "fpss_dev_hosts"`; the live configuration is
backed up as `config.toml.bak-live-20261107`. The Coordinator read the full
trade stream for 20 s (a local script, no login):
- **Replay day:** the dev feed replays **2025-04-28**, with 1,923 contracts
  active.
- **Subscribe timing:** the subscribe was answered `SUBSCRIBED` for request
  id 1, and also for id -1. The **first contract event arrived 1.5 s after
  subscribing.** A bulk subscribe on dev therefore delivers within seconds.
  Whether a single-contract subscribe on the live feed can wait 150 s is
  still open, for the live `--no-login` run on Monday.
- **Message types seen:** QUOTE (13,606), OHLC (5,384), TRADE (5,384),
  STATUS, STATE and REQ_RESPONSE. OHLC and STATE are new to this crate,
  which counts them as other types.

**W10 latency on the dev feed (2026-11-07).** Main 1affbaf, Windows on
the Ryzen 7 2700, the Terminal on the dev feed (replaying 2025-04-28), the
most-traded contracts. Added latency is measured from the frame read to the
consumer. All exit 0, none ended, 0 dropped, 0 unparsed, 0 heartbeat gaps.

| Run | Contracts | Quotes/s | p50 | p90 | p99 | p99.9 | Max | CPU (cores) | Allocations per frame |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `w10-dev-check-001` (30 s) | 10 | 1,761 | 18 µs | 30 µs | **57 µs** | 359 µs | 3.6 ms | 0.11 | 0.0003 |
| `w10-dev-10-001` | 10 | 1,682 | 16 µs | 26 µs | **50 µs** | 813 µs | 3.6 ms | 0.09 | 0.0001 |
| `w10-dev-100-001` | 100 | 10,047 | 16 µs | 23 µs | **38 µs** | 254 µs | 4.2 ms | 0.41 | 0.0001 |
| `w10-dev-1000-001` | 1,000 | 25,259 | 16 µs | 23 µs | **35 µs** | 117 µs | 4.1 ms | 1.07 | 0.0003 |

**The p99 ≤ 50 µs target, on the dev feed only:**
- **Met** at 100 and 1,000 contracts (38 and 35 µs).
- **Not reliably met at 10:** 50 µs in one run and 57 µs in the other.
- **Resolution is good enough:** the histogram has 1 µs buckets to 10 ms,
  and each quantile is reported as its bucket's upper edge, which is
  conservative. "50" means the p99 sample was between 49 and 50 µs. That
  meets the target by the letter, but one run does not make it robust. The
  live-market runs remain.

**Why fewer contracts give a worse tail: waking the consumer.**
- **The mechanism:** the consumer blocks in `recv_timeout` between events.
  At 1.7k quotes/s it is usually asleep when a quote arrives, so most
  quotes pay a thread wake-up. At 25k/s it usually finds the next event
  already queued.
- **Loopback reproduction** (Linux aarch64, a local server at fixed rates,
  the same consumer): parsing plus the hand-off cost about 1 µs; waking a
  sleeping consumer adds about 4 µs (p50 4.9 µs asleep against 1.0 µs
  spinning, at 1.7k/s). The Linux tails stay small (p99.9 ≤ 10 µs).
- **Windows wake-ups are much slower and more variable,** which is where
  the 813 µs p99.9 at 10 contracts comes from. Power states at low load are
  a likely contributor, not measured.
- **A spinning consumer removes the wake-up** (p50 1.0 µs), at the cost of
  a core. The user deferred offering it on 2026-11-07
  ([consumer wait strategy](../performance/consumer-wait-strategy.md)).

**The roughly 4 ms max: not explained.**
- It is about 4 ms at every load on Windows, and 18-24 µs on Linux
  loopback.
- **Consistent with:** the consumer thread being preempted for one
  scheduling slice, for example by the Terminal's JVM on the same machine.
- **Not measured:** the next step is to record the times of the slowest
  events, to see whether they are periodic (a GC or timer) or random.
  Done in the sleep-against-spin runs below: one backlog per run, and the
  consumer stopped taking events.

**CPU of 1.07 cores at 25k quotes/s: not spinning.** The consumer blocks in
`recv_timeout` and the reader in a socket read. That is about 42 µs of CPU
per quote, against about 10 µs per quote on Linux loopback (0.24 cores at
25k/s). The likely cost is a wake-up and a socket read per message on
Windows. It is not profiled.

**Heartbeats: 90 in 120 s at 1,000 contracts, with no gap counted.**
- Our reader counts every heartbeat frame before the queue, and nothing was
  dropped or unparsed. So the Terminal sent 90: one about every 1.33 s, with
  none more than 2 s apart.
- Under load it spaces heartbeats more widely than its "about every second".
  They are a liveness signal, not a clock. Recording the longest interval
  would make this visible.

**Sleeping against spinning on Windows (2026-11-07).** These are
observations from one run each, not a verified result. Main 461f17a, dev
feed, `latency --consumer sleep|spin`, 120 s each, runs
`w10-dev-{10,1000}-{sleep,spin}-001`. Latencies are in µs.

| Contracts | Consumer | Quotes/s | p50 | p90 | p99 | p99.9 | Max | CPU (cores) | Dropped | Heartbeat gaps |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 10 | sleep | 24,965 | 15 | 20 | 31 | 228 | 4,903 | 1.01 | 0 | 0 |
| 10 | spin | 24,002 | 3 | 4 | 7 | 103 | 13,579 | 1.53 | 0 | 0 |
| 1,000 | sleep | 24,976 | 16 | 22 | 34 | 103 | 3,069 | 1.08 | 0 | 45 (longest 3,286 ms) |
| 1,000 | spin | 23,462 | 3 | 4 | 7 | 41 | 3,677 | 1.57 | 0 | 0 |

- **Spinning cuts the p50 and the p99 about 5×** (to 3 and 7 µs), for
  about half a core more.
- **It does not improve the max.** In every run the 20 slowest events
  arrived in the same millisecond, with falling latencies; for example,
  13.6 down to 12.1 ms at 66,104 ms in the 10-contract spin run. The reader
  kept stamping and queueing frames while the consumer took nothing for
  about 12-13 ms. So the max is our consumer thread being descheduled, even
  while spinning. It is not upstream, which is outside this measurement.
  Pinning or priority might help; that is untested.
- **Heartbeat gaps:** 45 in the 1,000-contract sleep run only. They are
  unexplained; the reader counts them before the consumer, so they are most
  likely on the Terminal or feed side.
- **Caveat: the 10-contract runs ran at about 25k quotes/s,** like 1,000
  contracts, not at the 1.7k/s of the earlier 10-contract runs. The harness
  counts every quote received, so the Terminal delivered more than the 10
  subscribed contracts; why is not known. **The low-rate case is not yet
  measured** against spinning.
- **Decision:** unchanged. The user's deferral stands, with `Sleep` the only
  behaviour ([consumer wait strategy](../performance/consumer-wait-strategy.md)).

**For users, until the delay is explained:** run either the Terminal's REST
or our historical client on an account, not both. The Terminal's stream can
run alongside our client.

## Alternatives

Requiring Terminal for every user would add Java/startup/configuration costs to
the working direct path. A universal endpoint URL switch would hide incompatible
protocols and status domains. Automatically falling back after permission denial
would obscure authorization failures and could duplicate requests.

## Consequences

Each adapter needs explicit value/error/resource contracts and its own synthetic
and manual checks. Shared request/value abstractions can be reused only where
semantics are proven. Benchmark each delivery path separately; a vendor gateway
performance claim does not demonstrate lower latency for our Rust consumers.
REST/JAR/WebSocket work remains planned, not implemented by this ADR.

An opt-in consumer wait strategy (spinning instead of sleeping) for the
WebSocket stream was deferred by the user on 2026-11-07; see
[consumer wait strategy](../performance/consumer-wait-strategy.md).

See [connection inventory, diagram and sources](../connections.md),
[subscription verification](../subscription-testing.md), and
[library boundaries](0013-library-boundaries.md).
