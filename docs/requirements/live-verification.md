# Manual live verification and release evidence requirements

Status: Defined, 2026-10-04. Origin: Rust enhancement throughout.
Policy: [ADR-0017](../adr/0017-manual-live-verification.md).
Implementation: [manual Rust runner](../../crates/thetadata-client/examples/live_eod.rs).
Operation: [runbook](../live-verification.md). Release gates: [milestones](../releases.md).
Local implementation/testing does not establish passing live evidence.

| ID | Owner | L1 outcome |
| --- | --- | --- |
| LIVE-L1-001 | workspace/client | Maintainers can verify advertised Rust functionality against an explicitly selected real ThetaData account and retain reproducible private evidence without making service access a prerequisite for contributors or CI. |

| ID | L1 parents | Owner | L2 behavior |
| --- | --- | --- | --- |
| LIVE-L2-001 | LIVE-L1-001 | workspace/auth/client | Live checks are optional, manually invoked outside CI, explicitly credentialed/environment-selected, bounded and separate from default library behavior. |
| LIVE-L2-002 | LIVE-L1-001 | client | Retain private response evidence and compare it with synthetic-service delivery through the existing interfaces without claiming captures are sanitized or synthetic data is vendor evidence. |
| LIVE-L2-003 | LIVE-L1-001 | maintainers | Release claims name the supported surface and require reviewed correctness, interoperability, performance, packaging and origin evidence appropriate to that surface. |

| ID | L2 parents | Owner | L3 contract | Verification |
| --- | --- | --- | --- | --- |
| LIVE-L3-001 | LIVE-L2-001 | workspace | Require the manual feature/example, explicit live consent, environment, query and exactly one credential-file argument. Reject live execution with `CI` set. Do not add live execution, credentials or artifact uploads to CI workflows. | CLI argument and preflight tests; inspect unchanged workflows; default target exclusion. |
| LIVE-L3-002 | LIVE-L2-001 | auth/client | Read only the selected bounded credential file, authenticate once, hold session in memory and make no retries/refresh/storage writes. Use normal HTTPS/vendor endpoints. Never serialize auth bodies, tokens, email hints, requests or remote diagnostic text. | Mock request counts and sentinel checks; source review; manual real auth evidence pending. Payload contents remain quarantined, not guaranteed sanitized. |
| LIVE-L3-003 | LIVE-L2-001, LIVE-L2-002 | client | Limit manual capture to one symbol and at most 31 days between date endpoints, 16 responses, 8 MiB per envelope, 32 MiB aggregate, finite connect/query/idle budgets and up to three sequential EOD calls. Record known protobuf envelopes in order with hashes, source/build/protocol/environment/query/time provenance and explicit complete/partial state; never overwrite a prior run. | Bounds/path/file-collision tests; complete/partial mock capture; digests and manifest review. Existing typed resource policy remains in force. |
| LIVE-L3-004 | LIVE-L2-002 | client/core | Offline replay uses only loopback synthetic identity, validates format/descriptor/length/hash/complete EOF, and compares ordered exact values/schema across raw helper, numeric and Table interfaces. Batch boundaries may differ; mismatches fail. Keep every replay as a new report. | NONE/ZSTD mock capture replay, hash tampering, row reordering, exact values, incomplete capture rejection; manual live comparison remains pending. |
| LIVE-L3-005 | LIVE-L2-002 | maintainers | Keep captures private/ignored and labelled as real unreviewed data. Record observations and differences against synthetic tests. Public promotion requires contents/redistribution review and transformation provenance; constructed reproductions stay labelled synthetic. | Runbook, observation template and review record; no automated upload/promotion. |
| LIVE-L3-006 | LIVE-L2-003 | maintainers | Use ordered versioned milestones and per-release scope/exit gates. A live-compatibility claim needs maintainer-reviewed manual evidence on the candidate; failures/outages/empty data do not establish nonempty EOD interoperability. Preserve known performance regressions and unverified upstream/enhancement contracts. | Release checklist, archive/source hashes, linked acceptance matrix, manual observation record and documented performance decision. No publication implied by a passing test. |
