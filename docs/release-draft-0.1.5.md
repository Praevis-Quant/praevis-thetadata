# GitHub release draft: v0.1.5 (not published)

Status: **prepared 2026-11-21, not published or tagged.** Publishing needs
the user's tag `v0.1.5` and approval of the `crates-io` and `pypi`
environments ([checklist](crates-io-release-plan.md#015-publication-checklist-prepared-2026-11-21)).

**The release workflow uses this file:** the GitHub release's notes are
everything after the **Body** line below, as written
([docs/releasing.md](releasing.md)). The title is the tag, and the release
is marked latest. Nothing is filled in by hand.

---

**Body:**

> **The vendor package's 1.0.18 becomes the compatibility baseline. Two
> defaults change for current-day requests: read "Behaviour change" before
> upgrading.** praevis-thetadata is an independent, unofficial client for
> ThetaData's market-data service. It is not affiliated with, sponsored by or
> endorsed by Theta Data Inc.

## Behaviour change

ThetaData's Python package 1.0.18 changed two defaults. This release follows
them, in the Rust requests, `dispatch` and the Python methods alike. Both
affect **current-day requests only**; historical requests are unchanged.

| Requests | Setting | `0.1.4` | `0.1.5` | To keep `0.1.4`'s results |
| --- | --- | --- | --- | --- |
| The nine trade-Greeks requests (`option_history_trade_greeks_*`, `option_history_binomial_trade_greeks_*`) | `perf_boost_intraday` | `false` | `true`: the underlying price is looked up at 1-second, not millisecond, precision | Python `perf_boost_intraday=False`; Rust `.perf_boost_intraday(false)` |
| `option_history_trade_quote`, `stock_history_trade_quote` | `use_time_based_trade_quote` | `true` | `false`: each trade is matched with the last quote before it, not by millisecond timestamp | Python `use_time_based_trade_quote=True`; Rust `.use_time_based_trade_quote(true)` |

## Added

- **`corporate_action_dividend` and `corporate_action_split`**, 1.0.18's new
  methods, in Rust (`CorporateActionDividend`, `CorporateActionSplit`),
  `dispatch` and Python. **Not live-verified:** no request has been sent to
  the service with them yet, so they are not supported, and the coverage
  table says so. Each is sent as one request, as the vendor package does.
  Their Rust types were unstable, descriptor-only types, and change shape:
  `new(symbol, start_date, end_date)` with `NaiveDate` dates, as for every
  other wrapper, in place of `Default` and string setters.

## Dependencies

- **rustix 1.1.6**, fixing [RUSTSEC-2026-0486](https://rustsec.org/advisories/RUSTSEC-2026-0486)
  (build- and test-time only; not in the executables, wheels or the
  crates' runtime dependencies).
- **zbus and three related crates are gone from `Cargo.lock`**
  (RUSTSEC-2026-0370 to -0372, unmaintained). They were listed but never
  built; the Linux credential store is unchanged.

## Release artifacts

- **The seven crates are attested release artifacts.** Each `.crate` is
  packaged in the isolated build, reproduced byte for byte on a second
  runner, included in `SHA256SUMS` and the signed provenance, and attached
  below. crates.io serves the same files: each was downloaded back after
  publishing and compared. So the provenance verifies a crate as Cargo
  downloads it.
- **The provenance bundle is `praevis-thetadata-0.1.5.intoto.jsonl`**
  (named `.provenance.sigstore.jsonl` up to `0.1.4`; same content).

## Not changed

- **The protocol:** 1.0.18's descriptor is byte-identical to 1.0.17's.
- **Login:** 1.0.18 adds a client name and version to its login request;
  this release does not.
- **Option OHLC `expiration`** stays a date. 1.0.18 also accepts `"*"`;
  that needs a public Rust type change and is planned for `0.2.0`.

**API:** additive, apart from the two unstable corporate-action types
above; the defaults above are the only change in behaviour.
The full list, including unpublished tooling, is in
[CHANGELOG.md](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.5/CHANGELOG.md).

## Install

```toml
praevis-thetadata-client = "0.1"       # resolves 0.1.5
praevis-thetadata-stream = "=0.1.5"    # optional, real time, experimental: pin it
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

The same command verifies a crate downloaded from crates.io
(`https://static.crates.io/crates/<crate>/<crate>-0.1.5.crate`).

Offline: add `--bundle praevis-thetadata-0.1.5.intoto.jsonl
--custom-trusted-root trusted_root.jsonl` ([how](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.5/docs/releasing.md#verifying-a-release)).

The wheels need CPython 3.10 or newer. The known gaps of `0.1.0` still
apply ([list](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.5/docs/releases.md#known-gaps-shipped-in-010)),
less the one fixed in `0.1.1`. Coverage per endpoint, including which
wrappers are not live-verified, is in the
[coverage table](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.5/docs/endpoints/coverage.md). The security assurance report covers this
release ([report](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.5/docs/security/assurance-report.md)).

## Assets

| File | Platform |
| --- | --- |
| `praevis-thetadata-cli-0.1.5-x86_64-pc-windows-msvc.zip` | CLI, Windows x86_64 |
| `praevis-thetadata-cli-0.1.5-aarch64-pc-windows-msvc.zip` | CLI, Windows arm64 |
| `praevis-thetadata-cli-0.1.5-x86_64-unknown-linux-gnu.tar.gz` | CLI, Linux x86_64, glibc 2.28+ |
| `praevis-thetadata-cli-0.1.5-aarch64-unknown-linux-gnu.tar.gz` | CLI, Linux aarch64, glibc 2.28+ |
| `praevis-thetadata-cli-0.1.5-x86_64-unknown-linux-musl.tar.gz` | CLI, Linux x86_64, static |
| `praevis-thetadata-cli-0.1.5-aarch64-unknown-linux-musl.tar.gz` | CLI, Linux aarch64, static |
| `praevis-thetadata-cli-0.1.5-x86_64-apple-darwin.tar.gz` | CLI, macOS Intel (10.12+) |
| `praevis-thetadata-cli-0.1.5-aarch64-apple-darwin.tar.gz` | CLI, macOS Apple silicon (11.0+) |
| `praevis_thetadata-0.1.5-cp310-abi3-manylinux_2_28_x86_64.whl` | Python, Linux x86_64, glibc 2.28+ |
| `praevis_thetadata-0.1.5-cp310-abi3-manylinux_2_28_aarch64.whl` | Python, Linux aarch64, glibc 2.28+ |
| `praevis_thetadata-0.1.5-cp310-abi3-musllinux_1_2_x86_64.whl` | Python, Linux x86_64, musl 1.2+ |
| `praevis_thetadata-0.1.5-cp310-abi3-musllinux_1_2_aarch64.whl` | Python, Linux aarch64, musl 1.2+ |
| `praevis_thetadata-0.1.5-cp310-abi3-win_amd64.whl` | Python, Windows x86_64 |
| `praevis_thetadata-0.1.5-cp310-abi3-win_arm64.whl` | Python, Windows arm64 |
| `praevis_thetadata-0.1.5-cp310-abi3-macosx_10_12_x86_64.whl` | Python, macOS Intel |
| `praevis_thetadata-0.1.5-cp310-abi3-macosx_11_0_arm64.whl` | Python, macOS Apple silicon |
| `praevis-thetadata-proto-0.1.5.cdx.json` | SBOM (CycloneDX 1.5), the `praevis-thetadata-proto` crate |
| `praevis-thetadata-core-0.1.5.cdx.json` | SBOM (CycloneDX 1.5), the `praevis-thetadata-core` crate |
| `praevis-thetadata-auth-0.1.5.cdx.json` | SBOM (CycloneDX 1.5), the `praevis-thetadata-auth` crate |
| `praevis-thetadata-arrow-0.1.5.cdx.json` | SBOM (CycloneDX 1.5), the `praevis-thetadata-arrow` crate |
| `praevis-thetadata-client-0.1.5.cdx.json` | SBOM (CycloneDX 1.5), the `praevis-thetadata-client` crate |
| `praevis-thetadata-cli-0.1.5.cdx.json` | SBOM (CycloneDX 1.5), the `praevis-thetadata-cli` crate and the CLI executables |
| `praevis-thetadata-stream-0.1.5.cdx.json` | SBOM (CycloneDX 1.5), the `praevis-thetadata-stream` crate |
| `praevis-thetadata-wheel-0.1.5.cdx.json` | SBOM (CycloneDX 1.5), the extension module in every wheel |
| `praevis-thetadata-proto-0.1.5.crate` | Crate, as published on crates.io |
| `praevis-thetadata-core-0.1.5.crate` | Crate, as published on crates.io |
| `praevis-thetadata-auth-0.1.5.crate` | Crate, as published on crates.io |
| `praevis-thetadata-arrow-0.1.5.crate` | Crate, as published on crates.io |
| `praevis-thetadata-client-0.1.5.crate` | Crate, as published on crates.io |
| `praevis-thetadata-cli-0.1.5.crate` | Crate, as published on crates.io |
| `praevis-thetadata-stream-0.1.5.crate` | Crate, as published on crates.io |
| `SHA256SUMS` | SHA-256 of every file above |
| `praevis-thetadata-0.1.5.intoto.jsonl` | Signed build-provenance attestation (SLSA v1.0) of every file above, for offline verification (named `.provenance.sigstore.jsonl` up to `0.1.4`) |
| `praevis-thetadata-cli-0.1.5.sbom.sigstore.jsonl` | Signed SBOM attestation of the CLI executables |
| `praevis-thetadata-wheel-0.1.5.sbom.sigstore.jsonl` | Signed SBOM attestation of the wheels |
