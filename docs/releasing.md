# Releasing

How a release is built, approved and published (user decision 2026-11-15).
From `0.1.3`, **nothing is published by hand**: crates.io, PyPI and the
GitHub release are published only by
[`.github/workflows/trusted-publishing.yml`](../.github/workflows/trusted-publishing.yml), and only
after the user approves it. No tokens are stored or used.

## Who does what

| Step | Who |
| --- | --- |
| Release-prep PR (version bump, CHANGELOG, `docs/release-draft-<version>.md`, checklist) | An agent opens it; the maintainer merges it when CI is green |
| Pre-release live check on the merged commit (`scripts/live/run.sh --part anytime`) | The maintainer, on the user's machine |
| Dry run of the release workflow on `main` (optional, recommended) | The maintainer or the user (Actions → Trusted publishing → Run workflow) |
| Create and push the tag `v<version>` | **The user** (repository admin). A tag ruleset blocks creating, moving or deleting `v*` tags for everyone else |
| Approve the `crates-io` and `pypi` environments (two approvals per release) | **The user**, each environment's required reviewer |
| Publication record PR | An agent, after the user reports the release |

**Agents never publish, never create or push `v*` tags, and never approve
the `crates-io` or `pypi` environment** ([AGENTS.md](../AGENTS.md)).

## How a release runs

**1. Tag.** The user tags the commit that passed the live check:

```text
git tag -a v0.1.3 <commit> -m "praevis-thetadata 0.1.3"
git push origin v0.1.3
```

**2. Gate** (`check` job): the tag must be `v<workspace version>`, the
commit must be on `main`, CI (`ci.yml`) must have passed on that commit's
push to `main`, and `docs/release-draft-<version>.md` must exist. Anything
else stops the run before a build.

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

**4. Checksums and attestations** (`collect` job, still in the reusable
workflow):
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

**5. Approval, twice.** Two jobs wait: `publish-crates` for the
`crates-io` environment and `publish-pypi` for the `pypi` environment. Each
environment has the user as its required reviewer and admits only `v*`
tags. The user opens the run (Actions → Trusted publishing → the run for
the tag), checks it, and chooses **Review deployments**, then approves
`crates-io` and `pypi` (together, or one at a time). Before approving:
- the gate passed for the tag you pushed, on the commit the live check ran on;
- all 16 build jobs, the SBOM job and the checksums job are green;
- the run is for `refs/tags/v<version>`, not a branch.

Rejecting both, or letting them time out, publishes nothing. Approving
only one publishes to that registry only; the GitHub release then does not
run (it needs both), and approving the other later completes the release.

