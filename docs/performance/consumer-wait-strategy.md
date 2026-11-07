# Consumer wait strategy for the real-time stream (deferred)

Status: **deferred by the user on 2026-11-07.** Sleeping stays the only and
default behaviour, and nothing in this document is implemented in the
library. It is a complete write-up so the work can be picked up later
([ROADMAP](../ROADMAP.md), [ADR-0018](../adr/0018-explicit-connection-adapters.md)).

## The problem: waking a sleeping consumer

The W10 target is our added latency, **p99 ≤ 50 µs** from the frame read to
the user's code. On the dev feed it is met when quotes are dense and missed
when they are sparse ([W10 dev latency](../adr/0018-explicit-connection-adapters.md),
main 1affbaf, Windows on a Ryzen 7 2700, dev feed replaying 2025-04-28):

| Contracts | Quotes/s | p50 | p99 | p99.9 | Max | CPU (cores) |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 10 (30 s) | 1,761 | 18 µs | **57 µs** | 359 µs | 3.6 ms | 0.11 |
| 10 | 1,682 | 16 µs | **50 µs** | 813 µs | 3.6 ms | 0.09 |
| 100 | 10,047 | 16 µs | **38 µs** | 254 µs | 4.2 ms | 0.41 |
| 1,000 | 25,259 | 16 µs | **35 µs** | 117 µs | 4.1 ms | 1.07 |

**Fewer contracts give a worse tail.** When quotes are sparse, the consumer
is usually asleep when one arrives, so almost every quote pays a thread
wake-up. When they are dense, the next quote is usually already queued.

A loopback reproduction isolates the wake-up (Linux aarch64; a local server
sends quotes at a fixed rate; the same consumer, once sleeping in
`recv_timeout` and once polling `try_recv` in a loop):

| Rate | Consumer | p50 | p99 | p99.9 |
| ---: | --- | ---: | ---: | ---: |
| 1.7k/s | sleeping | 4.9 µs | 6.9 µs | 10.3 µs |
| 1.7k/s | spinning | **1.0 µs** | 1.4 µs | 3.1 µs |
| 25k/s | sleeping | 4.3 µs | 5.2 µs | 6.0 µs |
| 25k/s | spinning | **1.0 µs** | 1.1 µs | 1.3 µs |

So parsing plus the hand-off cost about 1 µs, and the wake-up about 4 µs
on Linux. On Windows the wake-up is much slower and more variable, which is
where the 813 µs p99.9 at 10 contracts comes from.

### Measured on Windows: sleeping against spinning (2026-11-07)

