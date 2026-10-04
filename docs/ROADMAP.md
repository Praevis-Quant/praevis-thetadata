# Roadmap

This is a forward-looking work list, not a changelog. Recommendations become requirements when selected for implementation; their inclusion here does not approve the entire market-data scope.

The next round is implementing the stock EOD slice's [defined L1/L2/L3 requirements](requirements/L1-stock-eod.md) and [performance-first contracts](requirements/performance.md) from [proposed ADRs 0007-0016](adr/README.md), [the detailed Python research](research/thetadata-1.0.12.md#deep-analysis-2026-10-04), and [coverage matrix](research/python-1.0.12-catalog.md). Start with the Rust-native fixtures and decoder baseline needed to close the [resource-policy measurement gate](requirements/stock-eod-evidence.md). Recommendations **#1, #3, and #6** inform resource/verification contracts; **#4** informs additive numeric delivery, and **#5** requires measured buffer-reuse evaluation. The original review remains verbatim as source material; implementation evidence belongs in the requirements and performance documentation.

## Next round: implement and verify stock EOD

Implement the typed one-symbol EOD request and pull stream against [the L3 contracts](requirements/L3-stock-eod.md). Extend the [Rust-native mock and retained decoder baseline](performance/eod-baseline.md) to the typed/numeric path. Use the [initial measurements](performance/eod-results.md) to design pre-allocation accounting and numeric timestamp/layout experiments, then choose finite resource defaults and bounded scheduling from equivalent before/after evidence. Report regressions and fixture/instrumentation effects as well as improvements. A raw generated binding is not acceptance evidence for a supported product feature.

Verify validation/wire mapping, exact values, session use, stream/schema/empty-result behavior, deadlines/cancellation, and partial-stream errors on Windows and Linux. Link implementation and passing evidence to each requirement before accepting the slice. Retain open service-column, date-inclusivity, ordering/adjustment, entitlement, and token-lifecycle questions until supported by authorized live evidence or vendor confirmation. Future optional-field endpoints need explicit false/zero-presence contracts when selected; they are not part of the three-field EOD query.

Extend the coverage matrix from each selected inventory item to its ADR, requirements, implementation, and verification without dropping unselected items. Preserve source hashes and earlier research, reconcile new findings, and retain explicit planned/supported/excluded/unresolved dispositions. Close session lifecycle/identity questions before unattended use; establish separate evidence and designs for flat files, corporate actions, and real-time subscriptions. Do not turn unverified findings into requirements or claim live behavior from artifact coverage alone.

## Performance-first library architecture and enhancements

Implement [ADRs 0013-0016](adr/README.md) and the [ARCH](requirements/architecture.md),
[PERF](requirements/performance.md), and [EXT](requirements/enhancements.md)
contracts alongside the EOD slice. Performance is the leading optimization
objective within correctness, security and bounded-resource/lifecycle contracts.
Build on the retained Rust-native decoder-to-consumer baseline to select
finite limits, numeric DataBatch layout and bounded scheduling. Keep formatted
Table output as an explicit compatibility adapter. Evaluate capped buffer/schema
reuse from original recommendations #4/#5 under these measurement gates.

Keep the current crate structure while separating internal responsibilities;
make native storage optional for library consumers and explicitly enabled by
the CLI. Use the [architecture review](research/architecture-performance-review.md)
to assess dependency costs before extracting further crates. Define optional
local filtering/projection after the unfiltered numeric path is verified;
preserve source validation and lifecycle semantics and benchmark adapter bypass.
Explore bounded aggregation/windowing, export adapters and caching only with
separate consumer requirements, state budgets and provenance/invalidation rules.

## Future Python interface through PyO3

Create a separate PyO3 binding project/package after the Rust contracts and
performance baseline stabilize. Expose the selected auth, query, value/batch and
transform interfaces through reusable Rust APIs; core/auth/client must not depend
on Python. Design batch ownership/lifetimes, async runtime integration,
cancellation/errors, interpreter detachment/free-threading support, packaging
and supported Python/platform versions when this scope is selected. Benchmark
Python transfer separately from Rust delivery; avoid default per-row Python
callbacks and unnecessary object conversion. Evaluate Arrow/buffer interchange
as options with explicit copy/lifetime evidence, not an assumed zero-copy promise.
Add binding ADRs/requirements and enhancement-origin records before implementation.

## Respond to upstream compatibility changes

For each newer stable release detected by [the scheduled PyPI monitor](upstream-monitor.md), research the versioned source artifacts and assess compatibility before updating the tracked Python baseline. Keep that baseline separate from the Rust crate version; a failed check initiates research and requirements review, not an automatic baseline bump.

Use the [upgrade impact template](upstream-update-template.md) and exhaustive
[origin register](requirements/origins.md). Reconcile upstream contracts and
Rust enhancements independently; preserve exact values, native-store/lifecycle
policy, numeric delivery, filtering and performance/bounds through adapter
changes. Run separate compatibility/enhancement checks and relevant before/after
measurements. A conflicting upstream change needs an explicit migration ADR,
not silent removal or reinterpretation of local functionality.

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
