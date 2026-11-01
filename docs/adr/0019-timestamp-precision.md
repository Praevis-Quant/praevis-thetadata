# ADR-0019: reject sub-millisecond timestamps until their semantics are defined

Status: Accepted, 2026-11-01. Origin: **mixed**: the upstream 1.0.16 schema adds
the fields, and a Rust policy protects exactness and explicit failure.
Owner: praevis-thetadata-core and praevis-thetadata-client maintainers.
Requirements: [EOD-L3-030](../requirements/L3-stock-eod.md), refining
[EOD-L2-003](../requirements/L2-stock-eod.md) alongside
[EOD-L3-009](../requirements/L3-stock-eod.md).

## Context

Python 1.0.16 added `nanos = 3` (uint32) and `precision = 4`
(`TimestampPrecision { MILLISECOND = 0; NANOSECOND = 1 }`) to the shared
`Endpoints.ZonedDateTime` response value
([research](../research/thetadata-1.0.13-1.0.17.md#release-1-0-16)). The
descriptor carries no comments. It is unverified whether `nanos` is a remainder
after `epoch_ms` or a nanosecond-of-second, whether `epoch_ms` is truncated or
rounded, and which RPCs, if any, emit `NANOSECOND`. Python 1.0.16 and 1.0.17
ignore both fields and build values from `epoch_ms` alone, as did this
workspace's decoders before the 1.0.17 baseline. That silently drops any
sub-millisecond content. [ADR-0009](0009-market-data-values-and-schema.md)
promises exact values and explicit failure for unsupported content.
[ADR-0014](0014-performance-first-data-path.md) stores numeric timestamps as
epoch milliseconds.

## Decision

Accept a timestamp only when `precision` is `MILLISECOND` (0) and `nanos` is 0;
the encoding of every pre-1.0.16 response is exactly this. Any other
combination is unsupported content: `NANOSECOND`, an unknown precision value,
or non-zero `nanos` under millisecond precision. It is never truncated.

- Core: `check_timestamp_precision(nanos, precision)` and
  `timestamp_with_precision(...)` return `DataError::TimestampPrecision`. The
  existing `timestamp()` and `Timestamp::from_wire()` keep their signatures and
  millisecond semantics.
- Typed EOD (numeric and Table adapters): the precision check runs before the
  instant and zone checks, and it is a terminal `EodError::Unsupported`. Batches
  already delivered remain partial results (EOD-L3-012). The error carries no
  payload values (EOD-L3-017).
- Raw table helper: `Error::Data(DataError::TimestampPrecision)`.
- Raw generated bindings expose `nanos` and `precision` unchanged.

## Alternatives

- **Truncate to milliseconds, as Python does.** Rejected: it silently loses
  data and contradicts the exact-value contract.
- **Carry nanoseconds in `Timestamp` and Table text.** Deferred: this is a
  public API and representation change, and it depends on vendor semantics
  that are not yet known. It needs a superseding ADR once ThetaData documents
  the fields.

## Consequences

Responses with millisecond precision are byte-for-byte and value-for-value
unchanged, and synthetic fixtures and benchmarks are unaffected apart from one
integer comparison per timestamp cell. If the service starts emitting
nanosecond timestamps for stock EOD, typed streams fail explicitly instead of
returning silently degraded values. That is the intended prompt to obtain the
semantics and supersede this decision. The open question is tracked in the
[EOD evidence record](../requirements/stock-eod-evidence.md).