These are observations from one run each, not a verified result. Main
461f17a, Windows 11 x86_64 on the Ryzen 7 2700, dev feed replaying
2025-04-28, `latency --consumer sleep|spin`, 120 s each. The run IDs are
`w10-dev-{10,1000}-{sleep,spin}-001`, and the artifacts are local only.
Latencies are in µs. **These runs were inflated by leftover Terminal
subscriptions** (see the caveat below, fixed in #114); the low-rate re-run
follows.

| Contracts | Consumer | Quotes | Quotes/s | p50 | p90 | p99 | p99.9 | Max | CPU (cores) | Dropped | Heartbeat gaps |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 10 | sleep | 2,995,751 | 24,965 | 15 | 20 | 31 | 228 | 4,903 | 1.01 | 0 | 0 |
| 10 | spin | 2,880,261 | 24,002 | **3** | 4 | **7** | 103 | 13,579 | 1.53 | 0 | 0 |
| 1,000 | sleep | 2,997,128 | 24,976 | 16 | 22 | 34 | 103 | 3,069 | 1.08 | 0 | 45 (longest interval 3,286 ms) |
| 1,000 | spin | 2,815,473 | 23,462 | **3** | 4 | **7** | 41 | 3,677 | 1.57 | 0 | 0 |

Allocations per frame were 0.000005 at 10 contracts and 0.00033 at 1,000,
in both modes.

**What it shows:**
1. **Spinning cuts the median and the p99 about 5×.** The p50 fell from
   15-16 µs to 3 µs, and the p99 from 31-34 µs to 7 µs, at both sizes. The
   p99.9 fell too, from 228 to 103 µs and from 103 to 41 µs.
2. **It costs about half a core.** Process CPU rose from 1.01-1.08 cores to
   1.53-1.57.
3. **It does not improve the max.** In every run the 20 slowest events were
   received in the same millisecond, with latencies falling in order. In
   the 10-contract spin run, for example, they ran from 13.6 ms down to
   12.1 ms, all received at 66,104 ms.

   Each latency runs from the reader's stamp to the consumer receiving the
   event. So the reader kept stamping and queueing frames for about 1.5 ms
   while **the consumer took nothing for about 12-13 ms**, then drained the
   backlog at once. The stall is between the queue and the consumer. That
   means our consumer thread did not run: it was preempted or descheduled
   even while spinning. It is not upstream; upstream delay happens before
   the stamp and is outside this measurement. Spinning does not prevent
   it. Pinning or priority might, which is untested.
4. **Heartbeat gaps:** the 1,000-contract sleep run saw 45, with a longest
   interval of 3.3 s; the other three runs saw none. This is unexplained.
   Gaps are counted at the reader, before the consumer, so the consumer's
   mode should not affect them, and it is most likely on the Terminal or
   feed side.
5. **Throughput is set by the feed:** about 24-25k quotes/s in every run.
   The spin runs measured 4-6% fewer quotes, with 0 dropped and 0 gaps. That
   is consistent with the replay's pacing varying, not with loss.

**Caveat: these runs were inflated by leftover Terminal subscriptions,
fixed in #114.**
- **What happened:** the 10-contract runs received about 25k quotes/s,
  the rate of 1,000 contracts. Closing a stream used to leave its
  subscriptions in the Terminal, so these runs also received quotes for
  contracts from earlier runs.
- **How it was confirmed:** the re-run below, after a Terminal restart and
  with #114, saw no quotes from unsubscribed contracts.
- **What still stands:** the 1,000-contract pair may also have included
  leftovers. Its rate matched the earlier 1,000-contract run (25,259/s), so
  it stands as a high-rate data point.

### Low-rate re-run (2026-11-07)

Again one run each, not a verified result. Main 4c7d30d (with #114), after
a Terminal restart, same host and feed, 10 contracts, 120 s each. Runs
`w10-dev-10-{sleep,spin}-002`. Of the 10 subscribed contracts, 8 sent
quotes. That is expected for thin contracts on the replay day. No quotes
came from unsubscribed contracts (`contracts_seen_unsubscribed` and
`quotes_unsubscribed` both 0). Latencies are in µs.

| Consumer | Quotes | Quotes/s | p50 | p90 | p99 | p99.9 | Max | CPU (cores) | Dropped | Heartbeat gaps |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| sleep | 80,814 | 673 | 17 | 27 | 37 | 60 | 256 | 0.027 | 0 | 0 |
| spin | 80,257 | 669 | **3** | 6 | **10** | 148 | 1,618 | **1.01** | 0 | 0 |

1. **Spinning still cuts the median and the p99 at a low rate:** the p50
   fell about 5.7×, from 17 to 3 µs, and the p99 about 3.7×, from 37 to
   10 µs.
2. **It costs a full core, against 0.027 cores for sleeping: about 38×
   the CPU.** While the stream is open, the spinner burns a core whether
   or not quotes arrive. This is the strongest argument for keeping it
   opt-in only.
3. **The tail was worse spinning:** p99.9 148 against 60 µs, and max 1,618
   against 256 µs. A thread that never yields is more likely to be
   preempted mid-spin, and when it is, the events wait. That fits the
   backfire risk in the options table. It is one run, but it is the
   opposite of what spinning is for.
4. **Sleeping met the p99 ≤ 50 µs target at a low rate** (37 µs), and with
   a tighter tail than the inflated -001 runs.

**Conclusion:** the data supports the deferral as written.
- **The benefit:** spinning is a large win at the median and the p99, at
  both low and high rates.
- **The cost:** about half a core at 25k quotes/s, and a full core at a low
  rate, where sleeping uses almost nothing.
- **What it does not fix:** the max, and at a low rate it worsened the
  p99.9 and the max.

It is worth offering later as an opt-in wait strategy. Per the user's
decision it stays on the ROADMAP only, with no library change.

## The mechanism today

1. **The reader thread** owns the Terminal's WebSocket. It blocks in a
   socket read (with a 20 ms timeout to service subscriptions), stamps
   `Event::received` once a frame is complete, parses it without allocation
   and calls `try_send` on a bounded, preallocated queue
   (`std::sync::mpsc::sync_channel`).
2. **The consumer** calls `Stream::recv_timeout`, which parks the thread
   until an event arrives or the timeout ends. Each event that finds the
   consumer parked costs one wake-up: a futex on Linux, a wait-on-address
   or event on Windows, plus the scheduler getting the thread onto a core.
3. **The measured latency** runs from the stamp to the consumer receiving
   the event. It includes the parse, the queue and the wake-up. It excludes
   the reader's own wake-up when data arrives, the socket, and everything
   upstream in the Terminal.

## The options

| Strategy | What it does | Latency effect | Costs and risks |
| --- | --- | --- | --- |
| **Sleep** (today, default) | `recv_timeout` parks until an event arrives | One wake-up per event that finds the consumer parked. Worst at low rates | Near-zero CPU when idle. No contention |
| **Spin** | Poll `try_recv` in a loop, with a CPU pause hint | Removes the wake-up (loopback p50 1.0 µs) | **A full core while the stream is open, idle or not.** More power and heat, laptops throttle, and it fights the Terminal's JVM and the user's strategy code for cores. With more spinners than cores the OS preempts them, and latency gets **worse** (backfire) |
| **Spin-then-park** | Spin for a bounded window (for example 20-100 µs) after each event, then park | Removes the wake-up when the next event comes within the window, which covers dense bursts. Sparse quotes still pay it | CPU grows with event rate × window. The window needs tuning per machine. Simple and bounded |
| **Spin + core pinning / priority** | Spin, and pin the consumer to a dedicated core and raise its priority | The best tail when a core is truly free | **Windows:** `SetThreadAffinityMask` and `SetThreadPriority`, and the power plan must keep cores out of deep idle (core parking). **Linux:** `sched_setaffinity`, ideally an isolated core. It needs a core to spare, and it is OS- and machine-specific. A wrong pin can starve the Terminal or the reader |

### What spinning cannot fix

- **The max of 3-5 ms, and once 13.6 ms,** seen at every load on Windows.
  The slowest events arrive as one backlog: the consumer stops taking events
  for several milliseconds, then drains the queue. This happened while
  spinning too, so the thread is losing its core, not waking slowly. Only
  pinning and priority, with a free core, could address it, and that is
  untested.
- **Everything upstream:** the vendor feed, the Terminal's Java process, its
  JSON encoding and the local WebSocket. Events carry no send time, so that
  delay cannot even be measured from our side.
- **The reader's own wake-up** when a frame arrives. It is outside our
  latency measurement, but real for end-to-end latency.

## Proposed API (when picked up)

```rust
/// How `Stream::recv*` waits for the next event. Default `Sleep`.
pub enum WaitStrategy {
    /// Park the thread until an event arrives (today's behaviour).
    Sleep,
    /// Poll for up to `window` after each event, then park.
    SpinThenPark { window: Duration },
    /// Poll without parking. Uses a full core while the stream is open.
    Spin,
}
```

- **Opt-in only:** set in `Options`, with the default `Sleep`, so nobody
  burns a core by accident. The documentation states the CPU cost next to
  each variant.
- **Pinning and priority stay out of the library.** They are the caller's
  thread and the caller's machine. The documentation can show how, per OS.

### Gates before shipping

1. **Measured on Windows and Linux**, at 10, 100 and 1,000 contracts, on the
   dev feed and then live. Each run must show that it ran at the intended
   rate (`contracts_seen`, `quotes_unsubscribed` 0, after a Terminal
   restart). **The low-rate case on the dev feed is met by the
   `w10-dev-10-*-002` runs**; Linux, 100 contracts and live remain. Each variant against `Sleep`, reporting p50,
   p99, p99.9, max, CPU and drops. The same harness and decision rules as
   other performance changes (benchmark policy, interleaved rounds).
2. **No backfire:** with more spinning consumers than free cores, latency
   must not get worse than `Sleep`. Otherwise the documentation must say
   when not to use it.
3. **Bounded CPU for `SpinThenPark`:** a measured CPU-against-rate curve,
   and a default window justified by it.
4. **Tests:** each strategy delivers every event in order, honours timeouts
   and ends on disconnect. `Spin` must not prevent shutdown.
5. **Docs:** the CPU and power cost, and the interaction with the Terminal
   on the same machine.

## Decision record

| Date | Decision |
| --- | --- |
| 2026-11-07 | **Deferred by the user.** No spinning or spin-then-park in the library now; `Sleep` stays the only and default behaviour. The p99 ≤ 50 µs target is met at 100 contracts or more on the dev feed (38 and 35 µs). The latency harness keeps `--consumer spin` as a measurement option only, not a user feature |
| 2026-11-07 | **The Windows comparison supports the deferral.** Spinning gave a p99 of 7 µs against 31-34 µs, for about half a core more, and no better max (one run each, at about 25k quotes/s). It stays ROADMAP-only. The low-rate comparison is still to be measured |
| 2026-11-07 | **The low-rate re-run confirms it.** At about 670 quotes/s, spinning cut the p99 from 37 to 10 µs, for a full core against 0.027 cores, and its p99.9 and max were worse (one run each). It stays ROADMAP-only and opt-in if ever offered. The -001 10-contract runs were inflated by leftover Terminal subscriptions, fixed in #114 |
