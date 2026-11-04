# ThetaDataDx review (performance plan H, P0)

Reviewed **2026-11-03** at
[`f42350fc69c897b7503030f9c274577e8628dd43`](https://github.com/userFRM/ThetaDataDx/tree/f42350fc69c897b7503030f9c274577e8628dd43)
(latest commit 2026-11-03), extending the
[2026-10-04 watchlist entry](related-projects.md#thetadatadx).

**Method:** a shallow clone was read only. **No code was copied, built,
executed or benchmarked**, and no account was used. Statements about runtime
behavior are the project's own documentation and code comments, not our
measurements. Decisions in [the plan](../architecture/performance-plan.md#h-related-project-review-thetadatadx)
cite this record.

## Identity and license

- **License:** Apache-2.0 (`LICENSE`), the same as this project. Any reuse
  would need file-level attribution and NOTICE handling under section 4.
  **None is planned.**
- **Activity:** the clone shows a recent, single-maintainer-dominated history.
  The Rust crate is published as `thetadatadx-rs`, with Python
  (`thetadatadx-py`), TypeScript and C++ bindings over one Rust engine.
- **Positioning:** "Connects straight to ThetaData, with nothing to install
  and run locally". It is not presented as an official vendor product.

## Architecture as built

| Area | What the code shows | Source |
| --- | --- | --- |
| Historical transport | tonic gRPC over its own hyper HTTP/2 handling to `mdds-01.thetadata.us` (`mdds-stage` for STAGE) | `thetadatadx-rs/src/grpc/mod.rs`, `config/environment.rs` |
| Connection pool | `ChannelPool`: N parallel HTTP/2 connections, least-loaded selection, so each connection has its own flow-control window and stream budget | `grpc/pool.rs` |
| Concurrency | The subscription tier integer from the Nexus auth response maps to a request semaphore of **2^tier** (FREE 1, VALUE 2, STANDARD 4, PRO 8), described as a default the server ultimately enforces; overridable | `mdds/tier.rs`, `mdds/client.rs` |
| Sharding | One large pull becomes N equal-span date/time bands dispatched concurrently. A comment claims "the server's per-account send rate caps a single stream well below the account's aggregate budget, so the fan-out multiplies bulk throughput" | `mdds/shard.rs` |
| Request identity | Sends `QueryInfo.client_type = "rust-thetadatadx"` and its crate version in `terminal_version` | `config/auth.rs`, `mdds/client.rs` |
| Decode | `ResponseData` → `DataTable` → **row structs** (`Vec<Tick>`), with a header-alias table mapping v3 column names to its tick schema | `mdds/decode/` |
| Values | Price columns are **`f64`** (`(@type price) => { f64 }`; `Price::to_f64` with power-of-10 tables); option strike `f64` | `tdbe/types/tick.rs`, `tdbe/types/price.rs` |
| Python boundary | `pyclass` row lists: `to_list()` creates one Python object per row; `to_arrow()` copies rows into Arrow columns once, then hands them to pyarrow through the Arrow C Data Interface; `to_polars()`/`to_pandas()` via pyarrow | `thetadatadx-py/src/ticks.rs` |
| Flat files | The same gRPC streaming; rows chunked to a handler or written to CSV/JSON/NDJSON/HTML (no Parquet), via a temporary file renamed on completion | `flatfiles/` |
| Real-time | A **direct FPSS client** (for example `nj-a.thetadata.us:20100`, dev port 20200) with reconnect, backoff, jitter and host failover; one live session per account | `config/environment.rs`, `config/fpss.rs`, `config/reconnect.rs`, `fpss/` |
| Python parity tracking | `parity.json` lists classes, functions and error codes across bindings | `parity.json` |
| Benchmarks | A decode-allocation bench and a streaming-throughput bench; **no comparison against the vendor Python package** found | `thetadatadx-rs/benches/` |

## Vendor terms

ThetaData's [subscriber agreement](https://www.thetadata.net/subscriber-agreement):

- **Section 1(b)** prohibits attempts to "reverse engineer, disassemble,
  decompile, decode, adapt, or otherwise attempt to derive". The FPSS wire
  protocol is not publicly documented; ThetaData documents streaming only
  through the Terminal. So a direct FPSS client is the area of highest
  terms-of-service risk, and **this project does not build one** (plan
  decision 4).
- **Section 1(a)** and Exhibit A limit data use to "internal personal use",
  forbid redistributing data or derived works (Section 8(e)), credential
  sharing, and simultaneous use of multiple accounts.
- Our direct gRPC path uses the descriptor that the vendor ships in its own
  Python package, which ThetaData documents as connecting "directly to Theta
  Data servers" over gRPC. The user is asking ThetaData to confirm that
  third-party clients on this route are acceptable (plan G.8).

## What we learn, and where we do better

| Topic | Lesson or improvement | Plan action |
| --- | --- | --- |
| Tier detection | Adopt: size the scheduler from the auth response's subscription fields (we already parse them) | P1 scheduler default; P0 records the fields |
| Fan-out and connections | **Hypothesis to measure**, not to assume: does one stream have a per-connection or per-stream send cap, so that sharding a request (and spreading it over connections) beats one request? | P0 records single-request throughput. On VALUE and above, measure 1 versus N shards and 1 versus N connections |
| Exact values | Do better: keep `Decimal128(38,10)` and nanosecond timestamps; never convert prices to `f64` | P1 columnar decoder |
| Decode shape | Do better: decode straight into Arrow builders; no row structs and no row-to-column copy | P1 |
| Python boundary | Do better: no per-row Python objects on any default path, with a measured gate; vendor names as zero-cost aliases | P1 D.2 gates |
| Flat files | Do better: stream into Parquet with bounded memory; keep the atomic rename-on-complete idea | P3/PRO |
| Real-time | Do not follow: official Terminal path only | Real-time phase (STANDARD) |
| Benchmarks | Fill their gap: an apples-to-apples comparison against the vendor Python package (`bench/python-baseline/`) | P0 |
| Identity | Keep identifying ourselves honestly in query metadata (`client=rust`) | — |

## P1 exit status of the improvements (2026-11-04)

The improvements above that P1 owned, as measured in the
[P1 exit record](../performance/p1-exit-results.md):

| Improvement | P1 status |
| --- | --- |
| Exact values | **Delivered:** prices are `Decimal128(38, 10)`, kept exact through Arrow, Parquet, Polars and pandas (Arrow-backed). Timestamps are millisecond with their zone, per decision D1; nanoseconds wait for a verified feed |
| Decode shape | **Delivered:** messages decode straight into Arrow builders, with no row structs, at 3.46 / 6.07 µs/row on the real P0 captures |
| Python boundary | **Delivered:** no per-row Python objects (CI gate), and vendor-named aliases add about 0.6 µs per call |
| Benchmarks against the vendor package | **Delivered:** the [baseline](../../bench/python-baseline/README.md) and the P1 exit comparison |
| Tier detection | **Partly:** the scheduler defaults to FREE's documented limits (decision D5), and other tiers are set explicitly. Sizing from the auth response's subscription fields is not done |
| Fan-out and connections | **Open:** to be measured on a paid tier before relying on it |

## Monitoring and re-review

This file is the **monitoring record** for ThetaDataDx. Re-review it **at each
phase exit** (P1, each tier upgrade) and before the real-time and flat-file
phases. Record the new revision, the date and any material change below. Keep
the related-project no-copy rule.

### Watch items

| Item | What to look for | Why it matters |
| --- | --- | --- |
| Direct-FPSS legality | Any ThetaData statement permitting or forbidding third-party direct FPSS clients; any change in their FPSS client or disclaimer | Our Terminal-only decision rests on subscriber agreement §1(b); new vendor guidance could change it |
| `f64` prices | Whether they move to exact decimals | Our exactness advantage, and the correctness risk their users carry |
| Per-row Python objects | Whether `to_list()` stays the default path, or they publish allocation numbers | Our D.2 gate claim: no per-row Python objects |
| Sharding throughput claim | Any measurement of single-stream caps or fan-out speedups | P0 found one request slow on the server's side; we measure fan-out on paid tiers before relying on it |
| Vendor-Python comparison | Any benchmark against the official `thetadata` package | Our baseline (`bench/python-baseline/`) is currently unique; if they add one, compare methods |
| Market-data sessions | Their README says the real-time feed is one live session per account, but market data "runs alongside streaming and across as many `MarketDataClient` instances as you like". Each instance logs in separately (`mdds/client.rs`, `connect` calls Nexus `authenticate_at`) | Our P1 exit run saw the first of two separately logged-in clients fail its first request about 118 ms in, each time, consistent with a new login ending the earlier session. That conflicts with their claim, and neither side has the gRPC code. Until a recorded run settles it, our clients share one session (`existing_authorized_client`) and treat the rule as unconfirmed |

### Review log

| Date | Revision | Notes |
| --- | --- | --- |
| 2026-11-03 | `f42350f` | Initial detailed review (this record) |
| 2026-11-04 | `f42350f` | **P1 phase-exit re-review.** No commits since `f42350f` (latest push 2026-11-03), and no release since v0.8.2 (2026-11-02). Every watch item is unchanged: direct FPSS, `f64` prices, `to_list()` per-row objects, the unmeasured sharding claim, and no vendor-Python comparison. **New watch item:** market-data sessions (above), where their README conflicts with our P1 observation. The P1 status of each improvement is added above. Read-only again: nothing copied, built or run |
