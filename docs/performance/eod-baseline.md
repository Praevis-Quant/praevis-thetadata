# Synthetic stock EOD decoder and delivery baseline

This document records the original before-change baseline. The subsequent
[numeric experiment](eod-numeric.md) extends the harness to three engines;
[its results](eod-numeric-results.md) use fresh common-harness measurements.
Statements about pending implementation below describe the original baseline.
The [acceptance follow-up](eod-acceptance-results.md) uses v3 reports with an
explicit slow-consumer delay; its guarded comparisons reject older formats.

The Rust-native fixture and benchmark exercise the current raw EOD helper and
unchanged Table decoder. They establish a baseline before implementing numeric
batches, allocation limits or decode offload. All data is synthetic; the
`synthetic_*` headers are not a captured or promised vendor EOD schema.
No ThetaData account, Python runtime, native credential-store access, or live
endpoint is needed. Future authorized live tests remain separate, optional
service-contract evidence; contributors and CI use this mock setup by default.

## Commands

Run from the repository root with Git and Rust available:

```text
cargo test --locked -p praevis-thetadata-client --test eod_fixture
cargo test --locked -p praevis-thetadata-client --example eod_benchmark
cargo build --release --locked -p praevis-thetadata-client --example eod_benchmark
cargo run --release --locked -p praevis-thetadata-client --example eod_benchmark -- run --output artifacts/eod/baseline-1.json
cargo run --release --locked -p praevis-thetadata-client --example eod_benchmark -- run --output artifacts/eod/baseline-2.json
cargo run --release --locked -p praevis-thetadata-client --example eod_benchmark -- compare --baseline artifacts/eod/baseline-1.json --candidate artifacts/eod/baseline-2.json --output artifacts/eod/repeatability.json
```

Default: three warmups, twelve measured samples, 10,000 rows for large shapes,
four concurrent streams, three batches per stream, and the existing 64 MiB
encoded/decompressed limit. Small mixed cases have 16 rows. All shapes have
eight columns. Use `--streams 1` for sequential delivery, or `--rows 100000`
for larger stress. Arguments are bounded; debug runs and overwriting an existing
report are rejected. CI uses 64 rows/two samples as a correctness smoke test,
not a calibrated performance gate. A separate
[manual workflow](../../.github/workflows/eod-performance.yml) records two
default-size runs and uploads raw JSON for 30 days on Windows/Ubuntu.

## Fixture design and verification

[Shared support](../../crates/praevis-thetadata-client/tests/support/mod.rs) builds
deterministic protobuf and NONE/ZSTD messages with mixed, null-heavy,
timestamp-heavy and price-heavy rows. The expected Table is independently
constructed, including fixed timestamp strings, exact negative price scaling,
int64 boundaries, booleans and absent/explicit null. Every measured output is
checked cell-for-cell. Compression/generation is outside measured decoding.

The loopback HTTP fixture supplies a synthetic session through the public auth
API. It shuts down before measurement. A loopback gRPC service runs on a
separate OS thread with a current-thread Tokio runtime; consumers use another
current-thread runtime, so server execution does not masquerade as client
executor blocking. Accepted mock sockets explicitly enable TCP_NODELAY to avoid
delayed-ACK/Nagle artifacts in small-message delivery. The service trait stubs
are generated from the checked-in descriptor; only GetStockHistoryEod is
implemented. The server checks no real
entitlement and does not emulate undocumented vendor business semantics.

[Integration tests](../../crates/praevis-thetadata-client/tests/eod_fixture.rs) exercise
wire identity/parameters, exact NONE/ZSTD equivalence, multiple batches,
successful empty/schema-empty results, NOT_FOUND, permission failure after
partial delivery, corrupt/unsupported/shape errors, idle timeout and insecure
transport opt-in. These validate the existing raw helper, not the forthcoming
typed API, fused lifecycle or whole-query cancellation contract.

## Measurement boundaries

