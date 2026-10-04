# ADR-0015: track upstream compatibility and Rust enhancements independently

Status: Proposed. Date: 2026-10-04. Owner: workspace maintainers.
Origin: **Rust enhancement** (maintenance and change-control contract).
Requirements: [EXT L1/L2/L3](../requirements/enhancements.md).

## Context

The Python inventory describes the vendor artifact, not the complete desired
Rust product. Native persistence, exact values, resource limits and forthcoming
numeric batches/transforms must survive upstream updates. A feature being
planned or supported says nothing about whether its origin is upstream or Rust.

## Proposed decision

Maintain the [origin register](../requirements/origins.md) and its machine-readable
classification for every canonical requirement and ADR. Origins are
`upstream-contract`, `rust-enhancement`, or `mixed`. Mixed records explicitly
name inherited behavior and the protected Rust policy. Origin does not change
implementation/acceptance status, imply vendor verification, or select every
inventoried endpoint for immediate implementation.

Preserve the existing source inventory and dispositions as the compatibility
record. Independently register Rust capabilities and link decisions,
requirements, implementation, tests and benchmark evidence as they become
available. A generated binding or upstream rename cannot delete a Rust feature.
Keep raw protocol surfaces separate from typed and enhanced interfaces; adapt
wire changes at the boundary rather than leaking them through domain APIs.

Each upstream update must include an impact record: old/new artifact and
descriptor hashes/version, source delta, affected contract and enhancement IDs,
intentional deviations, adapter changes, verification, performance comparison,
and unresolved questions. Run upstream compatibility and independent enhancement
regression suites. A conflict requires an explicit ADR/migration decision;
“upstream changed” does not authorize silently changing local semantics or
weakening performance/security/bounds. IDs are never reused. A newer PyPI
version remains an alert, not an automatic manifest update or generated overwrite.

## Alternatives and consequences

One undifferentiated parity list hides intentional improvements. Forking vendor
schema with local filters makes upgrades ambiguous. Keeping extensions outside
generated code plus an exhaustive origin register adds review work but makes
the distinction inspectable. Structural CI catches missing/duplicate/invalid
classification; it cannot establish semantic noninterference. Runtime contracts
and measured performance still require their own tests and change review.

This ADR proposes the ongoing process; classification/auditing delivered with
it does not mean future migration or runtime test coverage already exists.
The [upgrade template](../upstream-update-template.md) provides the review record.
