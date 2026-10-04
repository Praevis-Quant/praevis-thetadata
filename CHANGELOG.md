# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versioned releases will follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Future work and undecided requirements are tracked in [docs/ROADMAP.md](docs/ROADMAP.md).

The workspace currently declares `0.1.0` with publishing disabled. No release
tag exists as of 2026-10-04; the entries below describe implemented work, not a
published `0.1.0` release. The Python compatibility baseline is separately
tracked in [the protocol manifest](crates/thetadata-proto/schema/manifest.json).

## [Unreleased]

### Added

- Typed EOD local frame/ZSTD resource-error provenance and a per-requirement
  acceptance matrix covering both numeric and Table APIs. Larger mock workloads
  exposed an existing HTTP/2 progress failure; finite query admission and a
  dedicated typed channel with fixed receive credit prevent paused streams from
  starving active decoding. Repeated Windows/Linux slow-consumer measurements,
  failed-calibration evidence and measured decode regressions are retained.
  Full EOD/performance acceptance remains open.
- Typed EOD resource-audit fixes: bounded outer-envelope decoding, conservative
  String capacity accounting, separate schema/batch reservations, deterministic
  running-worker cancellation tests, and opt-in bounded inline scheduling for
  small uncompressed responses. Repeated Windows/Linux measurements retain
  bounded offload as the portable default because Windows gains were mixed;
  Linux small numeric delivery improved by 51-65% in this fixture. Full stock
  EOD acceptance remains separate.
- Experimental typed stock EOD numeric batches with flat cell storage, lazy
  timestamp presentation, a shared Table adapter, preflight allocation/count
  bounds, finite decode admission, schema continuity and terminal stream errors.
  Added synthetic contract tests and a common legacy/Table/numeric benchmark;
  full EOD acceptance and live-service verification remain open.
- crates.io prerelease plan covering the five existing packages, name checks,
  credential-verification limits, packaging, ownership and release approval.
- Credential-free Rust EOD HTTP/gRPC fixtures with deterministic NONE/ZSTD
  batches and exact-value, wire-request, empty/partial/error and timeout tests.
  Tests exercise the current raw client; they do not claim typed EOD acceptance
  or vendor-service validation.
- Rust-native release decoder/delivery benchmark with raw latency/throughput
  samples, a separate allocation/peak requested Rust heap probe, executor-delay
  observations, embedded source/compiler and executable hashes, and guarded
  report comparisons. Added Windows/Linux CI smoke checks and a manual artifact
  workflow. Production decoding remains unchanged for the before baseline.
- Repeated Windows/WSL decoder baseline results with retained raw-report/binary
  hashes, explicit measurement boundaries and variability. Corrected mock TCP
  coalescing before retaining small-message delivery results; no production
  decoder optimization or live-service performance claim is made.
- Architecture review against Fowler separation, ports/adapters and applicable
  Twelve-Factor principles. Proposed ADRs 0013-0016 and 36 ARCH/PERF/EXT
  requirements define optional persistence boundaries, numeric batch delivery
  with explicit Table conversion, measured performance acceptance, protected
  enhancements, and optional local filtering/projection. Runtime changes remain
  planned; future PyO3 bindings are recorded on the roadmap.
- Exhaustive origin classification for requirements and ADRs, with mutation
  checks and an upstream-update impact template separating compatibility from
  protected Rust behavior. Classification is independent of acceptance status.
- Defined stock EOD requirements: six L1 outcomes, twelve L2 system contracts,
  and twenty-nine L3 acceptance contracts with owners, source/ADR links,
  intentional Python deviations, Rust-native fixture/benchmark obligations,
  and explicit measurement/vendor-evidence gates. Runtime acceptance remains
  planned. Added a selected-slice disposition while retaining all 531 source items.
- Requirement audit coverage for new namespaces, duplicate definitions,
  undefined references, and immediate-level parents, with mutation tests.
- Proposed ADRs 0007-0012 for a first stock EOD library slice, typed request
  presence/validation, exact values and batch schemas, stream lifecycle and
  resource limits, Rust-native protocol verification, and hybrid requirements
  traceability. Linked feature dispositions to the proposed decisions without
  marking unimplemented market-data behavior supported.
- Exact PyPI wheel recovery instructions and hash verification for optional
  research regeneration after removing the untracked local ZIP/extraction.
- Complete Python 1.0.12 source analysis and generated Markdown catalogue:
  14 artifact members, 9 modules, 251 functions/methods, 80 query wrappers,
  82 RPCs, 176 messages, 778 fields, and two enums. Added 531 inventory rows
  and 26 feature dispositions tracing research to existing auth evidence and
  future requirements, with explicit unresolved contracts.
