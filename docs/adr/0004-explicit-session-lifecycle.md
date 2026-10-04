# ADR-0004: explicit session lifecycle

Status: Accepted. Date: 2026-10-03. Requirements: AUTH-L1-004, AUTH-L2-005, AUTH-L2-007.

## Context

An opaque session ID exists in the wheel, but no reviewed artifact defines expiry, refresh, validation, or remote logout. Keeping a token on disk does not prove the server accepts it. Staging is a separate connection environment; its existence does not prove interchangeable tokens.

## Decision

`theta auth` authenticates and saves a fresh session only after success. `theta auth status` reads local storage without credentials or network access and reports `validity: not_checked`. `theta auth logout` deletes only the selected local record. Applications explicitly load sessions; if the server rejects a session, they request reauthentication. Do not invent JWT parsing, expiry timers, silent refresh, or revocation calls.

## Alternatives

An assumed expiry would prematurely discard valid sessions or preserve invalid ones. A market-data request disguised as auth validation would conflate authentication with entitlement/data availability. Persisting passwords to enable automatic relogin expands scope and secret exposure.

## Consequences

There is no unattended recovery from server invalidation in this baseline. Failed login does not delete old state. Logout cannot invalidate in-memory copies in other processes. A successful status has a strictly local meaning. Add a new ADR once the vendor provides lifecycle semantics. The [open questions](../research/thetadata-1.0.12.md#questions-for-thetadata) describe the evidence needed.
