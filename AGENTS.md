# Praevis ThetaData

This file is the working guidance for Claude Code, Codex, and every other
coding agent in this repository. [CLAUDE.md](CLAUDE.md) only points here: change
this file, not that one.

## Project overview

Praevis ThetaData is an open-source, high-performance Rust replacement for the
ThetaData Python package, with a thin Python pass-through over it. It is derived
from the ThetaData Python 1.0.12 distribution and now tracks 1.0.17
(`source_version` in the proto manifest). It provides a reusable authentication
library, the `praevis-thetadata` CLI with optional (`auth --save`) Windows/Linux
native session persistence, typed requests for all 100 RPCs, exact Arrow
columns, a tier-aware scheduler, a Parquet sink and the Python package.
Authentication is the accepted requirements baseline. None of this is a claim of
complete Python compatibility: per-endpoint live evidence is in
[the coverage table](docs/endpoints/coverage.md).

The workspace uses Rust edition 2024, currently declares version
`0.1.0-alpha.2`, and sets `publish = ["crates-io"]`. Publication is manual
and owner-approved; never add `cargo publish` to a workflow. The Rust crate version and the tracked Python version
are different concepts. The canonical upstream compatibility version is
`source_version` in [the protocol manifest](crates/praevis-thetadata-proto/schema/manifest.json).
Do not maintain a second version literal in the scheduled PyPI checker.

[docs/ROADMAP.md](docs/ROADMAP.md) contains future work and the preserved
original review. [CHANGELOG.md](CHANGELOG.md) records delivered changes.
Update the appropriate document without turning the roadmap into a completion
log or presenting an unreleased change as a published release.

## The three things most likely to trip an agent up

1. **A persisted session is not a validated session.** Status and logout are
   local, synchronous operations. Status reports `validity: "not_checked"`;
   logout deletes the selected local record without revoking the server token.
   Only HTTP authentication starts a Tokio runtime. Preserve these contracts.
2. **The roadmap is not an approved requirements baseline.** Authentication is
   the current baseline; the market-data crates are foundations. Preserve the
   original review verbatim and trace Python compatibility evidence into
   requirements and ADRs as work is selected. Do not infer undocumented vendor
   behavior or discard unresolved research.

