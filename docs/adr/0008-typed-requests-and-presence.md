# ADR-0008: typed requests with explicit value presence

Status: Proposed. Date: 2026-10-04. Owner: thetadata-client maintainers.
Requirements: [EOD-L2-002](../requirements/L2-stock-eod.md),
[EOD-L3-001 through EOD-L3-005](../requirements/L3-stock-eod.md); relates to
AUTH-L1-001. Defined contracts, implementation/acceptance planned.

## Context

Python annotations do not enforce request validity. Its wrappers use different
date conversions and often omit explicit false/zero values while transmitting
those same values in the query metadata map. Some option messages have both a
ContractSpec expiration and an unused sibling expiration field. Generated
wire structs alone do not express the intended public input contract.

## Proposed decision

Keep the descriptor-generated types as the exact wire representation and add
typed request construction in `thetadata-client`. Do not put market-data
discovery or implicit environment reads in `thetadata-auth` or
`thetadata-core`. Preserve access to the raw bindings for advanced callers,
with explicit documentation that they bypass typed validation/defaults.

For the first EOD request, require a nonempty symbol and two explicit calendar
dates, reject start_date greater than end_date before starting a query, and
format dates as `YYYY-MM-DD`. Preserve symbol spelling/case; do not silently
uppercase, split commas, or invent an exchange-specific ticker grammar.
Exact whitespace handling and supported date range must be specified and
tested in the request requirements. Local range ordering does not establish
whether the server treats either endpoint as inclusive.

For later selected requests, use explicit optional presence: None means omit,
Some(false) means send false, and Some(0) means send zero wherever the protocol
supports presence. Apply documented defaults in the typed request layer,
separately from user intent; do not use truthiness. Give wildcard/date
expiration and scalar/repeated symbols distinct typed representations when
those features are selected. Do not populate duplicate semantic fields until
the chosen wire mapping has source/service evidence.

Construct QueryInfo from the caller-supplied session and stable Rust client
identity. Do not copy Python's free-form map of all argument strings by
default. Retain token/email fields already observed on the wire, never log
them, and leave undocumented terminal/version fields at their existing
defaults until their meaning is established. Additional diagnostic metadata
needs an explicit purpose and redaction contract.

## Alternatives

- Exposing only raw protobuf structs forces callers to rediscover defaults,
  validation, and field-presence rules.
- Reproducing Python's truthiness exactly loses caller intent and can make
  server defaults override explicit input.
- A universal string-map request API avoids types but moves errors to the
  server and obscures field numbers, formats, and missing values.

## Consequences and requirements to derive

The typed API deliberately improves on observed Python quirks; it does not
promise bug-for-bug compatibility. Record each deviation against its source
inventory row. Parameter legality beyond the inspected artifact remains
unverified; avoid speculative venue, entitlement, or calendar restrictions.

L2/L3 need pure request-to-wire fixtures, rejection before network activity,
deterministic formatting, and tests distinguishing absent/false/zero/empty.
For EOD, assert the three query fields and QueryInfo/envelope placement. The
generic optional-presence design is guidance for future selected endpoints,
not approval to implement all wrapper defaults now.

## Supporting evidence

- [Formatting and presence analysis](../research/thetadata-1.0.12.md#date-and-presence).
- [Wrapper metadata behavior](../research/thetadata-1.0.12.md#wrapper-semantics).
- [Complete signatures and construction tables](../research/python-1.0.12-catalog.md).
- [Standalone auth boundary](0001-standalone-auth.md) and
  [first-slice scope](0007-first-market-data-slice.md).
