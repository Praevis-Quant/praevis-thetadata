# Releasing

How a release is built, approved and published (user decision 2026-11-15).
From `0.1.3`, **nothing is published by hand**: crates.io, PyPI and the
GitHub release are published only by
[`.github/workflows/trusted-publishing.yml`](../.github/workflows/trusted-publishing.yml), and only
after the user approves it. No tokens are stored or used.

**The organisation's release standard.** The pipeline follows the
Praevis-Quant release standard,
[`docs/TRUSTED-PUBLISHING.md`](https://github.com/Praevis-Quant/.github/blob/main/docs/TRUSTED-PUBLISHING.md)
(adopted 2026-11-22; the Rust Commons standard with Praevis-Quant
additions), in its binary variant. The job ids are `gate`, `build`,
`publish`, `publish-pypi`, `github-release` and `verify`, plus the dry-run
`publish-path-check`.

## Who does what

| Step | Who |
| --- | --- |
| Release-prep PR (version bump, the CHANGELOG entry **dated** with the planned release date, the release notes `docs/releases/v<version>.md`, checklist) | An agent opens it; the maintainer merges it when CI is green. If the tag slips to another day, a one-line PR updates the date before tagging |
| Pre-release live check on the merged commit (`scripts/live/run.sh --part anytime`) | The maintainer, on the user's machine |
| Dry run of the release workflow on `main` (optional, recommended) | The maintainer or the user (Actions → Trusted publishing → Run workflow) |
| Create and push the tag `v<version>` | **The user** (repository admin). Ruleset "release tags creation" lets only admins create `v*` tags; ruleset "release tags immutable" stops everyone, admins included, from moving or deleting one |
| Approve the `crates-io` environment, then the `pypi` environment (two approvals per release) | **The user**, each environment's required reviewer; administrators cannot bypass either |
| Publication record PR | An agent, after the user reports the release |

**Agents never publish, never create or push `v*` tags, and never approve
the `crates-io` or `pypi` environment** ([AGENTS.md](../AGENTS.md)).

**The repository settings that enforce this** (read back 2026-11-22):

| Setting | Configuration |
| --- | --- |
| Environments `crates-io` and `pypi` | Required reviewer: the user. Deployments only from tags matching `v*`. **Administrators cannot bypass** the protection rules (`can_admins_bypass: false`, since 2026-11-21). Self-review is allowed (`prevent_self_review: false`): the user both pushes the tag and approves |
| Ruleset "release tags immutable" (id 27324055) | On `refs/tags/v*`, active: blocks deletion, update and non-fast-forward. **No bypass actors**, so it applies to admins too |
| Ruleset "release tags creation" (id 27825772) | On `refs/tags/v*`, active: blocks creation, with one bypass: the repository admin role. Only admins can create a `v*` tag |
| Ruleset "main protection" (id 27326370) | On the default branch: changes only by pull request, 17 required status checks, linear history, no deletion or force-push, no bypass actors. **0 required approving reviews** |

**What these controls do and do not separate.** The agents work under the
maintainer's own GitHub account (the user's decision; there is no bot
identity). So GitHub enforces that a person with that account clicks
**Approve** on each environment, and that no one can move a release tag.
But GitHub cannot tell an agent's actions from the maintainer's: the
separation between what agents may do (prepare, never tag, publish or
approve) and what only the maintainer does rests on **process** (these
instructions, AGENTS.md and review of each run), **not on a technical
control**.

## How a release runs

**1. Tag.** The user tags the commit that passed the live check, with an
**annotated** tag (`-a`):

```text
git tag -a v<version> <commit> -m "praevis-thetadata <version>"
git push origin v<version>
```