3. **Every GitHub Actions workflow stays green while you work.** This covers
   `CI` and `Auth performance` on every push and PR, the scheduled
   `Upstream Python compatibility` monitor on `main`, and
   `Synthetic EOD performance` whenever you dispatch it.
   - Check before you start and before every push or merge:
     `gh run list --branch main --limit 10` and `gh pr checks <number>`.
   - A red or pending check blocks merging. Never merge over a failure, and
     never use `--admin` or skip, disable, or weaken a check or test to get
     green.
   - A red `main`, including the scheduled upstream monitor, is the top
     priority. Stop feature work and fix it, or report it to the maintainer
     the same session.
   - Do not rerun a failed job until it passes. Find the root cause first. An
     intermittent failure is a bug in the test or the code: reproduce it, fix
     it in its own PR, and only then continue (see #34).
   - An upstream-monitor failure starts compatibility research. It does not
     authorize bumping the baseline (see *Upstream research and monitoring* below).

## Layout

```text
apps/praevis-thetadata-cli/      the CLI: credential discovery, auth/status/logout
crates/praevis-thetadata-auth/   HTTPS login, sessions, optional native persistence
crates/praevis-thetadata-proto/  checked-in descriptor and generated gRPC bindings
crates/praevis-thetadata-core/   exact prices, timestamps, values, tables
crates/praevis-thetadata-arrow/  columnar decoding into Arrow; Parquet sink
crates/praevis-thetadata-client/ gRPC, generated endpoints, query(), scheduler, jobs
crates/praevis-thetadata-py/     Python bindings (pyo3 abi3, maturin)
bench/python-baseline/           the only code allowed to run the vendor package
bench/p1/                        Python-side live checks and comparison runners
tools/                           repository checks, generators, research tooling
docs/                            requirements, ADRs, plans, research, performance
```

## Common commands

Use a current stable Rust toolchain. Windows needs the Visual Studio C++ build
tools; Linux needs a C/C++ compiler, make, and pkg-config. Normal Rust builds
require neither Python nor `protoc`.

```text
# Build only the authentication CLI
cargo build --locked -p praevis-thetadata-cli

# Rust checks used by CI
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

# Python package (inside a virtual environment with
# crates/praevis-thetadata-py/requirements-test.txt installed)
maturin build --release --locked -m crates/praevis-thetadata-py/Cargo.toml -o dist
python -m unittest discover -s crates/praevis-thetadata-py/tests

# Existing repository/tooling checks (Python is development tooling only)
python -m pip install -r tools/requirements.txt
python tools/check_repository.py
python tools/research_python.py --check
python tools/generate_endpoints.py --check   # typed endpoint requests up to date (regenerate without --check)
python -m unittest discover -s tools -p 'test_*.py'

# Windows native-store integration test, using a unique synthetic profile
cargo test --locked -p praevis-thetadata-cli --test persistence -- --ignored

# Linux equivalent, using a private D-Bus session and disposable keyring
bash tools/test-linux-persistence.sh

# CLI examples (requires credentials only for the first command)
cargo run --locked -p praevis-thetadata-cli -- auth
cargo run --locked -p praevis-thetadata-cli -- auth status --json
cargo run --locked -p praevis-thetadata-cli -- auth logout
```

The CLI examples use the user's selected real profile. For verification, use
the synthetic test fixtures rather than changing that profile. Linux native
tests additionally need `dbus-run-session`, `gnome-keyring-daemon`, and
`timeout`. Ordinary tests and CI do not require live ThetaData credentials.

The repository audit checks tracked/staged files, descriptor integrity,
requirement references, and local Markdown links. Stage new documentation
before running it so the new files are included. It is not a complete
feature-to-test traceability checker.

Optional real-service checks use the [manual Rust runner](docs/live-verification.md)
and [LIVE requirements](docs/requirements/live-verification.md). They require the
`live-tests` feature, explicit capture command/consent/environment/query and one
credential source (`--api-key-env` for process `THETADATA_API_KEY`, or a local
credential file). No discovery/fallback; auth itself still takes explicit inputs.
Keep live runs outside every CI workflow. CI only compile-checks the
feature-gated example (`cargo clippy --locked -p praevis-thetadata-client
--features live-tests --example live_eod -- -D warnings`); it never executes it.
PROD is selected for
initial integration; STAGE is an independent vendor connection, not an assumed
sandbox. Captures stay private under ignored `artifacts/live`; no automatic
fixture promotion or uploads. Test the runner locally with
`cargo test --locked -p praevis-thetadata-client --features live-tests --example live_eod`
and the P0/P1 measurement runner (`live_p0`: breakdown, limits, entitlement
map) with `--example live_p0`; CI runs the latter's loopback tests.
Follow the [release milestones](docs/releases.md) when proposing supported scope;
successful mocks or a single live query do not grant full EOD acceptance.

Use the [connection inventory/SVG](docs/connections.md) to distinguish direct
HTTPS/gRPC, future Terminal REST and future WebSocket events. Terminal adapters
and JAR supervision are proposed under ADR-0018, not implemented dependencies.
Use the [subscription matrix](docs/subscription-testing.md) for per-product/tier
evidence: expected PERMISSION_DENIED is a denial assertion, not data success.
Paid-account gaps remain not-run. The manual `probe-stock-quote` command performs
one bounded negative check; never sweep endpoints or parse remote text for tiers.
**One narrow exception, approved by the user on 2026-11-04:** the one-time FREE
entitlement map (`live_p0 entitlements`, P1 PR 2) sends one request per
endpoint family, paced, and stops on `RESOURCE_EXHAUSTED` or `UNAVAILABLE`. The
Coordinator runs it manually. Vendor error text stays in the private ignored
report under `artifacts/p0`: never committed, never sent. The Coordinator reads
it locally and sends only a classification per endpoint (entitled, tier-denied
with the tier named if stated, invalid-argument, no-data or other) plus the
gRPC code. `docs/endpoints/status.json` and every committed file hold codes and
classifications only; the generator rejects anything else. Re-running the map,
for example after a tier upgrade, needs the user's approval again. **The
options PRO map was approved by the user on 2026-11-04, as one run:** their
go-ahead to finish the option portion on PRO covers one run of
`live_p0 entitlements --set options-pro`, which sends every option RPC once
(not one per family), with the same pacing, stop rules and privacy rules. Any
further run needs approval again. **The stock, index, calendar and
interest-rate run was approved by the user on 2026-11-07, as one run:**
`live_p0 entitlements --set stock-index-rate` sends each of those 30 RPCs
once (the calendar per date and year), paced and sequential.
- **Same rules:** the same stop and privacy rules apply. The private report
  also keeps the calendar responses' decoded rows: calendar data, not market
  prices, which the user asked to see.
- **Flat files:** three flat files (index EOD, stock EOD and stock
  trade-quote) are bounded probes only, covered by this approval.
- **Gated:** the **stock quote flat file** is excluded unless
  `--include-stock-quote-flat-file` is given. If entitled, even a probe can
  start the vendor's generation of a very large file, so it needs explicit
  approval, like the option quote file.
- **Python side:** `bench/p1/stock_index_rate_checks.py` sends the same
  requests through the Python package.
- **Re-running** either side needs approval again.

**Flat files: probes are bounded, and full downloads are approved one file
at a time.**
- **Probes:** a flat-file probe or check reads at most one message of at
  most 1 MiB, through a client whose message limit refuses anything larger.
  It never pulls bulk data.
- **Full downloads:** approved by the Coordinator on 2026-11-05 for step G
  of the options phase: the **option open-interest, EOD and trade/quote**
  flat files, in that order. Each is downloaded to Parquet with memory
  measured (`live_job_memory --flat-file`). The Parquet output is private
  under the ignored `artifacts/` and deleted after its size is recorded.
- **Still needs approval:** the **option quote flat file (about 38 GB)**
  needs the user's explicit go-ahead, even for a probe, which starts the
  vendor's roughly 30-minute generation. Every other flat file (stock,
  index) needs approval too.