- Reproducible archive-to-inventory checks, offline CI coverage checks, and
  omission/mutation tests. Preserve the original research and inventory by
  hash; document undeclared dotenv, conflicting gRPC version minimums,
  optional-field/default differences, conversion edge cases, and the two
  descriptor-only corporate-action RPCs without claiming live-service parity.
- Rust workspace with an independent `thetadata-auth` library and `theta`
  authentication CLI. Supports API-key and email/password authentication,
  explicit credential discovery, PROD/STAGE environments, and named profiles.
- Native session persistence through Windows Credential Manager and Linux
  Secret Service, with auth/status/logout commands usable across processes.
  Status reports locally stored metadata with server validity `not_checked`;
  logout removes only the selected local record.
- Market-data foundations: checked-in protobuf descriptor and generated gRPC
  bindings for 82 RPCs, transport-independent data types, and a client for
  requests, ZSTD decoding, and batch streaming. These foundations do not claim
  complete Python parity or verified live-service behavior.
- Authentication L1/L2/L3 requirements and six ADRs covering library boundaries,
  wire/credential contracts, native persistence, explicit session lifecycle,
  verification/provenance, and CLI runtime ownership. Documented the hybrid
  requirements organization in [the documentation index](docs/README.md).
- Python 1.0.12 artifact research, source hashes, and an API inventory. Recorded
  the publishing repository and commit from PyPI attestations, public GitHub
  access limitations, and matching wheel/sdist Python sources in
  [the upstream source investigation](docs/research/upstream-source.md).
- Windows/Ubuntu CI for formatting, Clippy, workspace tests, repository and
  documentation auditing, Python tooling tests, and synthetic native-store
  persistence tests. No live ThetaData credentials are required.
- Scheduled and manually triggered [PyPI compatibility monitoring](docs/upstream-monitor.md).
  It reads the tracked version from the protocol manifest, compares stable
  non-yanked releases with PEP 440, and fails on a newer version or an inability
  to verify. It does not automatically update the compatibility baseline.
- Synthetic auth benchmarks and before/after CLI startup comparisons with
  release-build provenance, executable hashes, alternating sample order, raw
  samples, percentile summaries, isolated native-store fixtures, and Windows/
  Ubuntu workflow support. WSL comparisons can stage identical binaries onto
  the native temporary filesystem.
- Forward-looking roadmap preserving the original performance review and table
  verbatim, with planned compatibility research and native Rust tooling work.
- Shared coding-agent guidance in [AGENTS.md](AGENTS.md) and
  [CLAUDE.md](CLAUDE.md), including project-specific Git conventions, and this
  changelog separating implemented changes from future work.

### Changed

- CLI parsing, help/version, status, and logout now run synchronously. Only
  HTTP authentication starts a current-thread Tokio runtime, which ends before
  persistence/output. The async auth library remains caller-runtime-owned;
  credential precedence, record format, output, and exit codes are preserved.
- Recorded the startup change's measured results and limitations in
  [the evaluation](docs/performance/cli-startup.md): Windows median status
  latency improved 10.6-10.9% and auth/save 5.5-6.0% across two runs. Native
  Linux-filesystem status improved 11.5-12.1%, but Linux auth/save showed no
  reliable gain. Tail latency did not consistently improve, and execution from
  Windows-mounted WSL storage regressed. These are synthetic local findings,
  not a universal latency guarantee.

### Security

- Authentication defaults to HTTPS with redirects disabled and explicit
  timeouts. Plain HTTP requires an explicit test opt-in. Credential/session
  secrets are redacted from output; saved records omit passwords and API keys.
- Native-store failures remain explicit errors, with no plaintext or
  process-only persistence fallback. Failed authentication preserves an
  existing saved session. Tests isolate and clean up synthetic profiles.

### Licensing

- Adopted Apache-2.0 for newly authored Rust code and tooling, added the root
  [LICENSE](LICENSE) and [NOTICE](NOTICE), and retained
  [the upstream Apache-2.0 license](crates/thetadata-proto/LICENSE.upstream).

The initial implementation was recorded on 2026-10-03 (`0caf272`); upstream
monitoring, licensing, source investigation, startup optimization, and its
acceptance documentation followed on 2026-10-04 (`83f429e`, `2b4798a`,
`daa50db`, `af965f7`). These dates describe repository history, not releases.

Changelog structure adapted from [siat-foreign-analysis CHANGELOG.md](https://github.com/joey-huckabee/siat-foreign-analysis/blob/main/CHANGELOG.md)
on 2026-10-04 (source blob `1cf05b7c5f4dabfea95ebf669b2afbef3787c3d3`).
Entries describe this project's implementation; the reference project's
release history is not imported.