| Metric | Includes | Excludes / limitations |
| --- | --- | --- |
| `decode_ns` | Actual production decoder source: decompress, protobuf allocation, conversion, eager timestamp formatting, shape validation and internal buffer cleanup | Input cloning, fixture generation/compression, output equality and output destruction; reports total decode, not invented separate stage timings |
| `first_batch_ns` | Earliest batch delivered across the configured streams, starting before task dispatch/RPC initiation | Connection/auth setup; this is the group's first result, not every stream's first latency |
| `batch_wait_ns` | Each `next_batch()` wait through decoded return | Query setup, consumer equality/drop and EOF calls |
| `delivery_ns` | All streams through clean EOF, including loopback service cloning/encoding, transport, decode, exact-value consumer checks and destruction | Initial connection/auth/fixture construction; includes fixture and consumer cost, not pure vendor/network or decoder throughput |
| `executor_lateness_ns` | Maximum lateness of a repeating 1 ms sleep on the consumer runtime per sample | Includes timer granularity, OS scheduling, decode and equality-consumer work; not a pure decoder stall measurement or real-time SLA |
| Allocation calls / requested bytes | Separate decoder probe including owned input clone; allocations and reallocations on the calling Rust thread | Excludes C ZSTD allocation, other threads, allocator bookkeeping and OS/transport buffers; realloc counts full requested replacement size |
| Peak live bytes / retained live bytes | Maximum simultaneously requested Rust bytes during the probe; retained bytes while returned Table remains owned | Not process RSS or a complete memory bound. Reallocation's allocator-internal transient cost is not observed. |

The example installs a thread-local counting allocator. Counters are disabled
during timing, but the inactive probe check remains on allocation/deallocation.
This overhead is identical across comparable binaries, not a claim of zero
instrumentation overhead. Allocation probes run separately, before starting
that case's server. Returned Tables and fixtures are released between cases.
No normal library consumer installs this allocator.

Reports retain every raw timing, nearest-rank p50/p95, rows/cells/bytes rates,
sample counts, OS/CPU/host, logical CPU availability, runtime/concurrency,
embedded compiler identity, executable SHA-256, embedded decoder/core/client/
fixture/harness/lockfile hashes, descriptor hash, and runtime Git revision/dirty
state. Component fingerprints refer to compiled source; Git identity describes
the checkout at invocation, so always build and run from the matching checkout.
Build from a clean commit for retained baseline evidence. New source modules
must be included in provenance when introduced. Reports contain machine identity
but no account credentials; review host labels before publishing raw artifacts.

## Before/after comparisons

Preserve two clean checkouts and their release binaries with isolated target
directories, the same toolchain, build flags and fixture/harness code. Stop
competing builds/workloads before measurement. Run baseline A then candidate B,
then B then A, using new output filenames each time; repeat pairs on the same
host. Use the native Rust `compare` command for each matched pair. Preserve raw
reports and executables under ignored artifacts with hashes and a checked-in
results summary. On WSL, build/run the Linux binary on its native filesystem;
record that environment separately from Windows and native Linux hosts.

The comparator rejects different hosts, toolchains, lockfiles, descriptors,
harnesses, fixtures, workloads and incomplete samples. A dependency/harness
update requires rebuilding both versions under one deliberately reconciled
experiment, not suppressing mismatch checks. Median ratios are descriptive:
report regressions, both pair orders and variability. There is no calibrated
latency regression threshold yet. This task captures the **before** baseline;
it makes no optimization/speedup claim.

The [initial Windows/WSL results](eod-results.md) record two clean-commit
repetitions, measured limitations, retained report/binary hashes, and findings.

The baseline is the existing Table API. Comparing a future numeric API requires
a shared adapter-aware harness and fresh measurements of both paths, with
conversion costs reported separately. Do not label different semantic work as
a decoder speedup. Fine-grained stage instrumentation, allocator-independent
RSS/C-memory profiling, slow-consumer sweeps, layout/offload experiments and
calibrated budgets remain subsequent work.

## Requirement evidence boundary

This is partial foundation evidence for EOD-L3-004/006 through EOD-L3-012,
EOD-L3-024/026/027 and PERF-L3-001/007/008. The raw fixture does not satisfy all
typed-input, cross-batch schema, secret-safe error, fused/cancellation,
whole-query deadline or pre-allocation budget requirements. PERF-L3-002/003's
numeric interface is not implemented. All corresponding ADRs remain proposed;
the source feature disposition remains planned. Use
[the EOD evidence record](../requirements/stock-eod-evidence.md) for remaining gates.
