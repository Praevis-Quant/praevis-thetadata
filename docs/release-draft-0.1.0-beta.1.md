# GitHub release draft: v0.1.0-beta.1 (published)

Status: **published 2026-11-11** as the
[v0.1.0-beta.1 pre-release](https://github.com/Praevis-Quant/praevis-thetadata/releases/tag/v0.1.0-beta.1),
its body taken from this text, with the two wheels from CI run 43021749387 on
`da57cab` ([publication record](crates-io-release-plan.md#publication-record-010-beta1)).
The text below is kept as drafted.

**To fill in at publication:**
- the date;
- the commit;
- the exact wheel file names, from the CI artifacts `wheel-ubuntu-latest`
  and `wheel-windows-latest` of `da57cab`'s CI run. The same two files go to
  PyPI and to this release.

---

**Title:** `v0.1.0-beta.1`. Mark it as a pre-release.

**Body:**

> **Beta prerelease with a frozen API.** praevis-thetadata is an
> independent, unofficial client for ThetaData's market-data service. It is
> not affiliated with, sponsored by or endorsed by Theta Data Inc., and it is
> not full parity with the vendor's Python package.

## What is frozen

From this release, the public API below has **no breaking change without a
new prerelease version and release notes**:
- **`praevis-thetadata-auth`:** authentication, credentials and in-memory
  sessions, with the optional native store.
- **`praevis-thetadata-core`:** exact prices (with `numeric_cmp` and
  `numeric_eq`), timestamps, values and tables.
- **`praevis-thetadata-arrow`:** the columnar decoder and the Parquet sink.
- **`praevis-thetadata-client`:** `query()`/`job()` and their `_with`
  variants, `RequestOptions`, the tier-aware scheduler, and typed requests
  for all 100 RPCs.
- **`praevis-thetadata-cli`:** its commands, flags, exit codes and JSON
  output.
- **The Python package:** the 81 vendor-named methods (vendor 1.0.17
  signatures), plus `query`, `batches` and `to_parquet`.

## What is not frozen

- **`praevis-thetadata-stream`** (real time through your own Theta
  Terminal) stays **experimental.** Its Linux evidence is pending.
- **Raw gRPC access** (`raw_client`, `stream_raw`, `RawStream`) and
  **`praevis-thetadata-proto`** stay **unstable.** They expose generated
  tonic types.

## Changes since 0.1.0-alpha.3

**Breaking changes, made before the freeze:**
- **Room to grow:** public enums and output structs are
  `#[non_exhaustive]`.
  - Configuration structs are built with `Type::default().with_x(…)`.
  - Exhaustive `match`es on these enums need a wildcard arm.
- **`query_with` and `job_with`** take `RequestOptions` instead of
  `BatchPolicy`. Its timeout includes time queued for a slot.
- **`Endpoint` is sealed.** Only the generated requests implement it.
- **The stream crate** splits into a clonable `StreamControl` and a single
  `StreamEvents` receiver.

**Improvements:**
- **Memory:** the client assembles each response message itself, so an
  idle or stalled stream keeps nothing message-sized. The Rust-heap bound
  is stated and tested (see the user guide).
- **The stream crate:** closing now completes the WebSocket handshake, so
  unsubscribes reach the Terminal. The full trade stream had kept flowing
  into the next program.
- **Live evidence:** stock, index, calendar and interest-rate endpoints
  were checked on the current account. The real-time latency and the full
  trade stream were measured on the live market (Windows).
- **Documentation:**
  - the [user guide](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.0-beta.1/docs/user-guide.md):
    platforms, credential store, resource policy, and the differences from
    the vendor package;
  - error categories with their evidence;
  - runnable partial-result and cancellation examples in Rust and Python.

The full list is in [CHANGELOG.md](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.0-beta.1/CHANGELOG.md).

## Install

**Rust** (prereleases need an exact version):

```toml
praevis-thetadata-client = "=0.1.0-beta.1"
praevis-thetadata-stream = "=0.1.0-beta.1"   # optional, real time, experimental
```

```text
cargo install praevis-thetadata-cli --version 0.1.0-beta.1 --locked
```

**Python** (3.10 or newer), from PyPI. It is a pre-release, so pass `--pre`
or pin the version:

```text
pip install --pre praevis-thetadata
# or
pip install praevis-thetadata==0.1.0b1
```

Wheels exist for Windows x86_64 and Linux x86_64 (glibc 2.28 or newer)
only. There is no macOS or ARM wheel, and no source distribution, so pip
will not try to build it elsewhere.

**Alternative:** install the same wheel for your platform from this
release's assets:

```text
# Windows x86_64
pip install https://github.com/Praevis-Quant/praevis-thetadata/releases/download/v0.1.0-beta.1/praevis_thetadata-0.1.0b1-cp310-abi3-win_amd64.whl
# Linux x86_64 (glibc 2.28 or newer)
pip install https://github.com/Praevis-Quant/praevis-thetadata/releases/download/v0.1.0-beta.1/praevis_thetadata-0.1.0b1-cp310-abi3-manylinux_2_28_x86_64.whl
```

The wheel has no required dependencies. Install the dataframe library you
use: `pip install polars pyarrow`, or `pandas pyarrow`.

## Supported platforms

Windows x86_64, and Linux x86_64 with glibc 2.28 or newer. macOS and ARM are
not supported, and there are no wheels for them.

## Known gaps

From the [evidence review](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.0-beta.1/docs/requirements/beta1-evidence-review.md):

**Endpoints:**
- **60 of the 81 vendor-named endpoints are value-checked live.**
- **20 were only ever refused** on the test account (stock and index
  endpoints above FREE, and the stock and index flat files). Their decoding
  is the shared path, not checked on their own responses.
- **One was never probed:** the stock quote flat file, because even a probe
  may start a very large file.
- **The 19 descriptor-only RPCs** (futures, dividends, splits) are unstable
  and mostly unprobed.

**Accounts:** paid stock and index tiers, and the STAGE environment, were
never run.

**Behaviour:**
- **Retry:** retry on `RESOURCE_EXHAUSTED` and `UNAVAILABLE`, and the
  per-call timeouts, have synthetic evidence only.
- **`NoData` covers both an unknown symbol and a range with no trading
  days.** The service answers them identically: the same `NOT_FOUND` code,
  the same details and the same message (checked live, 2026-11-11). To
  tell them apart, check the symbol yourself, for example with
  `stock_list_symbols` or `index_list_symbols`.

**Platforms:**
- Live checks ran on Windows; Linux is covered by CI and install checks.
- CI tests Python 3.12; the wheel declares 3.10 or newer.

**Memory:** the bound covers the Rust heap only, never total process
memory. It does not cover results you collect or raw access.

**Data semantics, still open:**
- dividend and corporate-action adjustment;
- later corrections;
- early closes.

## Assets

| File | Platform |
| --- | --- |
| `praevis_thetadata-0.1.0b1-cp310-abi3-win_amd64.whl` | Windows x86_64, CPython 3.10+ |
| `praevis_thetadata-0.1.0b1-cp310-abi3-manylinux_2_28_x86_64.whl` | Linux x86_64, glibc 2.28+, CPython 3.10+ |
