# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versioned releases will follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Future work and undecided requirements are tracked in [docs/ROADMAP.md](docs/ROADMAP.md).

`0.1.0-alpha.1` was the first published prerelease (crates.io, tag
`v0.1.0-alpha.1`, 2026-11-02 UTC), followed by `0.1.0-alpha.2` (tag
`v0.1.0-alpha.2`, 2026-11-03 UTC) and `0.1.0-alpha.3` (tag `v0.1.0-alpha.3`,
2026-11-08). Each release heading covers the entries
below it, down to the previous release. The Python compatibility baseline is separately
tracked in [the protocol manifest](crates/praevis-thetadata-proto/schema/manifest.json).

## [Unreleased]

### Added

- **`Price::numeric_cmp` and `Price::numeric_eq`** (deferred item 5, for the
  beta.1 freeze). They compare prices by value, exactly, whatever their
  exponents: `123.45` and `123.4500` are numerically equal.
  - **Sorting:** `prices.sort_by(Price::numeric_cmp)`.
  - **How:** at a common exponent in `u128`, or, when the exponents are 10
    or more apart, by magnitude, since an `i32` mantissa has at most 10
    digits. So there's no overflow for any exponents.
  - **Unchanged:** `==` stays representational, and `Price` deliberately
    does not implement `Ord`, which would disagree with `Eq`.
  - **Tests:** exact and extreme cases, and 20,000 pseudo-random wire-range
    pairs checked against an independent scaling oracle. Moving the
    shortcut's boundary fails them.

### Changed

