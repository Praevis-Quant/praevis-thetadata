# Upstream compatibility update record template

Copy into a dated research/update document when selecting an upstream update.
Do not change the tracked version merely because the scheduled monitor failed.
Policy: [ADR-0015](adr/0015-upstream-and-enhancement-contracts.md),
[origin register](requirements/origins.md), EXT-L3-002 through EXT-L3-004.

| Field | Required record |
| --- | --- |
| Baselines | Old/new source versions; artifact URLs/hashes; descriptor hashes; Rust revisions; canonical manifest change |
| Source delta | Added/removed/changed wrappers, RPCs, fields, defaults, dependencies and behavior; retained prior evidence |
| Compatibility impact | Affected upstream/mixed requirement IDs and ADRs; wire/request/output changes; evidence versus inference |
| Enhancement impact | Affected enhancement/mixed IDs, including auth/store, exact values, typed validation, numeric delivery, bounds, filtering and performance; explicit unaffected rationale where appropriate |
| Reconciliation | Adapter changes; preserved intentional deviations; any conflict and linked ADR/migration decision; no silent ID reuse or feature removal |
| Correctness | Separate compatibility and enhancement test commands/results/revisions on Windows/Linux; native store where affected |
| Performance | Relevant same-host baseline/candidate hashes, fixtures, raw samples, first-batch/tail/throughput/memory results and calibrated regression decision; justified not-applicable scope if no data-path impact |
| Open evidence | Unverified vendor behavior, live evidence authorization/scope, unresolved tests; no synthetic-to-live inference |
| Acceptance | Reviewer decision and remaining gates; classification and inventory audits alone are insufficient |
