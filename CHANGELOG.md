# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versioned releases will follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Future work and undecided requirements are tracked in [docs/ROADMAP.md](docs/ROADMAP.md).

`0.1.0-alpha.1` is the first published prerelease (crates.io, tag
`v0.1.0-alpha.1`, 2026-11-02 UTC). Entries before it describe the work that
release contains. The Python compatibility baseline is separately
tracked in [the protocol manifest](crates/praevis-thetadata-proto/schema/manifest.json).

## [Unreleased]

### Changed

- **EOD benchmark policy and resource calibration** (PERF-L3-008,
  EOD-L3-023):
  - Decode p50 on the eight 10,000-row workloads gates performance
    acceptance, with tolerances approved on 2026-11-02: runners 1.05/0.95;
    the maintainer desktop 1.10/0.90 (provisional).
  - The harness gains `--extremes`, limit-sized text and header batches, and
    the workflow passes `run_args`.
  - The resource defaults are kept, with their effective ceilings documented:
    the 128 MiB accounted budget binds before the other limits combine.
    Limit-sized batches complete at the 16-stream cap on Windows and Linux.

- **Faster bounded EOD decoding.** Each cell is read by a hand-written
  decoder with prost-identical semantics, plus fast paths for two-byte
  null/boolean/small-integer cells, instead of building a prost `DataValue`.
  On the Windows and Linux x86 runners, decode is faster by 21-36% for nulls,
  8-16% for mixed and 6-12% for prices, against `main`. This recovers the
  recorded null/price regression; preflight, limits and errors are unchanged
  ([results](docs/performance/eod-decoder-results.md)).

### Added

- The **Synthetic EOD performance** workflow gains a before/after mode on
  Windows and Linux runners: clean worktree builds, alternating AB/BA rounds,
  native comparisons, raw reports, and a ratio summary
  (`tools/eod_compare_summary.py`). Use baseline = candidate for A/A noise.

- Publication record for `0.1.0-alpha.1`:
  - crates.io URLs and registry checksums, tag and commit;
  - Windows and Linux installs from crates.io;
  - Linux consumers built from crates.io only, with no native-store packages in
    the default auth/client trees, and the published CLI's lifecycle;
  - docs.rs status: four libraries built (auth with `native-store`), and the
    CLI is binary-only;
  - single-owner status, with a backup-owner recommendation.
- releases.md and ROADMAP mark alpha.1 as released and alpha.2 as the next
  target.
### Fixed

- The CLI package description and the README package table no longer imply
  that saving is automatic. Saving is opt-in with `auth --save`; the published
  alpha.1 description is corrected for the next release.

## [0.1.0-alpha.1] - 2026-11-02

First experimental prerelease on crates.io: `praevis-thetadata-auth`,
`-core`, `-proto`, `-client` and `-cli` (tag `v0.1.0-alpha.1`, commit
`0d3ea72`). It is independent and unofficial, APIs may change between alphas,
and it is not full Python parity. The Python compatibility baseline is 1.0.17.

### Changed

- **Publishing enabled for the experimental `0.1.0-alpha.1` prerelease**
  (`publish = ["crates-io"]`). Publication itself is manual and approved by
  the owner, in wave order (auth, core, proto, then client and CLI); no
  workflow publishes. The READMEs now describe crates.io installs with
  explicit prerelease versions and experimental wording.

- **Packaging (alpha.1, part B):**
  - The client archive is self-contained: its build script takes the
    descriptor from the proto crate (a build dependency) instead of a sibling
    path, and the checkout-only examples (EOD benchmark, `live_eod` runner)
    are excluded from the archive.
  - `tools/staging_registry.py` rehearses publication in a local `file://`
    registry: wave 1, then client and CLI resolved from it. It builds
    consumers outside the workspace, installs the CLI, and checks file lists,
    hashes, license files and forbidden inputs. A new "Packaging rehearsal" CI
    job runs it on Ubuntu and Windows.
  - Default auth and client consumers are checked to resolve no native-store
    packages. Nothing is published; `publish = false` is unchanged.

- **Packaging (alpha.1, part A):**
  - All five packages are now `0.1.0-alpha.1`, versioned together and
    independently of the Python baseline. Internal dependencies require exactly
    `=0.1.0-alpha.1`.
  - `rust-version` is 1.88, verified against the locked dependency graph.
  - Each package has public metadata and a README with supported platforms and
    an independent/experimental/no-full-parity note. Library READMEs double as
    crate documentation, and their examples run as doctests.
  - Every package archive includes `LICENSE` and `NOTICE` (proto also
    `LICENSE.upstream`), and copy drift is checked. NOTICE records the 1.0.17
    provenance.
  - `publish = false` is unchanged. No package was published.

