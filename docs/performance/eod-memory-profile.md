# EOD client process-memory profile (alpha.3 A7, deferred item 1, 2026-11-03)

Decision D-C deferred measuring the memory that the client's own accounting
does not cover: transport (HTTP/2) buffers, libzstd's C allocations,
allocator retention and process RSS. Decision D-H chose OS counters plus
libzstd's own size reporting, with no new dependency. The harness is
[`examples/eod_memory.rs`](../../crates/praevis-thetadata-client/examples/eod_memory.rs).

## Method

- **Isolation:** each configuration runs a fresh synthetic fixture server in a
  child process and a fresh client in another, so only client memory is
  counted and nothing is carried over from an earlier configuration.
- **Baseline:** a connected, idle client, with no warm-up query, because memory
  freed by a warm-up would stay in the allocator and hide growth. Values are
  peak growth over that baseline, during the configuration's concurrent
  queries; each query consumes and drops every batch.
- **Counters:**
  - **Rust heap:** a counting global allocator that covers every Rust
    allocation in the process, including tonic, h2 and hyper buffers.
  - **OS:** on Linux, `VmHWM` reset through `/proc/self/clear_refs`, plus
    `RssAnon`. On Windows, `K32GetProcessMemoryInfo` working set and private
    bytes, sampled every 1 ms (the Windows peak cannot be reset). It is
    declared directly in the example; no crate is added.
  - **libzstd:** `ZSTD_sizeof_DCtx` after decoding a frame that uses the full
    window, next to the client's fixed accounting of window + 16 MiB.
- **Workloads:** the 10,000-row mixed, nulls, prices and timestamps fixtures
  (8 messages per query), and the calibration's largest accepted text batch,
  `text-24mib` (2 messages per query). Each runs NONE and ZSTD, through the
  numeric and Table engines, with 1 or 16 concurrent queries, under the
  default `EodPolicy` (2 jobs, 16 streams, 512 MiB shared budget). The
  workloads are followed by stream-count, message-size and
  `concurrent_streams` sweeps.
- **Hosts:** GitHub `windows-latest` and `ubuntu-latest` (x86_64, run
  41633224260 at `903ca90`), and the aarch64 development sandbox. Raw reports:
  [Windows](eod-memory/windows-x86_64.md), [Linux x86_64](eod-memory/linux-x86_64.md),
  [Linux aarch64](eod-memory/linux-aarch64.md).

## Results

**libzstd** (identical on all three hosts): the decompression context is
0.10, 0.22, 0.59, 1.47 and **8.47 MiB** at window logs 10, 15, 17, 20 and
**23** (the default maximum). The client accounts 16.00-**24.00 MiB**, so the
fixed reservation covers libzstd with room to spare.

**Standard 10,000-row workloads, up to 16 concurrent queries** (largest
values over all modes and engines, MiB):

| Host | Rust heap peak | OS resident peak | Resident minus Rust heap |
| --- | ---: | ---: | ---: |
| Windows x86_64 | 62.8 | 63.3 | 4.2 |
| Linux x86_64 | 63.1 | 81.7 | 18.6 |
| Linux aarch64 | 63.5 | 75.1 | 11.6 |

Non-Rust memory (libzstd, allocator metadata and retention) is small. The
compressed `text-24mib` workload peaks below 99 MiB at 16 queries on every host.

**Large uncompressed messages.** These dominate, through transport buffering
that the client does not account for:

| Measure (`text-24mib`, NONE, numeric) | Windows x86_64 | Linux x86_64 | Linux aarch64 |
| --- | ---: | ---: | ---: |
| 1 query: Rust heap / resident | 74.0 / 73.8 | 73.5 / 97.9 | 73.4 / 96.0 |
| 16 queries: Rust heap / resident | 508.4 / 497.0 | 504.8 / 600.1 | 499.2 / 563.9 |
| Largest resident growth, main grid (any mode/engine) | 487.9 | 595.4 | 573.5 |
| Largest resident growth in any table (including sweeps) | 502.9 | 600.4 | 618.0 |

- **Stream count:** with more concurrent queries the heap grows by about one
  message each (Rust heap 74, 148, 196, 298, 508 MiB at 1, 2, 4, 8, 16 queries
  on Windows).
- **Message size:** at 16 queries, the Rust heap per query tracks the message
  size: about 11, 21 and 31 MiB for 8, 16 and 24 MiB messages on every host.
- **Not retention between messages:** a local check with **one** message per
  query still peaked at 433 MiB. Each open stream holds about one full encoded
  message in flight, even though only 2 decode jobs run at once. The exact
  layer (tonic decode buffer or h2 receive path) is **not yet identified**.
- **`concurrent_streams` bounds it:** 16 queries under the policy cap

  | `concurrent_streams` | Windows heap / resident (time) | Linux x86_64 heap / resident (time) |
  | ---: | --- | --- |
  | 16 | 508 / 503 MiB (1.44 s) | 502 / 587 MiB (1.64 s) |
  | 4 | 200 / 198 MiB (1.64 s) | 199 / 273 MiB (1.75 s) |
  | 2 | 148 / 147 MiB (1.60 s) | 147 / 269 MiB (1.86 s) |
  | 1 | 74 / 75 MiB (1.73 s) | 74 / 148 MiB (1.97 s) |

  Elapsed time changes little in this workload, because decoding is already
  limited to 2 jobs. Other workloads need their own benchmark before any
  default changes.

## Conclusions

1. **libzstd is covered.** The fixed 16 MiB + window reservation exceeds the
   measured context (at most 8.47 MiB).
2. **The decode path stays within its accounting.** Standard workloads grow by
   at most about 82 MiB of resident memory at 16 concurrent queries.
3. **Transport buffering is outside the shared budget.** Peak process memory
   is roughly a base plus **concurrent open streams × largest encoded
   message**. With the defaults (16 streams, a 64 MiB encoded limit), that can
   exceed 1 GiB, above the 512 MiB shared budget. Measured: up to 618 MiB
   resident at 16 × 24 MiB.
4. **Wording:** D-C's rule stands. Only the client's own accounted allocations
   may be called bounded. The client README now gives the measured transport
   behavior and the `concurrent_streams` mitigation.

Whether to (a) document only, (b) lower the default `concurrent_streams` after
benchmarking, or (c) identify the buffering layer and reserve it in admission
(so that total memory becomes bounded by the budget) is
[proposed deferred item 8](../releases.md#deferred-from-alpha2), pending a
maintainer decision.

To reproduce: `cargo run --release -p praevis-thetadata-client --example eod_memory -- run`,
or dispatch **Synthetic EOD performance** with `memory: true`.
