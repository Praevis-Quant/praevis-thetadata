# L2: authentication system requirements

Status: initial implemented baseline, 2026-10-03. Each requirement derives from the listed L1 outcomes.

| ID | L1 parents | System requirement |
| --- | --- | --- |
| AUTH-L2-001 | AUTH-L1-001, AUTH-L1-003 | The library shall accept explicit API-key or email/password credentials; the CLI shall implement a deterministic, documented discovery order. |
| AUTH-L2-002 | AUTH-L1-001, AUTH-L1-004 | The library shall authenticate using the observed 1.0.12 HTTPS request contract and bind the resulting session to PROD or STAGE. |
| AUTH-L2-003 | AUTH-L1-006, AUTH-L1-003 | Persistence shall use the OS user's native persistent credential store and a versioned record that excludes login credentials. |
| AUTH-L2-004 | AUTH-L1-006, AUTH-L1-004 | Profiles and environments shall select isolated records through a stable identity usable by other projects. |
| AUTH-L2-005 | AUTH-L1-004 | The CLI shall provide authentication/save, offline local status, and local deletion, with machine-readable output and defined exit codes. |
| AUTH-L2-006 | AUTH-L1-003 | Secrets shall be redacted from credential/session debug representations and omitted from normal CLI output and authentication rejection messages. |
| AUTH-L2-007 | AUTH-L1-001, AUTH-L1-004 | Invalid inputs, transport failures, unavailable storage, absent records, and invalid persisted records shall fail explicitly without silently changing storage or lifecycle behavior. |
| AUTH-L2-008 | AUTH-L1-005 | Automated checks shall exercise the auth contract and native persistence on Windows/Linux using synthetic data. |
| AUTH-L2-009 | AUTH-L1-003, AUTH-L1-005 | CI and performance tooling shall exclude the vendor archive/extraction and real credentials, build from checked-in Rust/schema inputs, and report reproducible measurement context. |
| AUTH-L2-010 | AUTH-L1-004, AUTH-L1-005 | The CLI shall initialize asynchronous execution only for HTTP authentication; local operations and argument handling shall preserve their behavior without runtime startup. |
| AUTH-L2-012 | AUTH-L1-001, AUTH-L1-006 | The library shall authenticate into an in-memory session by default and offer native persistence only through an explicitly enabled optional feature. The CLI shall follow the vendor credential order and save a session only when asked ([ADR-0020](../adr/0020-in-memory-sessions-and-optional-persistence.md)). |
| AUTH-L2-011 | AUTH-L1-005 | Startup optimizations shall be evaluated using reproducible before/after release binaries on the same host, preserving native persistence and reporting measured results without assuming a speedup or imposing an uncalibrated latency SLA. |

The L2 requirements specify project behavior, not additional vendor guarantees. For example, environment-bound storage is our policy; vendor staging documentation does not establish token interchangeability across environments.

AUTH-L2-003 and AUTH-L2-004 now derive from AUTH-L1-006, which replaced the retired AUTH-L1-002 on 2026-11-01 ([ADR-0020](../adr/0020-in-memory-sessions-and-optional-persistence.md)). Their storage rules apply whenever persistence is enabled.