- **CLI: Python-order credentials, and saving only with `auth --save`**
  ([ADR-0020](docs/adr/0020-in-memory-sessions-and-optional-persistence.md);
  AUTH-L3-025/026 replace the retired AUTH-L3-001/010).
  - Credential order is now `--creds-file`, `THETADATA_API_KEY`, a complete
    `THETADATA_EMAIL`/`THETADATA_PASSWORD` pair, then `THETADATA_CREDENTIALS_FILE`
    or `./creds.txt`, matching the vendor package. `THETADATA_CREDENTIALS_FILE`
    is no longer the highest precedence.
  - An incomplete or blank email/password pair falls through instead of
    failing. A selected file that fails still errors.
  - `praevis-thetadata auth` now logs in and reports the account **without
    saving**; `auth --save` saves as before. `--save` with `status` or
    `logout` is an argument error (exit 2).
  - Status, logout, the storage identity and the v1 record are unchanged, so
    records saved by earlier versions keep working. A failed login still
    leaves an existing record intact.
  - **Migration:** scripts that relied on `auth` saving must add `--save`, and
    scripts that relied on `THETADATA_CREDENTIALS_FILE` overriding
    `THETADATA_API_KEY` must unset the key or pass `--creds-file`. The
    benchmark tools pass `--save`; the before/after comparator detects
    whether each binary supports it.

- **Auth library: sessions are in memory by default; native persistence is
  optional** ([ADR-0020](docs/adr/0020-in-memory-sessions-and-optional-persistence.md),
  superseding ADR-0003). `SessionStore`, `StoredSession` and `StoreError`, and
  the `keyring` dependencies behind them, now require the `native-store` Cargo
  feature, which is off by default. A default auth build, and therefore the
  market-data client, no longer pulls in keyring, D-Bus or Windows credential
  packages for any target. With the feature enabled, the storage identity and
  v1 record are unchanged, so existing saved sessions keep loading. The CLI
  enables the feature, and its behavior is unchanged in this step. **Migration
  for library users of `SessionStore`:** add `features = ["native-store"]`. CI
  now tests auth with the feature off and on and checks the auth and client
  dependency trees.

- Documentation for the 1.0.17 baseline. The Python performance comparator is
  pinned to 1.0.17 (D4); its EOD wrapper and conversion code are unchanged
  from 1.0.12. The release plan records that the baseline update landed before
  alpha.1 (D5) and that the proto now has 100 RPCs. The ROADMAP lists the
  remaining follow-ups: futures research, explicit presence for the new
  optional fields, ADR-0019 supersession, and a possible ~1–2% Table-decode
  shift on mixed uncompressed fixtures that has not been measured on Windows.
  CLAUDE.md, the crates.io plan and the subscription matrix are updated, and
  the subscription matrix notes the documented service-side changes.

- **Timestamps with sub-millisecond or unknown precision are now rejected
  instead of silently truncated** ([ADR-0019](docs/adr/0019-timestamp-precision.md),
  EOD-L3-030). Python 1.0.16 added `ZonedDateTime.nanos`/`precision` with
  undefined semantics. Typed EOD streams end with `EodError::Unsupported`;
  batches already delivered stay partial. The raw table helper returns
  `DataError::TimestampPrecision`. Millisecond values, which is every
  response observed so far, are unchanged. The new core helpers are
  `check_timestamp_precision` and `timestamp_with_precision`; the existing
  `timestamp()` and `Timestamp::from_wire()` are unchanged.

- **Python compatibility baseline 1.0.12 → 1.0.17.** The descriptor
  (`35572458…5a58`) and manifest were regenerated from the hash-verified
  1.0.17 wheel. `praevis-thetadata-proto` now binds 100 RPCs instead of 82
  (17 futures RPCs and `GetStockFlatFileQuote` were added) and 212 messages.
  **Source-breaking for raw proto struct literals:** `Endpoints::ZonedDateTime`
  gained `nanos` and `precision`, so construct it with `..Default::default()`.
  Runtime decoding is unchanged: both decoders still ignore those fields,
  pending the ADR-0019 policy. The 1.0.17 inventory, catalogue and dispositions
  ledger are now the checked baseline, and the 1.0.12 set stays frozen. Futures
  RPCs and the new `stock_flat_file_quote` are `unresolved`, like the other
  flat files. The upstream monitor reports tracked = latest, 1.0.17.
  Auth requests, credentials and persisted sessions are unchanged.

