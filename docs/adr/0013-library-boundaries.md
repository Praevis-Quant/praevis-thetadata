# ADR-0013: cohesive libraries with explicit outer adapters

Status: Proposed. Date: 2026-10-04. Owner: workspace maintainers.
Origin: **Rust enhancement** (architecture independent of Python packaging).
Requirements: [ARCH L1/L2/L3](../requirements/architecture.md).

## Context

The current five crates isolate CLI, authentication, vendor schema, values,
and transport reasonably well. Native storage still accompanies session reuse,
and eager presentation/vendor mapping occurs in core values. Future consumers
include native Rust applications and a separately planned PyO3 adapter.
See the [source-based review](../research/architecture-performance-review.md)
for Fowler, ports/adapters, and Twelve-Factor applicability.

## Proposed decision

Keep the five current crates; use private modules to separate client request,
transport, codec, lifecycle/budget, and compatibility concerns. Generated wire
types stay in proto and vendor conversion belongs at the client boundary.
Core owns protocol-independent exact values, schemas, numeric batches and pure
operations; no networking, storage, environment discovery, or Python imports.
Existing public `from_wire`/formatting helpers may remain compatibility wrappers;
new fast-path code must not depend on their presentation behavior.

Make native persistence optional in auth so a session/HTTP consumer can build
without keyring dependencies; the CLI explicitly enables it and keeps its
existing storage behavior. Assess feature-disabled builds and dependency trees.
Do not split a session-only or codec crate until measured dependency cost or
independent reuse warrants the public surface and maintenance cost.

Keep application configuration, runtime construction and output in outer
adapters. A future Python package depends on Rust interfaces; Rust libraries
never depend on that binding. Do not create per-asset crates, a universal
repository abstraction, or a service deployment to satisfy a pattern.

## Alternatives and consequences

A single crate loses useful dependency seams. Many new crates or trait objects
without independent responsibilities add maintenance and possibly runtime cost.
Private modules and optional features preserve a small public surface, though
Cargo feature unification must be tested in actual consumer configurations.
Twelve-Factor service configuration does not override the auth CLI's deliberate
native persistence; service hosts own environment/secrets and pass explicit config.

No crate refactor is implemented by this ADR. Existing auth contracts remain
unchanged. Validate dependency direction, storage-disabled builds and consumers
before accepting the architecture requirements.
