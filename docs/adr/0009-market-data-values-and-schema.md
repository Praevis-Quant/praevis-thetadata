# ADR-0009: exact values and explicit batch schemas

Status: **Accepted with revisions** (beta.1 review, 2026-11-11; [notes](#beta1-review-2026-11-11)). Proposed 2026-10-04. Owner: praevis-thetadata-core and praevis-thetadata-client maintainers.
Requirements: [EOD-L1-002](../requirements/L1-stock-eod.md),
[EOD-L2-003 and EOD-L2-004](../requirements/L2-stock-eod.md),
[EOD-L3-006 through EOD-L3-012](../requirements/L3-stock-eod.md).
Defined contracts, implementation/acceptance planned.

Additive follow-up: [ADR-0014](0014-performance-first-data-path.md) defines a
numeric batch interface and an explicit Table conversion adapter.
[ADR-0019](0019-timestamp-precision.md) adds the 1.0.16 timestamp-precision policy:
only millisecond values are accepted. The Table
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

## Addendum (2026-11-08): column types that do not depend on the response

**Problem.** The wire carries no column types. `DataTable` headers are names
only, and each cell is a oneof (text, number, price, timestamp, `null_value`,
boolean). The columnar decoder types a column from its first non-null value,
so a column that is null throughout one response became Arrow `Null`.
- **Example:** `calendar_on_date` for a closed day returns `open`/`close`
  as `Null`, while an open day returns `Utf8`. Joining the two results
  fails on a schema mismatch.
- **Scope:** `unify` repairs this within one query or job, not across
  requests. Any endpoint can hit it, for example a price whose scale is 0
  (null) in every row.
- **The vendor package has the same behaviour:** Polars infers `Null` for an
  all-`None` column, and `pl.concat` then fails the same way.

**Decision (user, 2026-11-07).**
- **Registry:** a committed registry of observed column kinds per endpoint,
  [`docs/endpoints/schemas.json`](../endpoints/schemas.json). It holds column
  names and wire kinds only, each entry citing the live runs that observed
  it, and is checked against a closed vocabulary by
  `tools/generate_endpoints.py`.
- **Decoder:** a registered column starts with its observed type. A response
  in which it is entirely null still gets that type, with null values. A
  value of another kind is the existing `ColumnType` error.
- **Nothing else changes:** registered names the response lacks are not
  added, nothing is renamed or reordered, and columns not registered behave
  as before (`Null` when entirely null).
- **This preserves observed wire data:** a type comes only from values seen
  in live responses of that endpoint, never from a column name alone (`open`
  is text in the calendar and a price elsewhere) or from absent data.
- **Calendar:** the columns stay text (`date`, `type`, `open`, `close`), as the
  wire and the vendor send them. Parsing them into `date32`/`time64` would add
  a business meaning; it can be a later opt-in.
- **No other endpoint's types change.** Other endpoints are registered only
  from the column lists in the private live reports (names and Arrow types,
  never values; approved for commit), citing their run IDs.

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
- [Current core representation and tests](../../crates/praevis-thetadata-core/src/lib.rs).
- [Current decoder](../../crates/praevis-thetadata-client/src/decode.rs) and
  [stream lifecycle proposal](0010-stream-lifecycle-and-resource-bounds.md).

## Beta.1 review (2026-11-11)

The value rules stand. They hold on both output paths:
- exact prices, never floats;
- scale 0 is null, and a scale outside 1-19 is an error;
- distinct kinds, with no coercion;
- headers kept in order, with duplicates and ragged rows rejected;
- a successful empty result is distinct from `NOT_FOUND` and from an
  incomplete stream;
- rows delivered before an error are partial.

**Revised:**
- **Arrow became the main output.** `query()`/`job()` and the Python package
  return Arrow batches through `praevis-thetadata-arrow` (ADR-0014):
  - prices are `Decimal128(38, 10)`, exact;
  - timestamps are zone-aware `Timestamp(ms, "America/New_York")` (or
    `"UTC"`), not RFC 3339 text.
- **`Table` stays the compatibility output** of the stock EOD stream, with
  the RFC 3339 representation described above.
- **The schema rule** on the Arrow path:
  - the only allowed change between batches is a `Null` column becoming
    typed, repaired by `unify`;
  - the 2026-11-08 addendum's registry fixes the type of registered
    columns.
