# Security policy

## Supported versions

| Version | Security fixes |
| --- | --- |
| The latest `0.1.x` release | Yes |
| Earlier releases and prereleases | No: upgrade to the latest `0.1.x` |

Fixes are released as a new patch version through the release workflow
(`.github/workflows/release.yml`); a published version is never
replaced, and affected versions are yanked when that protects users.

## Reporting a vulnerability

**Please do not open a public issue.** Report it privately through GitHub:
**Security → Report a vulnerability** on
[this repository](https://github.com/Praevis-Quant/praevis-thetadata/security/advisories/new)
(GitHub private vulnerability reporting). Include what is affected (crate,
wheel or CLI, and version), how to reproduce it, and the impact you see.
Never include a real ThetaData API key, session token or market data.

## Response and fix targets

These are **targets for a single-maintainer project**, not guarantees.

| Step | Target |
| --- | --- |
| Acknowledge any report | within 7 days |
| Assess it and score it (CVSS) | within 7 days |

A fix is released within, by the vulnerability's CVSS v3.1 base score (or
CVSS v4.0 where the advisory provides it):

| Severity | CVSS base score | Fix released within |
| --- | --- | --- |
| Critical | 9.0–10.0 | 7 days |
| High | 7.0–8.9 | 30 days |
| Medium | 4.0–6.9 | 60 days |
| Low | 0.1–3.9 | 90 days, or the next release |

- **Coordinated disclosure:** a report stays private until the fix is
  released, and at most 90 days. We publish a GitHub security advisory (and
  a RustSec advisory for the crates when it applies), and credit the
  reporter unless they ask us not to.
- **Vulnerabilities in dependencies** use the score from that dependency's
  advisory, with the same targets. RustSec advisories against the locked
  dependencies fail CI, and Dependabot proposes the fixes.
- These targets meet the OpenSSF Best Practices "passing" criteria: a
  response to vulnerability reports within 14 days, and no medium or higher
  publicly known vulnerability left unpatched for more than 60 days.

## Scope

In scope: the published crates (`praevis-thetadata-*`), the
`praevis-thetadata` Python package, the CLI executables, and this
repository's release workflow and its artifacts.

Out of scope: ThetaData's service, the vendor's Python package and the
Theta Terminal (report those to Theta Data Inc.), and issues that need a
compromised machine or account.

How releases are built, attested and published: `docs/releasing.md`. A
security assurance report is planned.
