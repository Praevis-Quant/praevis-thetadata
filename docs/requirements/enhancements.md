# Rust enhancement and upgrade requirements

Status: Defined/proposed, 2026-10-04. Origin of every EXT requirement:
**Rust enhancement**. Owner: workspace maintainers; L3 names component owners.
Decisions: [ADR-0015](../adr/0015-upstream-and-enhancement-contracts.md),
[ADR-0016](../adr/0016-optional-batch-transforms.md).
Classification: [origin register](origins.md). Runtime/filter/migration
verification is planned. The origin audit delivered with these documents is
structural evidence only, not proof of runtime noninterference.

## L1

| ID | Requirement | Planned success measure |
| --- | --- | --- |
| EXT-L1-001 | Consumers shall retain explicitly documented Rust enhancements when upstream Python/protocol compatibility is updated. | Each update reconciles both compatibility and enhancement contracts; conflicting changes require an explicit migration decision. |
| EXT-L1-002 | Consumers shall optionally transform streamed data locally without changing baseline vendor query or delivery semantics. | Opt-in filter/projection fixtures preserve order, exactness, bounds, errors and lifecycle; unselected transforms do no work. |

## L2

| ID | L1 parents | Requirement | Planned verification |
| --- | --- | --- | --- |
| EXT-L2-001 | EXT-L1-001 | Every requirement and ADR shall classify its origin independently of status, and mixed contracts shall identify the protected Rust behavior. | Exhaustive origin register audit, valid references and explicit rationale; reviewers verify semantic accuracy. |
| EXT-L2-002 | EXT-L1-001 | Upstream updates shall preserve versioned source evidence, enumerate affected enhancement contracts, and rerun their independent correctness/performance checks. | Completed update-impact record and linked results; no automatic baseline bump or silent feature deletion. |
| EXT-L2-003 | EXT-L1-002 | Local projection and typed predicates shall be explicit optional Rust batch adapters with deterministic null/type/schema behavior and bounded state. | Valid/invalid transform fixtures, no-op/selective benchmarks, and lifecycle tests through the public stream adapter. |

## L3

| ID | L2 parents | Owner | Requirement | Planned verification |
| --- | --- | --- | --- | --- |
| EXT-L3-001 | EXT-L2-001 | workspace | Maintain one classification per canonical requirement and ADR: upstream-contract, rust-enhancement or mixed, with rationale/evidence and explicit protected policy for mixed entries. Audit missing, duplicate, unknown and invalid classification. A new requirement cannot silently bypass classification. | Origin-register mutation tests and repository CI audit; status remains defined in canonical documents. |
| EXT-L3-002 | EXT-L2-002 | workspace | Each upstream update shall record versions/artifact+descriptor hashes, source delta, affected requirement/ADR and enhancement IDs, adapter changes, preserved deviations, test results, performance results and unresolved evidence. Use the canonical manifest for tracking and retain the previous evidence. | Review a completed update template against the changed inventory; source/descriptor closure and independent contract suites. |
| EXT-L3-003 | EXT-L2-001, EXT-L2-002 | workspace, client | Generated/vendor updates shall not own or overwrite enhanced API semantics, local transforms, native persistence, numeric data or resource/performance policy. Conflicts require a linked ADR/migration with preserved historical IDs and tests for the chosen behavior. | Diff review and separate compatibility/enhancement fixtures; changes to mixed records explain both sides. |
| EXT-L3-004 | EXT-L2-002 | workspace | Maintain enhancement regression evidence separately from upstream feature disposition. Run affected enhancement tests and PERF-L3-007/008 comparisons for data-path changes even when raw wire fixtures still pass. No automatic claim of noninterference from classification or hashes alone. | Update-impact record links enhancement tests, measurements and decisions; unimplemented tests are explicit gaps. |
| EXT-L3-005 | EXT-L2-003 | core, client | The initial optional plan shall support ordered named-column projection and conjunctions of typed column/constant comparisons plus is-null. Resolve column indices once per schema. Reject missing/duplicate projection names and incompatible cell types; ordinary comparison with null is false. Exact price comparison must not round through floats; compare timestamps by instant and retain source zone. Validate all predicate inputs before short-circuit evaluation. | Mixed/null cells, exact unequal scales, timestamp zones, absent/duplicate names and rows rejected by earlier predicates still producing required type errors. |
| EXT-L3-006 | EXT-L2-003 | core, client | Apply predicates before projection, preserve matching row order and selected column order, and allow predicate-only columns. Reject an empty projection to avoid an ambiguous zero-column row contract. A filtered-empty batch retains projected schema and is not EOF. Keep one-batch bounded state and unchanged upstream failure/partial/completion semantics. Do not hide invalid source data by filtering it out. | Zero/some/all-match cases, invalid discarded columns, failure after selected data, cancellation/deadlines and demand-controlled empty batches; memory remains within shared budgets. |
| EXT-L3-007 | EXT-L2-003 | client, workspace | Do not serialize local transform options into the EOD wire request or claim bandwidth savings/server pushdown without evidence. No transform or Python callback runs when the native adapter is unselected. Benchmark local CPU/output-transfer cost and zero/partial/full selectivity separately. | Wire equality with/without local transforms, native bypass instrumentation and PERF-L3-007 cases; any future pushdown requires its own request evidence and equivalence tests. |
