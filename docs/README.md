# Authentication baseline

Baseline date: 2026-10-03. Scope: reusable authentication, Windows/Linux session persistence, the auth CLI, and verification. Market-data code is a foundation, not an accepted product requirements baseline.

| Document | Purpose |
| --- | --- |
| [L1 requirements](requirements/L1-auth.md) | User and product outcomes |
| [L2 requirements](requirements/L2-auth.md) | System behavior and parent L1 links |
| [L3 requirements](requirements/L3-auth.md) | Verifiable component behavior, implementation, and checks |
| [Architecture decisions](adr/README.md) | Decisions, alternatives, and consequences |
| [Vendor research](research/thetadata-1.0.12.md) | Evidence, contradictions, discoveries, and unanswered questions |
| [Artifact inventory](research/vendor-1.0.12.json) | Reproducible local artifact hashes and API inventory |
| [Performance baseline](performance/auth.md) | Synthetic measurements and how to interpret them |

Requirement IDs are permanent. L2 requirements name their L1 parents; L3 requirements name their L2 parents. Add new requirements and ADRs as scope grows; do not quietly reinterpret existing IDs. Changes to credential precedence, persisted data, store identity, or lifecycle behavior require updating the relevant ADR and tests.

Evidence labels distinguish **observed artifact behavior**, **documented vendor behavior**, **project decisions**, and **unverified vendor contracts**. A green synthetic test is not evidence that live subscription access or token lifetime has been validated.
