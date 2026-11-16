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

**3. Build** (no secrets, read-only token):

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

**4. Checksums and attestations** (`collect` job): `SHA256SUMS` over the 16
artifacts, and a signed [build-provenance attestation](https://docs.github.com/en/actions/security-for-github-actions/using-artifact-attestations)
for each, made with GitHub's OIDC identity through Sigstore.

**5. Approval, twice.** Two jobs wait: `publish-crates` for the
`crates-io` environment and `publish-pypi` for the `pypi` environment. Each
environment has the user as its required reviewer and admits only `v*`
tags. The user opens the run (Actions → Trusted publishing → the run for
the tag), checks it, and chooses **Review deployments**, then approves
`crates-io` and `pypi` (together, or one at a time). Before approving:
- the gate passed for the tag you pushed, on the commit the live check ran on;
- all 16 build jobs and the checksums job are green;
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
   publish` waits until the index serves each crate.
2. **`publish-pypi`** (environment `pypi`): **PyPI** by
   [trusted publishing](https://docs.pypi.org/trusted-publishers/), the
   eight wheels with PyPI attestations. No sdist.
3. **`github-release`** (no environment, only `contents: write`), after both
   succeeded: **the GitHub release** for the tag: the 16 artifacts and `SHA256SUMS`,
   its notes taken from the body of `docs/release-draft-<version>.md`
   (everything after `**Body:**`). Marked latest; a version with a
   pre-release suffix is marked pre-release instead.

**7. Record.** The user reports the result; an agent opens the publication
record PR (CHANGELOG dated, `releases.md`, the record in
[the release plan](crates-io-release-plan.md)), with the usual checks of
the crates, wheels, checksums, attestations and docs.rs.

## Dry run

**Actions → Trusted publishing → Run workflow** on `main` (or any branch) runs the gate
in report-only mode, builds and checks all 16 artifacts, writes
`SHA256SUMS` and attests them, and uploads them as the run's
`release-<version>` artifact. The publish jobs do not run: they need a tag
push, and the `crates-io` and `pypi` environments admit only `v*` tags anyway. Pull
requests that change the release tooling run the same dry run, without the
attestations.

## Verifying a release

```text
# Checksums: download the release assets and SHA256SUMS into one folder
sha256sum --check SHA256SUMS                       # Linux
shasum -a 256 --check SHA256SUMS                   # macOS

# Provenance: each artifact was built by trusted-publishing.yml in this repository
gh attestation verify praevis-thetadata-cli-0.1.3-x86_64-unknown-linux-musl.tar.gz \
  --repo Praevis-Quant/praevis-thetadata
```

On PyPI each file shows its attestation (the release workflow as publisher);
on crates.io each version shows that it was published by GitHub Actions
through trusted publishing.

## When something goes wrong

- **Before approval:** reject both deployments (or let them time out), fix on
  `main`, and release a new version. A pushed `v*` tag is never moved.
- **A publish job failed part-way:** re-run the failed job. `publish-crates` skips crates
  already on the index, `publish-pypi` skips files already uploaded, and `github-release`
  replaces the release's assets rather than duplicating them.
- **A published release is wrong:** crates.io and PyPI versions cannot be
  replaced, only yanked, and a fixed patch released.
  - crates.io: the owner yanks the version in the crates.io web UI (yanking
    is not publishing; it keeps existing lockfiles working).
  - PyPI: the owner yanks the release in the PyPI web UI.
  - GitHub: the user edits the release (or marks an earlier one latest). The
    tag stays.

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
| `cli`, `wheels` | `contents: read` | — |
| `collect` | `contents: read`, `id-token: write`, `attestations: write` | — |
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
