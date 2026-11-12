# GitHub release draft: v0.1.0-beta.2 (not published)

Status: **draft.** Publication needs the user's approval
([checklist](crates-io-release-plan.md#beta2-publication-checklist-and-order-prepared-2026-11-12)).
Nothing is tagged or published yet.

**To fill in at publication:**
- the date;
- the commit;
- the exact wheel file names, from that commit's CI artifacts
  `wheel-ubuntu-latest` and `wheel-windows-latest`. The same two files go
  to PyPI and to this release.

---

**Title:** `v0.1.0-beta.2`. Mark it as a pre-release.

**Body:**

> **Beta prerelease.** praevis-thetadata is an independent, unofficial client
> for ThetaData's market-data service. It is not affiliated with, sponsored by
> or endorsed by Theta Data Inc., and it is not full parity with the vendor's
> Python package.

## One deliberate breaking change

The library now does **what one caller asks, as fast as possible**, as the
vendor package does ([ADR-0021](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.0-beta.2/docs/adr/0021-per-call-library.md)):
- **No retry.** A request is sent once. A busy account (`RESOURCE_EXHAUSTED`)
  is the new `QueryError::Busy` (Python: kind `"busy"`).
- **No account-wide scheduler or rate limit.** `SchedulerPolicy` and
  `with_scheduler` are removed. A job's own concurrency defaults to the
  account tier's documented limit (FREE 1, VALUE 2, STANDARD 4, PRO 8), so one
  job alone stays within the plan. Several jobs at once are not coordinated.
- **No default timeout.** Set one per client or per call if you want one.

Coordinating many callers (scheduling, retry, rate limits, default deadlines)
is left to a future shared data service. Everything else frozen at beta.1 is
unchanged, as are the in-process memory and transport bounds.

## Migration

**Rust:**
- **Remove `.with_scheduler(policy)`.** Set a job's concurrency per call with
  `RequestOptions::default().with_concurrency(Some(n))`.
- **`scheduler_policy().concurrency`** becomes `tier().concurrency()`.
- **`ClientConfig` timeouts are `Option<Duration>`, default `None`.** To keep
  the old bounds:
  `.with_request_timeout(Some(Duration::from_secs(300))).with_idle_timeout(Some(Duration::from_secs(60)))`.
- **Match `QueryError::Busy`** where you matched `Remote(ResourceExhausted)`.
  Retry it yourself if you want to.

**Python:**
- **`Client(concurrency=…, per_minute=…, retries=…)` raises `TypeError`.** Pass
  `concurrency=` per call to `query`, `batches` and `to_parquet`.
- **`Client.scheduler`** becomes `Client.default_concurrency`.
- **To keep the old timeouts,** use `Client(request_timeout=300, idle_timeout=60)`.
  That applies to the vendor-named methods too.
- **A busy account** raises `QueryFailed` with kind `"busy"`.

The full list is in [CHANGELOG.md](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.0-beta.2/CHANGELOG.md).

## Install

**Rust** (prereleases need an exact version):

```toml
praevis-thetadata-client = "=0.1.0-beta.2"
praevis-thetadata-stream = "=0.1.0-beta.2"   # optional, real time, experimental
```

```text
cargo install praevis-thetadata-cli --version 0.1.0-beta.2 --locked
```

**Python** (3.10 or newer; tested on 3.10-3.13), from PyPI. It is a
pre-release, so pass `--pre` or pin the version:

```text
pip install --pre praevis-thetadata
# or
pip install praevis-thetadata==0.1.0b2
```

Wheels exist for Windows x86_64 and Linux x86_64 (glibc 2.28 or newer) only.
There is no macOS or ARM wheel, and no source distribution.

**Alternative:** install the same wheel for your platform from this release's
assets:

```text
# Windows x86_64
pip install https://github.com/Praevis-Quant/praevis-thetadata/releases/download/v0.1.0-beta.2/praevis_thetadata-0.1.0b2-cp310-abi3-win_amd64.whl
# Linux x86_64 (glibc 2.28 or newer)
pip install https://github.com/Praevis-Quant/praevis-thetadata/releases/download/v0.1.0-beta.2/praevis_thetadata-0.1.0b2-cp310-abi3-manylinux_2_28_x86_64.whl
```

The wheel has no required dependencies. Install the dataframe library you
use: `pip install polars pyarrow`, or `pandas pyarrow`.

## Supported platforms

Windows x86_64, and Linux x86_64 with glibc 2.28 or newer. macOS and ARM are
not supported, and there are no wheels for them.

## Known gaps

From the [evidence review](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.0-beta.2/docs/requirements/beta1-evidence-review.md):
- **Endpoints:** 60 of the 81 vendor-named endpoints are value-checked live.
  20 were only ever refused on the test account, and one (the stock quote
  flat file) was never probed.
- **Accounts:** paid stock and index tiers, and the STAGE environment, were
  never run.
- **`Busy`** has synthetic evidence only: the service's `RESOURCE_EXHAUSTED`
  was never observed live.
- **Live checks** ran on Windows, and the real-time crate also under WSL2.
  Native Linux is open, and the real-time p99 target was not met under WSL.
- **Memory:** the bound covers the Rust heap only, never total process memory.

## Assets

| File | Platform |
| --- | --- |
| `praevis_thetadata-0.1.0b2-cp310-abi3-win_amd64.whl` | Windows x86_64, CPython 3.10+ |
| `praevis_thetadata-0.1.0b2-cp310-abi3-manylinux_2_28_x86_64.whl` | Linux x86_64, glibc 2.28+, CPython 3.10+ |
