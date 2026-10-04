# ADR-0007: stock end-of-day history as the first market-data slice

Status: Proposed. Date: 2026-10-04. Owner: Praevis-Quant maintainers.
Requirements: [EOD-L1-001 through EOD-L1-006](../requirements/L1-stock-eod.md),
[EOD-L2-001](../requirements/L2-stock-eod.md),
[EOD-L3-001](../requirements/L3-stock-eod.md). Implementation/acceptance planned;
existing auth baseline unchanged.

## Context

The research inventories 80 Python wrappers and 82 RPCs. Generated Rust
helpers already exist, but their presence does not establish supported query
defaults, output schemas, error behavior, or live compatibility. A first
supported slice needs enough behavior to exercise the design while remaining
small enough to verify thoroughly.

## Proposed decision

Use **stock end-of-day history for one symbol and an explicit date range** as
the first library feature to take through requirements and verification.
Trace it to Python `ThetaClient.stock_history_eod` and `GetStockHistoryEod`.
The request query has exactly three string fields: symbol #1, start_date #2,
end_date #3. Its request envelope carries QueryInfo #1 and params #2; the
RPC returns a finite stream of ResponseData batches.

Deliver this through `thetadata-client`, using `thetadata-core` values and an
explicit session from `thetadata-auth`. Keep authentication independently
usable. Do not add a market-data CLI, a crate per asset class, or a second
authentication path in this slice. Dates, prices, timestamps, empty results,
multi-batch output, and terminal errors receive explicit contracts through
[ADR-0008](0008-typed-requests-and-presence.md),
[ADR-0009](0009-market-data-values-and-schema.md), and
[ADR-0010](0010-stream-lifecycle-and-resource-bounds.md).

Expose an ergonomic typed entry point alongside the current wire-level
helpers. Use a distinct name/module boundary so Rust method overloading is
not assumed and the existing generated signatures are not silently changed.
The exact public names belong in the component requirements and API review.
Document the typed slice as the supported scope only after its acceptance
checks pass; other generated methods remain explicitly provisional.

## Alternatives

- Listing symbols is smaller but leaves date validation and financial value
  conversion out of the first acceptance path.
- Porting all 80 wrappers immediately creates a large default/presence and
  fixture matrix before the shared contracts have been verified.
- Options Greeks, flat files, corporate actions, and live subscriptions add
  model, framing, entitlement, or transport questions unnecessary for this slice.

## Consequences and open questions

This is a scope proposal, not a delivery promise or a statement that the
service has been exercised. The exact returned EOD column schema, date-bound
inclusivity, ordering, adjustment policy, and entitlement need authoritative
evidence. Initially preserve wire headers and order rather than inventing a
fixed OHLC struct or claiming adjusted prices. Record those questions before
designating an end-to-end result as vendor-validated.

## Derived requirements and verification

The linked L1 requirements cover obtaining historical stock data from a
reusable Rust library, preserving value meaning, and observing completion/failure.
L2 covers typed requests, explicit session use, incremental output, and bounded
operation. L3 defines exact field mapping, invalid inputs, NONE/ZSTD fixtures,
multiple batches, empty results, and failures after data. Acceptance remains planned.
Use [ADR-0011](0011-market-data-verification-and-artifact-retention.md) for the
evidence boundary and [ADR-0012](0012-requirements-and-research-traceability.md)
for document ownership. Canonical requirement definitions reside in the linked
requirements documents rather than being duplicated in this ADR.

## Supporting evidence

- [Exact Python wrapper](../research/python-1.0.12-catalog.md#item-f0536a100eb7dbcc).
- [Request query fields](../research/python-1.0.12-catalog.md#item-5d5e92001216e097),
  [request envelope](../research/python-1.0.12-catalog.md#item-a63a3eacf9637c00),
  and [RPC path/stream shape](../research/python-1.0.12-catalog.md#item-92352009630c5a88).
- [Current client foundation](../../crates/thetadata-client/src/lib.rs) and
  [generated helper construction](../../crates/thetadata-client/build.rs).
