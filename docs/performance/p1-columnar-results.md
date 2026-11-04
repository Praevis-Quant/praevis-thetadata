# P1 columnar decoder: CPU per row (2026-11-04)

The [`praevis-thetadata-arrow`](../../crates/praevis-thetadata-arrow/README.md)
decoder ([P1 design §4](../architecture/p1-design.md#4-columnar-decoder)),
compared with the legacy and bounded decoders on the same messages.

**Headline:** replaying the real P0 captures on the maintainer's Ryzen, the
Arrow decoder uses **3.46 µs/row (W1) and 6.07 µs/row (W2)**. That is **15.9×
and 10.4× less CPU than the vendor Python package with Polars**, so P1 exit
criterion c (≥ 10×) is **met for decoding**. It is re-measured through the
query engine (PR 4a) for the P1 exit record.

## Method

- **Harness:** the `columnar_benchmark` example, release build, single thread,
  offline, 7 samples; the median is reported.
- **Engines:**
  - `legacy-table`: `decode.rs`, prost then formatted `Table`;
  - `bounded-numeric`: `bounded.rs`, the hand reader into a `DataBatch`;
  - `columnar-arrow`: `ColumnarDecoder`, one per request, coalescing into
    65,536-row batches.
- **Replay** (`--replay <dir>`): the private captures of the
  [vendor Python baseline](../../bench/python-baseline/README.md) run
  `py-20261103T164301Z`, one file per request. The tool prints timings, counts
  and ZSTD frame header shapes only, never values.
- **Synthetic workloads:** one row per message, each an independent ZSTD frame
  (level 3), with P0's column kinds and counts (W1-shaped: 16 columns; W2-shaped:
  20) and synthetic values, 20,000 messages.
  - `--frames sized` is the default and declares the content size, as the
    vendor's frames do.
  - `--frames streaming` declares no size and a 2 MiB window. It is kept as a
    stress case.
- **Timing and counting are separate.** Timed runs use the system allocator
  directly, with one sample's inputs cloned just before its clock starts, as
  P0's replay did. Allocations are counted in a separate run.
- **Allocations:** Rust heap allocation calls per message. The owned engines'
  figure includes one copy of each input message. ZSTD's C allocator is not
  counted.

## Replay of the real captures (Ryzen 7 2700, Windows, authoritative)

Branch `p1/capture-replay` at 7627dd0. Every frame declares its size (single
segment), no frame declares a window, and no decoder failed.

| Workload | Messages = rows | Legacy table | Bounded numeric | **Arrow** | Allocations/message (legacy / bounded / Arrow) |
| --- | ---: | ---: | ---: | ---: | --- |
| W1 (10 requests) | 2,520 | 8.72 µs | 7.40 µs | **3.46 µs** | 36 / 22 / 0.63 |
| W2 (1 request) | 9,164 | 11.99 µs | 10.66 µs | **6.07 µs** | 45 / 29 / 0.04 |

Against the vendor Python package's conversion
([P0](p0-results.md#3-cpu-per-row-same-responses-decoded-offline)), with the
same responses on the same machine:

| Workload | Python with Polars | Python with pandas | Arrow | Less CPU than Polars / pandas |
| --- | ---: | ---: | ---: | --- |
| W1 | 54.5 µs | 74.4 µs | 3.46 µs | **15.9× / 21.5×** |
| W2 | 63.0 µs | 81.5 µs | 6.07 µs | **10.4× / 13.4×** |

W1's 0.63 allocations per message come from per-request setup over 10 small
requests: a new decoder, ZSTD context, schema and batch for each of 252
messages. W2 is a single request at 0.04.

## Why the synthetic Windows figures were about 50 µs/row

The first Ryzen run of the synthetic workloads put the legacy and bounded
decoders at about 50 µs/row, against 9-13 in P0. The cause was the synthetic
frames, not the decoders or the harness:

- **The frames were the cause.** They came from a streaming encoder that
  declares no content size and a 2 MiB window. The legacy and bounded decoders
  create a ZSTD context per message, so each message needed a fresh window
  buffer from the C allocator. On Windows, that costs about 40 µs. On Linux,
  glibc reuses the block (about 3%).
- **The Arrow decoder is unaffected** because it reuses one context.
- **The harness was ruled out.** With P0's method (fresh inputs, system
  allocator) the figures stayed at about 50; counting allocations while timing
  added about 15% on Windows; holding all inputs at once made the spread wider.
- **The A/B on the Ryzen:**

  | Frames | W1 legacy / bounded / Arrow | W2 legacy / bounded / Arrow |
  | --- | --- | --- |
  | streaming (no size, 2 MiB window) | 59.1 / 55.7 / 4.93 µs | 47.2 / 43.5 / 5.64 µs |
  | sized (as the vendor sends) | 9.49 / 8.41 / 4.86 µs | 10.99 / 9.65 / 5.58 µs |

The vendor's frames all declare their size, so this does not affect real
traffic. It does show that a per-message ZSTD context is fragile on Windows,
and the Arrow decoder avoids it.

## Synthetic workloads on the aarch64 sandbox (Neoverse-V2)

Sized frames, 20,000 messages:

| Workload | Legacy table | Bounded numeric | **Arrow** | Allocations/message (legacy / bounded / Arrow) |
| --- | ---: | ---: | ---: | --- |
| W1-shaped | 5.41 µs | 4.56 µs | **2.96 µs** | 35 / 22 / 0.02 |
| W2-shaped | 6.27 µs | 5.40 µs | **3.37 µs** | 44 / 28 / 0.02 |

With streaming frames, every engine here is within 3-4% of these figures.

## End to end through the client (PR 4a, loopback, aarch64 sandbox)

`--loopback` starts the test fixture's gRPC server on localhost, which sends
the same messages for every request, and times the public client, warm
connection, 5 samples (median):
- **Before:** the typed stock EOD stream (`stock_eod_batches`, bounded numeric,
  default offload).
- **After:** `query()` (Arrow, coalesced, inline below 64 KiB).

The server's encoding and sending cost is in both.

| Workload (20,000 messages, sized frames) | Stock EOD stream | **`query()`** | First batch (stock EOD / `query()`) |
| --- | ---: | ---: | --- |
| W1-shaped | 17.23 µs/row | **5.90 µs/row** | 1.17 / 1.10 ms |
| W2-shaped | 18.12 µs/row | **6.14 µs/row** | 1.33 / 1.25 ms |

With Parquet output (PR 5, `to_parquet`, ZSTD level 1, rows to a closed
file), the same run measured 6.08 µs/row (W1) and 6.53 µs/row (W2), against
5.73 / 5.98 µs/row for `query()` alone. Writing adds about 0.4-0.6 µs/row.

`query()` takes about a third of the time per row, and its first batch is not
later. The existing stock EOD stream is unchanged by PR 4a, so the
[benchmark policy](benchmark-policy.md)'s before/after gating of an existing
data path does not apply. The Ryzen figures, and replay of the P0 captures
through `query()` for the P1 exit record, follow with PR 7.

## Python D.2 gates (PR 6a, aarch64 sandbox)

These are offline tests (`crates/praevis-thetadata-py/tests/test_gates.py`),
on synthetic rows decoded by the Rust decoder:

| Gate | Result |
| --- | --- |
| No per-row Python objects, as Arrow | 4 allocated blocks at 1M and at 10M rows; tracemalloc peak 284 bytes |
| No per-row Python objects, as Polars | 449 blocks at 1M rows, 887 at 10M: per batch (65,536 rows), not per row |
| Python layer overhead per call | 0.3 µs (2.12 against 1.84 µs for the native call); the gate is ≤ 5 µs |

Reproduce:

```text
cargo run --release --locked -p praevis-thetadata-client --example columnar_benchmark -- \
  --replay artifacts/python-baseline/py-20261103T164301Z --samples 7
cargo run --release --locked -p praevis-thetadata-client --example columnar_benchmark -- \
  --rows 20000 --samples 7 [--frames streaming]
cargo run --release --locked -p praevis-thetadata-client --example columnar_benchmark -- \
  --loopback --rows 20000 --samples 5    # also times Parquet output
```
