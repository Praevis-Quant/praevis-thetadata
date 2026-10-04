# CLAUDE.md

This file provides working guidance for Claude Code, Codex, and other coding
agents in this repository. [AGENTS.md](AGENTS.md) is the entry point; this file
is the shared context rather than a second, agent-specific set of rules.

## Project overview

ThetaData Rust is a Rust workspace derived from the ThetaData Python 1.0.12
distribution. Its first deliverable is a reusable authentication library and
the `theta` CLI with Windows/Linux native session persistence. Authentication
is the accepted requirements baseline. The protobuf, core, and market-data
client crates are foundations, not a claim of complete Python compatibility or
verified live market-data access.

The workspace uses Rust edition 2024, currently declares version `0.1.0`, and
sets `publish = false`. The Rust crate version and the tracked Python version
are different concepts. The canonical upstream compatibility version is
`source_version` in [the protocol manifest](crates/thetadata-proto/schema/manifest.json).
Do not maintain a second version literal in the scheduled PyPI checker.

[docs/ROADMAP.md](docs/ROADMAP.md) contains future work and the preserved
original review. [CHANGELOG.md](CHANGELOG.md) records delivered changes.
Update the appropriate document without turning the roadmap into a completion
log or presenting an unreleased change as a published release.

## Common commands

Use a current stable Rust toolchain. Windows needs the Visual Studio C++ build
tools; Linux needs a C/C++ compiler, make, and pkg-config. Normal Rust builds
require neither Python nor `protoc`.

```text
# Build only the authentication CLI
cargo build --locked -p thetadata-cli

# Rust checks used by CI
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

# Existing repository/tooling checks (Python is development tooling only)
python -m pip install -r tools/requirements.txt
python tools/check_repository.py
python tools/research_python.py --check
python -m unittest discover -s tools -p 'test_*.py'

# Windows native-store integration test, using a unique synthetic profile
cargo test --locked -p thetadata-cli --test persistence -- --ignored

# Linux equivalent, using a private D-Bus session and disposable keyring
bash tools/test-linux-persistence.sh

# CLI examples (requires credentials only for the first command)
cargo run --locked -p thetadata-cli -- auth
cargo run --locked -p thetadata-cli -- auth status --json
cargo run --locked -p thetadata-cli -- auth logout
```

The CLI examples use the user's selected real profile. For verification, use
the synthetic test fixtures rather than changing that profile. Linux native
tests additionally need `dbus-run-session`, `gnome-keyring-daemon`, and
`timeout`. Ordinary tests and CI do not require live ThetaData credentials.

The repository audit checks tracked/staged files, descriptor integrity,
requirement references, and local Markdown links. Stage new documentation
before running it so the new files are included. It is not a complete
feature-to-test traceability checker.

## Architecture

### Dependency boundaries

| Package | Responsibility | Boundary |
| --- | --- | --- |
| `crates/thetadata-auth` | HTTP authentication, credentials, sessions, native persistence | Reusable independently of market data; callers own async runtime |
| `apps/thetadata-cli` | Argument parsing, credential discovery, auth/status/logout, output and exit codes | Depends on auth, not the market-data client |
| `crates/thetadata-proto` | Generated protobuf types and gRPC bindings for 82 RPCs | Generated at build time from the checked-in descriptor |
| `crates/thetadata-core` | Prices, timestamps, values, tables | Transport-independent data types |
| `crates/thetadata-client` | gRPC requests, ZSTD decoding, batch streaming | Composes auth, core, and proto; still a foundation |

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
| `crates/<name>/docs/requirements/` or `apps/<name>/docs/requirements/` | Future component L3 | Add when a component has substantial independent requirements |
| [docs/adr/](docs/adr/README.md) | Decisions | Context, alternatives, consequences, and requirement links |

Keep L1/L2 central. Keep cross-component L3 central too; do not create empty
requirement trees for every crate. Each requirement has one canonical
definition, a permanent ID, parent links, an owner, and verification evidence.
Move definitions without duplication or renumbering and update the central
index. Never reuse a retired ID or silently reinterpret an existing one.

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
- **Native persistence is the contract.** Windows uses Credential Manager;
  Linux uses a persistent Secret Service collection on an unlocked user
  D-Bus session. Missing/unavailable services are errors, not an absent
  session. Do not add plaintext or process-only fallbacks.
- **Storage identity is stable.** The target is
  `praevis.thetadata.auth.v1:{PROD|STAGE}:{profile}`. Profiles are 1-64 ASCII
  letters, digits, underscores, or hyphens. Preserve environment/profile
  isolation and the versioned record. Store the session and account metadata,
  never the API key or password. Failed login leaves an existing record intact.
- **Credential precedence is observable.** Explicit/environment credentials
  file, API key, email/password pair, then `./creds.txt`, in that order. The
  CLI does not automatically load `.env`. Preserve documented invalid-input
  behavior rather than silently falling back to a different identity.
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
  checks bound encoded/decompressed bytes, not all allocations after table
  decoding. Decoding still performs CPU work synchronously inside async batch
  consumption, and timestamps are formatted eagerly. These are documented
  performance candidates, not already-solved guarantees.
- **Preserve platform and wire behavior.** Exercise Windows and Linux for
  changes to native persistence or process behavior. Preserve descriptor bytes
  and follow `.gitattributes`; write Markdown/source as UTF-8 with LF to avoid
  Windows newline churn. Do not rewrite the roadmap's verbatim review block,
  its table, or its historical citations.

## Performance work

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

The product and benchmarked executables are Rust. Python currently orchestrates
benchmarks, repository checks, upstream monitoring, and vendor research.
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

The [deep analysis and coverage matrix](docs/research/python-1.0.12-catalog.md)
inventory every source module/function, all 80 wrappers and 82 RPCs, protocol
fields, and explicit feature dispositions. `tools/research_python.py --check`
verifies committed evidence and generated documentation without the vendor
archive or research dependencies. Archive regeneration additionally needs
`tools/research-requirements.txt` and the exact original wheel bytes. The local
ZIP/extraction were removed after verification; use the documented
[hash-verified recovery procedure](docs/research/upstream-source.md#recovering-the-removed-local-artifacts)
when regeneration is needed. Never import or execute the vendor package.

The next planned work in [the roadmap](docs/ROADMAP.md) is writing the stock
EOD slice's requirements from that evidence and [proposed ADRs 0007-0012](docs/adr/README.md).
Those ADRs define scope, typed requests, outputs, stream behavior, verification,
and traceability; they do not establish an implemented market-data baseline.
Extend coverage to implementation and verification as work is selected.
Preserve the original research and unresolved contracts. Mechanical coverage
cannot prove undocumented live-service behavior or approve market-data scope.

## Reference documents

- [README.md](README.md): installation, CLI usage, credential precedence, persistence, and library reuse.
- [docs/README.md](docs/README.md): requirements organization and documentation index.
- [Architecture decisions](docs/adr/README.md): auth boundaries, wire contract, persistence, lifecycle, provenance, runtime.
- [Vendor research](docs/research/thetadata-1.0.12.md) and [artifact inventory](docs/research/vendor-1.0.12.json): current evidence to preserve and expand.
- [Protocol manifest](crates/thetadata-proto/schema/manifest.json): upstream baseline and descriptor provenance.
- [LICENSE](LICENSE), [NOTICE](NOTICE), and [upstream license](crates/thetadata-proto/LICENSE.upstream): Apache-2.0 licensing and attribution; preserve upstream material.
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