## Architecture

### Dependency boundaries

| Package | Responsibility | Boundary |
| --- | --- | --- |
| `crates/praevis-thetadata-auth` | HTTP authentication, credentials, in-memory sessions; optional `native-store` persistence | Reusable independently of market data; callers own async runtime |
| `apps/praevis-thetadata-cli` | Argument parsing, credential discovery, auth/status/logout, output and exit codes | Depends on auth, not the market-data client |
| `crates/praevis-thetadata-proto` | Generated protobuf types and gRPC bindings for 100 RPCs | Generated at build time from the checked-in descriptor |
| `crates/praevis-thetadata-core` | Prices, timestamps, values, tables | Transport-independent data types |
| `crates/praevis-thetadata-py` | Python bindings (pyo3 abi3, maturin wheel `praevis-thetadata`): a thin pass-through over Arrow | Depends on the client and arrow crates; never published as a crate. Builds without a Python interpreter (abi3); its tests are Python tests |
| `crates/praevis-thetadata-arrow` | Exact columnar decoding of `ResponseData` into coalesced Arrow batches; optional streaming Parquet sink | Core, proto and arrow-rs (`arrow-array`, `arrow-schema`; `parquet` with only `arrow` and `zstd` behind the `parquet` feature) only; keeps Arrow out of auth, core and proto. Not published yet |
| `crates/praevis-thetadata-client` | gRPC requests, ZSTD decoding, batch streaming | Composes auth, core, and proto; still a foundation |

Keep credential discovery in the CLI. The auth library accepts explicit
credentials and configuration; it must not silently read environment variables
or discover files on behalf of library callers. A client can reuse an existing
session through `ThetaClient::with_session` without authenticating again.

