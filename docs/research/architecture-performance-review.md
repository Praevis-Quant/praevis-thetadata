# Architecture and performance review

Reviewed 2026-10-04 against `d48a800` (stock EOD requirements branch).
Evidence: workspace manifests, auth/session/store code, client request/stream/
decoder code, core values, and the retained Python 1.0.12 inventory. This is
source inspection and design analysis; no new throughput measurements or live
queries were performed. Proposed contracts are not implemented guarantees.

## Assessment of the library boundaries

The five-package split is appropriate for the current size. Keep it and improve
the boundaries within it before adding crates. Fowler describes separating
presentation, domain behavior, and data access as logical modules rather than
necessarily separate deployment units. That supports CLI/binding adapters over
reusable Rust APIs here. It does not prescribe a crate for every endpoint or
asset class. [Fowler: Presentation Domain Data Layering](https://martinfowler.com/bliki/PresentationDomainDataLayering.html)

| Package | Current evidence | Assessment and next design |
| --- | --- | --- |
| `thetadata-auth` | Owns credentials, HTTP auth, session types, and native store module. Client imports it for session use; platform keyring dependencies are unconditional on supported OS targets. | Cohesive identity responsibility, but consumers that only reuse a session inherit persistence dependencies. Feature-gate native storage first; preserve the CLI's existing persistence contract. Consider a separate session-only crate only if dependency/build measurements justify another public boundary. |
| `thetadata-proto` | Checked-in descriptor generates messages and gRPC bindings. No core/auth dependency. | Appropriate vendor adapter boundary. Generated types are wire contracts, not the stable enhanced domain API. Splitting messages from tonic transport is a measured dependency candidate, not required now. |
| `thetadata-core` | No network/runtime dependencies. Owns prices, timestamp formatting, generic values/tables. `Price::from_wire` and timezone codes encode vendor rules despite no proto import. | Good transport independence, but presentation work and vendor-specific conversions belong at adapters. Add a numeric batch representation and explicit formatting conversion; retain current public helpers for compatibility. Do not grow an unstructured utility crate. |
| `thetadata-client` | Composes auth/core/proto, channel, request methods, ZSTD/protobuf conversion. Public raw types coexist with higher-level intent. | Suitable orchestration/transport boundary at this size. Separate internal request, transport, codec, stream/budget, and compatibility modules; keep raw and stable typed APIs visibly distinct. A codec crate is warranted only with independent reuse/ownership or measured build benefits. |
| `thetadata-cli` | Owns discovery, runtime construction, native persistence, and rendering; auth-only dependencies. | Correct application boundary. Keep formatting/configuration here; do not force CLI policy into the library or add market-data commands incidentally. |

These crates are technical modules, not automatically DDD bounded contexts.
Fowler's bounded contexts describe explicit model boundaries and relationships;
there is no evidence that stock/options/index endpoints need different models
and independent crates yet. [Fowler: Bounded Context](https://martinfowler.com/bliki/BoundedContext.html)

Ports/adapters are useful at real side effects: transport, credential storage,
CLI, test service, and future Python bindings. Use those seams to test the Rust
behavior independently. Do not add a trait object, allocation, or abstraction
layer to every cell operation just to draw a hexagon.
[Cockburn: Hexagonal Architecture](https://alistair.cockburn.us/hexagonal-architecture/)

## Twelve-Factor applicability

Twelve-Factor is guidance for deployed service applications, not a library
packaging or domain decomposition standard. Apply relevant principles at host
boundaries; this repository cannot certify future applications' compliance.
[The Twelve-Factor App](https://12factor.net/)

| Factor | Application here |
| --- | --- |
| I Codebase | Git workspace and explicit upstream provenance are appropriate. No need for a repository per crate. |
| II Dependencies | Cargo manifests/lockfile and reproducible builds fit. Feature choices should let headless consumers avoid native storage and presentation dependencies. |
| III Config | A service/CLI may read environment configuration and pass explicit Rust config. Libraries must not silently discover credentials/environment. Native desktop storage remains deliberate application policy. [Config](https://12factor.net/config) |
| IV Backing services | Inject endpoint/session/config; local service fixtures exercise the boundary. Do not bury endpoint selection in domain values. |
| V Build/release/run | Preserve checked-in schema, locked builds, and build-time generation; no vendor Python download on the data path. |
| VI Processes | Future hosted services own durable state externally as appropriate. A desktop auth CLI is deliberately stateful across runs via OS storage; do not remove that feature in the name of service statelessness. [Processes](https://12factor.net/processes) |
| VII Port binding | Not applicable to a client library; decide for a future hosted application. |
| VIII Concurrency | Deployment process scaling belongs to the host. Library decode concurrency remains bounded and caller-configured. More runtime workers are not automatically faster. |
| IX Disposability | Existing cheap CLI startup and proposed cancellation/deadlines support this intent. |
| X Dev/prod parity | Windows/Linux fixtures and explicit PROD/STAGE configuration help; synthetic CI is not vendor-service parity. |
| XI Logs | Host applications choose logging sinks. Library diagnostics must be redacted and disabled hot-path events must not format/allocate. [Logs](https://12factor.net/logs) |
| XII Admin processes | Future deployed applications own maintenance jobs. The scheduled upstream monitor is repository tooling, not a library runtime responsibility. |

## Performance findings and order of work

| Priority | Inspected issue | Proposed contract / evidence needed |
| --- | --- | --- |
| First | No decoder-to-consumer stage baseline; network time can obscure local cost. | Measure time to first batch, steady throughput, tail delivery latency, allocated bytes/cell, peak live memory, and competing-task delay; separate transport/decompress/decode/transform/format stages. |
| First | `DataTable::decode` allocates before shape validation. Existing 64 MiB byte ceiling is not a heap bound. | Bounded scan/decode, checked allocation accounting, ZSTD window control, and shared admission; measure the cost of validation and keep it enabled. |
| First | `Value::Timestamp(String)` formats each timestamp before delivery. | Add numeric timestamps preserving instant and source zone to the fast batch API; format only when the caller chooses a presentation adapter. |
| First | Decoding executes synchronously in `next_batch`; big work can delay other executor tasks. | Compare bounded inline work with bounded CPU offload using small/large batches and concurrency. Avoid assuming offload always wins. |
| Next | Protobuf row/cell allocations coexist with `Vec<Vec<Value>>` output; repeated headers and fresh decompression buffers can add work. | Evaluate contiguous batches, shared immutable schema, moved/borrowed data where lifetimes allow, and capped buffer reuse. Choose layout using consumer workloads, not a blanket zero-copy claim. |
| Next | Exact core values are mixed with vendor mappings and formatting. | Keep ingestion numeric and validated; explicit adapters handle vendor mapping and output formats. |
| Later | New filtering could accidentally collect results or invoke Python for every cell. | Optional Rust batch transforms; compile column lookup once per schema; benchmark bypass/no-op/selectivity paths; no per-row Python callback on the native fast path. |

Performance is the leading optimization objective **within correctness,
security, bounded-resource, and observable lifecycle contracts**. Maximize useful
delivery to the consumer, not just a decoder microbenchmark. Without workload
and host evidence, absolute “fastest possible” or a universal latency SLA is
not supportable. Rust abstraction techniques can avoid runtime costs, but a
particular design still requires measurement.
[Rust: loops and iterators](https://doc.rust-lang.org/book/ch13-04-performance.html)

## Enhancements beyond Python

Yes: filtering, projection, batch consumption, exact values, explicit memory
budgets, and multiple output adapters can be independent Rust capabilities.
Distinguish upstream query arguments from local transforms. A local predicate
does not reduce vendor bytes or entitlement cost; server pushdown requires an
observed supported request field with equivalent semantics. Do not invent SQL
or a filter parameter in GetStockHistoryEod.

Start with explicit projection and typed row predicates over validated batches.
Keep schema/order/null/error semantics deterministic and state bounded. Defer
sorting, joins, rolling windows, caches, analytics and Arrow/Polars integration
until their consumer need, memory model, and evidence are selected. Generic
header access is possible now; a fixed EOD business schema remains unverified.

Future PyO3 should be a separate outer adapter exposing selected Rust interfaces,
with batch transfer and deliberate async/ownership/error contracts. Evaluate
interpreter detachment and free-threaded builds when that work is selected;
do not lock in a current PyO3 version or promise GIL behavior now.
[PyO3 parallelism guidance](https://pyo3.rs/main/parallelism)

Decisions: [0013 boundaries](../adr/0013-library-boundaries.md),
[0014 performance](../adr/0014-performance-first-data-path.md),
[0015 enhancement protection](../adr/0015-upstream-and-enhancement-contracts.md),
[0016 transforms](../adr/0016-optional-batch-transforms.md).
See the [origin register](../requirements/origins.md) for explicit classification
of every existing and proposed requirement/ADR, independent of delivery status.
