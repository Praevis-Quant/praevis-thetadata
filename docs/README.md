# Authentication baseline

Baseline date: 2026-10-03. Scope: reusable authentication, Windows/Linux session persistence, the auth CLI, and verification. Market-data code is a foundation, not an accepted product requirements baseline.

| Document | Purpose |
| --- | --- |
| [L1 requirements](requirements/L1-auth.md) | User and product outcomes |
| [L2 requirements](requirements/L2-auth.md) | System behavior and parent L1 links |
| [L3 requirements](requirements/L3-auth.md) | Verifiable component behavior, implementation, and checks |
| [Architecture decisions](adr/README.md) | Decisions, alternatives, and consequences |
| [Vendor research](research/thetadata-1.0.12.md) | Evidence, contradictions, discoveries, and unanswered questions |
| [Upstream source identity](research/upstream-source.md) | Attested publishing repository/commit, source hashes, and access limitations |
| [Artifact inventory](research/vendor-1.0.12.json) | Reproducible local artifact hashes and API inventory |
| [Performance baseline](performance/auth.md) | Synthetic measurements and how to interpret them |
| [CLI startup evaluation](performance/cli-startup.md) | Before/after release comparisons for runtime startup |
| [Roadmap](ROADMAP.md) | Forward-looking candidates and the preserved original review |
| [Upstream compatibility monitor](upstream-monitor.md) | Scheduled PyPI version comparison and baseline policy |

Requirement IDs are permanent. L2 requirements name their L1 parents; L3 requirements name their L2 parents. Add new requirements and ADRs as scope grows; do not quietly reinterpret existing IDs. Changes to credential precedence, persisted data, store identity, or lifecycle behavior require updating the relevant ADR and tests.

Evidence labels distinguish **observed artifact behavior**, **documented vendor behavior**, **project decisions**, and **unverified vendor contracts**. A green synthetic test is not evidence that live subscription access or token lifetime has been validated.

## Requirements organization

Keep product L1 and system L2 requirements centrally under `docs/requirements/`. For the current auth baseline, retain L3 there too: its contract spans the auth library, CLI, and platform stores, and splitting it now would scatter related verification.

As independent components acquire substantial requirements, use a hybrid structure: component-owned L3 under `crates/<name>/docs/requirements/` or `apps/<name>/docs/requirements/`, linked from this central index. Cross-component L3 remains central. A requirement has exactly one canonical definition, permanent ID, parent links, owner, and verification evidence. Move definitions without duplicating or renumbering them; update the index and links. Folder hierarchy does not determine abstraction level. Do not create empty L1/L2/L3 trees for every crate.

For the startup work, AUTH-L1-004 and AUTH-L1-005 already express the product outcomes. New AUTH-L2-010/011 and AUTH-L3-019 through AUTH-L3-024 refine implementation and verification without inventing another L1.
