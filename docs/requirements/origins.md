# Contract origins and protected Rust enhancements

Owner: workspace maintainers. Classification date: 2026-10-04.
Decision: [ADR-0015](../adr/0015-upstream-and-enhancement-contracts.md).
The exhaustive [machine-readable register](origins.json) names **every
requirement ID and ADR path individually**, with origin and rationale. This
document explains the grouping; canonical requirement text remains in its
original document. The register does not duplicate definitions or assign status.

| Origin | Meaning | Upgrade treatment |
| --- | --- | --- |
| `upstream-contract` | Obligation directly derived from inspected upstream behavior. | Reconcile against versioned artifact evidence; do not mistake it for live verification. |
| `rust-enhancement` | Local product, safety, architecture, operational or performance policy beyond mirroring Python. | Preserve independently; only an explicit project decision/migration changes it. |
| `mixed` | Inherited functionality combined with an intentional local contract. | Assess wire compatibility and protected Rust semantics separately; preserve both through an adapter when possible. |

An enhancement can improve ergonomics, correctness, security or performance;
it does not necessarily add a new RPC. Planned/implemented/verified status is
orthogonal to origin. Existing auth remains implemented with its documented
gaps; EOD/ARCH/PERF/EXT runtime contracts remain planned. No entry proves a speedup.

| Register group | Scope | Protected behavior / canonical evidence |
| --- | --- | --- |
| `auth-wire` | Observed authentication JSON request | [Auth L3](L3-auth.md); evidence in the retained Python research. |
| `auth-mixed` | Authentication plus reusable library/validation semantics | [Auth L1](L1-auth.md), [L2](L2-auth.md), [L3](L3-auth.md); preserve explicit input, standalone sessions, validation and failure behavior. |
| `auth-rust-policy` | Native storage, CLI, local lifecycle, security, runtime and verification | [Auth L3](L3-auth.md); upstream changes must not reset credential precedence, stored identity, redaction or runtime scope. |
| `eod-mixed` | EOD endpoint, request and values plus typed validation/output/error policy | [EOD evidence/deviations](stock-eod-evidence.md); preserve exact prices, strict schema/zone checks and explicit complete/partial results. Table timestamps remain formatted; numeric delivery is additive. |
| `eod-rust-policy` | Local symbol policy, pull lifecycle, deadlines, bounded resources, fixtures and benchmarks | [EOD L3](L3-stock-eod.md); do not discard local contracts when replacing wire bindings. |
| `architecture` | All ARCH requirements | [Architecture](architecture.md); inward dependency direction, optional persistence and outer adapters. |
| `performance` | All PERF requirements | [Performance](performance.md); numeric delivery, measured allocation/work bounds, explicit conversion and regression policy. |
| `extensions` | All EXT requirements | [Enhancements](enhancements.md); independent upgrade protection and optional local filtering/projection. |

ADR classifications are exhaustive in the register and visible in the
[ADR index](../adr/README.md). The original accepted ADR text is retained;
classification adds provenance rather than rewriting its historical decision.
Mixed-entry rationales explicitly separate inherited behavior and local policy.

## Upstream change control

Use the [update template](../upstream-update-template.md). Capture the source
delta, affected IDs, adapter reconciliation, separate compatibility/enhancement
results and measured data-path impact. Retain unresolved facts. A monitored
PyPI release never automatically overwrites requirements or bumps the manifest.
Preserve source dispositions for all unselected endpoints; this register is an
additional Rust contract view, not a replacement for the 531-item inventory.

`check_repository.py` checks exhaustive classification of staged/tracked
requirements and numbered ADRs, unique entries, valid origins, rationale and
evidence paths. Tests mutate these records to verify rejection. These checks
cannot prove that a changed decoder preserves behavior or performance; runtime
tests, benchmarks and reviewed impact records remain required acceptance work.
Classification changes are intentional reviewed edits, not auto-regenerated
from the latest upstream package. Future PyO3 requirements must be classified
when selected; its roadmap entry is not an implemented feature or binding API.
