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

### Live market (2026-11-10): measured under extra load

**Superseded by the [clean re-run](#live-market-clean-re-run-2026-11-10)
below.** Kept because its tails were measured under leftover load.

One run each, not a verified result.
- **Setup:** Windows, main 7777151, the Terminal on live hosts and
  restarted at 14:48 UTC, the sleep consumer, 120 s per run.
- **Contracts:** the top N from 60 s of the live full trade stream
  (`discover_contracts`: 43,845 trades on 10,944 contracts).

**These runs do not meet the gate's "intended rate" condition.** All three
also received the full trade stream that `discover_contracts` had
unsubscribed:
- `quotes_unsubscribed` was 350,212 / 210,708 / 45,204;
- `contracts_seen` was 19,496 / 17,459 / 9,309;
- 118,573 / 115,696 / 42,044 frames of other types arrived.

**The cause** (reproduced on loopback, fixed in the stream crate):
- closing dropped the socket right after the close frame;
- with data still arriving, the OS resets the connection instead of
  closing it;
- a reset can discard requests the peer has received but not yet read,
  here the unsubscribe;
- on loopback, 15 of 20 such closes ended in a reset.

Closing now waits up to 2 s for the Terminal's close frame.

| Contracts | Quotes/s | p50 | p90 | p99 | p99.9 | Max | CPU (cores) | Dropped | Heartbeat gaps |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 10 | 4,725 | 15 | 18 | 25 | 41 | 240 | 0.26 | 0 | 0 |
| 100 | 21,533 | 15 | 18 | 27 | 221 | 22,469 | 0.95 | 0 | 31 (max interval 12,880 ms) |
| 1,000 | 24,273 | 15 | 20 | 33 | 9,399 | 95,254 | 1.04 | 0 | 1 |

Latencies are in µs. Even under the extra load, the p50 and p99 met the
50 µs target. The tails and heartbeat gaps at 100 and 1,000 contracts need
a clean re-run before they are read as the library's.

### Live market: clean re-run (2026-11-10)

One run each, so observations, not a verified result.
- **Setup:** Windows, main 7877d78 (with #141), the Terminal on the live
  hosts (`fpss_nj_hosts`) and restarted at 16:05 UTC, the sleep consumer,
  120 s per run.
- **Contracts:** the top N from 60 s of the live full trade stream
  (`discover_contracts`: 27,024 trades on 7,024 contracts).
- **#141 confirmed:** `discover_contracts` got the Terminal's answer to its
  bulk unsubscribe, `REQ_RESPONSE` with the literal value `SUBSCRIBED`.
  Every run then had `quotes_unsubscribed` 0 and `contracts_seen` no more
  than the contracts subscribed.

| Contracts | Quotes/s | p50 | p90 | p99 | p99.9 | Max | CPU (cores) | Dropped | Heartbeat gaps (max interval) |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 10 | 3,667 | 15 | 19 | 32 | 468 | 15,987 | 0.14 | 0 | 0 (1,016 ms) |
| 100 | 25,879 | 15 | 17 | 24 | 37 | 782 | 1.00 | 0 | 0 (1,287 ms) |
| 1,000 | 26,037 | 15 | 18 | 26 | 40 | 614 | 1.04 | 0 | 2 (2,479 ms) |

Latencies are in µs.
- **p99 met the 50 µs target at every load:** 32, 24 and 26 µs.
- **The live tails match the dev feed's:**
  - at 100 and 1,000 contracts, p99.9 was 37-40 µs and the max was under
    1 ms;
  - the 9.4 ms p99.9 and 95 ms max of the earlier 1,000-contract run did
    not recur.
- **At 10 contracts, one outlier:** a 16 ms max in a single run. The
  p99.9 of 468 µs is in line with the dev feed's waking cost at low rates.

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
  spinning too, so the thread is losing its core, not waking slowly.
  Raising the consumer's priority did not reliably change it, and pinning
  made a sleeping consumer much worse (the thread-priority experiment
  below, one run each).
- **Everything upstream:** the vendor feed, the Terminal's Java process, its
  JSON encoding and the local WebSocket. Events carry no send time, so that
  delay cannot even be measured from our side.
- **The reader's own wake-up** when a frame arrives. It is outside our
  latency measurement, but real for end-to-end latency.

### Thread-priority experiment (approved 2026-11-07)

The latency harness can set the consumer thread's scheduling, as a
measurement option only, with no library API change:
- `--consumer-priority normal|above-normal|highest|time-critical`
  (Windows `SetThreadPriority`; on Linux only `normal`, since raising it
  needs `CAP_SYS_NICE`);
- `--consumer-affinity <core>` (`SetThreadAffinityMask` or
  `sched_setaffinity`);
- `--process-priority high` (Windows `HIGH_PRIORITY_CLASS`).

The report records the values the OS reads back. The library's reader
thread keeps its defaults.

**Results (2026-11-08).** These are observations from one run each, not a
verified result.
- **Setup:** Windows 11 on the Ryzen 7 2700 (8 cores, 16 threads), the dev
  feed after a Terminal restart, main with #120, 120 s each. The run IDs
  are `w10-dev-{10,1000}-{sleep,spin}-{normal,highest,highest-pin6}-003`.
- **What the OS read back:** priority 2 (highest) and core 6. The reader
  thread was unchanged.
- **Every run:** 0 dropped and 0 quotes from unsubscribed contracts. At
  1,000 contracts, 978-987 contracts sent quotes.

Latencies are in µs.

| Contracts | Consumer | Scheduling | Quotes/s | p50 | p99 | p99.9 | Max | CPU (cores) | Heartbeat gaps |
| ---: | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 10 | sleep | normal | 772 | 16 | 35 | 61 | 6,033 | 0.03 | 0 |
| 10 | sleep | highest | 822 | 16 | 34 | 108 | 1,307 | 0.03 | 0 |
| 10 | sleep | highest + core 6 | 847 | 15 | 63 | **9,104** | **71,670** | 0.04 | 0 |
| 10 | spin | normal | 1,004 | 3 | 7 | 18 | 275 | 1.02 | 0 |
| 10 | spin | highest | 964 | 3 | 7 | 16 | 117 | 1.02 | 0 |
| 10 | spin | highest + core 6 | 920 | 3 | 6 | 11 | 167 | 1.02 | 0 |
| 1,000 | sleep | normal | 26,475 | 15 | 28 | 46 | 901 | 1.07 | 0 |
| 1,000 | sleep | highest | 25,598 | 15 | 30 | 51 | 1,773 | 1.04 | 21 |
| 1,000 | sleep | highest + core 6 | 26,232 | 15 | 43 | **2,446** | **15,810** | 1.08 | 2 |
| 1,000 | spin | normal | 25,283 | 3 | 6 | 20 | 1,547 | 1.59 | 0 |
| 1,000 | spin | highest | 25,162 | 3 | 6 | 20 | 510 | 1.54 | 0 |
| 1,000 | spin | highest + core 6 | 24,725 | 3 | 6 | 18 | 1,433 | 1.58 | 0, then 1 |

**What it shows:**
1. **Pinning a sleeping consumer is harmful.** Its p99.9 rose to 9.1 ms and
   2.4 ms, and its max to 72 ms and 16 ms. A pinned thread that wakes while
   its core is busy cannot move to a free one; it waits. **Do not pin a
   consumer that blocks.**
2. **Priority alone did not reliably change the max.**
   - It fell at 10 contracts: from 6.0 to 1.3 ms sleeping, and from 275 to
     117 µs spinning.
   - It rose at 1,000 contracts sleeping, from 0.9 to 1.8 ms.
   - That is within the run-to-run variance of a single run. The p50 and
     p99 did not change.
3. **Spinning keeps a p50 of 3 µs and a p99 of 6-7 µs at every
   priority.** Pinning a spinner did no harm but gained little.
4. **The 1,000-contract rate is genuine:** about 25-26k quotes/s, with
   978-987 contracts quoting and none unsubscribed. That confirms the
   earlier 1,000-contract runs as a real high-rate point.
5. **The max varies a lot from run to run.** At normal priority this batch
   had lower maxima than earlier ones (10-contract spin: 275 µs here
   against 1,618 µs in `-002`). A conclusion about priority would need
   repeated, verified runs.

**Conclusion:** there is no evidence that thread priority or affinity fixes
the Windows max.
- **Kept:** the harness options, for measurement.
- **The library does not change.**
- **The heartbeat gaps** (21 in the 1,000-contract sleep run at highest
  priority) stay unexplained, on the Terminal or feed side.

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
  thread and the caller's machine. The documentation can show how, per OS,
  and must warn: **do not pin a consumer that blocks** (a pinned sleeping
  consumer reached a 72 ms max in the thread-priority experiment).

### Gates before shipping

1. **Measured on Windows and Linux**, at 10, 100 and 1,000 contracts, on the
   dev feed and then live. Each run must show that it ran at the intended
   rate (`contracts_seen`, `quotes_unsubscribed` 0, after a Terminal
   restart). **The low-rate case on the dev feed is met by the
   `w10-dev-10-*-002` runs**. **Live on Windows, `Sleep` at 10, 100 and
   1,000 contracts, is met by the [clean re-run](#live-market-clean-re-run-2026-11-10)**
   (2026-11-10). **Linux under WSL2, `Sleep` live** (2026-11-11, ADR-0018
   W10 record): no drops, but a p99 of 61-68 µs, above the target.
   Native Linux, and each variant live, remain. Each variant against `Sleep`, reporting p50,
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
| 2026-11-08 | **The thread-priority experiment changes nothing.** Raising the consumer's priority did not reliably change the max. Pinning a sleeping consumer was harmful (max 72 ms). Spinning stayed at a p99 of 6-7 µs at every setting. One run each. The harness options stay for measurement; there is no library change |
| 2026-11-07 | **The low-rate re-run confirms it.** At about 670 quotes/s, spinning cut the p99 from 37 to 10 µs, for a full core against 0.027 cores, and its p99.9 and max were worse (one run each). It stays ROADMAP-only and opt-in if ever offered. The -001 10-contract runs were inflated by leftover Terminal subscriptions, fixed in #114 |
