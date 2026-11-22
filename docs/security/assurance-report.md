# Security assurance report: praevis-thetadata

**For:** a security reviewer or approving authority deciding whether to
approve praevis-thetadata for use, including on a **disconnected or
restricted network**.
**As of:** 2026-11-18, `main` of
[Praevis-Quant/praevis-thetadata](https://github.com/Praevis-Quant/praevis-thetadata).
**Status of claims:** every statement below is either **in place** (with a
link to the evidence in [the appendix](#appendix-claims-and-evidence)) or
marked **planned**. Nothing planned is presented as done.

## 1. Executive summary

praevis-thetadata is an independent, unofficial client library for
ThetaData's market-data service: Rust crates, a Python package and a
command-line tool. It is written in Rust, has no telemetry or update
mechanism, opens no listening ports, and makes outbound connections only to
the ThetaData endpoints in [section 3](#3-network-behaviour) (plus, in the
experimental real-time crate, a loopback connection to a locally run Theta
Terminal).

**What makes it trustworthy:**
- **Memory safety:** safe Rust throughout. Two audited calls into zstd's C
  library are the only `unsafe` code in what is shipped, and every crate
  forbids new unsafe code; CI enforces it.
- **Supply chain:** every release artifact is built in an isolated GitHub
  Actions workflow from a commit on `main`, merged by pull request after 17
  required checks passed (no approving review is required: a
  single-maintainer project), with a signed SLSA v1.0
  Build Level 3 provenance attestation, a CycloneDX SBOM and a SHA-256
  checksum. From `0.1.5` that includes the crates: **every published
  artifact has verifiable provenance**, and the `.crate` file crates.io
  serves is checked to be byte-identical to the attested one. Packages
  reach crates.io and PyPI only by trusted publishing, after the
  maintainer's approval, with no long-lived tokens anywhere.
- **Continuous checks:** RustSec advisories, licences and sources
  (cargo-deny), CodeQL, a workflow security linter (zizmor), OpenSSF
  Scorecard, fuzzing of every decoder of untrusted input, and a test suite
  across Windows, Linux and macOS.
- **Vulnerability handling:** private reporting, and published fix targets
  by severity ([section 7](#7-vulnerability-management)).

**What it cannot do disconnected:** fetching market data requires reaching
ThetaData's service. In a fully air-gapped enclave the software can be
installed and verified, but it cannot authenticate or fetch data; data
fetched elsewhere and transferred in (as the Parquet files it writes) is
processed with standard tools such as pyarrow, Polars or pandas. A restricted network that
allows egress to the endpoints in section 3 supports full use.

**Main residual risks** ([section 9](#9-residual-risks-and-limitations)):
it is a single-maintainer project; it depends on ThetaData's service and
on ~200 third-party crates; release executables are not code-signed by an
OS vendor (Windows Authenticode, Apple notarisation); and the real-time
crate is experimental.

## 2. Scope: the artifacts under review

**Version:** the first release built and published by the pipeline
described here is **`0.1.3`**, published on 2026-11-16 (tag `v0.1.3` on
`f694035`); its [publication record](../crates-io-release-plan.md#publication-record-013)
lists the checksums, provenance and SBOM checks run after publication. The
current release is **`0.1.4`** (2026-11-19, tag `v0.1.4` on `13e029a`,
[record](../crates-io-release-plan.md#publication-record-014)), which replaces
`chrono-tz` with `jiff` and is otherwise the same; review the latest. Earlier
versions (`0.1.0`–`0.1.2`) were published by hand and **have no build
provenance or SBOM**; review `0.1.3` or later.

| Artifact | Where it is obtained | Names |
| --- | --- | --- |
| Rust crates (7) | [crates.io](https://crates.io/search?q=praevis-thetadata), and from `0.1.5` the GitHub release | `praevis-thetadata-proto`, `-core`, `-auth`, `-arrow`, `-client`, `-cli`, `-stream`; on the release as `praevis-thetadata-<crate>-<version>.crate`, the same files crates.io serves |
| Python package | [PyPI](https://pypi.org/project/praevis-thetadata/) and the GitHub release | `praevis_thetadata-<version>-cp310-abi3-<platform>.whl`, 8 platforms (Linux glibc and musl, Windows, macOS; x86_64 and arm64) |
| CLI executables | the GitHub release | `praevis-thetadata-cli-<version>-<target>.zip`/`.tar.gz`, 8 targets |
| SBOMs (CycloneDX 1.5) | the GitHub release | `praevis-thetadata-<crate>-<version>.cdx.json` (7), `praevis-thetadata-wheel-<version>.cdx.json` |
| Checksums and signed attestations | the GitHub release | `SHA256SUMS`; `praevis-thetadata-<version>.intoto.jsonl` (provenance; up to `0.1.4` named `.provenance.sigstore.jsonl`); `praevis-thetadata-cli-<version>.sbom.sigstore.jsonl`; `praevis-thetadata-wheel-<version>.sbom.sigstore.jsonl` |

**Supported surface:** auth, core, arrow, the client's query and job API,
the CLI and the Python package. The proto crate is unstable and the stream
(real-time) crate is **experimental**: a site may exclude it
([releases](../releases.md)).

**How to obtain and verify offline:** [section 8](#8-installing-and-verifying-in-a-disconnected-enclave).

## 3. Network behaviour

**Every connection the shipped code makes:**

| # | Destination | Port | Direction | Protocol | Encryption | When | Component |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | `nexus-api.thetadata.us` | 443 | outbound | HTTPS (HTTP/1.1 or 2), one POST to `/identity/terminal/auth_user` | TLS 1.2/1.3 (rustls), server certificate checked against the bundled Mozilla root set (webpki-roots) | Each login | auth crate, CLI, Python |
| 2 | `mdds-01.thetadata.us` (production) | 443 | outbound | gRPC over HTTP/2 | TLS 1.2/1.3 (rustls), server certificate checked against the **operating system's** trust store | Each query | client crate, CLI, Python |
| 2a | `mdds-stage.thetadata.us` (only if the caller selects the STAGE environment) | 443 | outbound | gRPC over HTTP/2 | as row 2 | as row 2 | as row 2 |
| 3 | `127.0.0.1` or `localhost` | 25520 (default) | outbound, **loopback only** | WebSocket (`ws://`), JSON events | none (local process to local process) | Only when the experimental stream crate is used | stream crate, connecting to a locally run Theta Terminal |

Plus **DNS lookups** for the host names above, through the operating
system's resolver, and on Linux, when the optional session store is used,
**local IPC** with the user's D-Bus session bus (the Secret Service).

**Nothing else:**
- **No telemetry, analytics, crash reporting, licence checks or update
  checks.** No code path contacts any other host.
- **No listening ports.** No shipped component accepts connections. (The
  repository's test emulator listens on loopback; it is `publish = false`
  and is not part of any artifact.)
- **No redirects followed** by the auth client (redirect policy: none).
- **HTTPS is required** for rows 1 and 2. Plain HTTP is accepted only when
  the caller explicitly sets `allow_insecure`, which exists for local
  testing; the Python package accepts other endpoints only on loopback
  addresses.
- **The stream crate refuses any host but `127.0.0.1` or `localhost`.**
- **Flat-file manifests** (download links the service can return) are
  refused as unsupported; the library never downloads from them.

**For a firewall or proxy rule:**
- **Egress allow-list:** TCP 443 to `nexus-api.thetadata.us` and
  `mdds-01.thetadata.us` (and `mdds-stage.thetadata.us` only if STAGE is
  used). These are the vendor's names; resolve them, or allow by name on a
  proxy that supports it.
- **Proxies:** the login (row 1) uses `reqwest`, which honours the
  `HTTPS_PROXY` / `HTTP_PROXY` / `NO_PROXY` environment variables. The gRPC
  connection (row 2) has **no proxy support**: it needs direct TCP 443
  egress, or a transparent forwarder.
- **TLS inspection:** row 2 trusts the OS store, so an enterprise
  inspection CA installed there is accepted. Row 1 trusts only the bundled
  Mozilla roots, so **TLS inspection of `nexus-api.thetadata.us` breaks
  login**; exempt that host from inspection.

## 4. Data handling

| Data | Where it lives | Notes |
| --- | --- | --- |
| ThetaData credentials (API key, or email and password) | Supplied by the caller: argument, environment variable (`THETADATA_API_KEY`, or `THETADATA_EMAIL`/`THETADATA_PASSWORD`), or a credentials file the caller names | Sent only to row 1, over TLS. **Never written to disk or the OS store by this software.** `Debug` output prints `Credentials([REDACTED])` |
| Session (a session ID and the account's e-mail and subscription fields) | Memory by default | `Debug` prints `Session([REDACTED])` |
| Saved session (opt-in: CLI `auth --save`, or the `native-store` feature) | The OS credential store: Windows Credential Manager, Linux Secret Service, macOS Keychain | Scoped by profile and environment; `logout` deletes it. Never the credentials themselves |
| Market data | Memory, as Arrow batches | Written to disk only where the caller asks (the Parquet sink, to the caller's path). No cache, no temporary files |
| Logs | None of its own | Errors are typed codes; vendor error text and secrets are kept out of them, and a test captures dependency log output to check that no secret appears in it |

## 5. Supply-chain evidence

| Control | Status | What it gives |
| --- | --- | --- |
| **SLSA v1.0 Build Level 3 provenance** for every release artifact and SBOM | In place from `0.1.3`; the crates from `0.1.5` | A signed statement, verifiable offline, that the file was built by `build-release.yml` (an isolated reusable workflow) in this repository, from a named commit and tag, on GitHub-hosted runners |
| **Reproducible, verified crates** | In place from `0.1.5` | The seven `.crate` files are packaged in the isolated build and again on a second runner and architecture, and must be byte-identical. Publishing packages them a third time and must match before anything is uploaded. Then each crate is downloaded back from crates.io and must equal the attested file, or the release stops before the GitHub release ([releasing](../releasing.md)) |
| **CycloneDX SBOMs** | In place from `0.1.3` | The full Rust dependency graph of each crate, the executables and the wheels, attested to the artifacts they describe. Each wheel also carries its own SBOM inside it (`.dist-info/sboms/`, written by maturin), so it travels with the wheel |
| **SHA-256 checksums** (`SHA256SUMS`) | In place from `0.1.3` | Integrity of every file after transfer |
| **Trusted publishing** to crates.io and PyPI | In place from `0.1.3` | Publication only from `trusted-publishing.yml`, through OIDC, after the maintainer approves the `crates-io` environment and then (only once crates.io succeeded) the `pypi` environment. Both environments admit only `v*` tags, and administrators cannot bypass them (`can_admins_bypass: false`). **No long-lived registry tokens exist**; crates.io refuses token publishing for these crates. PyPI files carry PyPI attestations |
| **Release gate** | In place | A tag must match the version, be on `main`, with CI green |
| **Release tags** | In place | Ruleset "release tags creation" (id 27825772): only the repository admin role can create a `v*` tag. Ruleset "release tags immutable" (id 27324055): no one, admins included, can delete, move or force-update one (no bypass actors) |
| **Locked dependencies** (`Cargo.lock`; hash-pinned Python build requirements) | In place | Builds use exactly the reviewed versions (`--locked`) |
| **cargo-deny** | In place | RustSec advisories (vulnerable, unsound, unmaintained, yanked) against every crate in the graph, direct or transitive, on each of the eight released platforms; licences (permissive allow-list) and sources (crates.io only) fail CI; weekly as well |
| **Advisories fixed** | In place | Nine RustSec advisories found by the first cargo-deny run (rustls, rustls-webpki, webpki-roots, tokio, pulldown-cmark) were fixed before `0.1.3` |
| **Pinned GitHub Actions** | In place | Every action is pinned by full commit SHA; zizmor checks for unpinned, impostor or vulnerable actions |
| **Dependabot** | In place | Weekly grouped update proposals (7-day cooldown) and immediate security updates |
| **Branch protection** | In place | Ruleset "main protection" (id 27326370): pull requests required, 17 required checks, linear history, no force-push or deletion, no bypass actors. **0 required approving reviews** |
| **Separation of duties** | **Process, not a technical control** | The development agents work under the maintainer's own GitHub account (the maintainer's decision; no bot identity). GitHub enforces that the account approves each publishing environment and that release tags cannot be moved, but it cannot tell the agents' actions from the maintainer's. That agents never tag, publish or approve rests on process: the written rules ([AGENTS.md](../../AGENTS.md), [releasing](../releasing.md)) and the maintainer's review of each run |

## 6. Code assurance

| Control | Status | Evidence |
| --- | --- | --- |
| **Language** | In place | Rust (memory-safe) for all shipped code; the Python package is a thin layer over the Rust core |
| **Unsafe code** | In place | Audited: two calls to `ZSTD_getErrorCode` (a numeric lookup, no pointers), each with a SAFETY note. Every crate root forbids or denies `unsafe`, checked in CI |
| **Fuzzing** | In place | Four cargo-fuzz targets over every decoder of untrusted input (response envelopes, zstd frames, Arrow column decoding, the client's EOD decoders, Terminal events), with AddressSanitizer on ClusterFuzzLite: 5 minutes per pull request, 1 hour weekly. Synthetic seeds only. The C libraries underneath (zstd, ring) are built without instrumentation |
| **Static analysis** | In place | CodeQL (default setup, extended suite) on Rust, Python and Actions; clippy with warnings as errors |
| **Workflow security** | In place | zizmor on every workflow and the Dependabot configuration; findings fail CI |
| **OpenSSF Scorecard** | In place | Weekly and on `main`; results published and in code scanning |
| **Tests** | In place | Unit, integration and end-to-end tests against an emulator of the service, on Windows and Linux (x86_64), the Python package on 3.10–3.13, a macOS job, a native-store round trip on all three OSes, and a packaging rehearsal that installs the packaged crates outside the workspace. Release builds of all 16 artifacts are smoke-tested on their own platforms |
| **TLS** | In place | rustls only (no OpenSSL); HTTPS required by default; a test with both rustls crypto providers present checks that the client still connects |
| **OpenSSF Best Practices badge** | **Planned** | The questionnaire answers are prepared ([answers](openssf-best-practices.md)); registration is pending |

## 7. Vulnerability management

Policy: [SECURITY.md](../../SECURITY.md). Reports go through GitHub private
vulnerability reporting. These are **targets for a single-maintainer
project**:

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

- **Disclosure** is coordinated: private until the fix is released, and at
  most 90 days.
- **Dependencies:** a vulnerability in a dependency uses the score from
  that dependency's advisory and the same targets; RustSec advisories fail
  CI, and Dependabot proposes the fix.
- These targets meet the OpenSSF Best Practices "passing" criteria
  (a response within 14 days; no medium or higher publicly known
  vulnerability unpatched for more than 60 days).
- **Fixed versions** are new patch releases; a vulnerable version is yanked
  where that protects users. A version is never replaced in place.

## 8. Installing and verifying in a disconnected enclave

**On a connected staging machine:**
1. **Download** from the GitHub release for the version under review: the
   artifacts you need, the matching SBOMs, `SHA256SUMS`, and the three
   signed bundles (`*.intoto.jsonl` and `*.sbom.sigstore.jsonl`).
2. **Fetch the Sigstore trusted root** through GitHub's CLI:
   `gh attestation trusted-root > trusted_root.jsonl`.
3. **Optionally, the Rust sources:** for a build from source, also run
   `cargo vendor` in a checkout of the release tag, and keep the `vendor/`
   directory and the `.cargo/config.toml` it prints.
4. **Optionally, Python dependencies:** the package has **no required
   dependencies**. For DataFrame output, download `pyarrow` (and `polars`
   or `pandas`) wheels for the enclave's platform, and verify them by your
   usual process.

**Transfer** those files into the enclave by your approved process.

**In the enclave:**
1. **Checksums:**
   `sha256sum --check --ignore-missing SHA256SUMS` (Linux) or
   `shasum -a 256 --check --ignore-missing SHA256SUMS` (macOS).
2. **Provenance**, for each artifact (GitHub's `gh` CLI, no network
   needed with `--bundle`):

   ```text
   gh attestation verify <artifact> \
     --bundle praevis-thetadata-<version>.intoto.jsonl \
     --custom-trusted-root trusted_root.jsonl \
     --repo Praevis-Quant/praevis-thetadata \
     --signer-workflow Praevis-Quant/praevis-thetadata/.github/workflows/build-release.yml \
     --source-ref refs/tags/v<version>
   ```

   This checks the signature, the certificate chain to the trusted root,
   the transparency-log inclusion recorded in the bundle, the source
   repository, the build workflow and the release tag. Dry runs also
   attest, from a branch; `--source-ref` accepts only the tag's run.
3. **SBOM attestation** (optional): the same command (with
   `--source-ref`) with the `*.sbom.sigstore.jsonl` bundle and
   `--predicate-type https://cyclonedx.org/bom`.
4. **Install:**
   - **CLI:** extract the archive; it holds one executable and its licence
     files. The musl Linux builds are fully static.
   - **Python:** `pip install --no-index --find-links <dir> praevis-thetadata`
     (and the optional DataFrame wheels from the same directory).
   - **Rust, from vendored sources:** `cargo build --release --locked
     --offline` with the vendored configuration.
5. **Run:** with egress to section 3's endpoints, as normal. With none,
   only offline processing of transferred data is possible (below).

**Fully air-gapped use:** the software installs and verifies, and
`describe()` plans requests without sending them, but it cannot
authenticate or fetch data: both need ThetaData's service. The library has
no offline data source of its own. Data fetched on a connected system and
written to Parquet (its Parquet sink) can be transferred in and read with
standard tools (pyarrow, Polars, pandas, or Arrow and Parquet readers in
Rust).

## 9. Residual risks and limitations

- **Single maintainer.** Review depth and response capacity are limited;
  the targets in section 7 are targets, not guarantees.
- **An unmaintained dependency.** `chrono` (dates) is flagged unmaintained
  by RustSec; it has no known vulnerability. Its advisory is accepted until
  2027-02-18 with a recorded reason, enforced by a repository check, and any
  vulnerability advisory against it still fails CI. It is part of the public
  API in `0.1.x` and is also required by arrow-rs and pyo3; moving the public
  dates to `jiff` is [planned for 0.2.0](../ROADMAP.md#chrono-is-unmaintained-assurance-accepted-until-2027-02-18).
  (`chrono-tz`, flagged with it, was replaced by `jiff` in `0.1.4`.)
- **Third-party code.** About 200 crates (in the client's normal build)
  are trusted transitively, including C code in zstd (decompression) and
  ring (cryptography). They are locked, audited against RustSec, licence
  checked, and listed in the SBOMs; they are not independently reviewed by
  this project.
- **The vendor.** The service, its availability, its data and its
  certificates are ThetaData's. A compromised vendor endpoint could send
  hostile data; the decoders are bounded and fuzzed against that, but the
  data's correctness is the vendor's.
- **Not bit-for-bit reproducible (yet).** Two builds of the same commit
  (the `0.1.3` dry run and tag build) produced different checksums for
  every file. The Linux and macOS executables were identical between them;
  the differences are timestamps and serial numbers in archives, wheels and
  SBOMs, and the Windows linker's timestamp. So a reviewer cannot yet
  rebuild a release and compare checksums: the evidence that a file came
  from the tagged source is its signed SLSA provenance, not an independent
  rebuild. **The crates are the exception:** `cargo package` of the tag
  reproduces each `.crate` byte for byte, checked in every release run and
  independently for `0.1.4` (all seven crates on crates.io). Making builds reproducible is
  [planned](../ROADMAP.md#reproducible-release-builds-assurance-after-013).
- **No technical separation between the agents and the maintainer.** The
  agents act under the maintainer's GitHub account, so the environment
  approvals and the admin-only tag creation prove that the account acted,
  not which of them did. No approving code review is required for merges
  either (0 approvals): changes are gated by the 17 required checks.
- **GitHub releases are not immutable.** A published release's assets
  could be replaced by an account with write access to releases; the
  checksums and provenance would expose a replaced file, but nothing
  prevents it. Enabling GitHub's immutable releases is
  [recommended](../releasing.md#immutable-releases) and **planned**.
- **No OS code signing.** Executables are not Authenticode-signed or Apple
  notarised; their integrity rests on `SHA256SUMS` and the provenance
  attestation. macOS may prompt before the first run of a downloaded
  executable.
- **Experimental real-time crate.** The stream crate is outside the
  supported surface; it speaks unencrypted WebSocket to a local Terminal on
  loopback only.
- **Proxy and TLS inspection constraints** (section 3): no proxy for gRPC;
  no inspection of the login host.
- **Earlier releases** (`0.1.0`–`0.1.2`) have no provenance or SBOM, and
  the crates of `0.1.3` and `0.1.4` have no provenance of their own (their
  executables, wheels and SBOMs do).
- **Fuzzing coverage** instruments Rust code only; the C libraries rely on
  their own projects' fuzzing.
- **OpenSSF Best Practices badge:** planned, not yet held.

## Appendix: claims and evidence

| Claim | Evidence |
| --- | --- |
| Endpoints and ports | [auth URL](../../crates/praevis-thetadata-auth/src/lib.rs), [market-data hosts](../../crates/praevis-thetadata-client/src/lib.rs), [stream loopback rule](../../crates/praevis-thetadata-stream/src/lib.rs), [connection inventory](../connections.md) |
| Redirects off, HTTPS required | [auth client builder](../../crates/praevis-thetadata-auth/src/lib.rs) |
| Secrets redacted and not logged | [auth tests](../../crates/praevis-thetadata-auth/src/tests.rs), [dependency log capture test](../../crates/praevis-thetadata-client/tests/eod_logs.rs) |
| Session store per OS | [store](../../crates/praevis-thetadata-auth/src/store.rs), [user guide](../user-guide.md) |
| Unsafe code audit and enforcement | [repository check](../../tools/check_repository.py), [arrow decoder](../../crates/praevis-thetadata-arrow/src/lib.rs), [client decoder](../../crates/praevis-thetadata-client/src/bounded.rs) |
| Fuzzing | [fuzzing](fuzzing.md), [targets](../../fuzz), [workflow](../../.github/workflows/fuzz.yml) |
| Release pipeline, SLSA L3, SBOMs, offline verification | [releasing](../releasing.md), [trusted-publishing.yml](../../.github/workflows/trusted-publishing.yml), [build-release.yml](../../.github/workflows/build-release.yml) |
| cargo-deny policy | [deny.toml](../../deny.toml), [workflow](../../.github/workflows/supply-chain.yml) |
| Workflow security | [zizmor workflow](../../.github/workflows/zizmor.yml) |
| Scorecard | [workflow](../../.github/workflows/scorecard.yml), [results](https://scorecard.dev/viewer/?uri=github.com/Praevis-Quant/praevis-thetadata) |
| Dependabot | [configuration](../../.github/dependabot.yml) |
| Vulnerability policy | [SECURITY.md](../../SECURITY.md) |
| TLS provider test | [tls-providers](../../tools/tls-providers) |
| Test matrix | [CI workflow](../../.github/workflows/ci.yml) |
| OpenSSF Best Practices answers | [answers](openssf-best-practices.md) |
