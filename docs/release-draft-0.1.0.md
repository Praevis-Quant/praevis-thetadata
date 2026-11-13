# GitHub release draft: v0.1.0 (not published)

Status: **draft.** The user approved publication on 2026-11-13, as for the
betas; the release-candidate evidence passed the same day
([checklist](crates-io-release-plan.md#010-publication-checklist-and-order-prepared-2026-11-13)).
Nothing is tagged or published yet.

**To fill in at publication:**
- the date;
- the commit;
- the exact wheel file names, from that commit's CI artifacts
  `wheel-ubuntu-latest` and `wheel-windows-latest`. The same two files go
  to PyPI and to this release.

---

**Title:** `v0.1.0`. Not a pre-release: mark it as the latest release.

**Body:**

> **The first supported release.** praevis-thetadata is an independent,
> unofficial client for ThetaData's market-data service. It is not
> affiliated with, sponsored by or endorsed by Theta Data Inc., and it is
> not full parity with the vendor's Python package.

## What is supported

The beta.1 freeze scope, as amended by beta.2
([scope record](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.0/docs/releases.md#010-scope-record-user-2026-11-12)):

**Supported:**
- **`praevis-thetadata-auth`:** login, credentials and in-memory sessions,
  with optional native session storage.
- **`praevis-thetadata-core`:** exact prices, timestamps, values and tables.
- **`praevis-thetadata-arrow`:** exact Arrow columns, and the streaming
  Parquet sink.
- **`praevis-thetadata-client`:** `query()`, `job()`, `RequestOptions` and
  typed requests for all 100 RPCs.
  - **It does what one caller asks,** as the vendor package does: no retry,
    no account-wide scheduler or rate limit, and no default timeout
    ([ADR-0021](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.0/docs/adr/0021-per-call-library.md)).
  - **A job's own concurrency** defaults to the tier's documented limit.
  - **A busy account** is `QueryError::Busy`.
- **`praevis-thetadata-cli`:** its commands, flags, exit codes and JSON
  output.
- **The Python package `praevis-thetadata`:** the vendor-named methods, plus
  `query`, `batches` and `to_parquet`.

**There is no breaking change without a new minor version and release
notes.**

**Not supported:**
- `praevis-thetadata-proto` is unstable.
- `praevis-thetadata-stream` is experimental.
- Raw gRPC access is low-level.

Pin the proto and stream crates exactly.

**Changes since beta.2:** none to the supported API. 0.1.0 adds test
infrastructure (an emulator, scenarios run against it in CI and against
the live service, a drift check, a live runbook), none of it published.
The full list is in
[CHANGELOG.md](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.0/CHANGELOG.md).

## Speed

**Re-checked on the release candidate** without the scheduler beta.2
removed: W5c, SPY tick trades over 12 expirations
([evidence](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.0/docs/performance/rc-0.1.0-evidence.md)).
- **At 8 requests at once,** ours is 10.8x (Python) and 8.8x (Rust) the
  synchronous vendor package 1.0.17, and 5.4x and 4.4x the vendor package
  threaded at the same concurrency.
- **One request at a time,** it is 1.5x.

Single runs per cycle, on one host; the ratios, not the absolute times,
carry over.

## Install

**Rust:**

```toml
praevis-thetadata-client = "0.1"
praevis-thetadata-stream = "=0.1.0"   # optional, real time, experimental: pin it
```

```text
cargo install praevis-thetadata-cli --locked
```

**Python** (3.10 or newer; tested on 3.10-3.13), from PyPI:

```text
pip install praevis-thetadata
```

- **Wheels:** Windows x86_64 and Linux x86_64 (glibc 2.28 or newer) only.
  There is no macOS or ARM wheel, and no source distribution.
- **Dataframes:** the wheel has no required dependencies. Install the
  dataframe library you use: `pip install polars pyarrow`, or `pandas
  pyarrow`.

**Alternative:** install the same wheel for your platform from this release's
assets:

```text
# Windows x86_64
pip install https://github.com/Praevis-Quant/praevis-thetadata/releases/download/v0.1.0/praevis_thetadata-0.1.0-cp310-abi3-win_amd64.whl
# Linux x86_64 (glibc 2.28 or newer)
pip install https://github.com/Praevis-Quant/praevis-thetadata/releases/download/v0.1.0/praevis_thetadata-0.1.0-cp310-abi3-manylinux_2_28_x86_64.whl
```

**From the betas:** nothing to change from beta.2. From beta.1, see the
beta.2 migration notes in the CHANGELOG (the removed scheduler, retry and
default timeouts).

## Supported platforms

Windows x86_64, and Linux x86_64 with glibc 2.28 or newer. macOS and ARM are
not supported, and there are no wheels for them.

## Known gaps

These ship as documented gaps (user decision, 2026-11-12;
[list](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.0/docs/releases.md#known-gaps-shipped-in-010)):
- **Endpoints without data-path evidence:** 20 of the 81 vendor-named
  endpoints.
  - **19** were refused on the test account, naming VALUE, STANDARD or PRO
    stocks or indices.
  - **The option quote flat file** was refused once with `InvalidArgument`.

  They decode through the generic path that the other 60 endpoints verify,
  but not on their own responses.
- **Never probed:** the stock quote flat file, which is gated by its size.
- **Native Linux real-time latency is unmeasured.** The p99 ≤ 50 µs target
  is met on Windows and missed under WSL2 (61-68 µs), with no drops on
  either. This concerns only the experimental stream crate.
- **Accounts:** paid stock and index tiers, and the STAGE environment, were
  never run.
- **`Busy`** has synthetic evidence only: the service's `RESOURCE_EXHAUSTED`
  was never observed live.
- **One unexplained error:** a `Remote(Internal)` seen once on a second
  request.
- **Memory:** the bound covers the Rust heap only (configuration plus the
  messages in flight), never total process memory.

## Assets

| File | Platform |
| --- | --- |
| `praevis_thetadata-0.1.0-cp310-abi3-win_amd64.whl` | Windows x86_64, CPython 3.10+ |
| `praevis_thetadata-0.1.0-cp310-abi3-manylinux_2_28_x86_64.whl` | Linux x86_64, glibc 2.28+, CPython 3.10+ |