A lightweight tag (`git tag v<version>` without `-a`) is refused by the gate,
and because `v*` tags can never be moved or deleted (ruleset "release tags
immutable"), it **uses up that version**: the release then needs the next
version. Check with `git cat-file -t v<version>` (it must print `tag`)
before pushing.

**2. Gate** (`gate` job, "Release gate"). On a tag push, all of these must
hold, or the run stops before a build:
- the tag is an **annotated** tag named `v<workspace version>` (checked
  through the API: the ref must point at a tag object);
- the commit is on `main`, and CI (`ci.yml`) succeeded on that commit's push
  to `main`. The gate reads every `ci.yml` run for the commit (up to 100) and
  filters them itself, never trusting the API's status filter (the Rust
  Commons cargo-libc#44 lesson);
- the workspace's publishable crates are exactly the release set, all at
  the release version (from `cargo metadata`). The gate outputs the ordered
  list;
- `CHANGELOG.md` has a dated `## [X.Y.Z] - YYYY-MM-DD` entry, and the
  release notes `docs/releases/vX.Y.Z.md` exist.

On pull requests and manual runs the gate reports what it found as warnings,
sets `publish=false` and lets the dry run continue.

**3. Build** in [`build-release.yml`](../.github/workflows/build-release.yml),
a reusable workflow that `trusted-publishing.yml` calls (no secrets,
read-only token):

| Artifact | Platforms | How it is checked |
| --- | --- | --- |
| `praevis-thetadata-cli-<version>-<target>.zip` | `x86_64-pc-windows-msvc`, `aarch64-pc-windows-msvc` | Built on Windows x64 and Windows 11 Arm runners with the C runtime linked statically; runs `--version` |
| `praevis-thetadata-cli-<version>-<target>.tar.gz` | `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` | Built in the PyPA `manylinux_2_28` image; needs at most `GLIBC_2.28`; runs `--version` |
| | `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl` | Fully static, libdbus vendored; runs `--version` |
| | `x86_64-apple-darwin`, `aarch64-apple-darwin` | Built on macOS Intel and Apple silicon runners; runs `--version` |
| `praevis_thetadata-<version>-cp310-abi3-<platform>.whl` | `manylinux_2_28` and `musllinux_1_2` (x86_64, aarch64), `win_amd64`, `win_arm64`, `macosx_10_12_x86_64`, `macosx_11_0_arm64` | Installed in a clean environment on its own platform (Python 3.10 in the Linux images), then imported, version-checked and `describe()` planned offline |

Each archive holds the `praevis-thetadata` executable, `LICENSE`, `NOTICE`
and the CLI README. On every platform `auth --save` uses the native store:
Windows Credential Manager, Linux Secret Service (including the static musl
builds) or the macOS Keychain.

Alongside them, **CycloneDX 1.5 SBOMs** (`sbom` job, cargo-cyclonedx 0.5.10)
of the Rust dependency graph each artifact is built from:
`praevis-thetadata-<crate>-<version>.cdx.json` for each of the seven
published crates (the CLI's covers the executables), and
`praevis-thetadata-wheel-<version>.cdx.json` for the extension module in
every wheel.

**The seven crates** (`crates` job, from `0.1.5`): `cargo package` of the
published crates in one command, in publish order, so each later crate
resolves the earlier ones from the local package directory, with the
verification build. The `.crate` files, `praevis-thetadata-<crate>-<version>.crate`,
are release artifacts like the others. The `crates` job records its exact
Rust toolchain, and crates.io publishing uses that same toolchain.
- **Reproducible, and checked every run:** `crates-reproducible` packages
  them again on another runner and architecture (`ubuntu-24.04-arm`) with
  the same toolchain. Every `.crate` must be byte-identical, or nothing is
  attested or published.
- **Evidence (2026-11-21):** two checkouts of the same commit with
  different paths and file times give identical `.crate` files. `cargo
  package` of tag `v0.1.4` on an aarch64 machine reproduces all seven
  `0.1.4` crates on crates.io byte for byte, which `cargo publish` packaged
  on the x86_64 release runner one crate at a time against crates.io. So
  publishing one crate at a time gives the same bytes as packaging the
  seven together.
- **Dry run 44924900723 (2026-11-21, on `f6a9a5b`):** the `crates` job
  (x86_64) and `crates-reproducible` (arm64) agreed on all seven, and a
  third packaging on another aarch64 machine matched them too. The run
  produced 35 assets with 31 `SHA256SUMS` entries, and a `.crate`'s
  provenance verifies.

**4. Checksums and attestations** (`collect` job, "Checksums and
attestations", still in the reusable workflow; it signs **only on a `v*`
tag push**, never in a dry run):
- `SHA256SUMS` over the 31 files (16 artifacts, 7 crates, 8 SBOMs);
- a signed [build-provenance attestation](https://docs.github.com/en/actions/security-for-github-actions/using-artifact-attestations)
  for each of the 31, made with GitHub's OIDC identity through Sigstore;
- SBOM attestations binding the eight executables to the CLI's SBOM and the
  eight wheels to the wheel SBOM;
- the three signed bundles, kept as release assets for offline
  verification: the provenance bundle `praevis-thetadata-<version>.intoto.jsonl`,
  and `praevis-thetadata-cli-<version>.sbom.sigstore.jsonl` and
  `praevis-thetadata-wheel-<version>.sbom.sigstore.jsonl`. The provenance
  bundle is named with the `.intoto.jsonl` suffix that OpenSSF Scorecard's
  Signed-Releases check recognises; its content is the Sigstore bundle, as
  before. Up to `0.1.4` it was named
  `praevis-thetadata-<version>.provenance.sigstore.jsonl`.
- **Every published artifact has verifiable provenance** from `0.1.5`: the
  executables and wheels, and now the crates too (crates.io's own
  trusted-publishing record names the workflow run, but does not sign
  the file).
- **The bundles are not in `SHA256SUMS`, by design:** they are made after it
  (the provenance attests each `SHA256SUMS` entry), and are verified by
  their signatures, not by checksum.

**SLSA v1.0 Build Level 3.** The build and its attestations run in the
reusable `build-release.yml`, so each attestation names that workflow as
the builder (its signer), not the calling workflow. The caller's steps
cannot influence the build or forge its provenance, and a verifier can
require the signer workflow (below). This is GitHub's documented way to
reach Build L3 with artifact attestations.

**5. Approval, twice, in order.** First `publish` waits for the
`crates-io` environment. `publish-pypi` needs it, so the `pypi` approval is
requested only after crates.io has published and passed its byte-for-byte
checks. Each environment has the user as its required reviewer, admits only
`v*` tags, and cannot be bypassed by an administrator. The user opens the
run (Actions → Trusted publishing → the run for the tag), checks it, chooses
**Review deployments** and approves `crates-io`. When `publish` has
succeeded, **Review deployments** appears again, for `pypi`. Before
approving `crates-io`:
- the gate passed for the tag you pushed, on the commit the live check ran on;
- all 16 build jobs, the two crate jobs (packaging and the reproducibility
  check), the SBOM job and the checksums job are green;
- the run is for `refs/tags/v<version>`, not a branch.

Before approving `pypi`: `publish` is green, including its last
lines, `crates.io serves the attested .crate`, for all seven crates.

Rejecting `crates-io`, or letting it time out, publishes nothing, and `pypi`
is never requested. Rejecting `pypi` after crates.io published leaves the
crates out and no PyPI release or GitHub release; approving `pypi` in a
re-run of that job completes the release.

**6. Publish:** three jobs, one after another, each with only the permissions it needs, and
each first re-checking the downloaded bundle against `SHA256SUMS`, so what
is published is what was attested.
1. **`publish`** ("Publish to crates.io", environment `crates-io`): **crates.io** by [trusted publishing](https://crates.io/docs/trusted-publishing),
   in four waves: proto, core, auth; arrow; client; cli, stream. Each wave
   gets its own short-lived token, revoked when the job ends, and `cargo
   publish` waits until the index serves each crate. Two checks tie what
   crates.io serves to the attested `.crate` files:
   - **Before the first wave:** with the build's toolchain, the job packages
     the seven crates itself and each must be byte-identical to the
     attested one, or nothing is published.
   - **After each crate:** it is downloaded back from `static.crates.io`, and
     its SHA-256 and the index's checksum must equal the attested `.crate`;
     any difference fails the job before the GitHub release. A re-run
     checks crates already published too. Network errors, server errors
     and a slow index are retried with backoff (about four minutes), so a
     registry hiccup does not fail a release that has already published.
   - **`cargo publish --no-verify`:** this job holds the crates.io and OIDC
     tokens, so it runs no dependency build scripts or proc macros. The
     build job's packaging ran the verification build on the same bytes.
2. **`publish-pypi`** (environment `pypi`): **PyPI** by
   [trusted publishing](https://docs.pypi.org/trusted-publishers/), the
   eight wheels with PyPI attestations. No sdist.
3. **`github-release`** (no environment, only `contents: write`), after both
   succeeded (so after PyPI, which follows crates.io): **the GitHub release**
   for the tag, with the 16 artifacts, the 7 crates, the 8 SBOMs, the 3
   signed bundles and `SHA256SUMS` (35 assets). Its body is the whole of the
   committed `docs/releases/vX.Y.Z.md`, and its title that file's first
   heading (up to `0.1.5`: the body after `**Body:**` in
   `docs/release-draft-<version>.md`, titled with the tag). It is **drafted
   first** (`tools/release.py github-release`):
   - the tag must exist;
   - a draft is created, or an earlier draft resumed (found through the
     releases list, which shows drafts);
   - its tag, title and body are checked against the notes;
   - every missing asset is uploaded; an existing asset is never replaced,
     and one that differs stops the job;
   - every asset is checked by name, size and digest;
   - then the release is published (marked latest; a version with a
     pre-release suffix is marked pre-release instead) and checked again.

   A re-run resumes the draft, or only checks a release that is already
   published; it never modifies one.

**7. Verify publication** (`verify` job, from `0.1.6`): after the GitHub
release, [`verify-publication.yml`](../.github/workflows/verify-publication.yml)
checks the release from the outside, read-only (`contents: read`, no
environment, no OIDC token), with [`tools/verify_publication.py`](../tools/verify_publication.py).
It runs one leg per release target, each on its own native runner (Windows
x64 and arm64, Linux glibc and musl on x86_64 and arm64, macOS Intel and
Apple silicon). Every leg:
- checks that the release is published and has exactly the 35 expected
  assets, each matching GitHub's recorded digest;
- checks `SHA256SUMS` (exactly the 31 artifacts, each matching);
- verifies every artifact's provenance online and offline (the
  `.intoto.jsonl` bundle and Sigstore's trusted root) with
  `--signer-workflow build-release.yml --source-ref refs/tags/v<version>`,
  and the SBOM attestation of every executable and wheel;
- runs that platform's downloaded CLI, with no token in its environment:
  `--version` must print the version.

The Linux x86_64 leg also checks:
- the release body equals the draft's;
- each crate's crates.io API checksum, index checksum and download equal
  the attested `.crate`, and none is yanked;
- PyPI holds exactly the eight attested wheels;
- docs.rs built the six library crates. Pending is reported as pending,
  not as a pass; a failed build fails the leg;
- a clean `cargo install praevis-thetadata-cli --version =<version>
  --locked` and `pip install praevis-thetadata==<version>` (retried while
  the index lags) install and run.

Each leg uploads `publication-proof-<version>-<target>`: `report.json` and
every attestation verification. The same workflow can be run by hand for
any published version (Actions → Verify publication), and runs against the
latest release on pull requests that change it. A failure here comes after
publishing: nothing is unpublished. Follow
[When something goes wrong](#when-something-goes-wrong).

**8. Record.** The user reports the result; an agent opens the publication
record PR (CHANGELOG dated, `releases.md`, the record in
[the release plan](crates-io-release-plan.md)). The record cites the
`verify` job's result and proof artifacts for each target, and adds only
what the job does not cover, such as the release's behaviour checks.

## Dry run

**Actions → Trusted publishing → Run workflow** on `main` (or any branch) runs the gate
in report-only mode, builds and checks all 16 artifacts, the 7 crates (packaged
twice, on two runners, and compared) and the 8 SBOMs, writes
`SHA256SUMS`, and uploads them as the run's `release-<version>` artifact. It
**never signs**: attestations are made only on a `v*` tag push (the release
standard). Instead of the publish jobs, **`publish-path-check`** (a
Praevis-Quant addition to the standard) runs the crates.io job's checks with
no environment or token: the build's pinned toolchain, the bundle against
`SHA256SUMS`, its own packaging against the checksummed crates, and the
crates.io download checks against `0.1.4` (`check-known-good`). The publish
jobs do not run: they need a tag push, and the `crates-io` and `pypi`
environments admit only `v*` tags anyway. Pull requests that change the
release tooling run the same dry run. A superseded pull request run is
cancelled; a tag run never is (`concurrency: trusted-publishing-<ref>`).

## Verifying a release

**Online** (a machine that can reach GitHub):

```text
# Checksums: the release assets and SHA256SUMS in one folder
sha256sum --check --ignore-missing SHA256SUMS       # Linux
shasum -a 256 --check --ignore-missing SHA256SUMS   # macOS

# Provenance: built by this repository's isolated build workflow, for the
# release tag (any release asset, including the .crate files from 0.1.5)
gh attestation verify praevis-thetadata-cli-0.1.5-x86_64-unknown-linux-musl.tar.gz \
  --repo Praevis-Quant/praevis-thetadata \
  --signer-workflow Praevis-Quant/praevis-thetadata/.github/workflows/build-release.yml \
  --source-ref refs/tags/v0.1.5

# The SBOM attestation of an executable or a wheel
gh attestation verify praevis-thetadata-cli-0.1.5-x86_64-unknown-linux-musl.tar.gz \
  --repo Praevis-Quant/praevis-thetadata --predicate-type https://cyclonedx.org/bom \
  --signer-workflow Praevis-Quant/praevis-thetadata/.github/workflows/build-release.yml \
  --source-ref refs/tags/v0.1.5
```

**Always pass `--source-ref refs/tags/v<version>`.** Only a `v*` tag push
signs (since the release standard, 2026-11-22), so every attestation from
this workflow should be a release's. `--source-ref` stays as a second
safeguard: dry runs before then signed from branches, and because the crates
are reproducible, those runs attested `.crate` digests identical to a
release's. Such attestations are genuine, but they are not the release's;
`--source-ref` accepts only the tag's run.

**A crate as crates.io serves it** (from `0.1.5`): the file Cargo downloads
is the attested one, so its provenance verifies directly.

```text
curl -sSfLO https://static.crates.io/crates/praevis-thetadata-core/praevis-thetadata-core-0.1.5.crate
gh attestation verify praevis-thetadata-core-0.1.5.crate \
  --repo Praevis-Quant/praevis-thetadata \
  --signer-workflow Praevis-Quant/praevis-thetadata/.github/workflows/build-release.yml \
  --source-ref refs/tags/v0.1.5
```

**Offline** (an enclave with no network): the signed bundles are release
assets, so verification needs only one trust anchor fetched beforehand.
1. **On a connected machine,** fetch Sigstore's trusted root through GitHub
   (`gh attestation trusted-root > trusted_root.jsonl`), and download the
   release assets you need with `SHA256SUMS` and the three bundles
   (`*.intoto.jsonl` and `*.sbom.sigstore.jsonl`).
2. **Transfer** them into the enclave by your approved process.
3. **In the enclave:**

```text
sha256sum --check --ignore-missing SHA256SUMS
gh attestation verify praevis-thetadata-cli-0.1.5-x86_64-unknown-linux-musl.tar.gz \
  --bundle praevis-thetadata-0.1.5.intoto.jsonl \
  --custom-trusted-root trusted_root.jsonl \
  --repo Praevis-Quant/praevis-thetadata \
  --signer-workflow Praevis-Quant/praevis-thetadata/.github/workflows/build-release.yml \
  --source-ref refs/tags/v0.1.5
```

`gh attestation verify --bundle` checks the signature, the certificate
chain to the trusted root, the transparency-log inclusion recorded in the
bundle, the workflow identity and the tag, without contacting any service. The
trusted root changes rarely; refresh it with each transfer.

On PyPI each file shows its attestation (the release workflow as publisher);
on crates.io each version shows that it was published by GitHub Actions
through trusted publishing.

## When something goes wrong

- **Before approval:** reject both deployments (or let them time out), fix on
  `main`, and release a new version. A pushed `v*` tag is never moved.
- **A publish job failed part-way:** re-run the failed job. `publish` skips crates
  already on the index (and still checks them against the attested files),
  `publish-pypi` skips files already uploaded, and `github-release` resumes
  its draft, uploading only missing assets. It never replaces an asset or
  modifies a published release. If a draft holds an asset that differs from
  the bundle, the job stops: delete that draft (it was never public) and
  re-run.
- **A build job failed on infrastructure** (for example
  `actions/upload-artifact` reporting `Failed to CreateArtifact: Unable to
  make request: ENOTFOUND`, a transient DNS failure on the runner, seen on
  #222 after the wheel had built): re-run the failed job. Build, SBOM and
  checksum jobs publish nothing, so a re-run is always safe; the publish jobs
  only start once every build job has succeeded.
- **The crates are not reproducible.** It depends on where this is found:
  - **In the build** (`crates-reproducible` fails): nothing is attested or
    published. Find the difference (`diffoscope` on the two `.crate` files,
    or compare their unpacked contents), fix it on `main`, and release a
    new version. The tag is not moved.
  - **Before publishing** (the check before wave 1 fails): nothing is on
    crates.io, and `pypi` is never requested, so nothing is published
    anywhere. Continue with a new version as above.
  - **After publishing** (a downloaded crate differs): that crate and the
    ones before it are on crates.io and **cannot be replaced**; the job
    stops, so PyPI is never requested and the GitHub release is not
    created, and nobody publishes either by hand. Compare the served `.crate` with the attested one. If only the
    archive differs and the sources are the same, the published version is
    sound but has no matching provenance: the publication record says so,
    and the next version restores it. If the sources differ, yank the
    affected versions on crates.io (and their dependants among ours) and
    release a fixed version through the pipeline. Either way the version
    gets no GitHub release, and its record names the mismatch.
- **A published release is wrong:** crates.io and PyPI versions cannot be
  replaced, only yanked, and a fixed patch released.
  - crates.io: the owner yanks the version in the crates.io web UI (yanking
    is not publishing; it keeps existing lockfiles working).
  - PyPI: the owner yanks the release in the PyPI web UI.
  - GitHub: the user edits the release (or marks an earlier one latest). The
    tag stays.

## Immutable releases

**Status: not enabled.** The `v0.1.3` release reports `isImmutable: false`.
GitHub's *immutable releases* setting (repository or organisation settings)
makes a release unchangeable once it is published: its assets cannot be
added, replaced or deleted, its tag cannot be moved or deleted, and GitHub
signs a release attestation that `gh release verify` and
`gh release verify-asset` check. Drafts stay editable until published. It
applies only to releases published after it is enabled; `v0.1.3` stays as it
is.

**Recommended: enable it, after one change to the workflow.**
- **What it adds:** the published assets and the tag are then protected
  against later replacement even by an account with admin rights or a
  stolen token; today only our provenance and checksums would show such a
  change, not prevent it. It also fits the policy already followed: a wrong
  release is fixed by a new version, never in place.
- **The change first: done (2026-11-22).** The `github-release` job now
  drafts the release, uploads and checks every asset, and only then
  publishes; a re-run never replaces an asset of a published release. So
  immutable releases need no further workflow change.
- **What it costs:** a mistake in a published GitHub release can then only be
  corrected by a new version (as for crates.io and PyPI already); the
  release notes can still be edited.

## Platform notes

- **Linux musl, aarch64:** the vendored libdbus is built with
  `CFLAGS=-mno-outline-atomics`. Without it the static link fails on
  `__aarch64_ldadd4_sync`, a libgcc helper for gcc's outline atomics that a
  static Rust link does not include.
- **Linux glibc:** built in the PyPA `manylinux_2_28` image rather than on the
  runner, so the executable runs on glibc 2.28 and newer (RHEL 8, Debian 11,
  Ubuntu 20.04 era).
- **Windows:** the C runtime is linked statically (`+crt-static`), so no
  Visual C++ redistributable is needed.
- **macOS:** the minimum versions are Rust's defaults: macOS 10.12 on Intel,
  11.0 on Apple silicon. The executables are not notarised; macOS may ask
  before the first run of a downloaded archive.

## Permissions

| Job | Permissions | Environment |
| --- | --- | --- |
| `gate` | `contents: read`, `actions: read` | — |
| `publish-path-check` (dry runs only) | `contents: read` | — |
| `cli`, `wheels`, `sbom`, `crates`, `crates-reproducible` (in `build-release.yml`) | `contents: read` | — |
| `collect` (in `build-release.yml`; the calling `build` job grants these) | `contents: read`, `id-token: write`, `attestations: write` | — |
| `publish` | `contents: read`, `id-token: write` | `crates-io` (required reviewer: the user; `v*` tags only; no admin bypass) |
| `publish-pypi` (after `publish`) | `contents: read`, `id-token: write` | `pypi` (required reviewer: the user; `v*` tags only; no admin bypass) |
| `github-release` | `contents: write` | — |
| `verify` (`verify-publication.yml`, after the GitHub release) | `contents: read` | — |

Every action is pinned by full commit SHA. crates.io (all seven crates) and
PyPI (`praevis-thetadata`) trust only `trusted-publishing.yml`, crates.io
in the `crates-io` environment and PyPI in the `pypi` environment, and
crates.io refuses token publishing for these crates.

**The names are fixed:** the workflow file `trusted-publishing.yml` (the
organisation's standard) and the environments `crates-io` and `pypi` are
named in crates.io's and PyPI's trusted-publisher settings. Renaming any of
them, or moving a publish job to another file or environment, stops
publishing until those settings are changed to match. A workflow must never
name an environment that does not exist: GitHub would create it, without
protection.
