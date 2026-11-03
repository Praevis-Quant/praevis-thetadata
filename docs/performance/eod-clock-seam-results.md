# EOD transport seam and paused-clock tests (alpha.3 A4, deferred item 4, 2026-11-03)

EOD-L3-019 requires controlled-clock tests for the whole-query deadline, idle
time, empty-message trickles, admission waits, decoding, consumer pauses and
simultaneous expiry. A paused tokio clock cannot drive the real loopback gRPC
transport, so the stream now reads from an internal `Source` enum. Production
builds have only the gRPC variant, so the `match` is a direct call. Test
builds add a scripted variant whose sleeps run on the paused clock. tokio's
`test-util` feature is enabled for dev builds only; it adds no packages.

## Pinned semantics (exact virtual times)

| Interleaving | Result |
| --- | --- |
| Message 1 ms before / exactly at / 1 ms after the idle budget | Delivered / **delivered** / `Idle` at the budget. Idle requires a strictly longer wait, because a ready transport is polled before the idle timer. |
| Message, remote status or EOF exactly at the whole-query deadline | `Deadline`: expiry has priority over anything ready at the same instant |
| Remote status or EOF 1 ms before the deadline | `Remote(code)` / clean completion |
| Idle and deadline expiring together | `Deadline` |
| `NOT_FOUND` | `NoData` |
| Five 9 s gaps of skipped empty messages, then batches (idle 10 s) | Delivered at 54 s and 63 s: idle resets per message, including skipped ones |
| Empty messages every 9 s (deadline 30 s) | `Deadline` at exactly 30 s, after 4 transport polls |
| 50 s consumer pause (idle 10 s) | The next batch is delivered; idle measures only a pending transport wait |
| Pause past the deadline | `Deadline` on resumption, without polling the transport again |
| Only job slot held for 30 s | `Deadline` at 30 s; the transport is never polled |
| 20 s admission wait, then a message 5 s later (idle 10 s, deadline 40 s) | Delivered at 25 s: idle starts once the slot is held |
| Offloaded decode held while the clock passes idle (15 s) / the deadline (31 s) | Delivered / `Deadline`; the worker's job slot is retained until it exits |
| Pending call dropped during a transport wait or an admission wait; explicit `cancel()` between batches | The transport is released immediately (not polled again; never polled when dropped during admission), then `Cancelled` once, then fused |

Every terminal case also checks that the transport and stream slot are
released, and that later calls are fused with no further transport polls.

**Non-vacuity:** each of three mutations of the stream loop fails at least
one test:
- removing the deadline-priority check;
- computing the idle deadline once per call instead of per message;
- starting the idle clock before admission.

Under 12-way CPU oversubscription, 1,800 runs of the whole `eod::tests`
module produced 0 failures.

The initial-header wait happens before a stream exists, so it is outside this
seam. It remains covered by the real-transport stalled-header tests (EOD-L3-018).

## Benchmark protocol and result

- **Workflow:** before/after mode of the **Synthetic EOD performance**
  workflow, on GitHub `ubuntu-latest` and `windows-latest`.
- **Commits:** baseline `main` at `bf16c89` (#40); candidate `dcd9989`.
- **Engines:** numeric, Table, numeric-offload and Table-offload.
- **Runs:** run 41596041645 (3 rounds), then run 41597444965 (3 more rounds of
  all four engines). The extra rounds were required by
  [policy rule 2](benchmark-policy.md#tolerances-d-b-approved-2026-11-02),
  because Windows showed single-round excursions on every engine, up to 1.92
  on Table `nulls-zstd`. They are combined with the first three, not substituted.

These are decode p50 ratios (candidate/baseline) over the eight gating
workloads; the 16-row cases are excluded.

| OS | Engine | Rounds | Largest per-workload minimum | Smallest per-workload maximum | Outcome |
| --- | --- | ---: | --- | --- | --- |
| Linux | Numeric | 6 | 0.999 (nulls-none) | 0.993 (large-mixed-none) | Neutral |
| Linux | Numeric-offload | 6 | 0.998 (nulls-none) | 0.998 (prices-none) | Neutral |
| Linux | Table | 6 | 0.996 (large-mixed-none) | 0.995 (prices-none) | Neutral |
| Linux | Table-offload | 6 | 0.991 (nulls-zstd) | 0.986 (prices-zstd) | Neutral |
| Windows | Numeric | 6 | 0.996 (prices-zstd) | 1.013 (prices-zstd) | Neutral |
| Windows | Numeric-offload | 6 | 0.994 (large-mixed-zstd) | 1.023 (large-mixed-none) | Neutral |
| Windows | Table | 6 | 0.995 (nulls-none) | 1.003 (prices-none) | Neutral |
| Windows | Table-offload | 6 | 1.003 (nulls-none) | 1.011 (timestamps-zstd) | Neutral |

No delivery or first-batch minimum exceeds the 1.30 review flag. Reported for
completeness: the three Windows extra rounds alone had gating minimums of
1.012-1.027. That is within the 1.05 tolerance, and it was absent from the
first three rounds. **Conclusion:** within tolerance on both runners, with no
measurable performance effect. Raw reports are retained as the run artifacts.