- **Item 8 (transport buffering) is closed for beta.1** (decision A's gate;
  item 8 PR 4).
  - **The evidence:** option 2a on both callers (#129), the query-path
    bound test (#130), and `opt-msgsize-001`. Real messages were at most
    0.36 MiB over 1,820 messages, with none above 1 MiB (options W5c and
    W6c only; no flat files, stocks or indices).
  - **The bound, now stated in AGENTS.md, the client README, the W5 record
    and the release notes:** the Rust heap on `query()`/`job()` and the EOD
    stream is bounded by configuration, plus up to streams × `encoded_bytes`
    while messages are in flight (8 × 64 MiB worst case at PRO, about 3 MiB
    at observed sizes).
  - **Not covered:** process RSS and `stream_raw`.
  - **Held:** byte credits, until messages reach the tens of MiB.
  - **Kept:** `encoded_bytes` stays at 64 MiB.
- **ROADMAP: the reader-assigned `Event` sequence (#136) is held, not in
  beta.1** (Coordinator, to confirm with the user).
  - **The run:** in a 6-round desktop comparison, the median and the p99
    were unchanged. At 10 contracts, though, p99.9 was above 150 µs in 4 of
    6 candidate runs against 1 of 6 baseline runs, which is unexplained.
  - **Why holding is safe:** `Event` is non-exhaustive since #132, so the
    field can be added later without breaking callers.
- **Breaking (pre-beta.1): per-call request options** (API change 4 of 5).
  `query_with` and `job_with` take a `RequestOptions` instead of a
  `BatchPolicy`. It holds:
  - the batch policy;
  - an overall timeout;
  - an idle timeout;
  - for jobs, a concurrency of at most the scheduler's slots. More is
    refused with `Configuration`, and the read-ahead window scales with it.

  Each option left unset uses the client's setting.
  - **Decided and documented: the timeout covers time queued for a
    slot.** It is a wall-clock budget from submission, as
    `request_timeout` already was, so a busy scheduler cannot hold a caller
    past it. In a job, each part has its own budget. Tests:
    - a queued request fails with `Deadline` at its 300 ms budget without
      reaching the server;
    - a per-call idle timeout overrides the client's;
    - a job capped at 2 needs four rounds for 8 parts;
    - out-of-range concurrency is refused.

    Mutation-checked.
  - **Python:** `query`, `batches` and `to_parquet` take `timeout`,
    `idle_timeout` (seconds) and `concurrency` as keyword-only options.
    The vendor-named methods are unchanged.
- **Stream handles: one thread can subscribe while another waits for
  events** (API change 3 of 5).
  - **The problem:** `Stream` held a `std::sync::mpsc::Receiver`, so it was
    not `Sync`.
  - **The split:** `Stream::split()` now gives a clonable `StreamControl`
    (`Clone + Send + Sync`: subscribe, unsubscribe, stats, close) and the
    single `StreamEvents` receiver (`Send`). `Stream` keeps working as one
    owner.
  - **Closing:** a close from any handle, or dropping the events, still
    unsubscribes everything before the close frame (#114), once.
  - **Unchanged:** the reader thread and the event path, which stay free
    of allocation.
  - **Tests:** compile-time `Send`/`Sync` assertions, a cross-thread
    subscribe-while-waiting test, and close-from-any-handle tests. The
    latency harness uses the split.
- **Breaking (pre-beta.1): `Endpoint` is sealed** (API change 2 of 5). Only
  the generated requests implement it, through a private supertrait the
  generator emits. Code outside the crate still uses it as a bound, as the
  Python crate does, but cannot implement it. A `compile_fail` doctest
  checks this, and it fails if the trait is unsealed. Nothing outside the
  workspace has a legitimate need to implement it: a request is one vendor
  RPC with its exact wire mapping. Sealing also lets the trait gain items
  without breaking callers.
- **Breaking (pre-beta.1): the public API can now grow without breaking
  callers** (user-approved, 2026-11-08).
  - **Enums:** every public error, kind, status and policy enum is
    `#[non_exhaustive]`, so matches need a fallback arm.
  - **Output structs:** these are `#[non_exhaustive]` too.
  - **Configuration structs:** these are `#[non_exhaustive]` with one
    `with_<field>` setter per field (`ClientConfig::default().with_endpoint(…)`).
    Struct literals with `..Default::default()` no longer compile outside
    the crate. Assigning a field after `default()` still works.
  - **Generated requests:** all 100 are `#[non_exhaustive]`, and the
    descriptor-only ones gain field setters.
  - **Unchanged:** `Right`, `Contract`, `Root`, both `Price` types and
    `Table`.
  - **ROADMAP:** the out-of-scope shared-service section now lists the
    possible additive API and the limits by design.
- **ROADMAP: the "many workloads in one process" section is now marked
  out of scope** (user correction, 2026-11-08).
  - **Why:** this project replaces the official Python package. A future,
    separate shared data service built on this library would own
    cross-consumer scheduling, a shared read-ahead budget, fairness,
    deadlines while queued, and sessions.
  - **The licence:** any such service is for the licensee's own use only
    (Section 1(a), Exhibit A and Section 8(e)).
  - **Kept:** the code-reading observations, still marked untested. No load
    tests are planned.
  - **Also corrected:** the 1,000-workload target is removed from the item
    8 record. Item 8 stays a beta.1 gate on its own merits.
- **`live_p0 sizes` reports each request's largest encoded message and a
  size histogram** (item 8, PR 0; manual, never CI). It shows how large the
  messages that `query()`/`job()` streams assemble really are.
- **Item 8's record is extended for the query path.** There, a stream
  reads without a job slot, so option 2a removes idle retention and one
  copy, but N streams can still assemble N max-size messages at once. The
  record also holds the user's 2026-11-08 plan:
  - PRs 0, 1, 3 and 4 proceed;
  - byte credits are held until real messages reach the tens of MiB.
- **ROADMAP: "Many workloads in one process (1000 concurrent workloads)",**
  ahead of the beta.1 API freeze. It lists five concerns found by reading
  the code, untested:
  - a paused consumer holds its slot;
  - the read-ahead is per job, with no shared budget;
  - there is no fairness between jobs;
  - deadlines run while requests are queued;
  - one session serves all workloads.

  No design is selected, and load tests are to be discussed with the user
  first.
- **A query-path memory bound** (`eod_memory -- bound`, item 8 PR 3), run
  in the memory-profile workflow on Windows and Linux. It uses 8 `query()`
  streams of 24 and 64 MiB messages, in two scenarios:
  - **Drained at once:** peak growth must stay within the shared budget,
    plus 8 × `encoded_bytes`, the connection window and a margin.
  - **Stalled after one batch:** each stream may keep only its window,
    nothing message-sized.

  On `main` without 2a, the stalled streams keep 206 and 527 MiB and the
  check fails. With 2a they keep about 14 MiB.
- **The client assembles each response message itself** (item 8, option
  2a), on both `query()`/`job()` and the stock EOD stream.
  - **How:** `GuardedBody` checks each gRPC length prefix, allocates one
    exact-capacity buffer, assembles the message and parks it in a
    one-message slot, and tonic sees only an empty frame. The codec moves
    the message out instead of copying it.
  - **Memory effect:** tonic's decode buffer no longer grows to message
    size, so an idle stream keeps nothing message-sized. The copy from
    tonic's buffer is gone too.
  - **The size guard:** `GuardedBody` is now the only size guard, and its
    limit check still runs before any allocation.
  - **The tonic contract:** the design relies on tonic decoding each frame
    before polling again. A contract test checks this, and parking twice is
    an error, never a silent overwrite.
  - **Exclusion:** `stream_raw` stays on plain tonic and is documented as
    excluded.
  - **New `frame_alloc` example** (offline microbenchmark). It compares
    the old and new receive sequences per message, at 16 KiB to 2 MiB
    chunks:
    - old: a reused buffer, then `vec![0; n]` and a full copy;
    - 2a: `with_capacity` and a per-chunk copy;
    - plus zeroed and one-copy variants.
- **Alpha.3 publication recorded**
  ([record](docs/crates-io-release-plan.md#publication-record-010-alpha3)).
  It covers:
  - the seven crates and their checksums;
  - the tag and the GitHub pre-release with the two wheels;
  - the Windows and Linux post-publication checks;
  - docs.rs.

  The CHANGELOG section is dated, and the release draft and release train
  are marked published.
- **W10 thread-priority results recorded** (consumer wait strategy and
  ADR-0018; one run each).
  - **Priority:** raising the consumer's priority did not reliably change
    the Windows max.
  - **Pinning:** pinning a sleeping consumer was harmful (max up to 72 ms).
    The write-up now says not to pin a consumer that blocks.
  - **Spinning** stayed at a p99 of 6-7 µs at every setting.
  - **No library change.**

## [0.1.0-alpha.3] - 2026-11-08

Published to crates.io from `7402533` (tag `v0.1.0-alpha.3`), with two new
crates, `praevis-thetadata-arrow` and `praevis-thetadata-stream`. The
Python package's wheels are attached to the
[GitHub release](https://github.com/Praevis-Quant/praevis-thetadata/releases/tag/v0.1.0-alpha.3),
not on PyPI. See the [scope record](docs/releases.md#alpha3-scope-record-2026-11-08)
and the [publication record](docs/crates-io-release-plan.md#publication-record-010-alpha3).

### Release

- **Version `0.1.0-alpha.3`** across the workspace.
- **Two new published crates:** `praevis-thetadata-arrow` (the client
  depends on it) and `praevis-thetadata-stream` (experimental). Both get
  `documentation` links. The stream crate also gets LICENSE and NOTICE
  copies, keywords, categories and an `examples/` exclusion, and joins the
  repository's package-file check.
- **The client's description** now matches its surface: typed requests for
  every RPC, Arrow query and job streams, the scheduler and Parquet.
- **The ROADMAP's CLI description follow-up is closed.** The fix had
  already shipped with alpha.2.
- **The packaging rehearsal covers all seven published crates,** in
  publication order (stream last, as wave 4).
  - **External consumers:** new ones for arrow (an exact decimal and a
    registered all-null column) and stream (a parsed quote).
  - **Archive inspection:** it now also rejects key, certificate and SSH
    key files.
  - **Python:** CI smoke-imports the abi3 wheel in a clean environment
    (checking the version and the 100 endpoints) and keeps it as an
    artifact for the GitHub release. It is never uploaded to PyPI.
  - **Linux wheel:** built inside PyPA's `manylinux_2_28` image (pinned by
    digest) instead of on the runner. It installs on glibc 2.28 or newer
    (RHEL 8, Debian 11, Ubuntu 20.04-era), and it is the wheel the smoke
    test installs.
- **A GitHub release draft** ([text](docs/release-draft-0.1.0-alpha.3.md),
  not published). It covers the wheel assets and how to `pip install`
  from them, since the package is not on PyPI.

### Fixed

- **A column that is null throughout a response keeps its type when the
  endpoint's columns are registered.**
  - **The defect:** the wire carries no column types, and the columnar
    decoder took each column's type from its first value. An all-null
    column became Arrow `Null`. For example, a closed day's `calendar_on_date`
    returned `open`/`close` as `Null` and an open day's as `Utf8`, so joining
    the two results failed. The vendor package behaves the same.
  - **The fix:** `docs/endpoints/schemas.json` registers each endpoint's
    observed column kinds (names and kinds only, citing run IDs), and the
    generated `Endpoint::COLUMNS` passes them to the decoder
    (`ColumnarDecoder::with_column_kinds`). Rust, Python and Parquet all go
    through it.
  - **Coverage so far:** the three calendar RPCs (`cal-local-001`), as
    text. Other endpoints follow as their observed schemas are recorded;
    unregistered endpoints are unchanged.
  - **Recorded in:** an addendum to ADR-0009.
- **56 more endpoints register their observed column kinds.**
  - **Source:** the private Rust reports `opt-checks-001`, `opt-same-001`
    (options PRO) and `p1-checks-001` (FREE). Names and Arrow types only,
    each RPC citing its runs. There were no conflicts across runs and no
    all-null columns.
  - **Total:** 59 RPCs, with the calendar.
  - **Recorded as observed:** the option flat files carry `strike` and
    prices as text and `expiration` and `date` as numbers, unlike the query
    endpoints.
- **Closing or dropping a `Stream` now unsubscribes what it subscribed.**
  - **The defect:** before, it only closed the socket. The Terminal's
    subscriptions are shared by all its clients, and it does not document
    dropping them when a socket closes, so they could keep arriving on the
    next connection.
  - **Where it showed:** the W10 10-contract latency runs received about
    25k quotes/s, the rate of 1,000 contracts. Leftover subscriptions are
    the likely cause; this is not yet verified on the Terminal.
  - **The fix:** the stream tracks its quote and full-trade subscriptions,
    and on close sends `add:false` for each before the close frame.
    Loopback tests cover `close` and drop, and both fail without the fix.

### Changed

- **The alpha.3 scope is recorded** ([releases](docs/releases.md#alpha3-scope-record-2026-11-08)).
  The user decided on 2026-11-07 to prepare alpha.3: crates.io only, with
  the Python wheels attached to the GitHub release and not on PyPI. The
  record covers:
  - what ships since alpha.2;
  - a disposition for each deferred item;
  - the publish set in dependency order, with the two new crates (arrow,
    stream) and the unpublished Python crate.
- **Item 8 (transport buffering) is put to the user as a decision for
  alpha.3.** The user made alpha.3 wait on it on 2026-11-03.
  - **What was found:** the new query path still keeps tonic's
    per-stream message buffer.
  - **Correction:** the W5 memory record's stated worst case omitted that
    term (up to 8 × 64 MiB at PRO), and is corrected.
  - **The options:** ship with it open and move it to beta.1 (recommended),
    or hold alpha.3 for option 2a or 2b.
  - **Decided (user, 2026-11-08): A.** Alpha.3 ships with item 8 open,
    and it stays a gate for beta.1 / 0.1.0, with "only accounted
    allocations are bounded".
- **The roadmap reflects where the project stands.** It now opens with the
  current state:
  - all 81 vendor wrappers exist as generated Rust requests and same-name
    Python methods;
  - P0, P1 and options PRO are done;
  - 50 of the 51 option RPCs are value-checked;
  - W10 is done on the dev feed.

  A new **Python parity** section lists what remains:
  - live and value checks for the 30 stock, index, calendar and
    interest-rate wrappers;
  - an endpoint-by-subscription table;
  - futures and corporate actions, which are descriptor-only and blocked on
    vendor documentation.

  The deferred items are kept, and the pre-reset stock-EOD plan is reduced
  to a short history note. The verbatim original review stays at the end,
  unchanged.
- **The Python 1.0.17 catalogue's feature dispositions match the code.**
  - **Now supported,** each linked to its implementation and tests and
    stating its limits: query wrappers, date and time, presence and
    defaults, gRPC transport, errors, conversion, prices, timestamps,
    dataframes, decoded memory, construction and flat files.
  - **Live streaming stays unresolved** as parity, with the Terminal
    WebSocket adapter linked.
  - **Unchanged:** stock EOD, query metadata (still `client=rust` only) and
    the vendor examples.
- **W10 low-rate sleep-against-spin runs recorded** (consumer wait strategy
  and ADR-0018), one run each, after a Terminal restart and with #114:
  - **Result:** at about 670 quotes/s, spinning cut the p99 from 37 to
    10 µs, for a full core against 0.027 cores, with a worse p99.9 and max.
  - **Correction:** the earlier 10-contract runs are now marked as
    inflated by leftover Terminal subscriptions.
  - **Status:** the deferral stands.
- **The decimal-statistics default now cites the upstream bug.** The `parquet`
  memory growth behind it is apache/arrow-rs#12027, fixed by #12028. The fix
  is merged but not in a release; 60.1.0 is still the latest. The sink
  documentation, the READMEs and the options-phase record now say so. A
  ROADMAP item covers what to do once a release includes the fix: bump
  `parquet`, re-run the 140M-row memory check, and, if memory stays flat,
  re-enable Decimal128 statistics by default and drop the workaround. We do
  not switch to a git dependency in the meantime.
- **The latency harness reports the contracts it actually saw:**
  `contracts_seen`, `contracts_seen_unsubscribed` and `quotes_unsubscribed`,
  so a run's real rate and contract set are visible. CI now runs the
  real-time harness's offline unit tests.
- **The stream crate's zero-allocation test counts only its own thread.**
  - **The defect:** the counter was process-wide, so it also caught libtest's
    main thread allocating while the test ran. That failed CI once on
    unchanged code (4 allocations). On one core, 34 of 1,000 runs saw 2
    such allocations, none from the measuring thread.
  - **The fix:** a thread-local flag around the measured window. The
    zero-allocation assertion is unchanged.
  - **Checks:** a self-check confirms the counter sees this thread's
    allocations, and a mutation (one allocation in `parse`) fails with
    10,000.
- **Parquet output keeps memory flat: no min/max statistics on decimal
  columns by default.**
  - **The cause:** the `parquet` writer (60.1) holds each row group's
    decimal statistics as slices of the whole written batch until the file
    closes. Its memory grew linearly: 4.3 GiB for 140M rows, 1 GiB for a
    139M-row options job.
  - **After the fix:** 25.7 MiB and 107.6 MiB.
  - **Unchanged:** every other column keeps its statistics.
  - **Opt-in:** `ParquetOptions::decimal_statistics` and Python
    `to_parquet(decimal_statistics=True)` restore decimal statistics, at
    the stated memory cost.
  - **Tests and reproduction:** a test checks flat memory over 100 row
    groups (+0.4 MiB, against +100.5 MiB with decimal statistics).
    `examples/job_memory.rs` is the reproduction.
- **Jobs keep every request slot busy.** A job now sends a part whenever a
  request slot is free, not only after the head part has been read, and
  still returns rows in order.
  - **Bound:** at most 4× the concurrency parts are open, each buffering at
    most 2 batches.
  - **Why:** the live `job()` check (`opt-split-002`) was exact but took
    14.1 s against 9.1 s for the hand-made 8-way split. The job left slots
    idle behind a slow head part (head-of-line blocking; simulated 18.4 s
    against 10.0 s).
  - `live_p0 split-probe --no-splits` runs only the references and `job()`.
    It replaces an empty `--concurrency`, which Windows PowerShell 5.1 drops.
- **The vendor-alias overhead gate (D.2) is robust to runner noise.** It
  times the direct path and the alias in 21 interleaved blocks each, in
  alternating order, and compares each side's minimum. Noise only adds
  time, so the minimum is the cleanest estimate. A mutation test checks
  that an alias costing at least 10 µs more still fails.
  - **Before:** it timed all direct calls and then all alias calls, so runner
    noise between the two phases decided the result. Windows CI measured
    -12.6 to +9.3 µs on unchanged code and failed once on noise alone.
  - **Unchanged:** the 5 µs limit.
- Added a user guide, [where the time goes in a request](docs/performance/request-timing.md): measured option request waits and sizes, what is measured versus predicted, and practical advice.
- `AGENTS.md` is now the single home for agent guidance and `CLAUDE.md` only points to it, matching aero1553, so every agent reads the same rules. Stale statements (planned PyO3, the proposed primary path, next-work status) were updated in the move; nothing else was dropped.
- **One zstd wrapper:** the workspace now uses `zstd` 0.14 (`zstd-safe` 8),
  the version `parquet` already brought in, over the same `zstd-sys` 2.1.2
  (libzstd 1.5.7). The lockfile drops `zstd` 0.13 and `zstd-safe` 7. No code
  changed. All tests and the adversarial corpus pass with their pinned ZSTD
  classifications. Before and after (aarch64, 3 alternating rounds): the
  Arrow and legacy decoders are unchanged, and the bounded decoder is 1.7-2.4%
  faster. The D-B before/after workflow is the gate.

### Added

- **Thread-priority options in the latency harness** (W10 experiment,
  user-approved 2026-11-07; harness only, no library change).
  - **`--consumer-priority`** `normal|above-normal|highest|time-critical`:
    Windows `SetThreadPriority`. Linux supports `normal` only and refuses
    the rest.
  - **`--consumer-affinity <core>`:** `SetThreadAffinityMask` or
    `sched_setaffinity`.
  - **`--process-priority high`:** Windows only.
  - **Report:** `consumer_scheduling` records the values read back from
    the OS. A setting that cannot be applied fails the run before it
    connects. No new dependencies: kernel32 and glibc are called directly.
- **The stock, index, calendar and interest-rate checks** (manual; one run
  approved by the user on 2026-11-07).
  - **Rust:** `live_p0 entitlements --set stock-index-rate` sends each of
    the 30 RPCs with a vendor wrapper in those products once, paced and
    sequential. One response per request is decoded both ways (columnar
    and legacy) and compared. A refusal keeps the server's status text in
    the private report, for a person to classify the tier.
  - **Calendar:** `calendar_open_today`, `calendar_on_date` (a weekday, a
    weekend, 2025-12-25 and the 2025-11-28 early close) and `calendar_year`
    (2025 and 2026), with their decoded rows kept in the private report.
  - **Flat files:** index EOD, stock EOD and stock trade-quote are bounded
    probes. The stock quote flat file is **gated** behind
    `--include-stock-quote-flat-file`, off by default, because even a probe
    could start the vendor's generation of a very large file. Coverage shows
    it as "not probed (gated)".
  - **Python:** `bench/p1/stock_index_rate_checks.py` runs the same
    requests through the Python package.
- **Endpoint coverage shows the subscription each wrapper requires.**
  - **The rule:** the tier the service named when refusing, or FREE where a
    FREE tier of that product was entitled. Anything else is unknown; it is
    never inferred from the family.
  - **Summary:** a table counts the wrappers per product and tier.
  - **Relabelled:** the options PRO columns are now "PRO account" (options
    PRO; stocks and indices FREE).
- **Full trade stream and contract discovery** (`praevis-thetadata-stream`).
  - **Trades:** `Kind::Trade` (contract plus exact trade fields, from the
    vendor's documented example), `subscribe_full_trades` /
    `unsubscribe_full_trades`, and a `trades` counter.
  - **`discover_contracts`** (manual, no login): writes the top 10, 100 and
    1,000 contracts by trade count in the latency harness's format, with
    the trade date in each header. This is the replay day on the dev feed.
- **W10 dev-feed latency recorded** (ADR-0018). The p99 ≤ 50 µs target:
  - **Met** at 100 and 1,000 contracts (38 and 35 µs, up to 25k quotes/s, 0
    dropped).
  - **Not reliably met at 10:** 50 and 57 µs, because a sleeping consumer
    pays a thread wake-up per quote, and Windows wake-ups are slow and
    variable.
  - **Not yet explained:** a max of about 4 ms on Windows, and 1.07 cores at
    25k quotes/s.
  - **Not yet run:** the live market.
- **Latency diagnostics** (W10).
  - **Harness:** reports the 20 slowest events with their times, to tell
    periodic stalls from random ones. A measurement-only `--consumer spin`
    option isolates the cost of waking a sleeping consumer.
  - **Stream:** `Stats::heartbeat_max_interval_ms` shows how far apart the
    Terminal spaces heartbeats under load.
- **Consumer wait strategy for the real-time stream: deferred, with a full
  write-up.** [docs/performance/consumer-wait-strategy.md](docs/performance/consumer-wait-strategy.md)
  records the problem (the consumer's wake-up dominates the tail at low
  quote rates), the options and their CPU costs, what spinning cannot fix,
  a proposed opt-in `WaitStrategy` API, the gates before shipping, and the
  user's 2026-11-07 decision to keep sleeping as the only behaviour. Linked
  from the ROADMAP and ADR-0018. It includes the Windows sleep-against-spin
  runs (one each): p99 7 µs spinning against 31-34 µs sleeping, for about
  half a core, with no better max.
- **W10 record updated.**
  - **Restart:** a Terminal restart restores its REST session after our
    login ended it (verified, 200 within 15 s).
  - **Dev feed:** replays 2025-04-28, and a full-trade-stream subscribe
    delivered its first event in 1.5 s.
- **W10 findings recorded** (ADR-0018).
  - **Session:** our historical login ends the Terminal's REST session
    (200 to 478), but its live stream survives (24,135 quotes, no drops).
  - **The `Internal` on a later gRPC request stays unexplained.** An idle
    connection is ruled out (`w10-idle-001`: all OK after up to 180 s idle).
  - **The 150 s wait for the first quote is pending** the `--no-login`
    probe.
- **Diagnostics for the login-conflict probe's two open findings**
  (`w10-conflict-001`).
  - **Stream parser:** decodes the Terminal's `REQ_RESPONSE`
    (`SUBSCRIBED`, `MAX_STREAMS_REACHED`, `INVALID_PERMS`, `ERROR`, and the
    request id), and keeps the type name of other messages.
  - **`login_conflict_probe --no-login`:** watches a subscription alone, and
    logs every non-quote message and the first quote, with times.
  - **`live_p0 idle-reconnect`:** sends the same request after growing idle
    gaps, through `query()` and the raw stub. The server's status text stays
    in the private report only.
- **W10 latency harness** (`praevis-thetadata-stream` example `latency`,
  manual, `live-tests`).
  - **What it does:** subscribes to quotes for a contract file and records
    added latency, from frame read to consumer, at p50, p90, p99, p99.9 and
    max in a preallocated histogram.
  - **Also records:** quotes per second, drops, heartbeat gaps,
    allocations per frame and process CPU.
  - **No login:** the Terminal holds the credentials.
- **Step G recorded: the three approved option flat files to Parquet.**
  - **Results:** open interest (2.0M rows, 1.7 s), EOD (2.0M rows, 3.9 s)
    and trade/quote (13.8M rows, 38.1 s, 297.8 MB of Parquet against the
    vendor's 1.2 GB CSV). Heap peaks were 14-33 MiB.
  - **The stream is data:** no manifest anywhere in the streams.
  - **Timeout:** the 300 s default suffices.
  - **Coverage table:** marks the files as fully decoded, with values
    compared on the first message only.
- **`praevis-thetadata-stream`** (W10, unreleased, `publish = false`).
  Real-time option quotes from the operator's Theta Terminal WebSocket.
  - **Delivery:** one reader thread, an allocation-free parser with exact
    decimal prices, and a bounded, preallocated queue. When the queue is
    full it drops the newest event and counts it, and never blocks the
    Terminal.
  - **Counters:** heartbeat gaps, unparsed frames and drops.
  - **Dependency:** `tungstenite` (no TLS), approved for the real-time path
    only. Auth and history users do not pull it.
  - **Tests:** a loopback WebSocket server, and a counting allocator
    showing parse plus queue hand-off allocate nothing.
  - **`login_conflict_probe`** (manual, `live-tests`): what our gRPC login
    does to the Terminal's stream and its REST session.
- **W10 design note** (ADR-0018). The real-time slice covers:
  - **Connection:** the operator's Terminal WebSocket only, through a new
    `praevis-thetadata-stream` crate (`tungstenite`, no TLS).
  - **Events:** fixed-size, parsed without allocation, with exact decimal
    prices, delivered through a bounded queue that drops and counts when the
    consumer is slow.
  - **Measurement:** latency from the frame read to the consumer at p50,
    p99 and p99.9.
  - **Probe:** the login-conflict probe.
  - **Order:** the dev server first, then scale, then the live market.
- **Option snapshots value-verified on identical bytes** (`opt-same-001`).
  - **Snapshots:** all 14 decode equal through the columnar and legacy
    decoders from one response. The earlier differences were the market
    moving between two requests.
  - **Flat files:** the first message of each of the three probed files
    (8,192 rows) decodes as ordinary tables, equal both ways.
  - **Coverage table:** records both, in two new check values.
- **Step G runner: option flat files to Parquet** (`live_job_memory
  --flat-file open-interest|eod|trade-quote --date <day>`, manual).
  - **What it does:** downloads one whole file through `job()` to private
    Parquet, deleted afterwards, measuring heap, RSS, rows, batch sizes and
    time.
  - **Timeout:** a longer request timeout, 1800 s by default.
  - **Refused:** the option quote file (about 38 GB). It is not an accepted
    value.
  - **AGENTS.md:** records the flat-file rule. Probes are bounded; full
    downloads of these three files are approved, and the quote file and all
    other flat files still need approval.
- **`live_p0 option-checks --same-response`.** Each request is sent once,
  and every message of that response is decoded by both the columnar decoder
  (the one `query()` uses) and the legacy decoder, then compared.
  - **Why:** separate live requests made the snapshot checks
    inconclusive, because the market moves in between.
  - **Flat files:** the first message only (at most 1 MiB), decoded both
    ways. This tests whether flat-file data decodes as ordinary tables before
    step G downloads any file.
- **Step F results recorded** (`opt-checks-001`).
  - **Values equal to the legacy decoder:** 34 option endpoints.
  - **Rows equal, values pending:** 13 snapshots. The two requests ran
    seconds apart in a moving market; a same-response replay is next.
  - **Python:** all 47 OK.
  - **Snapshots:** entitled during market hours.
  - **Flat files:** the three probed files stream data, not manifests.
  - **Coverage table:** gains a value-check column from a closed
    vocabulary.
- **Step F live checks for every option endpoint** (manual, never CI).
  - **`live_p0 option-checks`:** every option RPC through `query()`, with
    values compared against the legacy decoder. Snapshots use a current
    expiration. Flat files are bounded probes that record whether the reply
    is a manifest; the 38 GB option quote file is opt-in only.
  - **`bench/p1/option_checks.py`:** the same requests through the Python
    package.
  - **Shared requests:** the PRO map and the checks now use one request set,
    so they cannot drift apart.
- **W5 memory target recorded as met for the streaming path.** Over 7 live
  runs of up to 26.6M rows, `job()` to Parquet peaked at 103-182 MiB of Rust
  heap, with no growth with rows.
  - **Why it varies:** the spread comes from stalls (a slow request lets the
    bounded read-ahead fill).
  - **Not bounded:** Parquet footer metadata (about 13 KB per row group) and
    the Python process baseline.
  - The failed single-run prediction stays in the record as the reason for
    the repeats.
  - **AGENTS.md:** the memory rule gains one narrow, cited allowance for
    this path's Rust heap.
- **`live_job_memory`** (manual, never CI). It writes one live W5 job to
  Parquet under a counting allocator.
  - **What it records:** the Rust heap against rows written, the process
    peak working set or RSS, and batch sizes.
  - **What it is for:** finding why M5 (two days) used 204 MiB where 135
    were predicted, before any fix.
  - **Also added:** workload M4b, M4's other day. CI's clippy step builds
    the example on Windows too.
- **M5 and the job memory reproduction's controls.**
  - **M5:** M4 over two days. Same part sizes, twice the rows.
  - **`job_memory.rs`:** `JOB_MEMORY_STRIKES` sets the number of parts, and
    the output adds the process's peak RSS. Offline, quadrupling the parts
    at a fixed part size moves the peak only by Parquet's footer metadata.
  - The memory reruns are recorded in the plan. The target is not yet
    recorded as met.
- **`job()` per-strike split verified live** (`opt-split-003`). It returned
  the same 938,663 rows, exactly equal, in 7.4 s. That is 1.9× faster than
  before the read-ahead fix at a similar service speed, and 5.3-6.5× faster
  than the single all-strike request that day.
- **Memory-bound runner for W5** (`bench/p1/options_memory.py`, manual).
  - **What it does:** runs one job in one mode per process, either streamed
    to Parquet or held as a Polars frame, and records peak memory, rows and
    file size.
  - **Workloads:** M1-M3 are the W5b shape over 15, 60 and 120 minutes.
  - The private Parquet file is deleted after its size is recorded.
- **D2 interleaved repeat recorded.** Three cycles, each running the vendor,
  ours Python, Rust at 8 and Rust at 1 in that fixed order, give the same
  ratios as the sequential run. W5c at 8 is 9.4× (Python) and 8.4× (Rust)
  the synchronous vendor package, and 3.4× and 3.0× its threaded variant.
  The worst-case W5c pair is 7.1×. This resolves the drift caveat.
- **Per-strike split for all-strike fine option quotes** (`job()` and the
  Python methods with `plan`).
  - **What splits:** an `option_history_quote` for every strike at a `tick`
    or `1s` interval, with one concrete expiration and no `strike_range` or
    `max_dte`. When the scheduler runs at least 2 requests at once, the job
    lists the expiration's strikes (one extra request) and sends one request
    per strike, skipping strikes without data.
  - **Why:** the service waits about 60 s before answering such a request
    whole. Split 8 at a time, the same 938,663 rows came back exactly equal
    in 9.1 s instead of 58.9-69.6 s (`opt-split-001`).
  - **What stays whole:** `query()`, `plan=False` and every other shape.
  - **Order:** by date, then strike ascending.
  - **Probe:** `live_p0 split-probe` also runs `job()` and compares it
    exactly with the reference.
- **Options baselines D2 recorded**
  ([results](docs/performance/options-d2-results.md)).
  - **W5c (8-way):** 9.8× the synchronous vendor package (Python) and 11.0×
    (Rust); 3.3× and 3.7× its threaded variant.
  - **W4c and W6c:** 5.0-8.1× threaded and 9.3-10.5× synchronous.
  - **Why the vendor's threads gain little:** its client-side time does not
    overlap. This is consistent with work held under the GIL, but not
    profiled.
  - **Caveats:** sequential runs, one host, one date, and per-expiration
    workloads only. The memory bound is not yet assessed.
- **`live_p0 split-probe`** (manual, never CI). It tests splitting
  all-strike tick and 1 s quote requests per contract.
  - **Why:** the shape probes showed that those requests pay about 60 s
    before any data, while one contract over a whole day takes about 1 s.
  - **What it compares:** the all-strike request, run before and after, with
    the same request once per listed strike, at concurrency 1 and 8.
  - **What it reports:** wall times, rows, and whether the results are
    exactly equal, using an order-independent digest of the row values. Only
    the equality is reported.
- **Options baselines harness (D2)** (manual, never CI):
  - `options_vendor.py` (vendor package) and `bench/p1/options_ours.py`
    (this package): time whole workloads into Polars, synchronously and
    threaded at the same concurrency.
  - `live_p0 workload`: the same workloads through Rust `query()`, up to 8
    at a time.
  - **Workloads:** scaled 12-request W4c/W5c and 8-request W6c for fan-out,
    plus shape probes S1-S5 for the server-slow windowed quote requests.
  - **Correction:** "one row per message" holds for EOD only.
- **Like-for-like incremental memory.** Both clients ran only the public
  calls (`baseline.py --e2e-only` for the vendor).
  - W1 is 0.91×, and W2 is 0.53×, just over the 0.5 line. Total RSS is 0.98×
    and 0.88×.
  - These replace the earlier 0.88× / 0.33× and 0.78×, whose vendor runs also
    kept a raw drain in the same process.
  - Criterion d stays a recorded miss.
- **Options workloads W4-W6 and `live_p0 sizes`** (manual, never CI). First
  bounded definitions in `bench/python-baseline/workloads.json`: SPY on
  2024-06-14 at 1-minute and 1 s quotes, tick trades and quotes, and Greeks.
  `sizes` sends them one at a time and counts decoded rows and bytes per
  request. It drops a request once it passes `--max-rows` (default 1M, the
  vendor's sizing guide), so the volumes are measured before any baseline.
- **Incremental memory recorded** in the P1 exit record: peak minus the idle
  baseline after imports and login, beside the total.
  - W1 is 10.1 MiB for ours against 11.5 MiB for the vendor (0.88×).
  - W2 is 14.9 MiB against 44.5 MiB (0.33×).
  - The vendor harness also keeps its drained responses and converts them in
    the same process, so these ratios are not like-for-like.
  - `baseline.py --e2e-only` runs only the public calls, for a like-for-like
    rerun.
  - Criterion d stays a miss on total RSS.
- **`SessionEnded` error.** The service answers `UNAUTHENTICATED` when a
  later login for the account has ended the session. The session probe
  `opt-session-001` confirmed this on 2026-11-05: the first of two logins
  failed with that code.
  - Every query path now reports it as `QueryError::SessionEnded`: `query()`,
    `job()` and the stock EOD streams. Python raises `SessionEnded` (a
    `QueryFailed`): "session is no longer valid; another login may have ended
    it".
  - It is not retried, and the client never logs in again by itself.
  - EOD-L3-016 and its stock EOD matrix row are amended (Coordinator
    decision, 2026-11-05). Before, `UNAUTHENTICATED` stayed
    `Remote(Unauthenticated)` on the stock EOD streams. Only the variant
    changes; the behaviour is the same.
  - The P1 exit record, the ThetaDataDx review and the client docs now call
    the one-session rule confirmed.
- **Options PRO entitlement map recorded** (`opt-pro-entitlements-001`).
  - **Entitled:** 33 of 51 option RPCs, including every Greeks and
    trade-Greeks order. None is tier-denied.
  - **Uncertain:** the 14 snapshots (no data) and the 4 flat files
    (invalid argument).
  - **Coverage table:** gains an options PRO column beside FREE.
  - **Generator:** checks the PRO entries against the same closed
    vocabulary.
- **Tier-aware scheduler.** A new client sizes its scheduler from the
  account's tier in the login response: the highest of the stock, options
  and indices subscriptions, since the vendor sets concurrency account-wide.
  - FREE keeps the documented 1 concurrent request and 20 per minute.
  - VALUE, STANDARD and PRO get 2, 4 and 8 concurrent requests, with no
    per-minute limit (the vendor documents concurrency limits only).
  - Rust: `Tier`, `SchedulerPolicy::for_tier`, `ThetaClient::tier` and
    `scheduler_policy`; `with_scheduler` still overrides.
  - Python: `Client.tier` and `Client.scheduler`. Unset `concurrency` and
    `per_minute` come from the tier; `per_minute=0` removes the limit.
  - An unrecognised subscription value counts as FREE.
- **Options PRO live tooling** (manual, never CI):
  - `live_p0 entitlements --set options-pro --flat-file-date <day>`: every
    option RPC once, each read for a few messages. Flat files come last and
    read one message of at most 1 MiB: a larger one is refused from its
    length prefix before its body is read, so no bulk data is pulled.
  - `live_p0 session`: a second login, then one request on each session,
    recording both gRPC codes.
  - CLAUDE.md records the user's approval of the one PRO map run.
- **Dependency A/B benchmark mode.** `eod_benchmark compare
  --dependency-change`, and the `dependency_change` input of
  `eod-performance.yml`, compare two revisions whose only difference is
  `Cargo.lock`.
  - Every other provenance field must still match, and the flag with
    identical lockfiles is refused.
  - Reports and the summary are labelled "dependency A/B" with both lock
    hashes.
  - The 1.05/0.95 decode-p50 gate and the decision rules are unchanged
    (`benchmark-policy.md`, "Dependency A/B").
- **Options phase plan** ([plan](docs/architecture/options-phase.md)): the
  user's 2026-11-04 decisions (options PRO, P1 done, the incremental memory
  metric), the vendor facts re-verified for PRO, and the step order from
  measurement to optimization.
- **ThetaDataDx P1 phase-exit re-review**
  ([record](docs/research/thetadatadx-review.md)).
  - No changes since the reviewed revision, and every watch item is
    unchanged.
  - New watch item, market-data sessions: their README says separately
    logged-in market-data clients coexist. That conflicts with our P1
    observation, so the one-session explanation in the P1 exit record is
    marked unconfirmed.
  - Each improvement now has its P1 status.
- **P1 exit results** ([record](docs/performance/p1-exit-results.md)).
  - **a, met:** all 10 FREE-entitled endpoints, with values equal to the
    legacy decoder.
  - **b, met on medians, within the service's noise.**
  - **c, met for the Rust decoder** (15.9× / 10.4× less CPU than Python with
    Polars). Through the Python package into Polars it is 9.0× / 9.2×.
  - **d, not met:** peak memory is 0.97× / 0.79× the vendor's, with an
    incremental metric proposed for a separate decision.
  - **e, met.**
  - **Finding:** one active session per account.
- **P1 exit runners** (P1 PR 7, manual only; `bench/p1/`).
  - `live_p0 checks` runs the 10 FREE-entitled endpoints through `query()`.
    Each is compared value by value with the legacy decoder on a second
    request; the report holds statuses, counts and schemas, never values.
  - `live_checks.py` runs the same endpoints through the Python package's
    vendor-named methods.
  - `compare.py` measures W1 and W2 through the Python package like the vendor
    baseline: end to end with Polars and pandas, plus conversion only from the
    baseline's captures, through the new native `_decode_frames`.
  - Peak memory is recorded by both `compare.py` and, now, `baseline.py`.
    The Windows reading uses declared handle types.
  - The loopback benchmark gains a `transport-drain` engine and a per-engine
    `client_us_per_row` (wall time added on top of the transport).
  - **One session per account.** A second login ends the first session, so
    the Python `Client` gains the vendor's `existing_authorized_client` to
    share one. `QueryFailed` now carries the gRPC code name.
  - CI runs the runners' offline tests.
- **Python vendor-named methods and credentials** (P1 PR 6b).
  - All 81 vendor `ThetaClient` methods are generated with the vendor 1.0.17
    signatures (names, order and defaults), plus a keyword-only `plan`. Each
    forwards every argument to the same native query.
  - The client's `dataframe_type` (`"polars"`, the vendor default, `"pandas"`
    or `"arrow"`) picks the zero-copy conversion.
  - Tests check every signature against the pinned inventory, and that
    forwarding the vendor defaults builds the same wire query as omitting
    them. An alias adds about 0.6 µs per call (gate ≤ 5 µs).
  - `Client()` and `Client.from_env()` choose credentials in the ADR-0020
    order: explicit `creds_file`, `api_key`, or `email` and `password`; then
    `THETADATA_API_KEY`, the `THETADATA_EMAIL` / `THETADATA_PASSWORD` pair,
    `THETADATA_CREDENTIALS_FILE` and `./creds.txt`.
    - Credential files are parsed by the same Rust code as the CLI.
    - Blank values fall through, and error messages never include values.
- **Python package, native core** (P1 PR 6a). `crates/praevis-thetadata-py` is
  a pyo3 abi3 extension (CPython ≥ 3.10, D4), built with maturin as the
  `praevis-thetadata` wheel (D3). It is not published.
  - `Client(api_key)` / `Client.from_env()` runs every query in Rust (the
    scheduler, transport and Arrow decoding) with the GIL released.
  - `query(endpoint, **vendor_params)` reaches all 100 endpoints by vendor or
    RPC name, through generated, typed keyword-argument mapping. An explicit
    `None` clears an optional field, as in the vendor wrappers. Unknown or
    missing arguments are `TypeError`.
  - Results implement `__arrow_c_stream__`, and batches implement
    `__arrow_c_array__`, so pyarrow, Polars and pandas import them without
    copying. pandas columns stay Arrow-backed by default, keeping exact
    decimals without per-value objects.
  - `batches(...)` streams batches as they arrive; `to_parquet(...)` writes
    from Rust; `describe(...)` shows the wire query without sending it.
  - The D.2 gates are tests. Python allocations do not grow with rows: Arrow
    stays at 4 blocks from 1M to 10M rows, and Polars grows only per batch
    (449 to 887). The Python layer adds about 0.3 µs per call.
  - CI builds the wheel on Linux and Windows and runs its offline tests with
    hash-pinned pyarrow, Polars and pandas. Nothing is uploaded.
  - New dependency: `pyo3` 0.30 (approved with arrow-rs and parquet), with
    arrow-rs's `ffi` feature.
- **Parquet output** (P1 PR 5).
  - `QueryStream::to_parquet` and `JobStream::to_parquet` (client feature
    `parquet`, on by default) stream batches into one file, through
    `praevis_thetadata_arrow::ParquetSink`.
  - The file is written as `<name>.partial` and renamed into place only when
    the whole stream is written. A failed or dropped query leaves nothing at
    the target, and an existing file is not replaced unless `overwrite` is set.
  - ZSTD level 1 by default (or uncompressed), in row groups of up to 131,072
    rows. The exact types round-trip: `Decimal128(38, 10)`, millisecond
    timestamps with their zone, `Utf8`, `Int64` and `Boolean`.
  - Columns still `Null` are held back until typed, up to 64 MiB, so the file
    gets the final types. A column written as `Null` that is typed later is a
    `Schema` error.
  - Writing runs on the blocking pool. It adds about 0.4-0.6 µs/row on the
    loopback benchmark (aarch64).
  - New dependency: `parquet` 60.1, with only its `arrow` and `zstd` features
    (arrow-rs, approved with arrow and pyo3). It adds 8 crates.
- **Planner and scheduler** (P1 PR 4b).
  - `SchedulerPolicy`, set with `ThetaClient::with_scheduler` and shared by
    clones of the client, admits every `query()` request.
    - The default is FREE's documented limits: 1 concurrent request and 20
      per minute (D5). `SchedulerPolicy::free_observed()` (3 concurrent,
      40/min) is an explicit opt-in.
    - A request holds its slot until its stream ends, fails or is cancelled.
    - Only `RESOURCE_EXHAUSTED` and `UNAVAILABLE` are retried, at open,
      before any row is delivered. Retries use jittered exponential backoff,
      and each one counts against the per-minute limit.
  - `ThetaClient::job(&request)` plans a request by date: EOD and at-time
    requests split at 366 days, intraday history per day. A request that sets
    `date` is not split.
    - Each part is a scheduled `query()`, with at most the concurrency limit
      in flight, merged in date order into one Arrow stream.
    - Parts with no data are skipped; an all-empty job is `NoData`. Other
      errors end the job after the rows before them.
    - Dropping or cancelling a job cancels its queued and running requests.
    - Columns keep the earlier part's type when a later part has no value.
  - Generated endpoints declare their `Split` rule and date-range accessors.
  - The typed stock EOD stream is not scheduled yet; it keeps its own stream
    admission until it becomes a wrapper over `query()`.
- **Generic query streams** (P1 PR 4a): `ThetaClient::query(&request)` streams
  any of the 100 generated typed requests as exact Arrow `RecordBatch`es.
  - Batching (`BatchPolicy`): the first rows are emitted as soon as they arrive.
    After that, a batch closes at 65,536 rows, 8 MiB, or 50 ms after its
    oldest row.
  - Frames are decoded in place from the bytes the framing assembled, with no
    `ResponseData` or payload copy (option 2a, absorbed). Frames up to 64 KiB
    are decoded inline; larger ones go to the blocking pool.
  - The stock EOD stream's deadline, idle timeout, admission, cancellation and
    partial-result rules apply. Rows decoded before an error are delivered
    first, and the error is returned once.
  - A column's Arrow type is fixed by its first value; text stays `Utf8`. The
    only schema change between batches is a `Null` column becoming typed.
    `collect()` / `unify` gives earlier batches the final type.
  - Loopback end to end (aarch64): 5.9 / 6.1 µs/row against 17.2 / 18.1 for
    the typed stock EOD stream, with the first batch no later
    ([results](docs/performance/p1-columnar-results.md)).
  - `EodError` is now `QueryError`, with `EodError` kept as an alias. Its
    messages no longer say "stock EOD".
  - Generated endpoints gain `PATH`, `Request` and `request()`.
  - The client now depends on `praevis-thetadata-arrow`. The packaging
    rehearsal stages it in its own wave, and it stays `publish = false` until
    the next approved release.
- **P1 criterion c met for decoding.** Replaying the real P0 captures on the
  maintainer's Ryzen, the Arrow decoder uses 3.46 µs/row (W1) and 6.07 µs/row
  (W2): 15.9× and 10.4× less CPU than the vendor Python package with Polars
  ([results](docs/performance/p1-columnar-results.md)).
  - `columnar_benchmark --replay <dir>` times the decoders on private captures,
    printing no values.
  - Synthetic frames now declare their content size, as the vendor's do.
    `--frames streaming` keeps the 2 MiB-window case. That case had inflated
    the older decoders on Windows to about 50 µs/row, because they create a
    ZSTD context per message.
  - Timing no longer counts allocations or holds every sample's inputs at
    once.
- **Columnar Arrow decoder** (P1 PR 3), in the new `praevis-thetadata-arrow`
  crate. It is not published yet and has no client API yet.
  - `ColumnarDecoder` turns `ResponseData` messages into exact, coalesced
    Arrow batches:
    - prices as `Decimal128(38, 10)`;
    - timestamps as `Timestamp(Millisecond, America/New_York | UTC)`;
    - `Utf8`, `Int64` and `Boolean`.
  - A column's type is fixed by its first value. Mixed kinds or zones in one
    column are an error, never a coercion.
  - Headers are compared in place, and the ZSTD context and buffer are
    reused, so a warm decoder makes no allocation per message.
  - Per-message limits match the bounded decoder's. A failed message keeps
    the rows before it.
  - The client's adversarial corpus adds a fifth oracle. Arrow values equal
    the legacy decoder's on every input both accept (11,902 in the short run).
    Rejections match the bounded decoder's class.
  - New `columnar_benchmark` on synthetic vendor-shaped workloads (aarch64
    sandbox): 3.08 / 3.45 µs/row against 4.76 / 5.47 for the bounded decoder
    ([results](docs/performance/p1-columnar-results.md)).
  - New dependencies: `arrow-array` and `arrow-schema` 60.1 (arrow-rs,
    approved with parquet and pyo3).
- **FREE entitlement map results** (run `p1-entitlements-001`, PROD, 2026-11-04,
  36 requests, no stop), in `docs/endpoints/status.json` and the coverage table:
  - **10 entitled:** stock, option and index EOD; stock, option and index
    symbol lists; stock dates; option expirations and strikes; interest-rate
    EOD.
  - **24 tier-denied** (`PERMISSION_DENIED`): VALUE required for 14, STANDARD
    for 8, and no tier named for 2 (stock flat-file EOD, option trade Greeks
    first order).
  - **2 `NOT_FOUND`** (futures symbol list, corporate-action splits), recorded
    as no-data and marked uncertain: they may mean "not served" or "not
    entitled".
  - The 64 RPCs that were not probed are shown as "not probed", never inferred
    from their family. Committed results hold classifications and gRPC codes
    only.
- **FREE entitlement map probe** (P1 PR 2): `live_p0 entitlements` sends 36
  single requests through the generated typed requests, one per endpoint
  family, strictly sequential and 3.5 s apart. It reads at most 5 messages and
  then drops the stream. It records status, status text, timings, message
  counts and bytes, never row values. Per-endpoint refusals are recorded and
  the run continues; `RESOURCE_EXHAUSTED` or `UNAVAILABLE` ends it. CI now runs
  the live runner's offline loopback tests.
  - It is a one-time, user-approved exception to "never sweep endpoints"
    (`CLAUDE.md`). Re-running it, for example after a tier upgrade, needs
    approval again.
  - Vendor error text stays in the private, ignored report. Committed results
    in `docs/endpoints/status.json` are a classification (entitled,
    tier-denied, invalid-argument, no-data, other) plus the gRPC code. The
    generator rejects any other field or value.
- **Typed requests for all 100 RPCs**, generated from the pinned vendor 1.0.17
  inventory by `tools/generate_endpoints.py` (P1 PR 1). Module
  `praevis_thetadata_client::endpoints`:
  - 81 requests mirror the vendor wrappers exactly: their defaults, date and
    time formatting, contract specs, and truthiness guards. An explicit
    `false`, `0` or `""` is not sent, as in Python.
  - The 19 descriptor-only RPCs (futures, corporate actions) are generated
    from their request messages and marked unstable.
  - `ThetaClient::stream_raw` dispatches any of them and returns the raw
    response stream.

  A generated coverage table (`docs/endpoints/coverage.md`) replaces
  per-clause matrices; CI fails if the generated files are stale.
- The P1 design, **approved 2026-11-04** with every recommendation ([docs/architecture/p1-design.md](docs/architecture/p1-design.md)):
  - generated typed requests for all RPCs;
  - an exact Arrow columnar decoder (Decimal128(38,10), millisecond
    timestamps proposed);
  - message coalescing and inline decoding;
  - a planner and scheduler;
  - a Parquet sink;
  - the Python pass-through package;
  - a PR sequence and decisions. Docs only.
- ThetaDataDx monitoring: a watch-item list, re-reviewed at each phase exit.
- P0 measurement results
  ([docs/performance/p0-results.md](docs/performance/p0-results.md)):
  - one row per gRPC message;
  - the vendor's network and server time dominates, varying up to 14×;
  - our decoders use 5-7× less CPU per row than the vendor Python package
    today, against a floor of about 2.5 µs/row set by libzstd for one ZSTD
    frame per row;
  - the first row arrives 7-40× sooner;
  - FREE served ≥ 3 concurrent requests with no per-minute limit at 40/min;
  - the EOD date span is capped at 366-730 days.
- The performance architecture plan is **approved** (2026-11-03). Recorded:
  - the user's decisions;
  - a thin pass-through Python design over Arrow C Stream data, with
    zero-cost vendor-name aliases and benchmark gates;
  - the Terminal-only real-time path;
  - the ThetaDataDx review (license and vendor terms).

  CLAUDE.md gains the scoped vendor-package benchmark exception and the
  lighter process; ADR-0007 is superseded.
- A proposed performance architecture plan
  ([docs/architecture/performance-plan.md](docs/architecture/performance-plan.md)).
  It covers:
  - researched vendor tier facts;
  - a workload catalogue with baseline-relative targets;
  - a scoped proposal to benchmark against the vendor Python package;
  - a target design: generated endpoints, Arrow, scheduler, PyO3, real-time;
  - a roadmap tied to subscription upgrades, and a lighter process.

  Planning only; no code changes.
- Publication record for `0.1.0-alpha.2`: registry checksums, the tag, the
  offline replay on the release commit, and Windows/Linux crates.io installs
  and consumers ([record](docs/crates-io-release-plan.md#publication-record-010-alpha2)).
- Paused-clock EOD lifecycle tests through an internal transport seam
  (EOD-L3-019, deferred item 4). They pin exact idle and deadline boundaries,
  simultaneous expiry, idle reset per message, empty trickles, consumer
  pauses, admission waits, offloaded decoding and cancellation. Neutral on
  the decode benchmark gate ([results](docs/performance/eod-clock-seam-results.md)).
- Trusted-TLS success tests for the typed and raw EOD APIs (EOD-L3-005,
  deferred item 3). Each test generates its own CA and server certificate with
  `rcgen`, a new dev-dependency using the ring backend (decision D-F b), so no
  private key is committed. An untrusted issuer and a wrong server name are
  rejected before any request. Normal builds are unchanged: no `rcgen`,
  OpenSSL or aws-lc.
- An adversarial decoder corpus (EOD-L3-021, deferred item 2, D-G). It runs
  exhaustive byte edits, structural, envelope and ZSTD framing catalogues, and
  seeded mutations through the envelope parser, both bounded modes and the
  legacy decoder, checked against prost and the legacy decoder as oracles.
  The short run is in CI; a 2,000,000-case long run is manual.
- A process-memory profile of the typed EOD client (EOD-L3-023, deferred
  item 1, D-H), with no new dependency: the `eod_memory` example and a
  `memory` option on the performance workflow. libzstd fits the accounted
  budget. Transport buffering adds about one encoded message per open stream
  and is bounded linearly by `concurrent_streams`; it is outside the shared
  budget, so total memory is still not claimed bounded
  ([record](docs/performance/eod-memory-profile.md); proposed item 8).
- Root-cause investigation of EOD transport buffering (proposed item 8),
  documentation and experiment harness only. Call-site attribution proves that
  tonic 0.14 keeps one exact-size decode buffer per open stream
  (`codec/decode.rs:199`) until the stream ends, independent of HTTP/2
  windows and message limits. Four alternatives were prototyped on unmerged
  branches and measured; own message framing would cut 16-stream peak heap
  from about 505 to 128 MiB
  ([record](docs/performance/eod-transport-buffering.md)). No data-path change.

### Fixed

- An abandoned offloaded decode job now releases its schema lease before its
  job permit, using a declaration-ordered `JobLeases`. A newly admitted job can
  no longer observe the previous job's schema lease, and the cancellation test
  asserts this without a wait (deferred item 7). Neutral on the decode
  benchmark gate ([results](docs/performance/eod-lease-order-results.md)).

### Changed

- The maintainer desktop's benchmark tolerances are now 1.05/0.95, the same
  as the runners, after a three-round A/A showed no every-round shift above
  0.3% (deferred item 6, [record](docs/performance/eod-desktop-aa-results.md)).
- `tools/check_repository.py` also verifies every local Markdown `#anchor`
  link (#39).

## [0.1.0-alpha.2] - 2026-11-03

### Changed

- **Client endpoint URLs with credentials, a query or a fragment are now
  rejected** (`Error::Config` / `EodError::Configuration`) before tonic parses
  them, matching the auth URL rule. No error, `Debug` or log output can echo
  such a URL (EOD-L3-017).

- **Timestamps after 9999-12-31T23:59:59.999Z are now rejected** in both the
  Table formatter and the numeric `Timestamp`. They previously formatted as
  `+10000-...`, which is not RFC 3339 (EOD-L3-009). A platform-independent
  boundary catalogue covers epoch, the New York DST edges and the limits.
- The client README and crate description no longer imply that total memory
  is bounded. Only the client's own accounted allocations are, until
  transport and ZSTD C-allocator profiling exists (a 0.1.0 exit gate, D-C).
- `Price` documents representational equality and per-value scale.

- **EOD benchmark policy and resource calibration** (PERF-L3-008,
  EOD-L3-023):
  - Decode p50 on the eight 10,000-row workloads gates performance
    acceptance, with tolerances approved on 2026-11-02: runners 1.05/0.95;
    the maintainer desktop 1.10/0.90 (provisional).
  - The harness gains `--extremes`, limit-sized text and header batches, and
    the workflow passes `run_args`.
  - The resource defaults are kept, with their effective ceilings documented:
    the 128 MiB accounted budget binds before the other limits combine.
    Limit-sized batches complete at the 16-stream cap on Windows and Linux.

- **Faster bounded EOD decoding.** Each cell is read by a hand-written
  decoder with prost-identical semantics, plus fast paths for two-byte
  null/boolean/small-integer cells, instead of building a prost `DataValue`.
  On the Windows and Linux x86 runners, decode is faster by 21-36% for nulls,
  8-16% for mixed and 6-12% for prices, against `main`. This recovers the
  recorded null/price regression; preflight, limits and errors are unchanged
  ([results](docs/performance/eod-decoder-results.md)).

### Added

- **alpha.2 readiness record** (2026-11-02):
  - All five `0.1.0-alpha.2` exit gates are met.
  - Every EOD L3 clause has a disposition: 24 closed, 5 closed with a named
    deferral, 1 not applicable, 0 open.
  - releases.md lists the release procedure awaiting approval and a beta.1
    API-freeze checklist, which includes the `Price` comparison helper.
  - Not released: the version stays `0.1.0-alpha.1`.

- **Observed PROD service semantics for stock EOD (EOD-L3-029)**, from seven
  authorized FREE-account captures on 2026-11-02. Numeric, Table and raw
  replay agree value-for-value on every data run. Observed (not guaranteed):
  - both date endpoints are inclusive; rows ascend, one per frame;
  - non-trading days are omitted;
  - weekend, holiday and unknown-symbol queries all return `NOT_FOUND`
    before data;
  - FREE history excludes August 2020;
  - history is not split-adjusted;
  - price scale varies per value.

  Open questions stay listed as unresolved in the
  [semantics table](docs/requirements/stock-eod-evidence.md#observed-service-semantics-eod-l3-029).

- A dependency-log capture test (`tests/eod_logs.rs`) shows that TRACE-level
  `tracing`/`log` output from hyper, h2, tonic and reqwest never contains the
  API key, session token, account email or remote payload/status text
  (EOD-L3-017). It adds `tracing` and `log` as dev-dependencies (D-D(a));
  both were already in the dependency graph.
- releases.md lists every item deferred from alpha.2 in one block, and
  proposes a `0.1.0-alpha.3` hardening milestone for them. Item 7 makes an
  abandoned worker release its schema lease before its job permit, the root
  cause of the CI race fixed in the test by #34.

- An EOD contract test with an injected auth-connection counter shows that
  typed queries never re-authenticate (EOD-L3-005). Typed errors are checked
  for the absence of the session token and account email.

- EOD acceptance tests:
  - per-value price scales across rows and batches;
  - `grpc-timeout` metadata capture;
  - a bounded adversarial decoder corpus (40,000 classified envelopes);
  - a runnable partial-failure consumer example (`eod_partial`).

- The **Synthetic EOD performance** workflow gains a before/after mode on
  Windows and Linux runners: clean worktree builds, alternating AB/BA rounds,
  native comparisons, raw reports, and a ratio summary
  (`tools/eod_compare_summary.py`). Use baseline = candidate for A/A noise.

- Publication record for `0.1.0-alpha.1`:
  - crates.io URLs and registry checksums, tag and commit;
  - Windows and Linux installs from crates.io;
  - Linux consumers built from crates.io only, with no native-store packages in
    the default auth/client trees, and the published CLI's lifecycle;
  - docs.rs status: four libraries built (auth with `native-store`), and the
    CLI is binary-only;
  - single-owner status, with a backup-owner recommendation.
- releases.md and ROADMAP mark alpha.1 as released and alpha.2 as the next
  target.
### Fixed

- The CLI package description and the README package table no longer imply
  that saving is automatic. Saving is opt-in with `auth --save`; the published
  alpha.1 description is corrected for the next release.

## [0.1.0-alpha.1] - 2026-11-02

First experimental prerelease on crates.io: `praevis-thetadata-auth`,
`-core`, `-proto`, `-client` and `-cli` (tag `v0.1.0-alpha.1`, commit
`0d3ea72`). It is independent and unofficial, APIs may change between alphas,
and it is not full Python parity. The Python compatibility baseline is 1.0.17.

### Changed

- **Publishing enabled for the experimental `0.1.0-alpha.1` prerelease**
  (`publish = ["crates-io"]`). Publication itself is manual and approved by
  the owner, in wave order (auth, core, proto, then client and CLI); no
  workflow publishes. The READMEs now describe crates.io installs with
  explicit prerelease versions and experimental wording.

- **Packaging (alpha.1, part B):**
  - The client archive is self-contained: its build script takes the
    descriptor from the proto crate (a build dependency) instead of a sibling
    path, and the checkout-only examples (EOD benchmark, `live_eod` runner)
    are excluded from the archive.
  - `tools/staging_registry.py` rehearses publication in a local `file://`
    registry: wave 1, then client and CLI resolved from it. It builds
    consumers outside the workspace, installs the CLI, and checks file lists,
    hashes, license files and forbidden inputs. A new "Packaging rehearsal" CI
    job runs it on Ubuntu and Windows.
  - Default auth and client consumers are checked to resolve no native-store
    packages. Nothing is published; `publish = false` is unchanged.

- **Packaging (alpha.1, part A):**
  - All five packages are now `0.1.0-alpha.1`, versioned together and
    independently of the Python baseline. Internal dependencies require exactly
    `=0.1.0-alpha.1`.
  - `rust-version` is 1.88, verified against the locked dependency graph.
  - Each package has public metadata and a README with supported platforms and
    an independent/experimental/no-full-parity note. Library READMEs double as
    crate documentation, and their examples run as doctests.
  - Every package archive includes `LICENSE` and `NOTICE` (proto also
    `LICENSE.upstream`), and copy drift is checked. NOTICE records the 1.0.17
    provenance.
  - `publish = false` is unchanged. No package was published.

- **CLI: Python-order credentials, and saving only with `auth --save`**
  ([ADR-0020](docs/adr/0020-in-memory-sessions-and-optional-persistence.md);
  AUTH-L3-025/026 replace the retired AUTH-L3-001/010).
  - Credential order is now `--creds-file`, `THETADATA_API_KEY`, a complete
    `THETADATA_EMAIL`/`THETADATA_PASSWORD` pair, then `THETADATA_CREDENTIALS_FILE`
    or `./creds.txt`, matching the vendor package. `THETADATA_CREDENTIALS_FILE`
    is no longer the highest precedence.
  - An incomplete or blank email/password pair falls through instead of
    failing. A selected file that fails still errors.
  - `praevis-thetadata auth` now logs in and reports the account **without
    saving**; `auth --save` saves as before. `--save` with `status` or
    `logout` is an argument error (exit 2).
  - Status, logout, the storage identity and the v1 record are unchanged, so
    records saved by earlier versions keep working. A failed login still
    leaves an existing record intact.
  - **Migration:** scripts that relied on `auth` saving must add `--save`, and
    scripts that relied on `THETADATA_CREDENTIALS_FILE` overriding
    `THETADATA_API_KEY` must unset the key or pass `--creds-file`. The
    benchmark tools pass `--save`; the before/after comparator detects
    whether each binary supports it.

- **Auth library: sessions are in memory by default; native persistence is
  optional** ([ADR-0020](docs/adr/0020-in-memory-sessions-and-optional-persistence.md),
  superseding ADR-0003). `SessionStore`, `StoredSession` and `StoreError`, and
  the `keyring` dependencies behind them, now require the `native-store` Cargo
  feature, which is off by default. A default auth build, and therefore the
  market-data client, no longer pulls in keyring, D-Bus or Windows credential
  packages for any target. With the feature enabled, the storage identity and
  v1 record are unchanged, so existing saved sessions keep loading. The CLI
  enables the feature, and its behavior is unchanged in this step. **Migration
  for library users of `SessionStore`:** add `features = ["native-store"]`. CI
  now tests auth with the feature off and on and checks the auth and client
  dependency trees.

- Documentation for the 1.0.17 baseline. The Python performance comparator is
  pinned to 1.0.17 (D4); its EOD wrapper and conversion code are unchanged
  from 1.0.12. The release plan records that the baseline update landed before
  alpha.1 (D5) and that the proto now has 100 RPCs. The ROADMAP lists the
  remaining follow-ups: futures research, explicit presence for the new
  optional fields, ADR-0019 supersession, and a possible ~1–2% Table-decode
  shift on mixed uncompressed fixtures that has not been measured on Windows.
  CLAUDE.md, the crates.io plan and the subscription matrix are updated, and
  the subscription matrix notes the documented service-side changes.

- **Timestamps with sub-millisecond or unknown precision are now rejected
  instead of silently truncated** ([ADR-0019](docs/adr/0019-timestamp-precision.md),
  EOD-L3-030). Python 1.0.16 added `ZonedDateTime.nanos`/`precision` with
  undefined semantics. Typed EOD streams end with `EodError::Unsupported`;
  batches already delivered stay partial. The raw table helper returns
  `DataError::TimestampPrecision`. Millisecond values, which is every
  response observed so far, are unchanged. The new core helpers are
  `check_timestamp_precision` and `timestamp_with_precision`; the existing
  `timestamp()` and `Timestamp::from_wire()` are unchanged.

- **Python compatibility baseline 1.0.12 → 1.0.17.** The descriptor
  (`35572458…5a58`) and manifest were regenerated from the hash-verified
  1.0.17 wheel. `praevis-thetadata-proto` now binds 100 RPCs instead of 82
  (17 futures RPCs and `GetStockFlatFileQuote` were added) and 212 messages.
  **Source-breaking for raw proto struct literals:** `Endpoints::ZonedDateTime`
  gained `nanos` and `precision`, so construct it with `..Default::default()`.
  Runtime decoding is unchanged: both decoders still ignore those fields,
  pending the ADR-0019 policy. The 1.0.17 inventory, catalogue and dispositions
  ledger are now the checked baseline, and the 1.0.12 set stays frozen. Futures
  RPCs and the new `stock_flat_file_quote` are `unresolved`, like the other
  flat files. The upstream monitor reports tracked = latest, 1.0.17.
  Auth requests, credentials and persisted sessions are unchanged.

- Research tooling is now version-agnostic. `research_python.py` takes its
  paths from the manifest `source_version`. It checks the frozen 1.0.12
  evidence alongside any newer baseline, and it fails if upstream removes,
  renumbers or retypes anything the frozen evidence recorded. One static
  inventory builder is now shared with `upstream_delta.py`, and it still
  regenerates the frozen 1.0.12 inventory byte-for-byte.
  `extract_protocol.py` reads a hash-verified wheel directly and takes the
  version from its METADATA. Futures RPCs get their own feature key.
  The baseline is still 1.0.12.

### Added

- **Alpha.1 PROD exit gate recorded (2026-11-01, Windows, FREE account):**
  - CLI auth lifecycle: `auth` stores nothing, `--save` persists the unchanged
    v1 record, then status/logout.
  - AAPL EOD capture `prod-aapl-alpha1-002` on the 1.0.17 baseline: one
    232-byte ZSTD frame. Numeric, Table and raw replay match each other and
    the 2026-10-04 capture's exact-value digest, and the real timestamps pass
    the ADR-0019 precision check.
  - FREE quote denial reproduced (code 7 at headers).
  - All five crates.io names returned 404 (not reserved).
- CI compile-checks the feature-gated `live_eod` runner with clippy, so a
  baseline bump cannot silently break it again (it broke after #17 and was
  fixed in #24). CI still never runs live checks.

- Constructed sixteen-column stock EOD regression coverage informed by the
  reviewed live schema, with invented values and independent exact-value checks
  across raw/numeric/Table interfaces and NONE/ZSTD. Added a revision-pinned
  ThetaDataDx watchlist and independent project naming brief. The CLI now has
  package-specific metadata and README; publication awaits name selection and
  the remaining archive/consumer checks.

- Connection inventory and editable SVG distinguishing direct HTTPS/gRPC from
  proposed Theta Terminal v3 REST/WebSocket adapters, with ADR-0018 and explicit
  enhancement requirements. Added a per-product/subscription manual test matrix
  and a bounded stock quote probe that passes only on the expected pre-data
  permission denial; other errors or unexpected data fail the assertion.
  A manual PROD AAPL probe reproduced the FREE account's gRPC code 7 at headers
  with zero batches. Paid-tier and Terminal verification remain untested.

- Explicit `--api-key-env` support for the manual live runner, reading only
  process `THETADATA_API_KEY` without requiring a file or exposing its value.
  The first authorized PROD AAPL single-day EOD check passed: live numeric/Table
  results and Windows/Linux offline raw/numeric/Table replay matched the private
  captured response. Broader service and performance acceptance remain open.

- Concrete versioned release milestones and exit gates, plus an optional
  feature-gated Rust live EOD capture/replay runner. Manual account checks use
  explicit credential files and retain private response/provenance artifacts;
  offline replay compares raw, numeric and Table interfaces. CI workflows stay
  unchanged and credential-free. Live observations are recorded separately.

- Typed EOD local frame/ZSTD resource-error provenance and a per-requirement
  acceptance matrix covering both numeric and Table APIs. Larger mock workloads
  exposed an existing HTTP/2 progress failure; finite query admission and a
  dedicated typed channel with fixed receive credit prevent paused streams from
  starving active decoding. Repeated Windows/Linux slow-consumer measurements,
  failed-calibration evidence and measured decode regressions are retained.
  Full EOD/performance acceptance remains open.
- Typed EOD resource-audit fixes: bounded outer-envelope decoding, conservative
  String capacity accounting, separate schema/batch reservations, deterministic
  running-worker cancellation tests, and opt-in bounded inline scheduling for
  small uncompressed responses. Repeated Windows/Linux measurements retain
  bounded offload as the portable default because Windows gains were mixed;
  Linux small numeric delivery improved by 51-65% in this fixture. Full stock
  EOD acceptance remains separate.
- Experimental typed stock EOD numeric batches with flat cell storage, lazy
  timestamp presentation, a shared Table adapter, preflight allocation/count
  bounds, finite decode admission, schema continuity and terminal stream errors.
  Added synthetic contract tests and a common legacy/Table/numeric benchmark;
  full EOD acceptance and live-service verification remain open.
- crates.io prerelease plan covering the five existing packages, name checks,
  credential-verification limits, packaging, ownership and release approval.
- Credential-free Rust EOD HTTP/gRPC fixtures with deterministic NONE/ZSTD
  batches and exact-value, wire-request, empty/partial/error and timeout tests.
  Tests exercise the current raw client; they do not claim typed EOD acceptance
  or vendor-service validation.
- Rust-native release decoder/delivery benchmark with raw latency/throughput
  samples, a separate allocation/peak requested Rust heap probe, executor-delay
  observations, embedded source/compiler and executable hashes, and guarded
  report comparisons. Added Windows/Linux CI smoke checks and a manual artifact
  workflow. Production decoding remains unchanged for the before baseline.
- Repeated Windows/WSL decoder baseline results with retained raw-report/binary
  hashes, explicit measurement boundaries and variability. Corrected mock TCP
  coalescing before retaining small-message delivery results; no production
  decoder optimization or live-service performance claim is made.
- Architecture review against Fowler separation, ports/adapters and applicable
  Twelve-Factor principles. Proposed ADRs 0013-0016 and 36 ARCH/PERF/EXT
  requirements define optional persistence boundaries, numeric batch delivery
  with explicit Table conversion, measured performance acceptance, protected
  enhancements, and optional local filtering/projection. Runtime changes remain
  planned; future PyO3 bindings are recorded on the roadmap.
- Exhaustive origin classification for requirements and ADRs, with mutation
  checks and an upstream-update impact template separating compatibility from
  protected Rust behavior. Classification is independent of acceptance status.
- Defined stock EOD requirements: six L1 outcomes, twelve L2 system contracts,
  and twenty-nine L3 acceptance contracts with owners, source/ADR links,
  intentional Python deviations, Rust-native fixture/benchmark obligations,
  and explicit measurement/vendor-evidence gates. Runtime acceptance remains
  planned. Added a selected-slice disposition while retaining all 531 source items.
- Requirement audit coverage for new namespaces, duplicate definitions,
  undefined references, and immediate-level parents, with mutation tests.
- Proposed ADRs 0007-0012 for a first stock EOD library slice, typed request
  presence/validation, exact values and batch schemas, stream lifecycle and
  resource limits, Rust-native protocol verification, and hybrid requirements
  traceability. Linked feature dispositions to the proposed decisions without
  marking unimplemented market-data behavior supported.
- Exact PyPI wheel recovery instructions and hash verification for optional
  research regeneration after removing the untracked local ZIP/extraction.
- Complete Python 1.0.12 source analysis and generated Markdown catalogue:
  14 artifact members, 9 modules, 251 functions/methods, 80 query wrappers,
  82 RPCs, 176 messages, 778 fields, and two enums. Added 531 inventory rows
  and 26 feature dispositions tracing research to existing auth evidence and
  future requirements, with explicit unresolved contracts.
- Reproducible archive-to-inventory checks, offline CI coverage checks, and
  omission/mutation tests. Preserve the original research and inventory by
  hash; document undeclared dotenv, conflicting gRPC version minimums,
  optional-field/default differences, conversion edge cases, and the two
  descriptor-only corporate-action RPCs without claiming live-service parity.
- Rust workspace with an independent `praevis-thetadata-auth` library and `theta`
  authentication CLI. Supports API-key and email/password authentication,
  explicit credential discovery, PROD/STAGE environments, and named profiles.
- Native session persistence through Windows Credential Manager and Linux
  Secret Service, with auth/status/logout commands usable across processes.
  Status reports locally stored metadata with server validity `not_checked`;
  logout removes only the selected local record.
- Market-data foundations: checked-in protobuf descriptor and generated gRPC
  bindings for 82 RPCs, transport-independent data types, and a client for
  requests, ZSTD decoding, and batch streaming. These foundations do not claim
  complete Python parity or verified live-service behavior.
- Authentication L1/L2/L3 requirements and six ADRs covering library boundaries,
  wire/credential contracts, native persistence, explicit session lifecycle,
  verification/provenance, and CLI runtime ownership. Documented the hybrid
  requirements organization in [the documentation index](docs/README.md).
- Python 1.0.12 artifact research, source hashes, and an API inventory. Recorded
  the publishing repository and commit from PyPI attestations, public GitHub
  access limitations, and matching wheel/sdist Python sources in
  [the upstream source investigation](docs/research/upstream-source.md).
- Windows/Ubuntu CI for formatting, Clippy, workspace tests, repository and
  documentation auditing, Python tooling tests, and synthetic native-store
  persistence tests. No live ThetaData credentials are required.
- Scheduled and manually triggered [PyPI compatibility monitoring](docs/upstream-monitor.md).
  It reads the tracked version from the protocol manifest, compares stable
  non-yanked releases with PEP 440, and fails on a newer version or an inability
  to verify. It does not automatically update the compatibility baseline.
- Synthetic auth benchmarks and before/after CLI startup comparisons with
  release-build provenance, executable hashes, alternating sample order, raw
  samples, percentile summaries, isolated native-store fixtures, and Windows/
  Ubuntu workflow support. WSL comparisons can stage identical binaries onto
  the native temporary filesystem.
- Forward-looking roadmap preserving the original performance review and table
  verbatim, with planned compatibility research and native Rust tooling work.
- Shared coding-agent guidance in [AGENTS.md](AGENTS.md) and
  [CLAUDE.md](CLAUDE.md), including project-specific Git conventions, and this
  changelog separating implemented changes from future work.

### Changed

- Renamed the project/repository to `praevis-thetadata`, the five Cargo packages
  and member directories to `praevis-thetadata-*`, Rust imports to
  `praevis_thetadata_*`, and the CLI executable from `theta` to `praevis-thetadata`.
  Updated documentation, tooling, workflow package targets and repository links;
  added the explicit independent-maintenance/non-affiliation statement. Credential
  variables, native session-store identity, wire protocol and upstream evidence
  remain unchanged. Older benchmark checkouts remain supported by the build helper.

- CLI parsing, help/version, status, and logout now run synchronously. Only
  HTTP authentication starts a current-thread Tokio runtime, which ends before
  persistence/output. The async auth library remains caller-runtime-owned;
  credential precedence, record format, output, and exit codes are preserved.
- Recorded the startup change's measured results and limitations in
  [the evaluation](docs/performance/cli-startup.md): Windows median status
  latency improved 10.6-10.9% and auth/save 5.5-6.0% across two runs. Native
  Linux-filesystem status improved 11.5-12.1%, but Linux auth/save showed no
  reliable gain. Tail latency did not consistently improve, and execution from
  Windows-mounted WSL storage regressed. These are synthetic local findings,
  not a universal latency guarantee.

### Security

- Authentication defaults to HTTPS with redirects disabled and explicit
  timeouts. Plain HTTP requires an explicit test opt-in. Credential/session
  secrets are redacted from output; saved records omit passwords and API keys.
- Native-store failures remain explicit errors, with no plaintext or
  process-only persistence fallback. Failed authentication preserves an
  existing saved session. Tests isolate and clean up synthetic profiles.

### Licensing

- Adopted Apache-2.0 for newly authored Rust code and tooling, added the root
  [LICENSE](LICENSE) and [NOTICE](NOTICE), and retained
  [the upstream Apache-2.0 license](crates/praevis-thetadata-proto/LICENSE.upstream).

The initial implementation was recorded on 2026-10-03 (`0caf272`); upstream
monitoring, licensing, source investigation, startup optimization, and its
acceptance documentation followed on 2026-10-04 (`83f429e`, `2b4798a`,
`daa50db`, `af965f7`). These dates describe repository history, not releases.

Changelog structure adapted from [siat-foreign-analysis CHANGELOG.md](https://github.com/joey-huckabee/siat-foreign-analysis/blob/main/CHANGELOG.md)
on 2026-10-04 (source blob `1cf05b7c5f4dabfea95ebf669b2afbef3787c3d3`).
Entries describe this project's implementation; the reference project's
release history is not imported.