**6. Publish:** three jobs, each with only the permissions it needs, and
each first re-checking the downloaded bundle against `SHA256SUMS`, so what
is published is what was attested.
1. **`publish-crates`** (environment `crates-io`): **crates.io** by [trusted publishing](https://crates.io/docs/trusted-publishing),
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
     checks crates already published too.
2. **`publish-pypi`** (environment `pypi`): **PyPI** by
   [trusted publishing](https://docs.pypi.org/trusted-publishers/), the
   eight wheels with PyPI attestations. No sdist.
3. **`github-release`** (no environment, only `contents: write`), after both
   succeeded: **the GitHub release** for the tag: the 16 artifacts, the 7
   crates, the 8 SBOMs, the 3 signed bundles and `SHA256SUMS` (35 assets),
   its notes taken from the body of `docs/release-draft-<version>.md`
   (everything after `**Body:**`). Marked latest; a version with a
   pre-release suffix is marked pre-release instead.

**7. Record.** The user reports the result; an agent opens the publication
record PR (CHANGELOG dated, `releases.md`, the record in
[the release plan](crates-io-release-plan.md)), with the usual checks of
the crates, wheels, checksums, attestations and docs.rs.

## Dry run

**Actions → Trusted publishing → Run workflow** on `main` (or any branch) runs the gate
in report-only mode, builds and checks all 16 artifacts, the 7 crates (packaged
twice, on two runners, and compared) and the 8 SBOMs, writes
`SHA256SUMS` and attests them, and uploads them as the run's
`release-<version>` artifact. The publish jobs do not run: they need a tag
push, and the `crates-io` and `pypi` environments admit only `v*` tags anyway. Pull
requests that change the release tooling run the same dry run, without the
attestations.

## Verifying a release

**Online** (a machine that can reach GitHub):

```text
# Checksums: the release assets and SHA256SUMS in one folder
sha256sum --check --ignore-missing SHA256SUMS       # Linux
shasum -a 256 --check --ignore-missing SHA256SUMS   # macOS

# Provenance: built by this repository's isolated build workflow (any
# release asset, including the .crate files from 0.1.5)
gh attestation verify praevis-thetadata-cli-0.1.3-x86_64-unknown-linux-musl.tar.gz \
  --repo Praevis-Quant/praevis-thetadata \
  --signer-workflow Praevis-Quant/praevis-thetadata/.github/workflows/build-release.yml

# The SBOM attestation of an executable or a wheel
gh attestation verify praevis-thetadata-cli-0.1.3-x86_64-unknown-linux-musl.tar.gz \
  --repo Praevis-Quant/praevis-thetadata --predicate-type https://cyclonedx.org/bom
```

**A crate as crates.io serves it** (from `0.1.5`): the file Cargo downloads
is the attested one, so its provenance verifies directly.

```text
curl -sSfLO https://static.crates.io/crates/praevis-thetadata-core/praevis-thetadata-core-0.1.5.crate
gh attestation verify praevis-thetadata-core-0.1.5.crate \
  --repo Praevis-Quant/praevis-thetadata \
  --signer-workflow Praevis-Quant/praevis-thetadata/.github/workflows/build-release.yml
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
  --signer-workflow Praevis-Quant/praevis-thetadata/.github/workflows/build-release.yml
```

`gh attestation verify --bundle` checks the signature, the certificate
chain to the trusted root, the transparency-log inclusion recorded in the
bundle and the workflow identity, without contacting any service. The
trusted root changes rarely; refresh it with each transfer.

On PyPI each file shows its attestation (the release workflow as publisher);
on crates.io each version shows that it was published by GitHub Actions
through trusted publishing.

## When something goes wrong

- **Before approval:** reject both deployments (or let them time out), fix on
  `main`, and release a new version. A pushed `v*` tag is never moved.
- **A publish job failed part-way:** re-run the failed job. `publish-crates` skips crates
  already on the index, `publish-pypi` skips files already uploaded, and `github-release`
  replaces the release's assets rather than duplicating them (while
  releases are not immutable; [below](#immutable-releases)).
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
    crates.io. Reject or cancel the `pypi` deployment if it is still
    waiting; if PyPI already published, that version's wheels stay, and the
    release continues as a new version as above.
  - **After publishing** (a downloaded crate differs): that crate and the
    ones before it are on crates.io and **cannot be replaced**; the job
    stops, so the GitHub release is not created, and nobody creates it by
    hand. Compare the served `.crate` with the attested one. If only the
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
- **The change first:** the `github-release` job creates the release with
  its assets in one `gh release create`, and on a re-run uploads with
  `--clobber`. With immutability on, the release must be created as a
  **draft**, the assets uploaded, and only then published (`gh release
  edit --draft=false`), and a re-run can no longer replace assets of a
  published release. The job should do exactly that, so a failure before
  publishing leaves an editable draft and a failure after it needs nothing.
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
| `check` | `contents: read`, `actions: read` | — |
| `cli`, `wheels`, `sbom` (in `build-release.yml`) | `contents: read` | — |
| `collect` (in `build-release.yml`; the calling `build` job grants these) | `contents: read`, `id-token: write`, `attestations: write` | — |
| `publish-crates` | `contents: read`, `id-token: write` | `crates-io` (required reviewer: the user; `v*` tags only) |
| `publish-pypi` | `contents: read`, `id-token: write` | `pypi` (required reviewer: the user; `v*` tags only) |
| `github-release` | `contents: write` | — |

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
