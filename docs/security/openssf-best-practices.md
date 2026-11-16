# OpenSSF Best Practices: answers for the "passing" badge

Prepared 2026-11-16 for registering this project at
[bestpractices.dev](https://www.bestpractices.dev/) (sign in with GitHub,
"Get Your Badge Now", repository `https://github.com/Praevis-Quant/praevis-thetadata`).
Paste each answer and its evidence into the matching criterion. **Status:
not yet registered.** Every "Met" below is in place on `main`; the one
"Unmet" is said plainly.

Links are to `main` of
`https://github.com/Praevis-Quant/praevis-thetadata` unless they are
absolute.

## Basics

| Criterion | Answer | Evidence and justification |
| --- | --- | --- |
| `description_good` | Met | [README](../../README.md): what it is (an independent, unofficial client for ThetaData's market-data service: Rust crates, Python package, CLI) and its supported surface |
| `interact` | Met | [README](../../README.md) (install), [issues](https://github.com/Praevis-Quant/praevis-thetadata/issues) (feedback), [CONTRIBUTING.md](../../CONTRIBUTING.md) (contributing) |
| `contribution` | Met | [CONTRIBUTING.md](../../CONTRIBUTING.md): issues first, pull requests against `main`, required checks |
| `contribution_requirements` | Met | [CONTRIBUTING.md](../../CONTRIBUTING.md): tests, local checks, no new unsafe code, dependency rules, commit messages, documentation |
| `floss_license` | Met | Apache-2.0 ([LICENSE](../../LICENSE)) |
| `floss_license_osi` | Met | Apache-2.0 is OSI-approved |
| `license_location` | Met | [LICENSE](../../LICENSE) at the root, and in every package |
| `documentation_basics` | Met | [README](../../README.md), [user guide](../user-guide.md), [documentation index](../README.md) |
| `documentation_interface` | Met | API reference on docs.rs for each crate; [client README](../../crates/praevis-thetadata-client/README.md); [CLI README](../../apps/praevis-thetadata-cli/README.md); the Python package's docstrings |
| `sites_https` | Met | GitHub, crates.io, PyPI and docs.rs are HTTPS only |
| `discussion` | Met | [GitHub issues](https://github.com/Praevis-Quant/praevis-thetadata/issues) and pull requests: searchable, addressable by URL, open to anyone with a GitHub account |
| `english` | Met | All documentation and discussion are in English |
| `maintained` | Met | Active: releases and commits in [the CHANGELOG](../../CHANGELOG.md) and the history |

## Change control

| Criterion | Answer | Evidence and justification |
| --- | --- | --- |
| `repo_public` | Met | `https://github.com/Praevis-Quant/praevis-thetadata` |
| `repo_track` | Met | Git: every change with its author and date |
| `repo_interim` | Met | Every change lands through a pull request; interim commits stay on their branches |
| `repo_distributed` | Met | Git |
| `version_unique` | Met | Each release has a unique version; [CHANGELOG](../../CHANGELOG.md) |
| `version_semver` | Met | Semantic Versioning, as the [CHANGELOG](../../CHANGELOG.md) states |
| `version_tags` | Met | Annotated `v<version>` tags; only repository admins can create them (tag ruleset) |
| `release_notes` | Met | [CHANGELOG](../../CHANGELOG.md) and the GitHub release notes (from `docs/release-draft-<version>.md`) |
| `release_notes_vulns` | Met | Fixed vulnerabilities are listed in the CHANGELOG with their advisory IDs (for example RUSTSEC-2026-0346 to -0349, -0381, -0382, -0386, -0414, -0433 fixed for 0.1.3) |

## Reporting

| Criterion | Answer | Evidence and justification |
| --- | --- | --- |
| `report_process` | Met | [GitHub issues](https://github.com/Praevis-Quant/praevis-thetadata/issues); [CONTRIBUTING.md](../../CONTRIBUTING.md#reporting) |
| `report_tracker` | Met | GitHub issues |
| `report_responses` | N/A | No bug reports have been filed yet (the tracker is empty as of 2026-11-16); reports are answered through the issue tracker |
| `enhancement_responses` | N/A | No enhancement requests have been filed yet; they are answered in the issue tracker |
| `report_archive` | Met | GitHub issues and pull requests are public and searchable |
| `vulnerability_report_process` | Met | [SECURITY.md](../../SECURITY.md) |
| `vulnerability_report_private` | Met | GitHub private vulnerability reporting ([SECURITY.md](../../SECURITY.md)) |
| `vulnerability_report_response` | Met | Acknowledgement within 7 days and assessment within 7 days ([SECURITY.md](../../SECURITY.md)), within the 14 days required |

## Quality

| Criterion | Answer | Evidence and justification |
| --- | --- | --- |
| `build` | Met | `cargo build --workspace --locked`; the Python wheel with `maturin build` ([AGENTS.md](../../AGENTS.md#common-commands)) |
| `build_common_tools` | Met | Cargo and maturin |
| `build_floss_tools` | Met | Rust, Cargo, maturin and Python are FLOSS |
| `test` | Met | `cargo test --workspace --locked`; the Python tests with `unittest` ([AGENTS.md](../../AGENTS.md#common-commands)) |
| `test_invocation` | Met | Standard: `cargo test`, `python -m unittest` |
| `test_most` | Met | Unit, integration and end-to-end tests (against an emulator of the service) cover the supported surface; see the [CI workflow](../../.github/workflows/ci.yml) |
| `test_continuous_integration` | Met | [CI](../../.github/workflows/ci.yml) on every push and pull request (Windows, Linux, macOS; Python 3.10–3.13), required before merge |
| `test_policy` | Met | [CONTRIBUTING.md](../../CONTRIBUTING.md): new functionality comes with tests, a bug fix with a test that fails without it |
| `tests_are_added` | Met | Recent changes add tests with the code (for example the planner split for Greeks EOD, the macOS Keychain round trip, the TLS provider test, the fuzz targets); see the history |
| `tests_documented_added` | Met | [CONTRIBUTING.md](../../CONTRIBUTING.md) |
| `warnings` | Met | `cargo clippy ... -D warnings` and rustc warnings fail CI |
| `warnings_fixed` | Met | CI runs with warnings as errors |
| `warnings_strict` | Met | Clippy with `-D warnings` on all targets |

## Security

| Criterion | Answer | Evidence and justification |
| --- | --- | --- |
| `know_secure_design` | Met | The maintainer applies secure design: least privilege in CI and publishing, HTTPS by default, no listening ports, bounded decoders ([assurance report](assurance-report.md)) |
| `know_common_errors` | Met | Memory safety by Rust (unsafe forbidden), bounded parsing of untrusted input, fuzzing, secrets never logged ([assurance report](assurance-report.md)) |
| `crypto_published` | Met | Only published algorithms, through rustls (TLS 1.2/1.3) and ring |
| `crypto_call` | Met | The project implements no cryptography; it calls rustls |
| `crypto_floss` | Met | rustls and ring are FLOSS |
| `crypto_keylength` | Met | rustls's defaults (no weak key sizes) |
| `crypto_working` | Met | rustls supports no broken algorithms |
| `crypto_weaknesses` | Met | rustls's defaults exclude SHA-1 signatures and other weak options |
| `crypto_pfs` | Met | TLS 1.3 and ECDHE suites only (rustls) |
| `crypto_password_storage` | N/A | The project stores no passwords. It never stores the user's ThetaData credentials; only an opt-in session in the OS credential store |
| `crypto_random` | N/A | The project generates no keys or nonces itself; TLS randomness is rustls's (OS-backed) |
| `delivery_mitm` | Met | Delivered over HTTPS (crates.io, PyPI, GitHub); release artifacts carry SHA-256 checksums and signed provenance ([releasing](../releasing.md)) |
| `delivery_unsigned` | Met | Checksums are not fetched over HTTP; verification uses `SHA256SUMS` from the release and Sigstore-signed attestations |
| `vulnerabilities_fixed_60_days` | Met | No medium or higher publicly known vulnerability is unfixed beyond 60 days; RustSec advisories fail CI; targets in [SECURITY.md](../../SECURITY.md) (medium within 60 days) |
| `vulnerabilities_critical_fixed` | Met | Critical within 7 days ([SECURITY.md](../../SECURITY.md)) |
| `no_leaked_credentials` | Met | No credentials in the repository; GitHub secret scanning with push protection is on; tests use synthetic data |

## Analysis

| Criterion | Answer | Evidence and justification |
| --- | --- | --- |
| `static_analysis` | Met | CodeQL (default setup, extended suite) on Rust, Python and Actions; clippy; zizmor on workflows; cargo-deny on dependencies |
| `static_analysis_common_vulnerabilities` | Met | CodeQL's security queries; zizmor's workflow-security audits |
| `static_analysis_fixed` | Met | Findings fail CI (clippy, zizmor, cargo-deny) or are fixed from code scanning |
| `static_analysis_often` | Met | On every push and pull request; Scorecard weekly |
| `dynamic_analysis` | Met | Fuzzing of every decoder of untrusted input with ClusterFuzzLite ([fuzzing](fuzzing.md)) |
| `dynamic_analysis_unsafe` | Met | AddressSanitizer on the fuzz runs; Rust is memory-safe, with unsafe forbidden except two audited calls |
| `dynamic_analysis_enable_assertions` | Met | The fuzzers are built with debug assertions and overflow checks on (`cargo fuzz build -O --debug-assertions`), and the tests run in debug builds; the fuzz targets check that every input ends in success or a clean error |
| `dynamic_analysis_fixed` | Met | A fuzz crash fails the job; findings are fixed before release |

## Not yet met

- **The badge itself:** the project is not registered yet. Register with
  these answers, then put the badge's project ID into the README badge
  (it is a placeholder until then).

## Vulnerability response targets (from SECURITY.md)

| Step | Target |
| --- | --- |
| Acknowledge any report | within 7 days |
| Assess it and score it (CVSS) | within 7 days |

| Severity | CVSS v3.1 base score (or v4.0 where the advisory gives it) | Fix released within |
| --- | --- | --- |
| Critical | 9.0–10.0 | 7 days |
| High | 7.0–8.9 | 30 days |
| Medium | 4.0–6.9 | 60 days |
| Low | 0.1–3.9 | 90 days, or the next release |

Coordinated disclosure: private until the fix is released, at most 90 days.
Dependencies: scored by their advisory, same targets. These are targets for
a single-maintainer project.
