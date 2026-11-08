# EOD transport buffering: root cause and design alternatives (item 8, 2026-11-03)

Investigation only. **No data-path change is merged.** The prototypes live on
separate, unmerged branches; the maintainer chooses an option after reading
this record. It follows the [memory profile](eod-memory-profile.md), which
found about one encoded message held per concurrently open stream, outside
the shared budget.

## Phase 1: root cause, proven

**Instrument.** The `eod_memory` example
([harness](../../crates/praevis-thetadata-client/examples/eod_memory.rs)) gains
two experiment modes:

- **Call-site attribution** (`PRAEVIS_SITES=1`): the counting allocator
  records a backtrace for every live allocation of at least 1 MiB, and
  snapshots the live set at each new peak, grouped by source location.
- **Retention snapshot** (`PRAEVIS_RETAIN=1`): read and drop one batch, hold
  the stream idle, then list what is still live.

Build with `CARGO_PROFILE_RELEASE_DEBUG=line-tables-only` for source lines.
The workload is the calibration's `text-24mib` batch (an encoded message of
about 24 MiB), NONE, numeric engine, default policy (2 jobs, 16 streams),
2 messages per query, in the aarch64 sandbox.

### Which allocation sites hold the bytes at peak

| Site | 1 query | 16 queries |
| --- | --- | --- |
| tonic 0.14.6 `codec/decode.rs:199`: `self.buf.reserve(len)` in `Streaming::decode_chunk` | 24 MiB × 1 | **384 MiB = 24 MiB × 16** |
| ours, `envelope.rs:43`: `FrameDecoder` copies tonic's buffer into an owned `Vec` | 24 MiB × 1 | 24 MiB × 1 |
| ours, `envelope.rs:116`: `payload.to_vec()` in the envelope parser | 24 MiB × 1 | 24 MiB × 1 |
| ours, `bounded.rs:177`: decoded text cells (output) | (after the frame is freed) | 24 × 1 MiB |
| Rust heap peak (all allocations) | 77 MiB | 508 MiB |

### Why tonic holds one message per stream

1. **Reserve on header.** When tonic reads a gRPC message header it calls
   `self.buf.reserve(len)` with the announced length (`decode.rs:199`). The
   per-RPC `BytesMut` starts at 8 KiB, so this is one allocation of exactly
   the message size. There is no doubling: each allocation is 24.0 MiB.
2. **Never released.** After the codec consumes the message, tonic only
   `advance`s the buffer (`DecodeBuf::advance`). The allocation stays with
   the `Streaming` until it is dropped.
   - **Proof (retention snapshot):** after one batch was read and dropped, with
     the stream idle, exactly one 24.0 MiB allocation from `decode.rs:199`
     was live. After the stream completed and was dropped: 0.0 MiB.
3. **Why every open stream has one.** A stream only reads its next message
   or its end-of-stream while holding a decode-job slot (2 by default). So
   after delivering a batch it waits idle, retaining its buffer, while the
   other streams take turns. With 16 open streams, 16 buffers are live, even
   with one message per stream (433 MiB measured).

### Controls that rule out other layers

- **h2 receive buffering is bounded by the window, and it is not the main
  cost.** In the `exp/transport-windows` branch the stream window was varied
  (connection window = (streams + 1) × stream window, as today):

  | Stream window | tonic retained | Rust heap peak | `text-24mib` × 16 time | `large-mixed` × 16 heap / time |
  | ---: | ---: | ---: | ---: | --- |
  | 64 KiB | 384 MiB | 457 MiB | 1,203 ms | 23 MiB / 310 ms |
  | 256 KiB | 384 MiB | 483 MiB | 1,008 ms | — |
  | **2 MiB (current)** | 384 MiB | 501 MiB | 1,017 ms | 49 MiB / 292 ms |
  | 16 MiB | 384 MiB | 688 MiB | 1,053 ms | 88 MiB / 302 ms |

  tonic's share is constant. Only the remainder grows with the window, which
  shows h2 buffers up to the window per stream and no more. Current settings:
  2 MiB stream window, 34 MiB connection window, adaptive window off.
