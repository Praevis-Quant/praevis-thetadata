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

**What to expect** (business days, from your report):

| | Target |
| --- | --- |
| Acknowledgement | 3 days |
| First assessment (accepted or not, and severity) | 7 days |
| Fix or mitigation for a high or critical issue | 30 days |
| Others | the next planned release |

We coordinate the disclosure with you, publish a GitHub security advisory
(and a RustSec advisory for the crates when it applies), and credit you
unless you ask us not to.

## Scope

In scope: the published crates (`praevis-thetadata-*`), the
`praevis-thetadata` Python package, the CLI executables, and this
repository's release workflow and its artifacts.

Out of scope: ThetaData's service, the vendor's Python package and the
Theta Terminal (report those to Theta Data Inc.), and issues that need a
compromised machine or account.

How releases are built, attested and published: `docs/releasing.md`. A
security assurance report is planned.
