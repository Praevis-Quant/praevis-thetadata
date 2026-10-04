# L2: stock EOD system requirements

Status: **Defined; implementation and acceptance verification planned**, 2026-10-04.
Owner of every L2 requirement: Praevis-Quant architecture maintainers.
Parents: [L1 outcomes](L1-stock-eod.md). Children: [L3 contracts](L3-stock-eod.md).
All verification described below is planned, not recorded passing evidence.

| ID | L1 parents | System requirement | Acceptance method |
| --- | --- | --- | --- |
| EOD-L2-001 | EOD-L1-001 | The client shall expose a validated typed EOD request and asynchronous query entry point alongside the existing raw protocol helper, without changing the auth library or CLI dependency boundaries. | Compile a library consumer using the public typed API; inspect the dependency graph and existing raw helper. |
| EOD-L2-002 | EOD-L1-001, EOD-L1-005 | The client shall map the symbol, dates, explicit session, and client identity to the observed GetStockHistoryEod wire request, with local input validation before query activity. | Decode fixture requests and assert exact fields; invalid queries produce zero RPC calls. |
| EOD-L2-003 | EOD-L1-002 | NONE and ZSTD response batches shall produce exact Rust values with explicit null, price, and timestamp rules; unsupported or malformed content shall fail explicitly. | Paired compression fixtures and value-boundary tests compare complete tables. |
| EOD-L2-004 | EOD-L1-002, EOD-L1-003 | A result shall preserve ordered columns and rows under one stable schema, including schema-bearing zero-row batches; schema violations shall terminate the result. | Empty, multi-batch, duplicate/ragged, and changed-schema fixtures. |
| EOD-L2-005 | EOD-L1-003, EOD-L1-004 | Queries shall deliver batches on demand with explicit successful completion, cancellation, and terminal failure semantics, without implicit collection, prefetch, or replay. | Controlled stream tests observe demand, state transitions, and failure after delivery. |
| EOD-L2-006 | EOD-L1-003, EOD-L1-005 | Failures shall identify input/configuration, transport, deadline, idle, no-data, remote status, decoding/schema, unsupported response, and resource-limit categories without leaking secrets. | Inject each error class and hostile server text; inspect public Display/Debug and diagnostics. |
| EOD-L2-007 | EOD-L1-004 | Positive connect, whole-query, and between-message idle budgets shall be enforced locally, with explicit behavior during caller backpressure and cancellation. | Deterministic clock/controlled-service tests for each budget and terminal transition. |
| EOD-L2-008 | EOD-L1-004 | The library shall apply finite byte, shape, allocation, ZSTD-window, and concurrent-decoding budgets before expensive expansion, sharing admission across clones. | Boundary and overflow tests plus allocation/peak-memory measurements; encoded size alone is insufficient evidence. |
| EOD-L2-009 | EOD-L1-004, EOD-L1-005 | The market-data path shall reuse the supplied session and configured secure channel without credential discovery, persistence changes, automatic refresh, or revocation. | Fixture request identity, zero-auth-call assertions, and transport configuration tests. |
| EOD-L2-010 | EOD-L1-001, EOD-L1-006 | Rust-native local gRPC and codec fixtures shall exercise the complete selected slice on Windows and Linux without Python, vendor credentials, or network access to ThetaData. | Credential-free CI jobs and a documented library example. |
| EOD-L2-011 | EOD-L1-004, EOD-L1-006 | Decoder and scheduling changes shall be evaluated with reproducible release-mode before/after measurements of throughput, allocation/peak memory, and executor responsiveness. | Retained raw samples, source/fixture hashes, environment context, and a report including regressions; no assumed speedup. |
| EOD-L2-012 | EOD-L1-002, EOD-L1-006 | Selected source items shall trace to decisions, requirements, implementation, and verification; unresolved vendor behavior and intentional deviations shall remain explicit. | Offline inventory/requirement audits and reviewed evidence status; synthetic success is never labeled vendor validation. |

Decisions: [scope](../adr/0007-first-market-data-slice.md),
[requests](../adr/0008-typed-requests-and-presence.md),
[values/schema](../adr/0009-market-data-values-and-schema.md),
[stream/resources](../adr/0010-stream-lifecycle-and-resource-bounds.md),
[verification](../adr/0011-market-data-verification-and-artifact-retention.md),
[traceability](../adr/0012-requirements-and-research-traceability.md).
The [evidence record](stock-eod-evidence.md) identifies acceptance gates.
