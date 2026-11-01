# ADR-0020: in-memory sessions by default, Python-order credentials, opt-in persistence

Status: Accepted, 2026-11-01. Supersedes [ADR-0003](0003-native-session-persistence.md).
Amends the credential-order paragraph of [ADR-0002](0002-auth-contract-and-credentials.md)
and implements the optional storage proposed in [ADR-0013](0013-library-boundaries.md).
Origin: **mixed**: the default session model and credential order follow the
vendor package, and native persistence remains a Rust enhancement.
Owner: praevis-thetadata-auth and praevis-thetadata-cli maintainers.
Requirements: [AUTH-L1-006](../requirements/L1-auth.md),
[AUTH-L2-012](../requirements/L2-auth.md),
[AUTH-L3-025 to AUTH-L3-027](../requirements/L3-auth.md),
[ARCH-L2-002/ARCH-L3-002](../requirements/architecture.md).

## Context

Python 1.0.12–1.0.17 keeps the authenticated session in memory only. It reuses
a session by passing `existing_authorized_client`, and it has no native or disk
persistence ([research](../research/thetadata-1.0.12.md#what-the-wheel-establishes-about-authentication);
the constructor is unchanged through 1.0.17). Its credential order is:

1. an explicit `creds_file`;
2. the `api_key` argument or `THETADATA_API_KEY`;
3. an explicit, complete email/password pair (an incomplete pair falls through);
4. `THETADATA_CREDENTIALS_FILE`, defaulting to `./creds.txt`, as the final
   fallback ([constructor table](../research/thetadata-1.0.12.md#constructor-and-configuration)).

ADR-0003 made native persistence the default for every auth consumer, on the
premise that a session must survive process exits. That premise is
unvalidated: token lifetime, invalidation and concurrent-use rules are unknown
([open questions](../research/thetadata-1.0.12.md#questions-for-thetadata)). It
also forces `keyring`, and on Linux a vendored D-Bus build plus a running,
unlocked Secret Service, onto every library user, including the market-data
client, which never stores sessions. The Rust CLI also differed from Python by
treating `THETADATA_CREDENTIALS_FILE` as a highest-precedence explicit file.

## Decision

**Library (`praevis-thetadata-auth`).** By default, explicit `Credentials` go
through `AuthClient::authenticate` and produce an in-memory `Session`; a caller
reuses it by cloning. `SessionStore`, `StoredSession` and `StoreError`, and the
`keyring` dependencies behind them, are available only with the optional
`native-store` Cargo feature, which is off by default. With the feature
enabled, storage behavior is unchanged:

- the identity is `praevis.thetadata.auth.v1:{PROD|STAGE}:{profile}`;
- the v1 record has the same fields and validation;
- the Windows target and Linux service attribute are unchanged;
- there is no plaintext fallback.

Records saved by earlier builds therefore keep loading.

**CLI (`praevis-thetadata`).** The CLI enables `native-store` explicitly.

- Credential order follows Python:
  1. `--creds-file` (explicit flag only);
  2. `THETADATA_API_KEY`;
  3. a complete `THETADATA_EMAIL`/`THETADATA_PASSWORD` pair;
  4. `THETADATA_CREDENTIALS_FILE`, otherwise `./creds.txt`.

  An incomplete or empty email/password pair falls through, as in Python. The
  email/password environment variables are a Rust-only extra, because Python
  takes that pair only as constructor arguments. No secret is accepted on the
  command line, and `.env` files are not loaded.
- `praevis-thetadata auth` logs in and reports the account **without
  saving**. `auth --save` also writes the session to the native store.
  `auth status` and `auth logout` are unchanged. A failed login never
  modifies an existing record.

| Behavior | Vendor-equivalent | Rust enhancement or deliberate difference |
| --- | --- | --- |
| In-memory session; reuse by clone | Yes (`existing_authorized_client`) | — |
| Credential order: file, key, pair, fallback file | Yes | Email/password come from env vars; no API-key or password flags |
| Incomplete email/password pair falls through | Yes | — |
| `.env` loading | No (Python loads it) | Not loaded, deliberately |
| Native persistence, status and logout | No | Optional feature; CLI opt-in with `--save` |
| HTTPS-only default, URL validation, redaction | No (Python does not enforce these) | Retained |

## Alternatives

- **Keep persistence on by default.** Rejected: it rests on an unvalidated
  lifetime premise and imposes OS keyring services on every consumer.
- **Remove persistence entirely.** Rejected: CLI users and existing records
  rely on it, and the implementation is tested on both platforms.
- **Keep the env-selected credentials file at highest precedence.** Rejected:
  it differs from the vendor and surprises users who move between the two.
- **Add `--api-key`/`--password` flags.** Rejected: secrets in argv leak
  through process listings and shell history.

## Consequences

- Library users who called `SessionStore` must enable `features = ["native-store"]`.
- CLI users who relied on `auth` saving implicitly must pass `--save`; their
  existing records keep working for `status`, `logout` and library loading.
- `THETADATA_CREDENTIALS_FILE` is now a fallback after the API key and the
  email/password pair.
- Default auth and client builds no longer need D-Bus, a C compiler for the
  vendored D-Bus library, or a running Secret Service.

CI tests auth with the feature off and on, checks that default auth and client
dependency trees contain no keyring/D-Bus packages, and keeps the native-store
process tests on Windows and Linux. The library change landed first; the CLI
precedence and `--save` change follows in a separate PR.
