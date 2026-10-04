# Roadmap

This is a forward-looking work list, not a changelog. Recommendations become requirements when selected for implementation; their inclusion here does not approve the entire market-data scope.

The next round is selecting the first market-data scope and deriving its requirements from [the detailed Python research](research/thetadata-1.0.12.md#deep-analysis-2026-10-04) and [coverage matrix](research/python-1.0.12-catalog.md). Performance recommendations **#1, #3, #4, #5, and #6** remain candidates for future requirements. The original review remains verbatim as source material; current requirements and implementation evidence belong in [the auth requirements](requirements/L3-auth.md) and [performance documentation](performance/auth.md).

## Next round: turn compatibility research into requirements

Select one small market-data query and its output/error contract before implementation. Use the inventory's source references, signatures, defaults, field-presence behavior, and feature dispositions to write the relevant ADRs and L1/L2/L3 requirements. Keep product L1 and system L2 central; keep cross-component L3 central and add component-owned L3 only when its scope warrants it. A raw generated binding is not acceptance evidence for a supported product feature.

Define request defaults and explicit false/zero presence, validation and date/time formatting, session use, stream/schema/empty-result behavior, deadlines and cancellation, and error handling including partial-stream failure. Capture deterministic protocol fixtures and identify what still needs an authorized live account or vendor confirmation. Decide intentional Python deviations explicitly; do not inherit logging leaks, ambiguous field choices, or dataframe assumptions by accident.

Extend the coverage matrix from each selected inventory item to its ADR, requirements, implementation, and verification without dropping unselected items. Preserve source hashes and earlier research, reconcile new findings, and retain explicit planned/supported/excluded/unresolved dispositions. Close session lifecycle/identity questions before unattended use; establish separate evidence and designs for flat files, corporate actions, and real-time subscriptions. Do not turn unverified findings into requirements or claim live behavior from artifact coverage alone.

## Respond to upstream compatibility changes

For each newer stable release detected by [the scheduled PyPI monitor](upstream-monitor.md), research the versioned source artifacts and assess compatibility before updating the tracked Python baseline. Keep that baseline separate from the Rust crate version; a failed check initiates research and requirements review, not an automatic baseline bump.

## Native Rust development tooling

Evaluate migrating the performance harness and upstream version checker to a Rust `xtask` so routine builds, verification, and benchmarks use the Rust toolchain. Preserve synthetic fixtures, native-store cleanup, before/after build provenance, raw samples, comparison semantics, and regression coverage. Validate the replacement against the existing measurements before retiring the current tooling. Keep Python-specific artifact analysis scoped to research. This is a separate candidate from the runtime startup implementation.

## Original review and recommendations (verbatim)

The review below is preserved exactly, including its table and original workstation links. Its measurements and validation paragraph are supporting evidence for the proposed work, not a completion log. Machine-local citations are historical and may not open on another machine.

The project has a good starting structure: authentication is isolated, HTTP/gRPC clients are reusable, and market data is consumed batch by batch. **The best immediate optimization is CLI startup; the largest scaling concerns are decoded-table memory and CPU work inside async calls.**

I reviewed commit `0caf272`, ran the workspace tests, and performed synthetic measurements. No tracked files changed.

1. **Bound decoded memory before expanding market-data usage.**  
   The limit in [decode.rs](/C:/Users/Joey/Documents/GIT-GitHub/Praevis-Quant/thetadata-rs/crates/thetadata-client/src/decode.rs:7) checks compressed and decompressed protobuf bytes. It does **not** bound the resulting table’s heap usage.

   A synthetic 100,000-row × 8-column null table occupies **1.8 MB on the wire but 28 MB just for Rust row/value storage**—excluding allocator overhead and intermediate decoding buffers. Concurrent streams multiply this cost.

   Recommend row/cell limits enforced before large allocations, a shared memory/concurrency budget, and an explicit ZSTD window limit. Checking row counts only after `DataTable::decode` would be too late to prevent its allocations. [ZSTD decoder controls](https://docs.rs/zstd/0.13.3/zstd/stream/read/struct.Decoder.html#method.window_log_max)

2. **Avoid starting a worker pool for every CLI command.**  
   [main.rs](/C:/Users/Joey/Documents/GIT-GitHub/Praevis-Quant/thetadata-rs/apps/thetadata-cli/src/main.rs:216) starts Tokio before parsing arguments. Consequently, even `status`, `logout`, and help initialize the runtime. Tokio defaults to a worker per available CPU. [Tokio documentation](https://docs.rs/tokio/latest/tokio/attr.main.html)

   On this Windows machine, two runs per configuration, each with 60 measured samples, produced:

   | Median latency | Default workers | One worker |
   |---|---:|---:|
   | Authentication + save | 31.8–32.4 ms | 30.0–31.4 ms |
   | Persisted status | 22.5–23.2 ms | 21.0 ms |

   Recommend synchronous argument parsing and status/logout paths, creating a `current_thread` runtime only for authentication. The experiment supports reducing startup work; it does not measure that proposed implementation directly.

3. **Move substantial decoding work off async executor threads.**  
   [next_batch()](/C:/Users/Joey/Documents/GIT-GitHub/Praevis-Quant/thetadata-rs/crates/thetadata-client/src/lib.rs:151) performs ZSTD decompression, protobuf decoding, timestamp formatting, and validation synchronously after receiving a message. Large batches can delay other tasks sharing that executor thread.

   Benchmark bounded `spawn_blocking` decoding or a dedicated CPU pool. Preserve backpressure and limit concurrent jobs; spawning unlimited decoding tasks could worsen memory pressure. Started blocking tasks also require deliberate cancellation handling. [Tokio guidance](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html)

4. **Keep timestamps numeric until presentation.**  
   [timestamp()](/C:/Users/Joey/Documents/GIT-GitHub/Praevis-Quant/thetadata-rs/crates/thetadata-core/src/lib.rs:65) converts every timestamp into an allocated RFC 3339 string during decoding—even when callers only need filtering or calculations.

   Recommend storing epoch milliseconds plus timezone information and formatting when displaying/exporting. Subsequently benchmark flat row storage or typed columns against `Vec<Vec<Value>>`. These are API changes with potentially meaningful allocation savings; throughput gains remain unmeasured.

5. **Reduce per-batch allocation churn after establishing a decoder benchmark.**  
   [The decompression path](/C:/Users/Joey/Documents/GIT-GitHub/Praevis-Quant/thetadata-rs/crates/thetadata-client/src/decode.rs:21) creates a decoder and grows a fresh output vector for every batch.

   Evaluate reusable decompression state, bounded buffer reuse, and conservative capacity reservation. Retain expansion checks and avoid retaining the largest-ever buffer indefinitely.

6. **Expand performance coverage before setting regression thresholds.**  
   [The benchmark](/C:/Users/Joey/Documents/GIT-GitHub/Praevis-Quant/thetadata-rs/tools/benchmark_auth.py:74) currently measures whole-process auth/status latency. Add separate measurements for decoding throughput, allocations, peak memory, timestamp conversion, and concurrent-stream responsiveness. Record raw samples, CPU/runtime configuration, and Rust version to improve comparisons.

Validation passed: **14 unit tests, one doctest, and the repository audit**. The synthetic auth/persistence benchmarks also completed successfully; results are in [performance-review artifacts](/C:/Users/Joey/Documents/GIT-GitHub/Praevis-Quant/thetadata-rs/artifacts/performance-review). Live-service throughput and decoder speedups were not measured.
