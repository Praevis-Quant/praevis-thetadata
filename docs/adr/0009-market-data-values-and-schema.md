# ADR-0009: exact values and explicit batch schemas

Status: Proposed. Date: 2026-10-04. Owner: thetadata-core and thetadata-client maintainers.
Requirements: [EOD-L1-002](../requirements/L1-stock-eod.md),
[EOD-L2-003 and EOD-L2-004](../requirements/L2-stock-eod.md),
[EOD-L3-006 through EOD-L3-012](../requirements/L3-stock-eod.md).
Defined contracts, implementation/acceptance planned.

Additive follow-up: [ADR-0014](0014-performance-first-data-path.md) defines a
numeric batch interface and an explicit Table conversion adapter. The Table
contract below is retained; it does not require eager formatting on the new
numeric path. See [PERF-L3-002/003](../requirements/performance.md).

## Context

Python accumulates every batch into a dataframe, multiplies prices into
floats, maps scale zero to NaN, and delegates dtype inference to Pandas or
Polars. Duplicate headers, empty tables, mismatched rows, changing schemas,
unknown zones, and negative price scales have surprising behavior. Rust
already has exact prices and validated table batches, but no accepted
market-data output contract.

## Proposed decision

Use transport-independent `Table`/`Value` batches for the first supported
slice. Keep text, signed int64, boolean, null, price, and timestamp meanings
distinct. Preserve prices as an integer mantissa plus decimal exponent;
scale zero represents missing price, and scales outside 0..19 are errors.
Do not convert money to floating point automatically. Null/NaN/export policy
must remain explicit at an output-adapter boundary.

Retain the current millisecond RFC3339 timestamp representation for this
slice, including the observed UTC/New York offset. Validate supported zones
and timestamp bounds. This stores an instant/offset string, not the original
timezone identifier; document that limit. Numeric/lazy timestamp storage is
a separate measured API proposal under roadmap recommendation #4. The first
query does not justify silently changing the current core representation.

Preserve header and row order without deduplication, sorting, or guessed EOD
column names. Reject duplicate headers and row-width mismatches. Preserve
headers on a zero-row batch. Establish the stream's schema from its first
schema-bearing batch and reject later schema changes rather than joining
incompatible columns. Requirements must define how schema-less empty batches
are represented and handled; do not silently infer columns from absent data.

Successful end-of-stream with no rows is distinguishable from an RPC
NOT_FOUND error and from an incomplete stream. Results already yielded before
an error remain partial; callers must observe successful stream completion
before describing a collected result as complete. Keep explicit null and
unset/unknown-only oneof behavior documented separately, even if the initial
compatibility policy maps both to Value::Null.

## Alternatives

- A dataframe dependency makes inference and whole-result accumulation part
  of the transport API and couples all consumers to that choice.
- A fixed EOD struct would promise column names/types before a verified
  response schema exists.
- Matching Python's floats, negative indexing, and duplicate-column merging
  would preserve defects at the expense of value meaning and diagnosability.

## Consequences and requirements to derive

Rust output is intentionally not dataframe-byte parity. Future dataframe,
CSV, JSON, or columnar adapters consume the core contract rather than define
it. Core errors remain distinct from transport errors. Conversion policy
needs documented examples and tests for exact signed prices, missing values,
booleans, timezone/DST boundaries, overflow, empty schemas, duplicates,
ragged rows, and schema changes across batches.

The final EOD business schema, ordering guarantees from the service, and
corporate-action adjustment semantics remain unverified. This ADR proposes
preserving observed wire data, not inventing those business meanings.

## Supporting evidence

- [Conversion analysis](../research/thetadata-1.0.12.md#conversion-and-output).
- [Wire value definitions](../research/thetadata-1.0.12.md#protocol-surface).
- [Current core representation and tests](../../crates/thetadata-core/src/lib.rs).
- [Current decoder](../../crates/thetadata-client/src/decode.rs) and
  [stream lifecycle proposal](0010-stream-lifecycle-and-resource-bounds.md).
