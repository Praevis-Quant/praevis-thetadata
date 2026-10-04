# L2: authentication system requirements

Status: initial implemented baseline, 2026-10-03. Each requirement derives from the listed L1 outcomes.

| ID | L1 parents | System requirement |
| --- | --- | --- |
| AUTH-L2-001 | AUTH-L1-001, AUTH-L1-003 | The library shall accept explicit API-key or email/password credentials; the CLI shall implement a deterministic, documented discovery order. |
| AUTH-L2-002 | AUTH-L1-001, AUTH-L1-004 | The library shall authenticate using the observed 1.0.12 HTTPS request contract and bind the resulting session to PROD or STAGE. |
| AUTH-L2-003 | AUTH-L1-002, AUTH-L1-003 | Persistence shall use the OS user's native persistent credential store and a versioned record that excludes login credentials. |
| AUTH-L2-004 | AUTH-L1-002, AUTH-L1-004 | Profiles and environments shall select isolated records through a stable identity usable by other projects. |
| AUTH-L2-005 | AUTH-L1-004 | The CLI shall provide authentication/save, offline local status, and local deletion, with machine-readable output and defined exit codes. |
| AUTH-L2-006 | AUTH-L1-003 | Secrets shall be redacted from credential/session debug representations and omitted from normal CLI output and authentication rejection messages. |
| AUTH-L2-007 | AUTH-L1-001, AUTH-L1-004 | Invalid inputs, transport failures, unavailable storage, absent records, and invalid persisted records shall fail explicitly without silently changing storage or lifecycle behavior. |
| AUTH-L2-008 | AUTH-L1-005 | Automated checks shall exercise the auth contract and native persistence on Windows/Linux using synthetic data. |
| AUTH-L2-009 | AUTH-L1-003, AUTH-L1-005 | CI and performance tooling shall exclude the vendor archive/extraction and real credentials, build from checked-in Rust/schema inputs, and report reproducible measurement context. |

The L2 requirements specify project behavior, not additional vendor guarantees. For example, environment-bound storage is our policy; vendor staging documentation does not establish token interchangeability across environments.
