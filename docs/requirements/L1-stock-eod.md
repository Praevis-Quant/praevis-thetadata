# L1: stock end-of-day history outcomes

Status: **Defined; implementation and acceptance verification planned**, 2026-10-04.
Owner of every L1 requirement: Praevis-Quant maintainers. This is the selected
next slice, not a claim of implemented or live-validated market-data support.
The authentication baseline remains separate.

Scope: a reusable Rust library query for **one stock symbol and an explicit
calendar-date range**, using a caller-supplied session. Python 1.0.12 evidence
establishes the request shape, not the service's business semantics.

| ID | Requirement | Success measure |
| --- | --- | --- |
| EOD-L1-001 | A Rust application shall request stock EOD history for one symbol and date range without a CLI, Python runtime, dataframe library, or Theta Terminal. | A credential-free Rust integration example queries a local gRPC fixture through the typed library API. |
| EOD-L1-002 | Callers shall receive the returned values, columns, and row order without silent rounding, sorting, omission, or schema substitution. | Exact-value and multi-batch fixtures verify every returned cell and distinguish intentional Python deviations. |
| EOD-L1-003 | Callers shall distinguish a complete result, a successful empty result, no-data rejection, and an incomplete result, including failure after partial delivery. | Terminal-state tests and API documentation demonstrate all four outcomes without treating partial rows as complete history. |
| EOD-L1-004 | Callers shall control local query duration and resource consumption without automatic authentication or replay. | Local timeout, cancellation, resource-boundary, and concurrent-client fixtures pass; no hidden login/retry occurs. |
| EOD-L1-005 | Session secrets shall remain protected while applications can identify actionable query failures. | TLS/default transport checks and adversarial diagnostic tests preserve useful error categories without exposing secrets or raw server content. |
| EOD-L1-006 | Maintainers shall demonstrate compatibility evidence and performance on Windows and Linux without paid access, while identifying what remains unverified against the vendor. | Rust-native synthetic tests and reproducible before/after measurements are linked to requirements; live evidence is recorded separately. |

Out of scope: a market-data CLI, multiple-symbol expansion, dataframe export,
other history/list/snapshot/streaming endpoints, flat-file retrieval, corporate
actions, automatic login/refresh/retry, subscription enforcement, and a vendor
latency SLA. Preserve all unselected source inventory items and dispositions.

Derivation: [ADR-0007](../adr/0007-first-market-data-slice.md),
[ADR-0011](../adr/0011-market-data-verification-and-artifact-retention.md), and
[ADR-0012](../adr/0012-requirements-and-research-traceability.md).
Children: [L2](L2-stock-eod.md) and [L3 acceptance contracts](L3-stock-eod.md).
Source links, deliberate deviations, and release gates are in the
[slice evidence record](stock-eod-evidence.md).