### Runtime ownership

Argument parsing, help/version, status, and logout run synchronously. Only the
HTTP authentication path constructs a Tokio `current_thread` runtime with I/O
and timers enabled. Construct the async HTTP client inside that runtime and
finish/drop it before saving the session and rendering output. The library
remains async; its callers choose their runtime.

This removes startup overhead for a sequential command. It is not a claim that
authentication benefits from a worker pool or that vendor latency decreases.
Do not change shared Tokio features just to optimize the CLI: other workspace
components have different runtime needs. See [ADR-0006](docs/adr/0006-cli-runtime-lifecycle.md).

## Requirements and traceability

Read [the requirements index](docs/README.md) and relevant ADRs before changing
behavior. The organization is deliberately hybrid:

| Location | Level | Purpose |
| --- | --- | --- |
| [docs/requirements/L1-auth.md](docs/requirements/L1-auth.md) | L1 | Product/user outcomes |
| [docs/requirements/L2-auth.md](docs/requirements/L2-auth.md) | L2 | System behavior, linked to L1 parents |
| [docs/requirements/L3-auth.md](docs/requirements/L3-auth.md) | L3 | Current cross-component auth obligations, implementation, and verification |
| [Stock EOD requirements](docs/requirements/L1-stock-eod.md) | L1/L2/L3 | Contracts for the typed stock EOD stream; every clause dispositioned in alpha.2 and now historical (see the lighter process below) |
| `crates/<name>/docs/requirements/` or `apps/<name>/docs/requirements/` | Future component L3 | Add when a component has substantial independent requirements |
| [docs/adr/](docs/adr/README.md) | Decisions | Context, alternatives, consequences, and requirement links |

Keep L1/L2 central. Keep cross-component L3 central too; do not create empty
requirement trees for every crate. Each requirement has one canonical
definition, a permanent ID, parent links, an owner, and verification evidence.
Move definitions without duplication or renumbering and update the central
index. Never reuse a retired ID or silently reinterpret an existing one.

