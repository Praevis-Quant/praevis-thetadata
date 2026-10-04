# ADR-0002: observed authentication contract and explicit credentials

Status: Accepted. Date: 2026-10-03. Requirements: AUTH-L2-001, AUTH-L2-002, AUTH-L2-006, AUTH-L2-007.

## Context

The supplied 1.0.12 source defines an HTTPS identity request outside the public Terminal OpenAPI schema. Current Python docs support API keys; an older FAQ contradicts them. Neither reviewed source supplies a versioned identity-service specification.

## Decision

Match the observed POST body, `TD-TERMINAL-KEY` identifier, endpoint, and `authEnv.envType`. Treat the header value shipped in the public client as a compatibility identifier, not as an account secret or a stable promised vendor contract. Default to HTTPS, reject URL-embedded credentials and redirects, and bound timeouts. Require a nonempty session ID/email and preserve subscriptions as JSON because their types are not specified.

The library takes explicit credentials. The CLI uses file override, API-key environment variable, complete email/password environment pair, then default file. Do not automatically load `.env`; do not add password/API-key command-line flags. File line trimming follows the source; explicitly supplied passwords retain whitespace.

## Alternatives

Using the Terminal OpenAPI for login would fabricate an endpoint. Blindly following the older FAQ would remove supported API-key behavior. Copying all Python discovery/logging behavior would add hidden filesystem reads and expose auth responses in logs.

## Consequences

These intentional discovery differences are documented. Identity URL overrides support tests; custom services should use separate profiles. Raw HTTP rejection bodies are omitted. Vendor changes to the identity contract require new evidence and compatibility tests. See [research](../research/thetadata-1.0.12.md), especially the source authority and unresolved questions sections.
