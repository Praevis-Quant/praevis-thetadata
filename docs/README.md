# Requirements and documentation index

Baseline date: 2026-10-03. Scope: reusable authentication, Windows/Linux session persistence, the auth CLI, and verification. Market-data code is a foundation, not an accepted product requirements baseline.

| Document | Purpose |
| --- | --- |
| [Changelog](../CHANGELOG.md) | Implemented changes, kept separate from future work |
| [Agent guidance](../CLAUDE.md) | Architecture, commands, working conventions, and Git workflow |
| [L1 requirements](requirements/L1-auth.md) | User and product outcomes |
| [L2 requirements](requirements/L2-auth.md) | System behavior and parent L1 links |
| [L3 requirements](requirements/L3-auth.md) | Verifiable component behavior, implementation, and checks |
| [Stock EOD L1](requirements/L1-stock-eod.md) | Defined product outcomes; implementation/acceptance planned |
| [Stock EOD L2](requirements/L2-stock-eod.md) | Typed requests, values, streams, bounds, and verification |
| [Stock EOD L3](requirements/L3-stock-eod.md) | Component owners and explicit acceptance contracts |
| [Stock EOD evidence and gates](requirements/stock-eod-evidence.md) | Source trace, Python deviations, measurement gate, and unverified vendor semantics |
| [Stock EOD acceptance matrix](requirements/stock-eod-matrix.md) | Every EOD L3 requirement, error/phase cases, passing synthetic evidence and remaining clauses |
| [Architecture review](research/architecture-performance-review.md) | Crate assessment, Fowler/ports-adapters, Twelve-Factor applicability and performance findings |
| [Architecture requirements](requirements/architecture.md) | ARCH L1/L2/L3: dependency boundaries and optional adapters |
| [Performance requirements](requirements/performance.md) | PERF L1/L2/L3: numeric fast path, consumer metrics and measured acceptance |
| [Enhancement requirements](requirements/enhancements.md) | EXT L1/L2/L3: protected local contracts and optional filtering/projection |
| [Contract origin register](requirements/origins.md) | Exhaustive upstream/enhancement/mixed classification, separate from status |
| [Upstream update template](upstream-update-template.md) | Compatibility and enhancement impact, tests and performance reconciliation |
| [Architecture decisions](adr/README.md) | Decisions, alternatives, and consequences |
| [First market-data slice](adr/0007-first-market-data-slice.md) | Proposed stock EOD design; ADRs 0007-0012 link to defined requirements, implementation pending |
| [Vendor research](research/thetadata-1.0.12.md) | Evidence, contradictions, discoveries, and unanswered questions |
| [Complete Python catalogue and coverage matrix](research/python-1.0.12-catalog.md) | Every file/symbol/RPC/message, signatures/defaults/fields, and feature-to-requirement dispositions |
| [Deep source inventory](research/python-1.0.12-inventory.json) | Regenerable source hashes, AST evidence, descriptors, metadata, and examples |
| [Feature dispositions](research/python-1.0.12-dispositions.json) | Supported/planned/excluded/unresolved behavior and explicit downstream evidence/gaps |
| [Upstream source identity](research/upstream-source.md) | Attested publishing repository/commit, source hashes, and access limitations |
| [Related project watchlist](research/related-projects.md) | Pinned ThetaDataDx review and release-planning comparison areas |
| [Project naming brief](project-naming.md) | Independent branding candidates, CLI/package identity and rename gates |
| [Recover research artifacts](research/upstream-source.md#recovering-the-removed-local-artifacts) | Exact wheel URL/hash and optional regeneration after local source cleanup |
| [Artifact inventory](research/vendor-1.0.12.json) | Reproducible local artifact hashes and API inventory |
| [Performance baseline](performance/auth.md) | Synthetic measurements and how to interpret them |
| [CLI startup evaluation](performance/cli-startup.md) | Before/after release comparisons for runtime startup |
| [Synthetic EOD baseline](performance/eod-baseline.md) | Rust-native loopback fixture, decoder/delivery measurements, scope and comparison procedure |
| [Initial EOD measurements](performance/eod-results.md) | Repeated Windows/WSL baseline, variability, allocation evidence and artifact hashes |
| [Bounded numeric EOD experiment](performance/eod-numeric.md) | Implementation, provisional budgets, common-harness comparisons and acceptance gaps |
| [Numeric EOD results](performance/eod-numeric-results.md) | Windows/Linux legacy, Table and numeric comparisons, regressions and retained hashes |
| [EOD resource audit](performance/eod-resource-audit.md) | Envelope preflight, capacity accounting, admission partition, running cancellation and bounded inline work |
| [EOD scheduling results](performance/eod-scheduling-results.md) | Repeated Windows/Linux inline/offload comparisons, portable default decision and retained hashes |
| [EOD fault/resource calibration](performance/eod-acceptance-results.md) | Local error provenance, concurrent-stream stall/fix, slow-consumer measurements, retained regressions and policy rationale |
| [Roadmap](ROADMAP.md) | Forward-looking candidates and the preserved original review |
| [Release milestones](releases.md) | Versioned scope, exit gates, owners and release evidence checklist |
| [Manual live verification](live-verification.md) | Explicit PROD/STAGE credentialed runs, private captures and offline comparison; outside CI |
| [Connection points and SVG](connections.md) | Direct HTTPS/gRPC versus planned Terminal REST/JAR and WebSocket paths, distinct errors and lifecycle |
| [Connection requirements](requirements/connections.md) | CONN L1/L2/L3 explicit modes, optional process ownership and adapter verification |
| [Subscription testing](subscription-testing.md) | Per-product/tier/connector expectations, positive/negative cases and paid-account coverage gaps |
| [Live verification requirements](requirements/live-verification.md) | LIVE L1/L2/L3 opt-in, provenance, replay and release obligations |
| [Live observation records](research/live-observations.md) | Reviewed service evidence template and synthetic comparison gaps |
| [crates.io release plan](crates-io-release-plan.md) | Useful prereleases, name checks, credential verification limits and packaging gates |
| [Upstream compatibility monitor](upstream-monitor.md) | Scheduled PyPI version comparison and baseline policy |

Requirement IDs are permanent. L2 requirements name their L1 parents; L3 requirements name their L2 parents. Add new requirements and ADRs as scope grows; do not quietly reinterpret existing IDs. Changes to credential precedence, persisted data, store identity, or lifecycle behavior require updating the relevant ADR and tests.

Evidence labels distinguish **observed artifact behavior**, **documented vendor behavior**, **project decisions**, and **unverified vendor contracts**. A green synthetic test is not evidence that live subscription access or token lifetime has been validated.

## Requirements organization

Keep product L1 and system L2 requirements centrally under `docs/requirements/`. For the current auth baseline, retain L3 there too: its contract spans the auth library, CLI, and platform stores, and splitting it now would scatter related verification.

As independent components acquire substantial requirements, use a hybrid structure: component-owned L3 under `crates/<name>/docs/requirements/` or `apps/<name>/docs/requirements/`, linked from this central index. Cross-component L3 remains central. A requirement has exactly one canonical definition, permanent ID, parent links, owner, and verification evidence. Move definitions without duplicating or renumbering them; update the index and links. Folder hierarchy does not determine abstraction level. Do not create empty L1/L2/L3 trees for every crate.

For the startup work, AUTH-L1-004 and AUTH-L1-005 already express the product outcomes. New AUTH-L2-010/011 and AUTH-L3-019 through AUTH-L3-024 refine implementation and verification without inventing another L1.

The stock EOD slice has six L1 outcomes, twelve L2 requirements, and twenty-nine
L3 contracts, all centrally defined with explicit owners and planned verification.
The repository checker validates canonical definitions, duplicate/undefined IDs,
namespace levels, and immediate-level parent links for AUTH and new namespaces,
including future component requirements directories. Canonical table rows start
with a bare ID; L2/L3 rows put parents in the second cell. Reference tables use
links or prose rather than duplicate defining rows. These structural checks do
not validate that tests passed or that evidence supports a behavior claim.

ARCH/PERF/EXT add 36 proposed requirements across all three levels, initially
kept central because they span components. Each has a classified origin,
owner and planned verification. Every canonical requirement and numbered ADR
must appear exactly once in the origin register. Classification checks do not
prove runtime noninterference or measured speedups. ADR-0014 defines the additive
numeric-batch API; the EOD Table representation remains a compatibility adapter.
