# GitHub release draft: v0.1.3 (published)

Status: **published 2026-11-16** by the release workflow as the
[v0.1.3 release](https://github.com/Praevis-Quant/praevis-thetadata/releases/tag/v0.1.3)
(the latest release, not a pre-release), its notes taken from the body below
([publication record](crates-io-release-plan.md#publication-record-013)).
The text is kept as published.

**The release workflow uses this file:** the GitHub release's notes are
everything after the **Body** line below, as written
([docs/releasing.md](releasing.md)). The title is the tag, and the release
is marked latest. Nothing is filled in by hand.

---

**Body:**

> **A patch release of `0.1.2`, and the first built, attested and published
> by GitHub Actions.** praevis-thetadata is an independent, unofficial client
> for ThetaData's market-data service. It is not affiliated with, sponsored
> by or endorsed by Theta Data Inc.

## Added

- **Prebuilt CLI executables** for Windows (x86_64, arm64), Linux (glibc
  2.28+ and fully static musl, each x86_64 and aarch64) and macOS (Intel,
  Apple silicon), attached below. No Rust toolchain needed.
- **Python wheels for eight platforms** (were two): Linux glibc and musl,
  x86_64 and aarch64; Windows x86_64 and arm64; macOS Intel and Apple
  silicon.
- **`auth --save` on macOS:** the session store uses the macOS Keychain, as
  it uses Credential Manager on Windows and Secret Service on Linux.
- **Provenance and SBOMs:** every asset has a signed build-provenance
  attestation (SLSA v1.0 Build Level 3) and is listed in `SHA256SUMS`, and
  CycloneDX SBOMs of the crates, the executables and the wheels are
  attached and attested; the crates and wheels were published by
  trusted publishing from this repository's release workflow.

## Fixed

**Option binomial Greeks EOD over every expiration is now planned one day
per request.** The vendor documents that any expiration `*` for this
endpoint must be requested day by day, as for Greeks EOD, which `0.1.1`
fixed after the service refused it over several days. On `0.1.2` and
earlier, `job()`, Python's planned calls (`query`, `batches`, `to_parquet`
and the vendor-named methods) and `describe()` sent it as one request over
the whole range.
- **Now:** the planner splits it per day. A single expiration keeps the
  366-day split
  ([known issues](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.3/docs/endpoints/known-issues.md)).
- **On `0.1.2` and earlier:** send one day per request, or one expiration.
- **Untested look-alikes:** at-time trade and quote also accept `*` with a
  366-day split. Nothing documents a limit for them, and they are
  unchanged.

**The gRPC client no longer panics when the application also enables
rustls's aws-lc-rs provider** (for example through `object_store`'s AWS
support): "Could not automatically determine the process-level
CryptoProvider". If the application installed a process-default provider,
the client uses it; otherwise it uses ring explicitly, as the auth client
already did.

**API:** no change. The full list, including unpublished tooling, is in
[CHANGELOG.md](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.3/CHANGELOG.md).

## Install

```toml
praevis-thetadata-client = "0.1"       # resolves 0.1.3
praevis-thetadata-stream = "=0.1.3"    # optional, real time, experimental: pin it
```

```text
pip install --upgrade praevis-thetadata
cargo install praevis-thetadata-cli --locked    # or download an executable below
```

**Verify a download:**

```text
sha256sum --check --ignore-missing SHA256SUMS
gh attestation verify <file> --repo Praevis-Quant/praevis-thetadata \
  --signer-workflow Praevis-Quant/praevis-thetadata/.github/workflows/build-release.yml
```

Offline: add `--bundle praevis-thetadata-0.1.3.provenance.sigstore.jsonl
--custom-trusted-root trusted_root.jsonl` ([how](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.3/docs/releasing.md#verifying-a-release)).

The wheels need CPython 3.10 or newer. The known gaps of `0.1.0` still
apply ([list](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.3/docs/releases.md#known-gaps-shipped-in-010)),
less the one fixed in `0.1.1`.

## Assets

| File | Platform |
| --- | --- |
| `praevis-thetadata-cli-0.1.3-x86_64-pc-windows-msvc.zip` | CLI, Windows x86_64 |
| `praevis-thetadata-cli-0.1.3-aarch64-pc-windows-msvc.zip` | CLI, Windows arm64 |
| `praevis-thetadata-cli-0.1.3-x86_64-unknown-linux-gnu.tar.gz` | CLI, Linux x86_64, glibc 2.28+ |
| `praevis-thetadata-cli-0.1.3-aarch64-unknown-linux-gnu.tar.gz` | CLI, Linux aarch64, glibc 2.28+ |
| `praevis-thetadata-cli-0.1.3-x86_64-unknown-linux-musl.tar.gz` | CLI, Linux x86_64, static |
| `praevis-thetadata-cli-0.1.3-aarch64-unknown-linux-musl.tar.gz` | CLI, Linux aarch64, static |
| `praevis-thetadata-cli-0.1.3-x86_64-apple-darwin.tar.gz` | CLI, macOS Intel (10.12+) |
| `praevis-thetadata-cli-0.1.3-aarch64-apple-darwin.tar.gz` | CLI, macOS Apple silicon (11.0+) |
| `praevis_thetadata-0.1.3-cp310-abi3-manylinux_2_28_x86_64.whl` | Python, Linux x86_64, glibc 2.28+ |
| `praevis_thetadata-0.1.3-cp310-abi3-manylinux_2_28_aarch64.whl` | Python, Linux aarch64, glibc 2.28+ |
| `praevis_thetadata-0.1.3-cp310-abi3-musllinux_1_2_x86_64.whl` | Python, Linux x86_64, musl 1.2+ |
| `praevis_thetadata-0.1.3-cp310-abi3-musllinux_1_2_aarch64.whl` | Python, Linux aarch64, musl 1.2+ |
| `praevis_thetadata-0.1.3-cp310-abi3-win_amd64.whl` | Python, Windows x86_64 |
| `praevis_thetadata-0.1.3-cp310-abi3-win_arm64.whl` | Python, Windows arm64 |
| `praevis_thetadata-0.1.3-cp310-abi3-macosx_10_12_x86_64.whl` | Python, macOS Intel |
| `praevis_thetadata-0.1.3-cp310-abi3-macosx_11_0_arm64.whl` | Python, macOS Apple silicon |
| `praevis-thetadata-proto-0.1.3.cdx.json` | SBOM (CycloneDX 1.5), the `praevis-thetadata-proto` crate |
| `praevis-thetadata-core-0.1.3.cdx.json` | SBOM (CycloneDX 1.5), the `praevis-thetadata-core` crate |
| `praevis-thetadata-auth-0.1.3.cdx.json` | SBOM (CycloneDX 1.5), the `praevis-thetadata-auth` crate |
| `praevis-thetadata-arrow-0.1.3.cdx.json` | SBOM (CycloneDX 1.5), the `praevis-thetadata-arrow` crate |
| `praevis-thetadata-client-0.1.3.cdx.json` | SBOM (CycloneDX 1.5), the `praevis-thetadata-client` crate |
| `praevis-thetadata-cli-0.1.3.cdx.json` | SBOM (CycloneDX 1.5), the `praevis-thetadata-cli` crate and the CLI executables |
| `praevis-thetadata-stream-0.1.3.cdx.json` | SBOM (CycloneDX 1.5), the `praevis-thetadata-stream` crate |
| `praevis-thetadata-wheel-0.1.3.cdx.json` | SBOM (CycloneDX 1.5), the extension module in every wheel |
| `SHA256SUMS` | SHA-256 of every file above |
| `praevis-thetadata-0.1.3.provenance.sigstore.jsonl` | Signed build-provenance attestation (SLSA v1.0) of every file above, for offline verification |
| `praevis-thetadata-cli-0.1.3.sbom.sigstore.jsonl` | Signed SBOM attestation of the CLI executables |
| `praevis-thetadata-wheel-0.1.3.sbom.sigstore.jsonl` | Signed SBOM attestation of the wheels |
