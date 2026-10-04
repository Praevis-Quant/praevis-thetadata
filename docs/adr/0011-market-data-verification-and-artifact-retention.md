# ADR-0011: Rust protocol fixtures and recoverable research inputs

Status: Proposed. Date: 2026-10-04. Owner: Praevis-Quant maintainers.
Requirements: market-data verification L1/L2/L3 definitions pending;
extends the evidence approach of AUTH-L2-008 and AUTH-L2-009 without changing them.

## Context

Complete source enumeration is not proof of live compatibility. The source
Git repository is not publicly accessible, but the exact versioned wheel is
available through PyPI. The original local ZIP/extracted tree are ignored
inputs, while hashes, descriptors, license, and detailed research are tracked.
Keeping redundant local copies is unnecessary for ordinary builds or CI.

## Proposed decision

Use **Rust-native** fixtures and performance benchmarks for new market-data
implementation. Build a local synthetic gRPC service and protobuf/ZSTD
responses with the Rust toolchain. Cover request envelopes, output cases,
stream status/failure order, limits, timeouts, and cancellation. Distinguish
constructed fixtures from sanitized captures of actual vendor traffic; a
synthetic fixture must not be described as a verified vendor response.

For performance changes, build before/after release artifacts on the same
host/toolchain, preserve source/binary hashes and raw samples, control warmups
and execution order, and report regressions. Include decode throughput,
allocations/peak memory, and responsiveness as appropriate. Do not require
the Python package at build/test/runtime. Existing Python benchmark tooling
remains until its separate Rust migration is selected; new protocol tests
and decoder benchmarks do not add to that dependency.

Keep ordinary CI synthetic and credential-free on Windows and Ubuntu. Live
verification is a separately authorized operation, with recorded version,
environment, query, subscription context, and sanitized evidence. A green
synthetic suite is necessary but insufficient for a vendor-validated feature
claim. Flat files and corporate actions remain unresolved until their actual
framing/schema/availability has evidence.

Retain the versioned inventory, generated catalogue, original research,
source/descriptor hashes, upstream license, and attested publishing identity
in Git. Permit deletion of the ignored local wheel/extraction after verifying
them against that evidence. Document the exact recovery URL/hash and verify
downloaded bytes before inspecting them. Fetching a source artifact is an
explicit maintenance action, never a hidden normal-build dependency.

The offline research checker validates committed evidence without the archive.
A full source-to-inventory regeneration requires reacquiring the exact wheel
and research-only tooling. Python AST analysis is scoped to inspecting Python
source; it is not a product or Rust protocol-test dependency. Recovery relies
on the artifact remaining available: hashes prove byte identity, not future
availability. If long-term retention becomes necessary, use an approved
artifact archive with the same digest rather than placing the vendor tree
in the Rust source repository.

## Alternatives

- Installing/importing the Python client for every test adds undeclared and
  conflicting dependency behavior and would couple Rust verification to it.
- Live calls in every PR require credentials and conflate client correctness
  with account policy and service availability.
- Committing the entire wheel/extraction duplicates research inputs and
  violates the established repository boundary.
- Deleting the hashes, inventory, or preserved license would lose the basis
  for reproducibility; those tracked records remain essential.

## Consequences and requirements to derive

Requirements should identify the exact evidence needed for each feature
claim, fixture ownership, redaction, platform coverage, and benchmark metadata.
Keep missing live evidence visible rather than blocking unrelated synthetic
work or labeling unverified behavior supported. The artifact cleanup itself
does not approve a new market-data implementation.

## Supporting evidence

- [Research verification and limits](../research/thetadata-1.0.12.md#deep-analysis-2026-10-04).
- [Upstream source identity and recovery](../research/upstream-source.md#recovering-the-removed-local-artifacts).
- [Existing provenance decision](0005-verification-and-provenance.md).
- [Source inventory/checker](../../tools/research_python.py) and
  [before/after methodology](../performance/cli-startup.md).