- **The `max_decoding_message_size` limit does not size the buffer.** With
  `encoded_bytes` (and so tonic's limit) set to 32 MiB or 64 MiB, the retained
  size is identical: 16 × 24 MiB. tonic reserves the announced `len`, not the
  limit.
- **Message size:** at 16 streams, the heap per stream follows the encoded
  message: about 11, 21 and 31 MiB for 8, 16 and 24 MiB
  ([profile](eod-memory-profile.md)).
- **Our pipeline:** while a job decodes, one message exists 3 times (tonic's
  buffer, our `FrameDecoder` copy, our payload copy), plus the decoded output.
  The payload copy is short-lived: the frame is freed after parsing.

**Root cause:** tonic 0.14's `Streaming` keeps one exact-size decode buffer per
RPC for its whole lifetime, sized to the largest message it has seen. The
client cannot release it, and our admission does not see it. Our own pipeline
adds a redundant full copy (`FrameDecoder`) per active job.

## Phase 2: design alternatives

Each prototype is on its own unmerged branch, measured with the same harness
(aarch64; runner results below):

- [`exp/transport-windows`](https://github.com/Praevis-Quant/praevis-thetadata/tree/exp/transport-windows)
  (option 1, an environment override of the stream window);
- [`exp/transport-own-framing`](https://github.com/Praevis-Quant/praevis-thetadata/tree/exp/transport-own-framing)
  (option 2a);
- [`exp/transport-payload-in-place`](https://github.com/Praevis-Quant/praevis-thetadata/tree/exp/transport-payload-in-place)
  (option 3).

Option 2a's raw runner memory reports:
[Windows](eod-memory/option-2a/windows-x86_64.md), [Linux x86_64](eod-memory/option-2a/linux-x86_64.md).

| # | Option | Branch | Peak Rust heap, 1 / 16 queries (`text-24mib`) | Throughput | Complexity | Risks | Public API | Total memory bounded? |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| — | **Current** | `main` | 74-77 / 501-508 MiB | baseline | — | — | — | No: per stream × largest message |
| 1 | Small HTTP/2 windows | `exp/transport-windows` | unchanged / 457 MiB at 64 KiB | -18% on 24 MiB messages at 64 KiB | trivial | the stall fix's receive-credit invariant must hold for any window | none | **No**: tonic's buffer is independent of the window |
| 2a | **Own message framing in our body layer, bypassing tonic's buffer** | `exp/transport-own-framing` | **50 / 128 MiB** | Gate neutral. Large messages: Linux 11-17% faster, Windows up to about 8% slower in some cases (below) | small (about 60 lines in `framing.rs`/`envelope.rs`) | relies on tonic decoding each yielded frame before polling again (FIFO; covered by tests) | none | **Yes, by configuration** (see below), pending a bound test |
| 2b | Full streaming decode (chunk-sized frames) | not prototyped | estimated 2a minus about one message per active job | unknown | high: incremental preflight, decoder and ZSTD | a new incremental parser over adversarial input | none | yes |
| 3 | Remove our extra copies (in-place payload) | `exp/transport-payload-in-place` | 73 / 501 MiB: **no peak change** | not measured: no memory effect | trivial | none | none | no |
| 4 | Reserve the transport buffer in admission | analysis | like a dynamic stream cap: about 200 MiB at an effective 4 streams ([cap sweep](eod-memory-profile.md)) | lowers concurrency for large messages | moderate | must reserve the largest possible message for a stream's whole lifetime, because tonic retains until drop | none | yes, by lowering concurrency |
| 5 | Upstream tonic: release the decode buffer when it empties | upstream issue/PR | as 2a, if accepted | — | external | timeline outside our control | none | yes |

### Option 2a in detail

`GuardedBody` already parses every gRPC length prefix before tonic, to
enforce our limits. In the prototype it:

- allocates one exact-capacity `Vec` when a message header passes the limit
  check;
- appends the data chunks;
- parks the complete message in a per-stream slot;
- forwards only an **empty** 5-byte gRPC frame to tonic.

Our codec takes the parked message (a move, no copy) and hands it to the
decode job. tonic's buffer stays at its 8 KiB initial size. All tonic request,
header, trailer, status and error handling is unchanged.

- **Measured** (aarch64): 16 queries 508 → **128 MiB** Rust heap
  (OS 616 → 255 MiB); 1 query 77 → 50 MiB; an idle stream after a delivered
  batch retains **nothing** message-sized. At peak only the active jobs hold
  message-sized memory: 2 × (frame + payload copy).
- **Correctness:** the full client suite passes unchanged: contract fault,
  status and framing-limit matrices, typed lifecycle, cancellation and logs.
  The framing unit tests were updated to the new contract: empty frames to
  tonic, exact messages parked in order with capacity equal to length.
- **HTTP/2 stall fix and receive-credit invariant:** unchanged. Windows,
  admission order and "poll the body only while holding a job slot" are
  untouched. The message buffer is allocated at the same moment tonic used
  to reserve it.
- **Cancellation:** dropping the stream drops the body and any partial
  message, and h2 resets the stream, as before.
- **Bound:** after 2a, message-sized memory exists only inside a job, which
  holds a slot from the accounted shared budget. Each stream otherwise holds
  at most its h2 window (structurally at most the 34 MiB connection window)
  plus tonic's 8 KiB. libzstd is within the accounting (profile). A
  **total-memory bound by configuration** therefore becomes plausible: shared
  budget + connection window + fixed overhead, plus caller-retained batches.
  It must be proven by a bound test across the profile workloads before any
  wording changes (D-C).
- **Combining with 3** removes the payload copy too: less copying, same peak,
  because the decoded output then coexists with the frame.

### Benchmark (D-B) and runner memory for option 2a

**Memory on the runners** (memory profile at `4d736bf`, run 41641101360,
against the `main` profile, run 41633224260; MiB):

| Workload | Windows: `main` → 2a | Linux x86_64: `main` → 2a |
| --- | --- | --- |
| `text-24mib` × 16, Rust heap | 508 → **128** | 505 → **128** |
| `text-24mib` × 16, OS resident | 495 → **123** | 600 → **228** |
| Stream sweep 1 / 4 / 16 queries, Rust heap | 74 / 196 / 508 → **50 / 103 / 128** | 74 / 192 / 505 → **50 / 104 / 128** |
| Standard 10k-row workloads, largest Rust heap | 63 → **47** | 63 → **48** |

With 2a, memory grows by about 3.5 MiB per additional stream (the h2 window
and bookkeeping) instead of about 27 MiB.

**Throughput:**

- **D-B gate** (decode p50, run 41641085975, 3 rounds × numeric, Table,
  numeric-offload and Table-offload): all gating minimums ≤ 1.016 on both
  runners, within 1.05. No improvement is claimed: decode excludes the
  transport by design. Single-round excursions occurred (Linux Table up to
  1.34; Windows numeric up to 1.37), so a merge candidate needs the rule-2
  extra rounds.
- **Delivery, 10k-row workloads (informational):** mean candidate/baseline
  ratio 0.98-1.02 per engine and platform; no minimum above the 1.30 flag.
- **Large messages, 16 streams** (run 41643106167, `--extremes --streams 16`,
  3 alternating rounds, informational delivery):

  | Case (delivery p50, min-max) | Linux x86_64 | Windows x86_64 |
  | --- | --- | --- |
  | numeric `text-max-none` | **0.831-0.891** | 1.008-1.127 |
  | Table `text-max-none` | **0.815-0.887** | 0.982-1.034 |
  | numeric `headers-max-none` | 0.996-1.062 | 1.061-1.084 |
  | Table `headers-max-zstd` | 0.989-1.015 | 1.049-1.302 |

- **Local alternating A/B** (aarch64, 6 rounds, elapsed time per query set):

  | Workload | Result | Rust heap |
  | --- | --- | --- |
  | `text-24mib` × 16 | 2a faster in all 6 rounds (median ~5%) | 505 → 127 MiB |
  | `text-24mib` × 1 | about 12% faster in every round | 73 → 49 MiB |
  | `large-mixed` × 16 | neutral (±3%) | 49 → 38 MiB |

**Reading:** memory improves sharply and consistently on all three platforms.
Throughput is neutral on the gate. For large messages it is faster on Linux
(runner and sandbox) and up to about 8% slower in some Windows cases in every
round. That Windows cost is unexplained so far; an implementation should
profile it, for example per-chunk `extend_from_slice` against a single
reserve-and-copy.

## Addendum (2026-11-08): the query path, and the plan

**The bound above was measured on the EOD path, and the query path differs.**
On the EOD path a stream reads only while it holds a job slot, so 2a leaves
message-sized memory only inside a job.
- **On `query()`/`job()`, a stream reads without a slot.**
  `QueryStream::next_inner` awaits `inner.message()` without one, and takes
  a job slot (`pool.jobs`) only to decode a large frame on the blocking
  pool.
- **The effect:** every open stream assembles its next message
  independently.
- **What 2a still gives here:** it removes the idle retention (nothing
  message-sized stays after delivery) and one copy (tonic's buffer into the
  owned frame).
- **What it does not:** with N open streams, N max-size messages can still
  be assembled at once. Without further work the query-path worst case stays
  **streams × `encoded_bytes`** (8 × 64 MiB at PRO's 8 streams).

**Decision (user, 2026-11-08, after a risk review).** The target use case
is 1,000 of the licensee's own workloads in one process. Four PRs proceed,
in this order:

| PR | What |
| ---: | --- |
| 0 | Tooling: the largest encoded message per request for the W5c and W6c shapes (`live_p0 sizes`), bounded, with no flat files |
| 1 | 2a on both callers (`query()` and the EOD stream), with a one-message parked slot. Full D-B benchmark, and the Windows large-message cost profiled. `stream_raw` stays on plain tonic |
| 3 | A bound test on the query path: 8 streams, 24 and 64 MiB messages, overlapping and with a stalled consumer, Windows and Linux |
| 4 | Docs: close item 8 with the honest bound, tested and documented |

**Held: byte credits (PR 2).** That is a shared byte budget for messages
being assembled, which would bound the concurrent term. **The trigger to
revisit it:** if PR 0's real maxima reach the tens of MiB.

**`encoded_bytes` is not lowered.**

## Recommendation

**Adopt option 2a** (own message framing in the body layer), optionally with
option 3. In order:

1. Implement 2a properly: a `ParkedFrame` type instead of a raw queue, a
   debug assertion that exactly one frame is parked per decode, and tests that
   pin the parking contract under fragmentation, errors, trailers and
   cancellation.
2. Run the full D-B protocol, including the rule-2 extra rounds, and the
   large-message delivery comparison. Profile the Windows `headers-max`/
   `text-max` delivery cost before acceptance.
3. Add a **configuration-bound test** to the memory profile: peak growth ≤
   shared budget + connection window + a fixed margin across all profile
   workloads, on Windows and Linux. Only if it holds, propose new D-C wording
   ("total client memory is bounded by configuration") for maintainer approval.
4. Optionally report the retention upstream to tonic (option 5), so that the
   parking workaround could later be dropped.

**Not recommended:**

- Option 1 alone: it leaves tonic's buffer untouched, and small windows cost
  throughput.
- Option 4: it amounts to a stream cap, which the maintainer rejected.
- Option 2b: the fully streaming decoder is a large rewrite for about one more
  message per active job. Defer it unless 2a's bound proves insufficient.
