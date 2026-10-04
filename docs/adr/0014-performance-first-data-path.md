# ADR-0014: numeric batches and measured consumer delivery

Status: Proposed. Date: 2026-10-04. Owner: core/client maintainers.
Origin: **Rust enhancement**, not required for Python dataframe parity.
Requirements: [PERF L1/L2/L3](../requirements/performance.md).

## Context

Performance is the user's leading design objective. The current path allocates
a protobuf object tree, converts it to nested row vectors, and eagerly formats
timestamps on an async executor thread. Byte limits do not bound decoded heap.
These are inspected costs, not new benchmark results.

## Proposed decision

Optimize end-to-end useful delivery within correctness, exact-value, security,
memory and lifecycle constraints. Establish a release baseline before decoder
changes. Measure first-batch latency, steady delivery, tail latency, allocations,
peak live memory and executor interference for realistic and adversarial shapes.
Use Rust-native harnesses and retained before/after evidence on both platforms.
Choose layouts, concurrency/offload, capacities and regression thresholds from
those results; no invented universal speedup, zero-allocation, or latency promise.

Add a protocol-independent `DataBatch` representation with exact price values,
numeric timestamps (epoch milliseconds plus validated source-zone identity),
nulls and immutable schema. The proposed typed fast entry is
`ThetaClient::stock_eod_batches`; its pull stream yields DataBatch. Reuse one
validated request/transport/lifecycle implementation, not two EOD engines.
Keep data numeric through decode, validation and optional transforms. Perform
date/time rendering, text/JSON/dataframe and future Python conversion only at
an explicitly chosen adapter. No automatic store/auth/log formatting in that path.

**Reconciliation with ADR-0009 and the EOD requirements:** the planned
`stock_eod` Table interface and existing raw helpers retain their documented
RFC 3339 output. EOD-L3-009 describes that compatibility representation, not
the new numeric batch representation. Implement the typed Table interface as
an explicit adapter over the numeric batch engine. EOD-L3-001/013/025 describe
the Table-facing signatures; all EOD validation, wire, schema, null, terminal,
error and resource semantics also apply to the numeric path. PERF-L3-002/003
define the additive representation/adapter contract. No existing requirement
is silently reinterpreted and no released API is removed. Legacy raw helpers
do not automatically acquire typed guarantees.

Amortize schema/column lookup; move ownership or borrow only with sound lifetimes.
Use finite allocation/work budgets before materialization, capped buffer reuse,
and bounded decode scheduling. Preserve pull backpressure. Validate malformed
or unselected fields as required even when projection/filtering is requested;
optimization must not hide a required error. Include optional-adapter bypass
in benchmarks. No Python dependency or callback is necessary for native delivery.

## Alternatives and consequences

Keeping only eager Table output makes avoidable formatting mandatory. Replacing
Table in place breaks the documented contract. An additive numeric interface
with explicit compatibility conversion costs a small API surface but isolates
presentation work. Columnar/Arrow, flat row-major or borrowed layouts remain
measured alternatives; do not choose a layout solely to anticipate Python.
Unbounded offload/prefetch or removing validation may improve one microbenchmark
while violating memory/latency guarantees and is rejected.

An [experimental implementation](../performance/eod-numeric.md) and
[performance results](../performance/eod-numeric-results.md) now provide partial
evidence. The measurement and resource-policy gate in EOD-L3-023 remains open;
this ADR adds first-batch and
adapter-cost criteria and selects the numeric-path direction before coding.
See the [architecture review](../research/architecture-performance-review.md).
