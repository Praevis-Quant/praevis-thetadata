# Performance-first delivery requirements

Status: Defined/proposed, 2026-10-04; partial experimental implementation,
calibration and full acceptance open. Origin of every PERF requirement: **Rust enhancement**.
Owner: workspace performance maintainers; L3 names component owners.
Decision: [ADR-0014](../adr/0014-performance-first-data-path.md).
Related contracts: [EOD](L3-stock-eod.md), [architecture](architecture.md),
[enhancements](enhancements.md), [origin register](origins.md).

Performance is the primary optimization objective within correctness, security,
resource and lifecycle constraints. These requirements apply from the low-level
decoder to consumer delivery; they do not guarantee vendor latency or a
workload-independent optimum. Verification below states acceptance obligations;
see the [experiment](../performance/eod-numeric.md) and
[results](../performance/eod-numeric-results.md) for current evidence and gaps.

## L1

| ID | Requirement | Planned success measure |
| --- | --- | --- |
| PERF-L1-001 | Consumers shall receive validated data with minimal measured local delay and resource cost, without mandatory formatting or language conversion. | Numeric-path first-batch, tail-latency, throughput, allocation and memory measurements with equivalent output validation. |
| PERF-L1-002 | Maintainers shall treat delivery performance as a versioned acceptance concern alongside compatibility and correctness. | Reproducible before/after evidence and reviewed regression budgets accompany relevant changes and upstream updates. |

## L2

| ID | L1 parents | Requirement | Planned verification |
| --- | --- | --- | --- |
| PERF-L2-001 | PERF-L1-001 | The primary data path shall retain exact native values and numeric timestamps; presentation and compatibility conversion shall be explicit adapters. | Equivalent numeric/legacy results; allocation/formatting counters prove unused adapters are absent. |
| PERF-L2-002 | PERF-L1-001 | Streaming/decode/transform work shall obey finite memory and CPU-admission budgets, amortize setup per schema/batch, and preserve pull backpressure. | Allocation/shape/window/concurrency boundary tests and competing-task benchmarks. |
| PERF-L2-003 | PERF-L1-001, PERF-L1-002 | Rust-native measurements shall separate transport, decompression, wire decoding, validation, transformation and presentation from useful consumer delivery. | Same-fixture stage and end-to-end release reports on Windows/Linux. |
| PERF-L2-004 | PERF-L1-002 | Performance-sensitive changes shall retain baselines and explicit regression decisions; thresholds shall be calibrated to measured host noise and representative workloads. | Reviewable benchmark policy and repeat runs; no unsupported speedup or arbitrary global threshold. |

## L3

| ID | L2 parents | Owner | Requirement | Planned verification |
| --- | --- | --- | --- | --- |
| PERF-L3-001 | PERF-L2-001, PERF-L2-003 | client, workspace | Establish a release baseline before changing decoder/layout/scheduling. Measure both request-to-first-batch and locally buffered-input-to-consumer time, steady rows/cells/bytes per second, p50/p95 batch latency, allocations/bytes per cell, peak live memory and unrelated-task delay. Separate vendor/network latency from controlled local cost. | EOD-L3-026/027 fixtures plus small, large, null/text/timestamp-heavy, concurrent and slow-consumer cases; retain raw samples and measurement boundaries. |
| PERF-L3-002 | PERF-L2-001 | core, client | Add `DataBatch` and `stock_eod_batches` with exact numeric values, explicit nulls, immutable ordered schema, and timestamp epoch milliseconds plus validated source zone. Reject unsupported zones/ranges under the same accepted domain as the Table adapter; no timestamp string or JSON/Python object is required before numeric delivery. | Numeric boundary/DST equivalence fixtures, shared schema tests and formatting/allocation instrumentation. |
| PERF-L3-003 | PERF-L2-001 | core, client | Keep `stock_eod` and its RFC 3339 Table contract as an explicit adapter over the same typed engine. EOD validation/wire/schema/resource/error/termination rules apply to both; only representation/signature differ as recorded in ADR-0014. Unused output adapters shall do no per-row work. | Compare complete rows, exact prices, timestamp instants/offsets and every terminal/error category through both APIs; benchmark bypass versus Table conversion. |
| PERF-L3-004 | PERF-L2-002 | client, core | Validate byte/shape/allocation/window budgets before expensive expansion; account for simultaneously live buffers and cap reuse/retention. Reuse validated immutable schema and compiled column indices where applicable, move ownership when sound, and avoid full-result collection. Layout choice must report consumer-access and peak-memory tradeoffs. | EOD-L3-020 through EOD-L3-023; malicious/tiny-wire fixtures and allocation profiles compare candidate layouts with identical semantic results. |
| PERF-L3-005 | PERF-L2-002 | client | Choose inline versus bounded CPU offload using measured small/large/concurrent workloads. Shared admission shall bound queued and active work across clones; cancellation retains reservations until started work releases them. No per-row task spawn, unbounded queue or automatic prefetch. | Executor interference, queue/budget occupancy, deadline/cancellation and backpressure tests; report scheduling overhead separately. |
| PERF-L3-006 | PERF-L2-001, PERF-L2-002 | client, core | The numeric path shall do no implicit credential discovery/auth/storage, output serialization or Python callbacks. Disabled diagnostics shall not format strings or allocate per cell; optional transforms shall have a measured bypass path. Keep required validation/error observability enabled. | Instrumented fast-path tests, disabled-log/no-transform allocation comparisons and security/error fixtures. |
| PERF-L3-007 | PERF-L2-003, PERF-L2-004 | workspace | Preserve EOD-L3-026/027 benchmark provenance, equivalent semantic work, warmups, alternating order and raw samples. Measure zero/partial/full filter selectivity and projection widths separately, including conversion cost and caller-retained output. Do not present output reduction alone as faster decoding. | Reproducible Rust-native report with all regressions, host/runtime details, source/fixture/binary hashes and metric definitions. |
| PERF-L3-008 | PERF-L2-004 | workspace | Before performance acceptance, record the designated workloads/hardware, repeated-run variability and per-metric regression tolerances with rationale. Controlled benchmark regressions block acceptance until explained and explicitly resolved; noisy shared-runner timings remain informational until calibrated. Functional and resource-boundary failures always fail CI. | Checked-in benchmark policy and baseline report; relevant PR/upstream impact records link before/after evidence and any deliberate tradeoff decision. |

Open acceptance gates: measured defaults/offload under EOD-L3-023, concrete
DataBatch layout, baseline noise/tolerances, and passing implementation tests.
Do not invent numeric targets before those measurements. The numeric interface
is selected as an additive design now, not claimed implemented or zero-copy.
