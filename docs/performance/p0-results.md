# P0 results: baselines, breakdown and FREE limits (2026-11-03)

Phase P0 of the [performance plan](../architecture/performance-plan.md#e-phased-roadmap-tied-to-tiers):
measure before optimizing. Every live run was made by the Coordinator on the
user's machine (Windows 11, AMD Ryzen 7 2700, the FREE account, PROD) on
2026-11-03, with secret scans of every console and report clean. No rows,
keys, emails or subscription fields are in any report. Raw captures stay
private.

| Run | Tool | Source |
| --- | --- | --- |
| `py-20261103T164301Z` | [vendor Python baseline](../../bench/python-baseline/README.md), thetadata 1.0.17, Polars 1.44.2, pandas 2.3.3, Python 3.12.10 | `main` after #50 |
| `p0-breakdown-001` | `live_p0 breakdown`, release build | `main` 0675d65 |
| `p0-limits-001` | `live_p0 limits` | `main` 0675d65 |
| `p0-batch-001` | `live_p0 limits --only batch-sizes` | `main` 39eb31d |

The workloads ([`workloads.json`](../../bench/python-baseline/workloads.json))
were run 3 rounds each:
- **W1:** stock EOD, ten symbols × 2024, 252 rows each.
- **W2:** SPY option-chain EOD for 2024-06-14, all expirations; the wildcard
  was accepted; 9,164 rows.

## 1. The response format: one row per gRPC message

Both clients, every request: **one row per message.** W1 is 252 messages for
252 rows (about 230-250 bytes each); W2 is 9,164 for 9,164 (at most 293
bytes). Each message is an independent ZSTD frame. The vendor chooses this
format; no client can change it.

## 2. The network and vendor server dominate, with wide variance

| Measure | Python 1.0.17 | Praevis (Rust) |
| --- | --- | --- |
| W1 drain (stub, no conversion), 30 requests | 251 ms to 3,367 ms, median 1,916 ms | 110 ms to 3,757 ms, same spread |
| W1 time to first message | about 40-180 ms | 38-281 ms |
| W2 drain, 3 requests (+1 in the batch probe) | 5.63 / 6.23 / 7.49 s | 9.20 / 2.72 / 8.25 s (+12.82 s) |
| W2 time to first message | 1.07-1.34 s | 0.57-1.47 s |
| W2 delivery rate | 1,200-1,600 messages/s | 700-3,400 messages/s |

Identical requests vary by up to **14×** in a few minutes. The variance is the
service's, and dominates both clients. The fastest W2 drain (2.7 s, Rust) is
one sample and is not claimed as a client advantage.

**The smallest request** (1 row, `limits` probe, 61 requests) takes **37-55 ms
round trip** from this machine, with rare spikes to 170-180 ms: the floor for
a single query.

Python's conversion is **0.8%** (W1) and **6.4%** (W2) of its end-to-end time.

## 3. CPU per row (same responses, decoded offline)

| Decoder | W1 (16 columns) | W2 (20 columns) |
| --- | ---: | ---: |
| Python `_convert_response_stream`, Polars | 54.5 µs | 63.0 µs |
| Python `_convert_response_stream`, pandas | 74.4 µs | 81.5 µs |
| Praevis legacy Table decoder | about 12 µs | about 13 µs |
| Praevis bounded numeric | about 9 µs | about 12 µs |
| Praevis bounded Table | about 9 µs | about 12 µs |

The measured advantage today is **5-7× less CPU than Python**, below the
approved ≥ 10× target (P1 exit criterion c). Offline bisection on a
vendor-shaped table (16 columns, one row per ZSTD message, aarch64 sandbox):

| Component | Cost per row |
| --- | ---: |
| libzstd: decompress one tiny independent frame | **about 2.5 µs**, unavoidable. Creating a context costs 0.05 µs; reusing it or declaring the content size saves nothing (2.48-2.64 µs). Python pays the same inside its total |
| Our per-message overhead with the schema already known | **about 2 µs**: header strings re-allocated per message (16-20 `String`s) to compare with the known schema; fresh frame, payload, output and cell buffers per message. A zeroed 64 KiB stack chunk costs 0.44 µs of this |
| Total, bounded numeric | 5.5 µs with ZSTD; 2.3 µs uncompressed |

The Ryzen figures are about 2× the sandbox ones (older CPU, Windows allocator).

**Outlook:** with the per-message overhead removed, the sandbox floor is about
3 µs per row, so roughly 6 µs on the Ryzen: **about 9-13× better than Python
with Polars.** That is at the ≥ 10× target, with the vendor's
one-ZSTD-frame-per-row format setting the floor. We record this rather than
promise more.

## 4. Time to first row

| | Python | Praevis |
| --- | --- | --- |
| W1 | nothing until the whole frame is built: e2e median 1.81 s (Polars) | first row with the first message: 38-281 ms, typically 40-50 ms |
| W2 | Polars 4.9-10.7 s, pandas 2.5-10.7 s | 0.6-1.5 s |

This is the clearest win available on FREE: about **7-40× sooner**.

## 5. FREE limits: observed, not guaranteed

| Limit | Documented | Observed |
| --- | --- | --- |
| Concurrency | 1 (2 per older page) | **≥ 3 served in parallel**: 3 simultaneous tiny requests finished in 38-53 ms in all 3 trials. With 2 at once, one trial's second request answered at 177 ms (possibly brief queueing). **Not probed above 3**, by instruction |
| Per-minute limit | "20" in prose, "30" in tables | **None hit at 24/min, then 40/min** (61 tiny requests in 114 s) |
| EOD date span | Recommended: 1 month | **Server maximum between 366 and 730 days.** 365 and 366 days OK; 731 days `INVALID_ARGUMENT`. One year starting exactly at the FREE boundary (2023-06-01..2024-05-31) OK. The boundary was not the cause |
| Over-limit behavior | 429 on the Terminal REST API | Not reached |

These may be enforced differently under sustained load or for larger requests.
The scheduler must still respect a configured limit and back off on
`RESOURCE_EXHAUSTED`.

## 6. P1 design adjustments from this evidence

1. **Coalesce messages into batches.** Today's typed stream returns one batch
   per gRPC message, so W2 would yield 9,164 one-row batches. The columnar
   decoder must append rows from many messages into one Arrow batch, flushing
   on a row, byte or time threshold so first-row latency stays low.
2. **No per-message allocation** once the schema is known: compare header
   bytes in place, reuse builders and buffers, and keep a per-worker
   decompression scratch area.
3. **Inline small messages.** A 250-byte message should not be handed to a
   blocking worker: per-message offload costs more than the decode.
4. **Split by date span:** at most 366 days per EOD request (the observed
   server limit), and the vendor's sizing guidance for tick data.
5. **Make time to first row a headline metric**, next to CPU per row and peak
   memory. Total time on FREE is the vendor's.
6. **Measure fan-out before relying on it.** One request is slow on the
   server's side (about 7.6 ms per message on W1 at the median). On paid tiers,
   compare 1 against N parallel shards (and connections), as the ThetaDataDx
   review proposes.
7. **Benchmarks need distributions.** With up to 14× server variance,
   network-bound comparisons need many interleaved rounds and medians with
   spread. CPU comparisons use offline replay of identical captures, the
   private `W*.bin` files from the Python run.

## Open items

- Replay the private Python captures through both decoders offline, once P1's
  columnar decoder exists, for an identical-bytes CPU comparison on the
  Ryzen.
- Re-measure FREE concurrency only with the user's approval.
- The exact EOD span maximum (367-730 days) is not needed: P1 uses ≤ 366.
