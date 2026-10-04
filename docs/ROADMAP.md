# Roadmap

This is a forward-looking work list, not a changelog. Recommendations become requirements when selected for implementation; their inclusion here does not approve the entire market-data scope.

The first candidate is recommendation **#2: CLI startup**. Its requirements and evaluation belong in [the auth requirements](requirements/L3-auth.md) and [performance documentation](performance/auth.md). Keep this candidate open until the branch is evaluated and accepted; track completed work outside this roadmap.

## Next round: complete Python compatibility research

Deeply analyze the original bundled Python 1.0.12 distribution and expand [the research Markdown](research/thetadata-1.0.12.md) into the source for future ADRs and L1 requirements, with L2/L3 details where evidence supports them. Preserve and reconcile existing research rather than replacing it with a summary.

Completion requires a source-hashed inventory of every module, public feature/function, signature, default, configuration/environment variable, authentication/session behavior, transport operation, conversion, error path, dependency, and platform assumption. Record internal helpers where they explain observable behavior. Reconcile all 80 public wrappers with all 82 descriptor RPCs and explicitly investigate unmatched items. Preserve source path/symbol references, examples, limitations, and contradictory or unverified vendor documentation.

Use a coverage matrix from each inventory item to detailed research and, as work is selected, its ADR, L1/L2/L3 requirements, implementation, and verification. Every item needs an explicit disposition: planned, supported, intentionally excluded with rationale, or unresolved. Check inventory counts, identifiers, and hashes mechanically; retain gaps openly. This establishes auditable coverage of the inspected distribution, not a claim that undocumented live-service behavior has been proved. Do not silently discard earlier discoveries or turn unverified findings into requirements.

## Monitor upstream compatibility

Run a scheduled GitHub Actions check against PyPI's latest stable `thetadata` release. Fail the scheduled pipeline if it exceeds the Python version tracked by this Rust port. Keep that compatibility baseline explicit and separate from the Rust crate version; a failed check initiates research and requirements review, not an automatic baseline bump. Also support manual dispatch. Record this candidate here until the branch is accepted.

## Licensing decision

Adopt the confirmed Apache-2.0 license for newly authored code/tooling when accepting this branch. Preserve upstream license and attribution material independently.

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
