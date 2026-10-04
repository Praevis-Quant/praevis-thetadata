# Stock EOD evidence, deviations, and acceptance gates

Date: 2026-10-04. Owner: Praevis-Quant maintainers. Status: requirements defined;
runtime implementation and acceptance verification planned. Canonical definitions:
[L1](L1-stock-eod.md), [L2](L2-stock-eod.md), [L3](L3-stock-eod.md).

## Source-to-requirement trace

The immutable [Python 1.0.12 inventory](../research/python-1.0.12-inventory.json)
contains these four selected items. Their generated catalogue entries point
back to the `stock-eod` feature disposition. Shared conversion, transport,
and schema evidence remains separately inventoried; selecting this slice does
not select all other wrappers or RPCs.

| Source evidence | Decision | Requirements |
| --- | --- | --- |
| [ThetaClient.stock_history_eod](../research/python-1.0.12-catalog.md#item-f0536a100eb7dbcc) | [0007 scope](../adr/0007-first-market-data-slice.md), [0008 requests](../adr/0008-typed-requests-and-presence.md) | EOD-L1-001; EOD-L2-001, EOD-L2-002; EOD-L3-001, EOD-L3-002, EOD-L3-003 |
| [StockHistoryEodRequestQuery](../research/python-1.0.12-catalog.md#item-5d5e92001216e097) | [0008 requests](../adr/0008-typed-requests-and-presence.md) | EOD-L2-002; EOD-L3-003, EOD-L3-004 |
| [StockHistoryEodRequest](../research/python-1.0.12-catalog.md#item-a63a3eacf9637c00) | [0008 requests](../adr/0008-typed-requests-and-presence.md) | EOD-L2-002, EOD-L2-009; EOD-L3-004, EOD-L3-005 |
| [GetStockHistoryEod](../research/python-1.0.12-catalog.md#item-92352009630c5a88) | [0007 scope](../adr/0007-first-market-data-slice.md), [0010 streams](../adr/0010-stream-lifecycle-and-resource-bounds.md) | EOD-L1-003; EOD-L2-005, EOD-L2-007; EOD-L3-012, EOD-L3-013, EOD-L3-018, EOD-L3-019 |
| [Conversion/output research](../research/thetadata-1.0.12.md#conversion-and-output) | [0009 values/schema](../adr/0009-market-data-values-and-schema.md) | EOD-L1-002; EOD-L2-003, EOD-L2-004; EOD-L3-006 through EOD-L3-012 |
| [Transport/error research](../research/thetadata-1.0.12.md#transport-and-errors) | [0010 streams/resources](../adr/0010-stream-lifecycle-and-resource-bounds.md) | EOD-L1-004, EOD-L1-005; EOD-L2-006 through EOD-L2-009; EOD-L3-014 through EOD-L3-023 |
| [Artifact research](../research/thetadata-1.0.12.md#artifact-and-packaging), [retention decision](../adr/0011-market-data-verification-and-artifact-retention.md) | [0011 verification](../adr/0011-market-data-verification-and-artifact-retention.md), [0012 traceability](../adr/0012-requirements-and-research-traceability.md) | EOD-L1-006; EOD-L2-010 through EOD-L2-012; EOD-L3-024 through EOD-L3-029 |

## Deliberate project policies

| Area | Observed Python behavior | Selected Rust contract |
| --- | --- | --- |
| Request validation | Wrapper forwards strings and formats dates without the selected local validation. | One symbol, no lists/wildcards, explicit valid ordered dates; preserve accepted spelling. Years 0001-9999 are a serialization policy, not a vendor history guarantee. |
| Output collection | Fully accumulates a dataframe. | Pull batches; partial output is usable but never called complete after failure. |
| Prices | Floating-point conversion, NaN for scale zero; invalid negative indexing can misbehave. | Exact mantissa/exponent, null for scale zero, reject unsupported scales. |
| Timestamps | Unknown zones fall back to UTC. | Reject unknown zones/ranges; keep the existing eager RFC 3339 API for this slice. |
| Table shape | Conversion can merge duplicate headers, ignore extra cells, fail short rows, or lose empty headers. | Reject duplicate/ragged/changing schemas, retain schema-bearing empty batches. Skip wholly schemaless empty batches. |
| Diagnostics/session | Python can log tokens and builds parameter diagnostics. | Explicit supplied session; no raw status/payload logging or hidden authentication. |
| Protobuf null | Explicit null and absent/unknown-only value map to missing data. | Preserve that collapse as documented `Value::Null`; do not claim unknown future value types were interpreted. |

## Acceptance gates and retained uncertainties

| Gate / unknown | Required next evidence or decision | Effect on acceptance |
| --- | --- | --- |
| Resource defaults and decoding strategy | EOD-L3-023: first run EOD-L3-026/027 on the existing decoder. Select finite row/cell/text/accounted-allocation/ZSTD-window/shared-job defaults from observed costs on both platforms; document worst-case accounting, headroom, and the reason for synchronous or bounded offload. Then run boundary/rejection and after-change measurements. | Blocks accepting the implemented slice; no numeric defaults or executor-responsiveness guarantees are claimed today. Existing 64 MiB encoded/decompressed setting is only a baseline candidate, not a heap limit. |
| Default timeouts | Retain existing 10/300/60-second configuration values but implement and test the distinct local timing semantics in EOD-L3-018/019. | Values are project policy, not vendor guarantees; local enforcement remains pending. |
| Output columns and value meanings | Versioned vendor specification or authorized sanitized capture with entitlement context. | Preserve generic wire headers/values now; do not advertise a fixed OHLC or adjusted-price schema. |
| Date inclusivity, market calendar, available history | Vendor confirmation or controlled live queries at boundary dates. | Forward exact dates; no client-side trimming or inclusivity guarantee. |
| Service row ordering, duplicates, corrections/adjustments | Vendor evidence across controlled queries. | Preserve observed order/duplicates; no sorted/unique/adjusted-history promise. |
| Entitlements, environments, token expiry/revocation | Authorized live evidence or current vendor contract. | Local fixtures do not prove access or lifecycle; no automatic refresh/retry. |
| Synthetic acceptance versus live validation | Link passing Rust tests/benchmarks to each requirement; record any authorized live checks separately under EOD-L3-029. | Synthetic acceptance can establish the library contract while service semantics remain unverified; explicitly label the scope of every support claim. |

No source artifact needs to be reinstalled for these requirements. The
[recovery procedure](../research/upstream-source.md#recovering-the-removed-local-artifacts)
remains available for full regeneration. Preserve all 531 inventory items.

Verification status vocabulary: **planned** means required but not demonstrated;
**inspection** identifies reviewed source/artifact evidence; **passed synthetic**
requires a linked test/run/revision; **live verified** additionally requires
authorized vendor evidence and its scope. All new runtime acceptance checks
are currently planned. Documentation audits verify links and structure only.

Performance-first extension: [ADR-0014](../adr/0014-performance-first-data-path.md)
adds numeric batches plus an explicit adapter preserving EOD-L3-009's Table
format. New [PERF requirements](performance.md) add first-batch and adapter-cost
measurements to the resource-policy gate. This resolves the direction toward
lazy presentation without claiming implementation or choosing an unmeasured
layout. [Origin classification](origins.md) protects both inherited wire behavior
and deliberate Rust semantics during later upstream updates.

## Initial raw-client fixture evidence (2026-10-04)

The [Rust mock and benchmark](../performance/eod-baseline.md) now exercise the
existing raw EOD helper and unchanged Table decoder without a vendor account.
The linked integration tests cover exact NONE/ZSTD values, request mapping,
multiple/empty batches, partial remote failure, malformed responses and idle
timeout. This is partial foundation evidence, not acceptance of the typed EOD
slice. Typed validation, fused/cancelled lifecycle, cross-batch schema checking,
safe error wrapping and pre-allocation resource enforcement remain planned.
The benchmark establishes before-change measurements; numeric layout, offload,
complete memory accounting and calibrated limits still require implementation
and equivalent after-change evidence. Live checks remain optional and separate.

## Typed numeric experiment (2026-10-04)

The [bounded numeric experiment](../performance/eod-numeric.md) supersedes the
initial implementation-status statements above, without accepting the complete
slice. Canonical requirements and origin classifications are unchanged.

| Contracts | Implementation / verification | Current evidence and gaps |
| --- | --- | --- |
| EOD-L3-001 through EOD-L3-005 | `client/src/eod.rs`; `tests/eod_typed.rs` | Typed calendar/symbol validation, exact request mapping, supplied session and explicit transport configuration; full invalid-configuration/no-side-effect matrix remains open |
| EOD-L3-006 through EOD-L3-011; PERF-L3-002/003/004 | `client/src/bounded.rs`, `core/src/batch.rs`; bounded/core tests and typed fixture | NONE/ZSTD equivalence, flat exact values, numeric timestamp/DST adapter, schema identity/change and malformed-input tests; exhaustive adversarial protobuf corpus remains open |
| EOD-L3-012 through EOD-L3-019 | `client/src/eod.rs`; typed fixture | Partial NOT_FOUND, safe remote codes, empty schema, fusion, explicit/pending-future cancellation, idle and caller-pause deadline tests; deterministic header/admission/running-worker deadline/cancellation matrix remains open |
| EOD-L3-020 through EOD-L3-023; PERF-L3-005 | `client/src/bounded.rs`, `client/src/eod.rs` | Actual expansion/count/text/accounted-storage checks, ZSTD window setting and shared admission implemented; outer envelope allocations, worst-case accounting and active-worker saturation need further audit before acceptance |
| EOD-L3-024 through EOD-L3-027; PERF-L3-007/008 | Typed/raw fixtures, compiled API example and three-engine Rust harness | Credential-free checks and comparable measurement method implemented; policy remains provisional and performance thresholds uncalibrated |
| EOD-L3-028/029 | Origin register, feature ledger, this evidence record | Existing offline coverage checks retained; no claim of live validation |

Paths in this table are relative to `crates/thetadata-client` and
`crates/thetadata-core` as indicated. Tests establish the listed cases, not every
verification clause of each grouped requirement. ADRs 0007-0016 remain proposed;
the resource-policy gate and full slice acceptance are still open.
