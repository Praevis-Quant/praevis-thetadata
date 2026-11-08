# GitHub release draft: v0.1.0-alpha.3 (published)

Status: **published 2026-11-08** as the
[v0.1.0-alpha.3 pre-release](https://github.com/Praevis-Quant/praevis-thetadata/releases/tag/v0.1.0-alpha.3),
its body taken from this text, with the two wheels from CI run 42450063300
on `7402533` ([publication record](crates-io-release-plan.md#publication-record-010-alpha3)).
The text below is kept as drafted.

**To fill in at publication:**
- the date;
- the commit;
- the exact wheel file names from the CI artifacts `wheel-ubuntu-latest`
  and `wheel-windows-latest`.

---

**Title:** `v0.1.0-alpha.3`. Mark it as a pre-release.

**Body:**

> **Experimental prerelease.** praevis-thetadata is an independent,
> unofficial client for ThetaData's market-data service. It is not
> affiliated with, sponsored by or endorsed by Theta Data Inc. APIs may
> change between alphas, and this is not full Python parity.

## What's new since 0.1.0-alpha.2

- **Every RPC as a typed request:** generated requests for all 100 RPCs;
  the 81 with a vendor wrapper keep the vendor's defaults.
- **Arrow results:** `ThetaClient::query()` streams exact Arrow batches
  (decimal prices, zone-aware timestamps). `job()` splits large requests
  by date (and all-strike fine option quotes by strike) and runs them in
  order on a tier-aware scheduler: PRO 8 concurrent, FREE 1 and 20 per
  minute.
- **New crate `praevis-thetadata-arrow`:** the columnar decoder, a Parquet
  sink, and stable column types for 59 endpoints.
- **New crate `praevis-thetadata-stream`** (experimental): option quotes and
  the full trade stream from your own running Theta Terminal's local
  WebSocket.
- **Python package `praevis_thetadata`:**
  - vendor-named methods returning Polars, pandas or Arrow;
  - streaming batches;
  - Parquet output.

  It is **attached to this release, not on PyPI.**

The full list is in [CHANGELOG.md](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.0-alpha.3/CHANGELOG.md).

## Install

**Rust** (prereleases need an exact version):

```toml
praevis-thetadata-client = "=0.1.0-alpha.3"
praevis-thetadata-stream = "=0.1.0-alpha.3"   # optional, real time
```

```text
cargo install praevis-thetadata-cli --version 0.1.0-alpha.3 --locked
```

**Python** (3.10 or newer; **not on PyPI**). Install the wheel for your
platform from this release's assets:

```text
# Windows x86_64
pip install https://github.com/Praevis-Quant/praevis-thetadata/releases/download/v0.1.0-alpha.3/praevis_thetadata-0.1.0a3-cp310-abi3-win_amd64.whl
# Linux x86_64 (glibc 2.28 or newer)
pip install https://github.com/Praevis-Quant/praevis-thetadata/releases/download/v0.1.0-alpha.3/praevis_thetadata-0.1.0a3-cp310-abi3-manylinux_2_28_x86_64.whl
```

The wheel has no required dependencies. Install the dataframe library you
use: `pip install polars pyarrow`, or `pandas pyarrow`.

## Known limits

- **Experimental:** only authentication and the stock EOD slice were the
  accepted baseline before this alpha. The rest is experimental. Live
  checks cover 50 of 51 option RPCs on options PRO; stock, index, calendar
  and rate endpoints are checked on FREE only. See
  [endpoint coverage](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.0-alpha.3/docs/endpoints/coverage.md).
- **Memory:** only the client's own accounted allocations are bounded. The
  transport keeps up to one message per open stream outside that budget
  ([item 8](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.0-alpha.3/docs/releases.md#item-8-decision-a-user-2026-11-08)).
- **Real time** needs your own Theta Terminal. Its latency was measured
  on the Terminal's dev feed only. Running the Terminal's REST API and our
  historical client on one account at the same time ends the Terminal's
  REST session.
- **Wheels:** Windows and Linux x86_64 only. No macOS or Linux ARM wheels in
  this release.

## Assets

| File | Platform |
| --- | --- |
| `praevis_thetadata-0.1.0a3-cp310-abi3-win_amd64.whl` | Windows x86_64, CPython 3.10+ |
| `praevis_thetadata-0.1.0a3-cp310-abi3-manylinux_2_28_x86_64.whl` | Linux x86_64, glibc 2.28+, CPython 3.10+ |

---

**Linux wheel:** built in PyPA's `manylinux_2_28` image (CI), so it
installs on glibc 2.28 or newer: RHEL 8+, Debian 11+, Ubuntu 20.04-era and
later.
