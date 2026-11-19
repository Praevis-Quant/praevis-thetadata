# GitHub release draft: v0.1.4 (published)

Status: **published 2026-11-19** by the release workflow as the
[v0.1.4 release](https://github.com/Praevis-Quant/praevis-thetadata/releases/tag/v0.1.4)
(the latest release, not a pre-release), its notes taken from the body below
([publication record](crates-io-release-plan.md#publication-record-014)).
The text is kept as published.

**The release workflow uses this file:** the GitHub release's notes are
everything after the **Body** line below, as written
([docs/releasing.md](releasing.md)). The title is the tag, and the release
is marked latest. Nothing is filled in by hand.

---

**Body:**

> **A patch release of `0.1.3`: one dependency replaced, nothing else
> changes.** praevis-thetadata is an independent, unofficial client for
> ThetaData's market-data service. It is not affiliated with, sponsored by or
> endorsed by Theta Data Inc.

## Changed

- **`chrono-tz` is replaced by `jiff`.** RustSec marked `chrono-tz`
  unmaintained ([RUSTSEC-2026-0555](https://rustsec.org/advisories/RUSTSEC-2026-0555)).
  New York times now take their UTC offset from jiff 1.0's bundled time-zone
  database (never the host's), and are formatted as before. **Timestamps are
  unchanged**, including across daylight-saving transitions; the tests that
  pin them pass unchanged.
- **Dependencies:** `chrono-tz`, `phf`, `phf_shared` and `siphasher` leave
  the build; `jiff`, `jiff-core` and `jiff-tzdb` join it.
- **Not changed:** `chrono` stays in the public API for `0.1.x` (dates are
  `chrono::NaiveDate`). RustSec also marks it unmaintained
  ([RUSTSEC-2026-0543](https://rustsec.org/advisories/RUSTSEC-2026-0543)); it
  has no known vulnerability, and moving the public dates to jiff is planned
  for `0.2.0`.

**API:** no change. The full list, including unpublished tooling, is in
[CHANGELOG.md](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.4/CHANGELOG.md).

## Install

```toml
praevis-thetadata-client = "0.1"       # resolves 0.1.4
praevis-thetadata-stream = "=0.1.4"    # optional, real time, experimental: pin it
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

Offline: add `--bundle praevis-thetadata-0.1.4.provenance.sigstore.jsonl
--custom-trusted-root trusted_root.jsonl` ([how](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.4/docs/releasing.md#verifying-a-release)).

The wheels need CPython 3.10 or newer. The known gaps of `0.1.0` still
apply ([list](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.4/docs/releases.md#known-gaps-shipped-in-010)),
less the one fixed in `0.1.1`. The security assurance report covers this
release ([report](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.4/docs/security/assurance-report.md)).

## Assets

| File | Platform |
| --- | --- |
| `praevis-thetadata-cli-0.1.4-x86_64-pc-windows-msvc.zip` | CLI, Windows x86_64 |
| `praevis-thetadata-cli-0.1.4-aarch64-pc-windows-msvc.zip` | CLI, Windows arm64 |
| `praevis-thetadata-cli-0.1.4-x86_64-unknown-linux-gnu.tar.gz` | CLI, Linux x86_64, glibc 2.28+ |
| `praevis-thetadata-cli-0.1.4-aarch64-unknown-linux-gnu.tar.gz` | CLI, Linux aarch64, glibc 2.28+ |
| `praevis-thetadata-cli-0.1.4-x86_64-unknown-linux-musl.tar.gz` | CLI, Linux x86_64, static |
| `praevis-thetadata-cli-0.1.4-aarch64-unknown-linux-musl.tar.gz` | CLI, Linux aarch64, static |
| `praevis-thetadata-cli-0.1.4-x86_64-apple-darwin.tar.gz` | CLI, macOS Intel (10.12+) |
| `praevis-thetadata-cli-0.1.4-aarch64-apple-darwin.tar.gz` | CLI, macOS Apple silicon (11.0+) |
| `praevis_thetadata-0.1.4-cp310-abi3-manylinux_2_28_x86_64.whl` | Python, Linux x86_64, glibc 2.28+ |
| `praevis_thetadata-0.1.4-cp310-abi3-manylinux_2_28_aarch64.whl` | Python, Linux aarch64, glibc 2.28+ |
| `praevis_thetadata-0.1.4-cp310-abi3-musllinux_1_2_x86_64.whl` | Python, Linux x86_64, musl 1.2+ |
| `praevis_thetadata-0.1.4-cp310-abi3-musllinux_1_2_aarch64.whl` | Python, Linux aarch64, musl 1.2+ |
| `praevis_thetadata-0.1.4-cp310-abi3-win_amd64.whl` | Python, Windows x86_64 |
| `praevis_thetadata-0.1.4-cp310-abi3-win_arm64.whl` | Python, Windows arm64 |
| `praevis_thetadata-0.1.4-cp310-abi3-macosx_10_12_x86_64.whl` | Python, macOS Intel |
| `praevis_thetadata-0.1.4-cp310-abi3-macosx_11_0_arm64.whl` | Python, macOS Apple silicon |
| `praevis-thetadata-proto-0.1.4.cdx.json` | SBOM (CycloneDX 1.5), the `praevis-thetadata-proto` crate |
| `praevis-thetadata-core-0.1.4.cdx.json` | SBOM (CycloneDX 1.5), the `praevis-thetadata-core` crate |
| `praevis-thetadata-auth-0.1.4.cdx.json` | SBOM (CycloneDX 1.5), the `praevis-thetadata-auth` crate |
| `praevis-thetadata-arrow-0.1.4.cdx.json` | SBOM (CycloneDX 1.5), the `praevis-thetadata-arrow` crate |
| `praevis-thetadata-client-0.1.4.cdx.json` | SBOM (CycloneDX 1.5), the `praevis-thetadata-client` crate |
| `praevis-thetadata-cli-0.1.4.cdx.json` | SBOM (CycloneDX 1.5), the `praevis-thetadata-cli` crate and the CLI executables |
| `praevis-thetadata-stream-0.1.4.cdx.json` | SBOM (CycloneDX 1.5), the `praevis-thetadata-stream` crate |
| `praevis-thetadata-wheel-0.1.4.cdx.json` | SBOM (CycloneDX 1.5), the extension module in every wheel |
| `SHA256SUMS` | SHA-256 of every file above |
| `praevis-thetadata-0.1.4.provenance.sigstore.jsonl` | Signed build-provenance attestation (SLSA v1.0) of every file above, for offline verification |
| `praevis-thetadata-cli-0.1.4.sbom.sigstore.jsonl` | Signed SBOM attestation of the CLI executables |
| `praevis-thetadata-wheel-0.1.4.sbom.sigstore.jsonl` | Signed SBOM attestation of the wheels |
