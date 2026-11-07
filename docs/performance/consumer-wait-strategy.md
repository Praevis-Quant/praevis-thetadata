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

### Measured on Windows (pending)

> **PLACEHOLDER: to be filled in from the sleep-against-spin comparison
> after #111 merges.** The runs are 10 and 1,000 contracts on the dev feed,
> `latency --consumer sleep` and `--consumer spin`, 120 s each. For each
> run, record p50, p99, p99.9, max, the 20 slowest events and their times,
> CPU (cores), drops and the longest heartbeat interval.

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

- **The roughly 4 ms max** seen at every load on Windows. That is the thread
  losing its core (preemption, for example by the Terminal's JVM), not a
  wake-up. Only pinning and priority, with a free core, could address it;
  #111's slowest-event times will show whether it is periodic.
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
   dev feed and then live. Each variant against `Sleep`, reporting p50,
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
