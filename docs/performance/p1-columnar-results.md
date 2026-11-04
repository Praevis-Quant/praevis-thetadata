# P1 PR 3: columnar decoder CPU per row (2026-11-04)

The [`praevis-thetadata-arrow`](../../crates/praevis-thetadata-arrow/README.md)
decoder ([P1 design §4](../architecture/p1-design.md#4-columnar-decoder))
compared with the legacy and bounded decoders on the same messages. These are
**synthetic, vendor-shaped workloads**, not P1 exit evidence. Exit criterion c
is measured by replaying the private P0 captures on the maintainer's machine.

## Method

- **Harness:** the `columnar_benchmark` example, release build, single thread,
  offline.
- **Workloads:** one row per message, each an independent ZSTD frame (level 3),
  as observed in [P0](p0-results.md#1-the-response-format-one-row-per-grpc-message).
  - W1-shaped: 16 columns (2 timestamps, 6 prices, 8 integers);
  - W2-shaped: 20 columns (W1 plus symbol, expiration, strike and right).
  - Values are synthetic. 20,000 messages, 7 samples; the median is reported,
    with the range.
- **Engines:**
  - `legacy-table`: `decode.rs`, prost then formatted `Table`;
  - `bounded-numeric`: `bounded.rs`, the hand reader into a `DataBatch`;
  - `columnar-arrow`: `ColumnarDecoder`, coalescing into 65,536-row batches.
- **Timing and counting are separate.** Timed runs use the system allocator
  directly, with one sample's inputs cloned just before its clock starts, as
  P0's replay did. Allocations are counted in a separate run.
  `--inputs prebuilt --timing-allocator counting` reproduces this record's
  first method, which held every sample's input copies at once and counted
  while timing. On Linux, all four combinations agree within 1%.
- **Allocations:** Rust heap allocation calls per message over one full run.
  The owned engines' figure includes one copy of each input message. ZSTD's C
  allocator is not counted.

## Results (aarch64 sandbox, Neoverse-V2)

| Workload | Engine | µs/row (median) | Range | Allocations/message |
| --- | --- | ---: | --- | ---: |
| W1-shaped | legacy-table | 5.58 | 5.57-5.59 | 35 |
| W1-shaped | bounded-numeric | 4.76 | 4.75-4.76 | 22 |
| W1-shaped | **columnar-arrow** | **3.08** | 3.07-3.08 | **0.02** |
| W2-shaped | legacy-table | 6.58 | 6.53-6.59 | 44 |
| W2-shaped | bounded-numeric | 5.47 | 5.47-5.47 | 28 |
| W2-shaped | **columnar-arrow** | **3.45** | 3.44-3.46 | **0.02** |

- The columnar decoder uses **35-37% less CPU per row** than the bounded
  numeric decoder, and 45-48% less than the legacy one.
- Once warm, it makes **no allocation per message**. The remaining 0.02 per
  message are the builders' geometric growth and the batch flushes.
- P0 put libzstd's per-frame floor at about 2.5 µs/row on this machine, and
  the outlook at about 3 µs/row with our per-message overhead removed. The
  measured 3.1-3.5 µs/row is at that outlook. Most of what remains is libzstd.

## Not measured here

- **The Ryzen and the vendor Python package.** P0 measured about 2× the
  sandbox figures on the Ryzen, and Python with Polars at 54.5-63.0 µs/row. The
  ≥ 10× target (criterion c) is checked by replaying the identical P0 captures
  there, after the query engine (PR 4) exists.
- **Before/after regression gating** of an existing data path (the
  [benchmark policy](benchmark-policy.md)): this PR adds a decoder and changes
  no existing path. PR 4, which routes streams through it, follows that
  protocol.

Reproduce:

```text
cargo run --release --locked -p praevis-thetadata-client --example columnar_benchmark -- \
  --rows 20000 --samples 7 --output artifacts/columnar-cpu.json
```