- Research tooling is now version-agnostic. `research_python.py` takes its
  paths from the manifest `source_version`. It checks the frozen 1.0.12
  evidence alongside any newer baseline, and it fails if upstream removes,
  renumbers or retypes anything the frozen evidence recorded. One static
  inventory builder is now shared with `upstream_delta.py`, and it still
  regenerates the frozen 1.0.12 inventory byte-for-byte.
  `extract_protocol.py` reads a hash-verified wheel directly and takes the
  version from its METADATA. Futures RPCs get their own feature key.
  The baseline is still 1.0.12.

### Added

- **Alpha.1 PROD exit gate recorded (2026-11-01, Windows, FREE account):**
  - CLI auth lifecycle: `auth` stores nothing, `--save` persists the unchanged
    v1 record, then status/logout.
  - AAPL EOD capture `prod-aapl-alpha1-002` on the 1.0.17 baseline: one
    232-byte ZSTD frame. Numeric, Table and raw replay match each other and
    the 2026-10-04 capture's exact-value digest, and the real timestamps pass
    the ADR-0019 precision check.
  - FREE quote denial reproduced (code 7 at headers).
  - All five crates.io names returned 404 (not reserved).
- CI compile-checks the feature-gated `live_eod` runner with clippy, so a
  baseline bump cannot silently break it again (it broke after #17 and was
  fixed in #24). CI still never runs live checks.

- Constructed sixteen-column stock EOD regression coverage informed by the
  reviewed live schema, with invented values and independent exact-value checks
  across raw/numeric/Table interfaces and NONE/ZSTD. Added a revision-pinned
  ThetaDataDx watchlist and independent project naming brief. The CLI now has
  package-specific metadata and README; publication awaits name selection and
  the remaining archive/consumer checks.

- Connection inventory and editable SVG distinguishing direct HTTPS/gRPC from
  proposed Theta Terminal v3 REST/WebSocket adapters, with ADR-0018 and explicit
  enhancement requirements. Added a per-product/subscription manual test matrix
  and a bounded stock quote probe that passes only on the expected pre-data
  permission denial; other errors or unexpected data fail the assertion.
  A manual PROD AAPL probe reproduced the FREE account's gRPC code 7 at headers
  with zero batches. Paid-tier and Terminal verification remain untested.

- Explicit `--api-key-env` support for the manual live runner, reading only
  process `THETADATA_API_KEY` without requiring a file or exposing its value.
  The first authorized PROD AAPL single-day EOD check passed: live numeric/Table
  results and Windows/Linux offline raw/numeric/Table replay matched the private
  captured response. Broader service and performance acceptance remain open.

- Concrete versioned release milestones and exit gates, plus an optional
  feature-gated Rust live EOD capture/replay runner. Manual account checks use
  explicit credential files and retain private response/provenance artifacts;
  offline replay compares raw, numeric and Table interfaces. CI workflows stay
  unchanged and credential-free. Live observations are recorded separately.

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
- Rust workspace with an independent `praevis-thetadata-auth` library and `theta`
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

- Renamed the project/repository to `praevis-thetadata`, the five Cargo packages
  and member directories to `praevis-thetadata-*`, Rust imports to
  `praevis_thetadata_*`, and the CLI executable from `theta` to `praevis-thetadata`.
  Updated documentation, tooling, workflow package targets and repository links;
  added the explicit independent-maintenance/non-affiliation statement. Credential
  variables, native session-store identity, wire protocol and upstream evidence
  remain unchanged. Older benchmark checkouts remain supported by the build helper.

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
  [the upstream Apache-2.0 license](crates/praevis-thetadata-proto/LICENSE.upstream).

The initial implementation was recorded on 2026-10-03 (`0caf272`); upstream
monitoring, licensing, source investigation, startup optimization, and its
acceptance documentation followed on 2026-10-04 (`83f429e`, `2b4798a`,
`daa50db`, `af965f7`). These dates describe repository history, not releases.

Changelog structure adapted from [siat-foreign-analysis CHANGELOG.md](https://github.com/joey-huckabee/siat-foreign-analysis/blob/main/CHANGELOG.md)
on 2026-10-04 (source blob `1cf05b7c5f4dabfea95ebf669b2afbef3787c3d3`).
Entries describe this project's implementation; the reference project's
release history is not imported.
