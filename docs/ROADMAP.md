# Roadmap

This is a forward-looking work list, not a changelog. Delivered work is in
[CHANGELOG.md](../CHANGELOG.md). A recommendation becomes a requirement only
when it is selected for implementation. Its inclusion here does not approve
the whole market-data scope.

## Where the project stands (2026-11-08)

The [performance architecture plan](architecture/performance-plan.md)
(approved 2026-11-03) replaced the earlier slice-by-slice plan (see
[History](#history)).

- **Endpoints:**
  - **All 81 vendor wrappers** of the pinned Python 1.0.17 package exist in
    two forms:
    - generated Rust requests (`endpoints::*`, with explicit presence and the
      vendor's defaults);
    - same-name Python methods with the vendor's signatures
      (`praevis_thetadata.Client`).
  - **The other 19 RPCs are descriptor-only:** 17 futures and 2 corporate
    actions.
  - Per-endpoint status is in [endpoint coverage](endpoints/coverage.md).
- **Phases:**
  - **P0 is done** ([results](performance/p0-results.md)).
  - **P1 is done** ([exit results](performance/p1-exit-results.md)), with
    two recorded misses: Python into Polars at 9.0-9.2× against the 10×
    target, and criterion (d).
- **Options PRO is done** ([options phase](architecture/options-phase.md)):
  - **50 of the 51 option RPCs are value-checked live:** the columnar
    decoder against the legacy decoder, plus the Python package.
  - **The remaining one is the option quote flat file** (about 38 GB). Even a
    probe needs the user's go-ahead.
  - **The D2 targets are met** against the vendor package
    ([results](performance/options-d2-results.md)).
  - **Memory is bounded by configuration** on the streaming path, with the
    stated limits.
- **Real-time (W10) is done on the dev feed, and for quotes on the live
  market** (ADR-0018 W10 record; the
  [stream crate](../crates/praevis-thetadata-stream/README.md)):
  - quotes and the full trade stream through the operator's Terminal;
  - p99 ≤ 50 µs met at the measured rates, live included (2026-11-10:
    24-32 µs at 10, 100 and 1,000 contracts, Windows).

  The live full trade stream ran with 0 dropped at midday and across the
  close run-up (2026-11-10, Windows), and under WSL2 (2026-11-11). Under
  WSL the p99 target was not met; native Linux remains (below).
- **Releases:**
  - **`0.1.0-beta.2` was published on 2026-11-12**
    ([record](crates-io-release-plan.md#publication-record-010-beta2)), on
    crates.io and on PyPI as
    [`praevis-thetadata` 0.1.0b2](https://pypi.org/project/praevis-thetadata/0.1.0b2/).
    It has one deliberate breaking change:
    [ADR-0021](adr/0021-per-call-library.md).
  - **`0.1.0-beta.1`, the API freeze, was published on 2026-11-11**
    ([record](crates-io-release-plan.md#publication-record-010-beta1)), on
    crates.io and, for the first time, on PyPI as
    [`praevis-thetadata` 0.1.0b1](https://pypi.org/project/praevis-thetadata/0.1.0b1/).
  - `0.1.0-alpha.1`, `0.1.0-alpha.2` and `0.1.0-alpha.3` (2026-11-08) came
    before it ([releases](releases.md)). Alpha.3 added the arrow and stream
    crates. Its wheels were on the GitHub release only, not on PyPI.
  - **`0.1.0`, the first supported release, was published on 2026-11-13**
    ([record](crates-io-release-plan.md#publication-record-010)), on
    crates.io and on PyPI as [`praevis-thetadata` 0.1.0](https://pypi.org/project/praevis-thetadata/0.1.0/). It is the
    beta.1 surface as amended by beta.2, with the known gaps listed
    ([scope record](releases.md#010-scope-record-user-2026-11-12)).

## Waiting on resources (blocks 0.1.0 evidence)

Two evidence items for `0.1.0` cannot run on what the user has today:
- no stock or index subscription (the account is stocks FREE and indices
  FREE);
- no bare-metal Linux machine.

**Release status:**
- **Decided (user, 2026-11-12): both ship in `0.1.0` as documented known
  gaps.** They do not block it. Native Linux W10 latency concerns only the
  experimental stream crate.
- **The `0.1.0` release notes list them as they stand below**
  ([0.1.0 scope record](releases.md#010-scope-record-user-2026-11-12)).

### 1. Wrappers seen only as refusals

**What is missing:** data-path evidence for 20 of the 81 vendor wrappers,
and the one wrapper never probed
([evidence review](requirements/beta1-evidence-review.md),
[coverage table](endpoints/coverage.md)).
- **19 tier refusals** on this account (`PermissionDenied`, `sir-checks-001`).
  The tier is the one the service named:
  - **stocks VALUE:** `stock_snapshot_ohlc`, `stock_snapshot_quote`,
    `stock_history_ohlc`, `stock_history_quote` and `stock_at_time_quote`;
  - **stocks STANDARD:** `stock_snapshot_trade`,
    `stock_snapshot_market_value`, `stock_history_trade`,
    `stock_history_trade_quote` and `stock_at_time_trade`;
  - **stocks PRO:** `stock_flat_file_trade_quote` and `stock_flat_file_eod`;
  - **indices VALUE:** `index_history_price` and `index_at_time_price`;
  - **indices STANDARD:** `index_snapshot_ohlc`, `index_snapshot_price`,
    `index_snapshot_market_value` and `index_history_ohlc`;
  - **indices PRO:** `index_flat_file_eod`.
- **The 20th, `option_flat_file_quote`:** an options PRO wrapper. It was
  refused once with `InvalidArgument`, a classification that is uncertain.
  A subscription would not help. A re-probe is gated on the user's
  go-ahead because of the file's size (see
  [Python parity](#python-parity)).
- **Never probed:** `stock_flat_file_quote`. It is gated, because even a
  probe may start the vendor's generation of a very large file. The tier
  it needs is unknown; PRO is likely, like the other stock flat files.

**Why it matters:**
- These wrappers decode through the generic path. That path is checked on
  other RPCs, but not on these responses.
- Their column kinds are not in [`schemas.json`](endpoints/schemas.json),
  so a column that is null throughout a response has no registered type.
- `0.1.0` would ship them unverified against real data.

**What unblocks it:** a stock and/or index subscription at the tier the
refusal named:
- VALUE or STANDARD for the snapshot, history and at-time wrappers;
- PRO for the flat files.

Either product alone unblocks its own wrappers. The stock quote flat file
also needs the user's explicit approval to probe.

**The run, once unblocked:** each re-run needs the user's approval.
1. **Rust:** `cargo run -p praevis-thetadata-client --features live-tests
   --example live_p0 -- entitlements --set stock-index-rate --confirm-live
   --api-key-env --run-id <id> --flat-file-date <a date in the last 7 days>`.
   The key comes from `THETADATA_API_KEY` only.
   - It covers every stock, index, calendar and interest-rate wrapper once,
     paced and sequential. Each response is decoded both ways and compared
     value by value.
   - The flat files are probes of at most 1 MiB.
   - Add `--include-stock-quote-flat-file` only with that separate approval.
2. **Python:** `python bench/p1/stock_index_rate_checks.py --run-id <id>`,
   the same requests through the package's vendor-named methods.
3. **Then:**
   - classify the refusals and successes from the private reports;
   - update [`status.json`](endpoints/status.json) and the subscription
     table, then register the observed column kinds in `schemas.json`;
   - regenerate with `tools/generate_endpoints.py`.

   Only codes and classifications are committed.

### 2. Native Linux W10 latency

**What is missing:** the W10 latency target on native Linux. The
p99 ≤ 50 µs target:
- **Windows:** met (24-32 µs live, 2026-11-10).
- **Linux under WSL2:** missed, at 61-68 µs (2026-11-11, `w10-wsl-*`;
  [W10 above](#real-time-stream-w10-what-remains)).
- **Native Linux:** never measured.

Drops were 0 everywhere, so only the latency target is open.

**Why it matters:**
- Linux is a supported platform for the stream crate.
- WSL2 virtualisation is the likely cause of the miss, but that is not
  established.
- Until a native run, the Linux p99 claim is unknown.

**What unblocks it:** a bare-metal Linux x86_64 machine, not a VM or WSL,
running the Terminal (Java 21) on an options PRO account.

**The run, once unblocked:** the same as the WSL runs, during market hours,
with the Terminal running and started by the user.
- **Build:** release, as under WSL. The WSL builds were `cargo build
  --release --locked -p praevis-thetadata-stream --features live-tests
  --examples`, run as `target/release/examples/{discover_contracts,latency}`.
  The `cargo run --release` commands below are equivalent.

1. **Contracts:** `cargo run -p praevis-thetadata-stream --features
   live-tests --release --locked --example discover_contracts -- --confirm-live
   --seconds 60`. It writes the 10, 100 and 1,000 lists.
2. **Quotes:** `cargo run -p praevis-thetadata-stream --features live-tests
   --release --locked --example latency -- --confirm-live --feed live --run-id
   <id> --contracts <list>`, once each at 10, 100 and 1,000 contracts.
3. **Full stream:** the same with `--full-trades --seconds 1200` in place
   of `--contracts`.

Record p50 to max, drops, heartbeat gaps, CPU and the Rust heap, as in the
WSL table (ADR-0018 W10 record). Single runs are observations.

## Python parity

Parity here means the vendor package's 81 wrappers, each with:
- a live entitlement result on the account's tiers;
- values checked through our decoders and our Python package.

Options are done. What remains:

1. **Stocks (16 wrappers), indices (10), calendar (3) and interest rate (1):
   checked as far as this account allows.**
   - **The sweep ran on 2026-11-10** as `sir-checks-001`, with
     `sir-checks-py-001` for the Python package. It was the one run of all
     30 that the user approved on 2026-11-07
     (`live_p0 entitlements --set stock-index-rate`).
   - **Entitled and value-checked:** the stock and index list and EOD
     wrappers, every calendar request and the rate history.
   - **Refused, naming the tier:** the other stock and index wrappers. The
     account is stocks FREE and indices FREE.
   - **Not probed:** the stock quote flat file, which is gated.
   - **What remains needs a subscription:**
     [Waiting on resources](#1-wrappers-seen-only-as-refusals).
2. **An endpoint-by-subscription table: done.** The
   [coverage table](endpoints/coverage.md) gives, per wrapper, the minimum
   tier the service named (FREE, VALUE, STANDARD or PRO, per product), from
   the FREE map, the options PRO map and that sweep. Where no run has
   established it, the table says unknown.
3. **Futures (17 RPCs) and corporate actions (2): descriptor-only.**
   - **Blocked:** they have no vendor wrapper, no public documentation, no
     symbology and no result schemas.
   - **Watched** by the [upstream monitor](upstream-monitor.md). Act when a
     vendor release adds wrappers or documentation.
   - Until then, keep the raw bindings without promising service behaviour.

The option quote flat file (above) is the one option RPC left. It is gated
on the user's go-ahead because of its size, not blocked.

## Out of scope: a shared data service for many consumers

**Not part of this project** (user correction, 2026-11-08). This project
replaces the official ThetaData Python package: one program's client,
with no target of many concurrent workloads in one process. **No load
tests are planned here.**

**The shared data service is a separate project: the private repository
`Praevis-Quant/praevis-data`,** with its own agent. It builds a persistent
service on this library, so that several services can read from it, or tap
a stream at the same time, without pulling more data from the vendor.

**The split, decided by the user (2026-11-13):**

| Lives in the service (`praevis-data`) | Lives in this library |
| --- | --- |
| cache keys and de-duplication | one caller's requests, as fast as possible (ADR-0021) |
| backfill progress, resume and persistence | the in-process resource bounds |
| stream reconnect and resubscribe | the Terminal stream and its close handshake |
| fan-out to many consumers | |
| scheduling across consumers, within the account's plan | a job's own concurrency |
| retry (for example on `Busy`) and rate limits | `QueryError::Busy`, never retried |
| default deadlines, a shared read-ahead budget, fairness, session handling | per-call deadlines when the caller sets one |

**Additions to this library, only when the service asks for them,** each
additive (no existing call changes) and in its own pull request. **None
of them is in `0.1.0`:**
- calling an endpoint by name at runtime from Rust (dispatch from data);
- public job planning: the date splits and the per-strike split;
- a getter for the stream's current subscriptions;
- stock and index streaming, if a real need appears;
- #136 (a reader-assigned `Event` sequence), only if the service needs gap
  detection.

**The library does what one caller asks** (user decision 2026-11-12,
[ADR-0021](adr/0021-per-call-library.md), from `0.1.0-beta.2`): no retry,
account-wide scheduler, rate limit or default deadline, as in the vendor
package. A job's own concurrency defaults to the tier's documented limit.

**The licence limits any such service to the licensee's own use.**
ThetaData's subscriber agreement permits "internal personal use" only
(Section 1(a) and Exhibit A), and forbids redistributing data or derived
works (Section 8(e)) and sharing credentials. A shared service must
therefore serve only the licensee and never other people
([performance plan, H.2](architecture/performance-plan.md#h2-license-and-vendor-terms)).

**What this library does today, for that project to start from.** These
observations come **from reading the code and are untested**:
1. **Nothing coordinates callers** (from beta.2). Jobs and queries running at
   the same time can together exceed the account's plan; the service then
   answers `RESOURCE_EXHAUSTED`, which the library reports as `Busy`.
2. **The read-ahead is per job,** with no budget shared across jobs. Each
   job's configured worst case is 4 × concurrency parts × 2 batches × 8 MiB
   (32 × 2 × 8 MiB = 512 MiB at PRO), so many concurrent jobs multiply it.
3. **There is no fairness between jobs.** The only shared admission is the
   in-process stream limit (`EodPolicy::concurrent_streams`, a memory bound),
   and one job with a deep read-ahead can take most of it.
4. **No deadline unless the caller sets one.** A set deadline still counts
   time spent waiting for an in-process stream slot.
5. **One session serves everything.** A single login, and a single point of
   failure if it ends (`SessionEnded`).
6. **Many open streams can each assemble a message at once** (item 8 on
   the query path: up to streams × `encoded_bytes` while messages are in
   flight; closed for this project as a stated, tested bound).

**Earlier ideas, now settled by the split above:**
- **Become library additions, on request:** dispatch by name, the public
  planner, and stock and index streaming.
- **Moved to the service:** a cache key per request, and stream reconnect.
- **Not planned in the library unless the service asks:** part-level
  results from a job, and request metrics (time to open, time to first
  row).
- **#136 stays closed and parked** (2026-11-11; not rejected, its commits
  are kept and it can be reopened). It comes back only if the service needs
  gap detection. Its 10-contract p99.9 was unexplained in a 6-round desktop
  comparison.

**Limits by design:**
- the stream reads a Terminal on the same machine only;
- one session per account;
- `QueryError` carries `tonic::Code` for remote statuses, accepted for now.

Item 8 (transport buffering) was a beta.1 gate on its own merits, not for
this use case. It closed on 2026-11-10.

## Real-time stream (W10): what remains

- **Live `--no-login` subscribe probe: done 2026-11-10.** The first quote
  came 1,032 ms after the subscribe, so the earlier 150 s delay did not
  reproduce. It stays unexplained.
- **Live-market latency runs:**
  - **Done 2026-11-10 on Windows, after #141:** 10, 100 and 1,000
    contracts, with `quotes_unsubscribed` 0. p99 was 24-32 µs, and p99.9
    was 37-40 µs at 100 and 1,000 contracts. These replace a first attempt
    measured under leftover load.
- **Live full trade stream: done 2026-11-10 on Windows.**
  - **Midday:** 15 minutes and 410,468 trades, with 0 dropped.
  - **Across the close run-up** (`w10-live-full-close-001`, 20:37-20:57
    UTC):
    - 2,020,545 events (481 trades/s) with 0 dropped and 0 heartbeat gaps;
    - p99 27 µs, p99.9 312 µs and a 29 ms max, from two descheduling
      bursts;
    - about 0.085 cores, and a flat Rust heap.
  - **The W10 "sustained full-stream load without drops" exit criterion is
    met** (single runs, Windows). The close run's rate was only about 5%
    above midday.
- **Linux under WSL2: done 2026-11-11** (`w10-wsl-*`, single runs).
  - **No drops** at 10, 100 and 1,000 contracts, and on a 20-minute full
    trade stream at about 1.7× the Windows close rate (989,373 trades).
  - **But the p99 was 61-68 µs, above the 50 µs target.** The p90 was
    44-47 µs against 17-20 on Windows; the p50 was unchanged. WSL2
    virtualisation is the likely cause, not established.
  - **Still to do:** native Linux. That decides the target there.
- **The `Remote(Internal)` error on a second gRPC request** is unexplained.
  An idle connection was ruled out. It stays open until there is evidence.
- **For users:** run either the Terminal's REST API or our historical client
  on one account, not both (ADR-0018 W10 record).

## Optional Theta Terminal connection track

[ADR-0018](adr/0018-explicit-connection-adapters.md) and the
[CONN requirements](requirements/connections.md) define three adapters. The
WebSocket subscription adapter exists (the stream crate). Two remain
candidates:
- a REST adapter for an operator-started v3 JAR;
- optional process supervision, with explicit ownership and version
  provenance.

The rules for the track:
- Keep the direct HTTPS/gRPC path Java-free, and allow no implicit transport
  fallback.
- Use the [connection inventory](connections.md) to keep endpoints, errors,
  entitlements and lifecycle evidence distinct.
- Terminal support does not bypass subscription restrictions or imply
  complete Python parity.

## Deferred work

**Hardening (proposed `0.1.0-alpha.3`).** Seven items were deferred from
alpha.2 on 2026-11-02:
1. memory profiling;
2. the exhaustive adversarial corpus;
3. a synthetic trusted-TLS fixture;
4. controlled-clock interleavings with a transport seam;
5. a `Price` numeric helper;
6. finalizing the desktop tolerances;
7. a deterministic schema-lease/job-permit release order.

They are listed with sources and targets in
[Deferred from alpha.2](releases.md#deferred-from-alpha2). Item 1, the
memory profile, found transport buffering outside the shared budget. That was
item 8, **closed on 2026-11-10** with a tested bound
([closure](performance/eod-transport-buffering.md#closure-2026-11-10-under-decision-as-beta1-gate)).
Byte credits stay held until real messages reach the tens of MiB.

**Re-enable Decimal128 Parquet statistics after the upstream fix ships.**
`ParquetSink` writes no decimal min/max statistics by default. This works
around an upstream bug: `parquet` 60.1 retains whole batches through those
statistics ([apache/arrow-rs#12027](https://github.com/apache/arrow-rs/issues/12027)).
The fix, [apache/arrow-rs#12028](https://github.com/apache/arrow-rs/pull/12028),
is merged but not yet released
([record](architecture/options-phase.md#memory-bound-under-w5-planned)).

When a `parquet` release containing #12028 ships (60.1.1 or 61):
1. Bump `parquet`.
2. Re-run the 140M-row Parquet memory check (`examples/job_memory.rs`).
3. If memory stays flat, re-enable Decimal128 statistics by default and drop
   the workaround.

Do not switch to a git dependency in the meantime.

**Consumer wait strategy for the real-time stream (deferred 2026-11-07).**
An opt-in spin or spin-then-park wait for `Stream::recv*` could remove the
consumer's wake-up from the stream latency, at the cost of CPU. The user
deferred it; sleeping stays the only and default behaviour. The problem,
options, costs, proposed API, gates and decision record are in
[consumer wait strategy](performance/consumer-wait-strategy.md).

**Library extensions, only with their own requirements:**
- optional local filtering and projection;
- bounded aggregation and windowing;
- further export adapters and caching.

Each needs separate consumer requirements, state budgets and
provenance/invalidation rules, and must be benchmarked against the adapter
bypass. Use the
[architecture review](research/architecture-performance-review.md) to weigh
dependency costs before extracting further crates.

**crates.io follow-ups:**
- add a backup crates.io owner.

(The CLI description fix shipped with alpha.2.)

**Python package before a first publish:**
- packaging and wheels;
- the supported Python versions and platforms;
- free-threading support.

Publishing it needs separate approval, like any release.

### Coverage-guided fuzzing (after 0.1.0)

The maintainer explicitly wants this; decided 2026-11-03 (D-G). Add
coverage-guided fuzzing with `cargo-fuzz` (libFuzzer, nightly toolchain) in a
**separate crate outside the workspace**. Stable builds, the published
packages and ordinary CI then never depend on nightly or the fuzzer.

- **Targets:** the bounded decoder in numeric and Table modes, the legacy raw
  decoder, the outer envelope parser, and the ZSTD framing path.
- **Seeds:** the deterministic [adversarial corpus](../crates/praevis-thetadata-client/src/corpus.rs).
- **Oracles:** the corpus oracles carry over:
  - no panic;
  - classified errors only;
  - accepted output within `DecodeLimits`;
  - numeric/Table agreement;
  - envelope agreement with prost;
  - exact-value agreement with the legacy decoder.
- **Runs:** time-boxed manual or scheduled runs, never a blocking CI step.
- **Findings:** each finding becomes a minimized deterministic regression case
  in the plain-Rust corpus. Crash inputs that could contain vendor data are not
  committed.

The plain-Rust corpus (deferred item 2, closed in alpha.3) remains the CI gate.

### Native Rust development tooling (xtask)

Evaluate migrating the performance harness and upstream version checker to a Rust `xtask` so routine builds, verification, and benchmarks use the Rust toolchain. Preserve synthetic fixtures, native-store cleanup, before/after build provenance, raw samples, comparison semantics, and regression coverage. Validate the replacement against the existing measurements before retiring the current tooling. Keep Python-specific artifact analysis scoped to research. This is a separate candidate from the runtime startup implementation.

## Respond to upstream compatibility changes

For each newer stable release detected by [the scheduled PyPI monitor](upstream-monitor.md),
research the versioned source artifacts and assess compatibility before updating the
tracked Python baseline. Keep that baseline separate from the Rust crate version; a
failed check initiates research and requirements review, not an automatic baseline bump.

The 1.0.12 → 1.0.17 update is complete ([plan and record](upstream-update-1.0.17.md)).
Remaining follow-ups:

- **Futures and corporate actions:** see [Python parity](#python-parity), item 3.
- **Timestamp precision:** supersede ADR-0019 with a representation decision
  once ThetaData defines `nanos`/`precision`.
- **ADR-0019 performance follow-up:** the Linux before/after comparison in #18
  showed a possible ~1–2% Table-adapter decode increase on `small-mixed-none`
  and `large-mixed-none`. The shift was consistent across three rounds and just
  outside the baseline's A/A spread. It was not seen on ZSTD variants, the
  numeric engine or the timestamp-heavy cases. Windows was not benchmarked.
  Reproduce on both platforms under PERF-L3-007/008 before deciding whether to
  act; this is an open observation, not an accepted regression.

Use the [upgrade impact template](upstream-update-template.md) and exhaustive
[origin register](requirements/origins.md). Reconcile upstream contracts and
Rust enhancements independently; preserve exact values, native-store/lifecycle
policy, numeric delivery, filtering and performance/bounds through adapter
changes. Run separate compatibility/enhancement checks and relevant before/after
measurements. A conflicting upstream change needs an explicit migration ADR,
not silent removal or reinterpretation of local functionality.

## Related project research

Review [ThetaDataDx and other tracked projects](research/related-projects.md)
at release planning and before selecting event, binding or dataframe features.
Record revisions and independently verify useful design/performance claims;
keep third-party ideas separate from the pinned Python compatibility baseline.

## History

**Before the 2026-11-03 direction reset,** this roadmap planned
slice-by-slice work:
- complete and accept the one-symbol stock EOD slice against its L1-L3
  requirements;
- extend the coverage matrix item by item;
- then add typed option, index, calendar and rate slices, a PyO3 adapter,
  and a Terminal track.

The [performance plan](architecture/performance-plan.md) replaced that
ordering with generated endpoints, a columnar Arrow path, a tier-aware
scheduler, PyO3 and real-time streaming. The EOD slice's open evidence items
stay in [stock EOD evidence](requirements/stock-eod-evidence.md). The old
text is in this file's git history.

The original performance review that started this work is preserved
below, byte for byte (`tools/research_python.py` checks it).

## Original review and recommendations (verbatim)

The review below is preserved exactly, including its table and original workstation links. Its measurements and validation paragraph are supporting evidence for the proposed work, not a completion log. Machine-local citations are historical and may not open on another machine.

The project has a good starting structure: authentication is isolated, HTTP/gRPC clients are reusable, and market data is consumed batch by batch. **The best immediate optimization is CLI startup; the largest scaling concerns are decoded-table memory and CPU work inside async calls.**

I reviewed commit `0caf272`, ran the workspace tests, and performed synthetic measurements. No tracked files changed.

1. **Bound decoded memory before expanding market-data usage.**  
   The limit in [decode.rs](/C:/Users/Joey/Documents/GIT-GitHub/Praevis-Quant/thetadata-rs/crates/thetadata-client/src/decode.rs:7) checks compressed and decompressed protobuf bytes. It does **not** bound the resulting table’s heap usage.

   A synthetic 100,000-row × 8-column null table occupies **1.8 MB on the wire but 28 MB just for Rust row/value storage**—excluding allocator overhead and intermediate decoding buffers. Concurrent streams multiply this cost.

   Recommend row/cell limits enforced before large allocations, a shared memory/concurrency budget, and an explicit ZSTD window limit. Checking row counts only after `DataTable::decode` would be too late to prevent its allocations. [ZSTD decoder controls](https://docs.rs/zstd/0.13.3/zstd/stream/read/struct.Decoder.html#method.window_log_max)

2. **Avoid starting a worker pool for every CLI command.**  
   [main.rs](/C:/Users/Joey/Documents/GIT-GitHub/Praevis-Quant/thetadata-rs/apps/thetadata-cli/src/main.rs:216) starts Tokio before parsing arguments. Consequently, even `status`, `logout`, and help initialize the runtime. Tokio defaults to a worker per available CPU. [Tokio documentation](https://docs.rs/tokio/latest/tokio/attr.main.html)

   On this Windows machine, two runs per configuration, each with 60 measured samples, produced:

   | Median latency | Default workers | One worker |
   |---|---:|---:|
   | Authentication + save | 31.8–32.4 ms | 30.0–31.4 ms |
   | Persisted status | 22.5–23.2 ms | 21.0 ms |

   Recommend synchronous argument parsing and status/logout paths, creating a `current_thread` runtime only for authentication. The experiment supports reducing startup work; it does not measure that proposed implementation directly.

3. **Move substantial decoding work off async executor threads.**  
   [next_batch()](/C:/Users/Joey/Documents/GIT-GitHub/Praevis-Quant/thetadata-rs/crates/thetadata-client/src/lib.rs:151) performs ZSTD decompression, protobuf decoding, timestamp formatting, and validation synchronously after receiving a message. Large batches can delay other tasks sharing that executor thread.

   Benchmark bounded `spawn_blocking` decoding or a dedicated CPU pool. Preserve backpressure and limit concurrent jobs; spawning unlimited decoding tasks could worsen memory pressure. Started blocking tasks also require deliberate cancellation handling. [Tokio guidance](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html)

4. **Keep timestamps numeric until presentation.**  
   [timestamp()](/C:/Users/Joey/Documents/GIT-GitHub/Praevis-Quant/thetadata-rs/crates/thetadata-core/src/lib.rs:65) converts every timestamp into an allocated RFC 3339 string during decoding—even when callers only need filtering or calculations.

   Recommend storing epoch milliseconds plus timezone information and formatting when displaying/exporting. Subsequently benchmark flat row storage or typed columns against `Vec<Vec<Value>>`. These are API changes with potentially meaningful allocation savings; throughput gains remain unmeasured.

5. **Reduce per-batch allocation churn after establishing a decoder benchmark.**  
   [The decompression path](/C:/Users/Joey/Documents/GIT-GitHub/Praevis-Quant/thetadata-rs/crates/thetadata-client/src/decode.rs:21) creates a decoder and grows a fresh output vector for every batch.

   Evaluate reusable decompression state, bounded buffer reuse, and conservative capacity reservation. Retain expansion checks and avoid retaining the largest-ever buffer indefinitely.

6. **Expand performance coverage before setting regression thresholds.**  
   [The benchmark](/C:/Users/Joey/Documents/GIT-GitHub/Praevis-Quant/thetadata-rs/tools/benchmark_auth.py:74) currently measures whole-process auth/status latency. Add separate measurements for decoding throughput, allocations, peak memory, timestamp conversion, and concurrent-stream responsiveness. Record raw samples, CPU/runtime configuration, and Rust version to improve comparisons.

Validation passed: **14 unit tests, one doctest, and the repository audit**. The synthetic auth/persistence benchmarks also completed successfully; results are in [performance-review artifacts](/C:/Users/Joey/Documents/GIT-GitHub/Praevis-Quant/thetadata-rs/artifacts/performance-review). Live-service throughput and decoder speedups were not measured.
