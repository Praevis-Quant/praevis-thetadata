# L1: authentication outcomes

Status: initial implemented baseline, 2026-10-03. Owners: Praevis-Quant maintainers. These are product outcomes for authentication only.

| ID | Requirement | Success measure |
| --- | --- | --- |
| AUTH-L1-001 | Other Rust projects shall authenticate with ThetaData through a reusable library without requiring the CLI, Theta Terminal, or market-data transport. | An application imports `praevis-thetadata-auth`, authenticates, and receives an opaque session. |
| AUTH-L1-002 | **Retired 2026-11-01; replaced by AUTH-L1-006** ([ADR-0020](../adr/0020-in-memory-sessions-and-optional-persistence.md)). Original text: a user shall save and reuse a session across process exits on Windows and Linux. | Superseded; persistence is now optional. |
| AUTH-L1-003 | Authentication shall avoid exposing login credentials and session tokens through normal diagnostics, command output, or repository content. | Redaction tests, native storage, and repository exclusion checks pass. |
| AUTH-L1-004 | Users shall be able to distinguish local persistence from server-side session validity and deliberately replace or remove a stored session. | `auth`, `auth status`, and `auth logout` have documented, distinct effects. |
| AUTH-L1-006 | By default a session shall live only in the authenticating process and be reused in memory, as in the vendor package. A user may optionally save it and reuse it across process exits on Windows and Linux. | The default library build has no native-store dependencies. With `native-store` (CLI `auth --save`), a second process loads the saved session without credential input. |
| AUTH-L1-005 | Maintainers shall verify authentication behavior on both supported operating systems and measure its local overhead without a paid/live account. | CI, native-store process tests, and synthetic performance artifacts can run without vendor secrets. |

Out of scope: market-data CLI commands, streaming subscriptions, flat-file downloads, entitlement enforcement, automatic refresh, remote logout, sharing secrets between OS accounts/machines, and performance guarantees for ThetaData servers. Vendor lifetime/revocation rules remain open research questions.