**Lighter process (approved 2026-11-03, [plan F](docs/architecture/performance-plan.md#f-process-simplification)):**
- Generated endpoints get **one generated coverage table** (endpoint → typed
  builder → decoder test → live evidence status), not per-clause L1/L2/L3 and
  acceptance matrices.
- Write ADRs only for architecture-level choices; routine decisions are short
  notes in the plan or the PR.
- The stock-EOD matrices are historical. No requirement ID is deleted.
- Every performance claim cites a measured W-workload, not synthetic data
  alone.
- Kept unchanged: CI rule 3, exact values, decoder and lifecycle tests, live
  evidence and the D-B benchmark discipline.

Changes to credential precedence, persisted data, store identity, or lifecycle
behavior require corresponding ADR/requirement updates and meaningful tests.
Separate observed artifact behavior, documented vendor behavior, project
decisions, and unverified vendor contracts. Synthetic tests establish local
behavior; they do not prove subscription entitlements, token lifetime, or live
service compatibility.

## Conventions worth preserving

- **Persistence does not prove validity.** `status` reads only the native
  store and reports `validity: "not_checked"`. `logout` deletes only the
  selected local record. Do not invent token expiry, refresh, validation, or
  revocation contracts absent upstream evidence.
- **Sessions are in memory by default; persistence is opt-in** (ADR-0020).
  The auth library's `native-store` feature is off by default, and the CLI
  saves only with `auth --save`. When enabled, Windows uses Credential
  Manager and Linux uses a persistent Secret Service collection on an
  unlocked user D-Bus session. Missing/unavailable services are errors, not
  an absent session. Do not add plaintext fallbacks.
- **Credential order is observable and follows the vendor (ADR-0020):**
  `--creds-file`, `THETADATA_API_KEY`, the `THETADATA_EMAIL`/`THETADATA_PASSWORD`
  pair (a Rust-only extra; an incomplete pair falls through), then
  `THETADATA_CREDENTIALS_FILE` or `./creds.txt`. A selected file that is missing
  or invalid is an error. No `.env` loading and no secret-bearing flags.
  Preserve documented invalid-input behavior rather than silently falling back
  to a different identity.
- **Storage identity is stable.** The target is
  `praevis.thetadata.auth.v1:{PROD|STAGE}:{profile}`. Profiles are 1-64 ASCII
  letters, digits, underscores, or hyphens. Preserve environment/profile
  isolation and the versioned record. Store the session and account metadata,
  never the API key or password. Failed login leaves an existing record intact.
- **Keep secrets out of output and artifacts.** Never log credentials or
  session tokens. Do not commit credential files, `.env` files, local vendor
  archives/extractions, build output, or benchmark artifacts. Preserve
  `.gitignore` and repository audit exclusions.
- **Validate arguments before side effects.** Preserve exit codes: 0 success,
  1 operational/authentication failure, 2 invalid arguments, 3 missing stored
  session on status. Keep JSON stdout machine-readable and diagnostics on
  stderr; do not expose secrets through error bodies or debug formatting.
- **HTTPS is the default.** Plain HTTP requires the explicit local-test
  opt-in. Preserve disabled redirects and configured authentication timeouts;
  do not weaken transport behavior to make a fixture pass.
- **Generated protocol output is not a hand-maintained API inventory.** Change
  the source descriptor and provenance deliberately, then regenerate. Do not
  edit generated files under `target/` or infer complete Python behavior from
  the RPC names alone.
- **A byte limit is not a heap limit.** The client's current 64 MiB batch
  checks on raw helpers bound encoded/decompressed bytes, not all allocations.
  The experimental typed EOD path adds preflight counts/accounting, numeric
  batches and shared bounded offload; see [the resource audit and gaps](docs/performance/eod-resource-audit.md).
  Small NONE-frame inline decoding is opt-in; the portable default stays offload.
  Typed queries have a separate channel and finite active-stream admission;
  preserve the receive-credit relationship that prevents paused-stream stalls.
  The [acceptance matrix](docs/requirements/stock-eod-matrix.md) retains the
  current verification gaps; do not call them accepted. The null/price decode
  regressions were resolved in alpha.2 ([decoder record](docs/performance/eod-decoder-results.md)).
- **Never claim total memory is bounded.** Docs, READMEs and crate descriptions
  may say only that the client's own accounted allocations are bounded. The
  2026-11-03 profile ([record](docs/performance/eod-memory-profile.md)) showed
  transport buffering of about one encoded message per open stream outside the
  shared budget (deferred item 8).
  Do not claim complete heap/RSS or typed-slice acceptance from those checks.
  **One narrow addition, from the 2026-11-05 W5 evidence:** for the streaming
  job-to-Parquet path only, docs may say the **Rust heap** is bounded by
  configuration (read-ahead, batch limits, stream windows, one row group) plus
  Parquet footer metadata of about 13 KB per row group, which grows with the
  file. They must cite the [record](docs/architecture/options-phase.md#memory-bound-under-w5-planned)
  (heap peaks 103-182 MiB, 7 live runs). This does not extend to process RSS
  (interpreter, libraries, allocator), to collected frames, or to other
  paths.
- **Preserve platform and wire behavior.** Exercise Windows and Linux for
  changes to native persistence or process behavior. Preserve descriptor bytes
  and follow `.gitattributes`; write Markdown/source as UTF-8 with LF to avoid
  Windows newline churn. Do not rewrite the roadmap's verbatim review block,
  its table, or its historical citations.

## Performance work

Performance is the leading optimization objective within correctness, security,
resource and lifecycle contracts. Read [ADRs 0013-0016](docs/adr/README.md),
[the architecture review](docs/research/architecture-performance-review.md),
and [PERF requirements](docs/requirements/performance.md) before data-path work.
The primary path is `query()`/`job()`: exact coalesced Arrow batches for every
generated endpoint. The typed stock EOD stream (numeric DataBatch, with a Table
adapter that preserves RFC 3339 timestamps) remains as an existing interface.
New filtering/projection is optional native Rust behavior. The Python package is
a thin PyO3 pass-through outside the Rust core: Rust does the work, Arrow moves
without copying, and no Python object is created per row.

Every canonical requirement and ADR must have an explicit origin in
[the origin register](docs/requirements/origins.md): upstream-contract,
rust-enhancement or mixed. Preserve local policies independently of upstream
updates. Use [the impact template](docs/upstream-update-template.md), retain
separate compatibility/enhancement test evidence and measure data-path changes.
Origin coverage checks do not prove semantic noninterference or passing runtime
requirements. Do not silently change an enhancement to match a vendor update.

Use [the auth baseline](docs/performance/auth.md) and
[the CLI startup evaluation](docs/performance/cli-startup.md) for commands,
measurement conditions, and results. Build both revisions in release mode
with the same toolchain, isolated build directories, and recorded source and
executable hashes. The existing helpers are `tools/build_cli_benchmark.py` and
`tools/compare_cli_startup.py`; the latter compares actual Rust executables.

Preserve alternating measurement order, warmups, raw samples, p50/p95, host
context, fixture isolation, and native-store cleanup. A regression remains a
reported result. Do not add an arbitrary performance gate or omit inconvenient
samples. Windows showed modest median gains; Linux auth/save did not show a
reliable speedup, and WSL executable location materially affected results.
Do not claim universally improved latency.

The product core and benchmarked executables are Rust. Apart from the thin
Python package, Python orchestrates benchmarks, repository checks, upstream
monitoring, and vendor research.
Migration of routine tooling to a Rust `xtask` is a roadmap candidate, not an
implemented capability. Preserve comparison semantics and evidence when that
work is selected; do not mix it into unrelated changes.

## Upstream research and monitoring

[The source investigation](docs/research/upstream-source.md) records PyPI's
attested publishing repository `AXIOMXLLC/thetadata-python` and commit
`a7912e4dfa72a985b03d4d75516568b230aabb92`. At investigation time, both that
repository and the older metadata link `AXIOMXLLC/python_library` returned
public 404 responses. Do not substitute the unrelated archived legacy client
or claim the attested repository is publicly accessible. Use the versioned,
hashed PyPI wheel/sdist as the available source evidence.

The [scheduled PyPI monitor](docs/upstream-monitor.md) reads the manifest's
`source_version`, compares stable non-yanked releases using PEP 440, and fails
for a newer release or inability to verify. A failure initiates compatibility
research; it does not authorize silently bumping the baseline. The checker
does not install or execute the upstream package.

The [current 1.0.17 catalogue](docs/research/python-1.0.17-catalog.md)
inventories every source module/function, all 81 wrappers and 100 RPCs, protocol
fields, and explicit feature dispositions. The [frozen 1.0.12 catalogue](docs/research/python-1.0.12-catalog.md)
(80 wrappers, 82 RPCs) is preserved alongside it, and the
[1.0.13–1.0.17 records](docs/research/README.md) document every change.
`tools/research_python.py --check` verifies the current and frozen evidence,
additions-only compatibility and generated documentation without the vendor
archive or research dependencies; `tools/upstream_delta.py check` verifies the
generated change records. Archive regeneration additionally needs
`tools/research-requirements.txt` and the exact original wheel bytes. The local
ZIP/extraction were removed after verification; use the documented
[hash-verified recovery procedure](docs/research/upstream-source.md#recovering-the-removed-local-artifacts)
when regeneration is needed. Never import or execute the vendor package, with
one approved exception (2026-11-03, [plan C.1](docs/architecture/performance-plan.md#c1-vendor-python-baseline-the-approved-scoped-exception)).
- **Only** the Python baseline harness in `bench/python-baseline/` may run it.
  That harness is outside the Cargo workspace and excluded from packaging, CI,
  tests and `tools/`.
- It runs `thetadata==1.0.17` from a hash-pinned (`--require-hashes`)
  isolated virtual environment, and only **manually by the Coordinator on the
  user's machine**, with the user's key taken from the environment (never
  written to disk or logs).
- It outputs timings only; captures stay in ignored `artifacts/`.
- No other code may import the package. A newer version needs the same
  approval.

Work follows the approved
[performance architecture plan](docs/architecture/performance-plan.md)
(2026-11-03). P0 (measurement) and P1 (generated endpoints, Arrow columns, the
scheduler, Parquet and PyO3) are complete; see the
[P1 exit record](docs/performance/p1-exit-results.md). The current work is the
[options phase](docs/architecture/options-phase.md) on options PRO. ADR-0007
(stock EOD as the first slice) and the slice-by-slice roadmap are superseded. ADRs
0008-0012 remain valid in substance where the plan does not change them; they
define typed requests, outputs, stream behavior, verification and traceability
for the existing EOD code.
Extend coverage to implementation and verification as work is selected.
Preserve the original research and unresolved contracts. Mechanical coverage
cannot prove undocumented live-service behavior or approve market-data scope.

## Reference documents

- [Performance plan](docs/architecture/performance-plan.md), [options phase](docs/architecture/options-phase.md) and [P1 design](docs/architecture/p1-design.md): direction and current work.
- [Endpoint coverage](docs/endpoints/coverage.md): generated per-endpoint status and live evidence.
- [README.md](README.md): installation, CLI usage, credential precedence, persistence, and library reuse.
- [docs/README.md](docs/README.md): requirements organization and documentation index.
- [Architecture decisions](docs/adr/README.md): auth boundaries, wire contract, persistence, lifecycle, provenance, runtime.
- [Vendor research](docs/research/thetadata-1.0.12.md) and [artifact inventory](docs/research/vendor-1.0.12.json): current evidence to preserve and expand.
- [Protocol manifest](crates/praevis-thetadata-proto/schema/manifest.json): upstream baseline and descriptor provenance.
- [LICENSE](LICENSE), [NOTICE](NOTICE), and [upstream license](crates/praevis-thetadata-proto/LICENSE.upstream): Apache-2.0 licensing and attribution; preserve upstream material.
- [CI workflow](.github/workflows/ci.yml): current Windows/Linux verification commands.

## Git conventions

Do **not** add `Co-Authored-By: Claude ...`, `Co-Authored-By: Codex ...`,
`Claude-Session: ...`, or equivalent coding-agent attribution/session trailers
to commit messages on this repo, even if a tool's default instructions suggest
it. Commit messages are the human-authored record of intent; tool attribution
belongs in tool logs, not history.

The same applies to pull request bodies: no `Generated with Claude Code`,
`Generated with Codex`, or equivalent footer, and no session links.

Other conventions:

- Work on a branch, open a PR, let CI go green, then squash-merge and delete
  the branch. `main` stays clean. Follow the user's authorized scope for
  publishing and merging; this workflow does not independently authorize a
  merge or deployment.
- Commit messages explain **why**, not just what. State the problem the change
  solves and relevant alternatives rejected; omit invented rationale.
- `Co-authored-by: dependabot[bot] ...` trailers on dependency bumps are
  legitimate and stay.

Adapted from [GitHub-Metrics CLAUDE.md](https://github.com/joey-huckabee/GitHub-Metrics/blob/main/CLAUDE.md)
on 2026-10-04 (source blob `537a0815e74bf6d45ff75c386a1cd2901f46ed87`).
The structure and Git conventions are retained; commands, architecture,
contracts, and evidence are specific to this Rust workspace.

Adapted from [siat-foreign-analysis AGENTS.md](https://github.com/joey-huckabee/siat-foreign-analysis/blob/main/AGENTS.md)
on 2026-10-04 (source blob `74c96e5e8edf3335094c8ba2981f85dc695575c0`).
The `CLAUDE.md` pointer layout follows [aero1553](https://github.com/joey-huckabee/aero1553).
