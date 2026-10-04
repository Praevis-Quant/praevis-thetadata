# ADR-0003: native persistent session storage

Status: Accepted. Date: 2026-10-03. Requirements: AUTH-L2-003, AUTH-L2-004, AUTH-L2-006, AUTH-L2-007.

## Context

Auth must survive process exits and be shared by other applications on Windows and Linux. The Python package only copies authentication from an existing client in memory.

## Decision

Use Windows Credential Manager and Linux persistent Secret Service through `keyring`. Do not use Linux kernel session keyrings, an in-memory mock, or plaintext files as a fallback. Persist a private v1 record containing token, user metadata, environment, and save time. Never persist the API key or password.

Scope identity by OS user, profile, and PROD/STAGE. Keep the Windows target unchanged. On Linux use the stable identifier as a `service` attribute in the default collection; `new_with_target` would select a collection there and is deliberately avoided. The two OS stores do not synchronize. Reject invalid profiles, unknown versions, and environment mismatches.

## Alternatives

Plain JSON files simplify servers but expose bearer material to ordinary file handling. A custom encrypted file requires managing another key and a recovery/rotation contract. Process-only storage does not meet persistence. A kernel keyring alone does not provide the chosen durable collection semantics.

## Consequences

Headless Linux needs a D-Bus user session and unlocked persistent provider; containers must preserve provider storage. Host policy governs unlock, access by same-user applications, and Windows roaming. Storage capacity/provider errors are explicit. Concurrent successful writes use last-write-wins; there is no distributed locking. Session strings are redacted in diagnostics but not guaranteed to be zeroized in memory. Native-store tests use synthetic isolated profiles. See [setup](../../README.md#build-and-authenticate).
