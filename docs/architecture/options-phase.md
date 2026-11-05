# Options phase plan (options PRO)

Status: **started 2026-11-05.** The user's goal: "finish the option portion
of the project. Keep it performant." This plan follows the approved
[performance plan](performance-plan.md) (its upgrade 3 row, options only) and
proceeds without separate approval unless a decision below is needed.

## Decisions (user, 2026-11-04)

| # | Decision |
| --- | --- |
| 1 | The incremental memory metric (peak minus the idle baseline after imports and login) is reported **alongside** total peak RSS, for both clients |
| 2 | **P1 is done** as [recorded](../performance/p1-exit-results.md). Python into Polars at 9.0-9.2× and criterion d stay recorded misses |
| 3 | The account has **options PRO**; VALUE and STANDARD are skipped. Stocks and indices stay **FREE** (EOD only). Stock tick (W7) is out of scope |
| 4 | The two-login session probe is approved |
| 5 | The FREE concurrency question above 3 is closed: moot on PRO |
| 6 | The PRO entitlement map of **every option RPC** is approved as one run (AGENTS.md) |

Live check (Coordinator, 2026-11-04): the login response has subscription
fields options = 3, stock = 0 and indices = 0. We read 3 as PRO and 0 as
FREE. 1 = VALUE and 2 = STANDARD are inferred from the order, not observed.

## A. Vendor facts re-verified for PRO (2026-11-05)

| Fact | Today's vendor docs | Change from the plan |
| --- | --- | --- |
| Concurrency | FREE 1, VALUE 2, STANDARD 4, **PRO 8**; account-wide, set by the highest tier across products ([doc][conc]) | None. So stock and index requests also get 8 slots |
| Rate limit | "Your requests are NOT rate-limited in any way", concurrency only. The page no longer limits that to paid tiers ([doc][conc]) | The subscriptions page still says 20/min (prose) and 30/min (table) for FREE ([doc][subs]). FREE keeps its documented defaults |
| Over the limit | Queue of 16 (up to 128), then HTTP 429; written for the Terminal REST API | Direct gRPC behaviour is still **unknown**. We stay at ≤ 8 and do not probe above it |
| Options PRO | Tick level, from **2012-06-01**, real time, 8 concurrent ([doc][subs]) | None |
| Greeks | The table marks 2nd/3rd order and every trade-Greeks row with a single tick, which reads as **PRO only** ([doc][subs]) | The plan inferred STANDARD. The map settles it |
| Request sizing | Option tick: 1 week, **1 day for liquid roots** (AAPL, SPX, SPY); 100 ms: 1 week; 1 s: 1 month; under **1M ticks** per response ([doc][sizing]) | None |
| Flat files | **PRO only.** The 7 most recent calendar days; the previous day is ready about 00:30-01:00 ET. Files: option trade_quote (about 1.2 GB CSV, 3 min), **option quote (38 GB, 30 min)**, option open interest (50 MB), option EOD (150 MB), plus stock and index files ([doc][flat]) | Option quote, open interest and EOD files are new to us. The docs show the Terminal REST path; the pinned descriptor also has direct gRPC RPCs (`GetOptionFlatFile*`), whose format is unknown until the map |
| Streaming | Terminal only (`ws://127.0.0.1:25520/v1/events`), **one connection**; PRO: 15,000 quote contracts, unlimited trades via the full trade stream ([doc][stream], [doc][subs]) | None. Sessions per account are not documented |

## Options PRO entitlement map (`opt-pro-entitlements-001`, 2026-11-05)

Run by the Coordinator on Windows at 8aaa445, 2026-11-05 03:3x UTC (22:3x ET
on 2026-11-04, market closed). 51 requests, exit 0, no
`RESOURCE_EXHAUSTED` or `UNAVAILABLE`, secret scan clean. Per-RPC results are
in the [coverage table](../endpoints/coverage.md).

| Group | RPCs | Result |
| --- | ---: | --- |
| Lists | 5 | entitled |
| History, every Greeks and trade-Greeks order, binomial included | 26 | entitled |
| At time | 2 | entitled |
| Snapshots | 14 | `NotFound`, no-data (**uncertain**) |
| Flat files | 4 | `InvalidArgument` (**uncertain**); 0 messages, 0 bytes read |

- **No option RPC is tier-denied on PRO.** Higher-order and trade Greeks are
  served, which settles the Greeks question above for PRO.
- **Snapshots:** most likely the probe's expiration (2024-06-21, long expired)
  and the closed market, not entitlement.
- **Flat files:** the probe asked for 2026-11-04 while it was still that day
  in New York; the vendor publishes a day's files about 00:30-01:00 ET the
  next day. The probe's date choice is the likely cause.
- Both are settled by the step F live checks (a current expiration during
  market hours, and a past flat-file date). That is a live check, not a map
  re-run.
- `GetOptionHistoryBinomialGreeksEod` returned 1 message, where the other
  history RPCs reached the 5-message cap. Its request covered one day.

## Workload sizes (`opt-sizes-001`, 2026-11-05)

`live_p0 sizes` on main 1cae6e4 (Windows, PROD): 12 requests, all OK, none
over 1M rows. All are SPY on 2024-06-14.

| Request | Rows | Messages | First message | Total |
| --- | ---: | ---: | ---: | ---: |
| W4a 1-minute quotes, 4 expirations | 95k-177k each | 244-452 | 1.9-3.5 s | 2.2-3.9 s |
| W4b 1 s quotes, calls, 10:00-10:30 | 407k | 226 | **71.8 s** | 80.4 s |
| W5a tick trades, 4 expirations | 12k-123k each | 129-296 | 1.0-1.7 s | 1.2-2.2 s |
| W5b tick quotes, calls, 10:00-10:15 | 939k | 287 | **59.4 s** | 62.3 s |
| W6a 1-minute Greeks | 177k | 452 | 3.3 s | 5.9 s |
| W6b trade Greeks | 47k | 296 | 3.2 s | 6.1 s |

- **Rows per message:** option history is not one row per message (that was
  EOD). Here it is about 90-3,300 rows per message.
- **The server builds the result before sending.** The first message
  arrives after 53-95% of the total time, so the time is server work, not
  transport or decoding. Overlapping decode with receipt hides little; only
  parallel requests can.
- **Two requests were 20-30× slower than the rest:** 1 s quotes (5k rows/s)
  and tick quotes (15k rows/s), both with a time window. Shape probes S1-S5
  separate the window from the quote resolution before any baseline: a
  shorter window, one contract over the day, and windows on cheap requests.

## Shape probes (`opt-sizes-002`, 2026-11-05)

`live_p0 sizes` on main 480f558: 37 requests, all OK, none over 1M rows. All
are SPY 2024-06-21 calls on 2024-06-14.

| Probe | Shape | Rows | First data | Total |
| --- | --- | ---: | ---: | ---: |
| S1 | tick quotes, every strike, 10:00-10:01 | 73k | **60.8 s** | 64.5 s |
| W5b | tick quotes, every strike, 10:00-10:15 | 939k | **59.4 s** | 62.3 s |
| S3 | 1 s quotes, every strike, 10:00-10:01 | 14k | **63.4 s** | 70.0 s |
| W4b | 1 s quotes, every strike, 10:00-10:30 | 407k | **71.8 s** | 80.4 s |
| S2 | tick quotes, **one contract**, whole day | 486k | **0.2 s** | 1.1 s |
| S4 | 1-minute quotes, every strike, 10:00-10:30 | 7k | 3.4 s | 3.8 s |
| S5 | tick trades, every strike, 10:00-10:15 | 1.6k | 1.6 s | 1.7 s |

**The slow shape is every strike × fine-grained quotes.** It costs about
60 s before the first data, whatever the window or the row count. One
contract over the whole day took 1.1 s (about 440k rows/s). The 1-minute
quotes and tick trades over every strike are not slow.

**Hypothesis (to test, single samples so far):** split tick and 1 s quote
requests per contract and run them 8 at a time. `live_p0 split-probe` tests
this, comparing for the same request (W5b):
- the all-strike request, run before and after;
- the same request once per listed strike (`option_list_strikes`), at
  concurrency 1 and 8.

It reports wall times, rows, and whether the split results equal the
reference exactly: an order-independent digest of every row's values, of
which only the equality is reported.
- **Request sizing:** each per-strike request is one contract for 15
  minutes, well inside the vendor's sizing guide (one day for a liquid
  root, under 1M ticks).
- **Load:** at most 8 at a time, the PRO limit. About 4M rows in total.

## Baselines D2

Recorded in [options-d2-results.md](../performance/options-d2-results.md):
every time target is met for the per-expiration workloads. The memory bound
is not yet assessed, and the slow all-strike quote shape is not covered.

## Per-strike split (`opt-split-001`, 2026-11-05)

`live_p0 split-probe` on main f91554c used W5b (tick quotes, SPY calls
expiring 2024-06-21, on 2024-06-14, 10:00-10:15) and 232 listed strikes.
The reference was stable (identical before and after).

| Run | Rows | Time |
| --- | ---: | ---: |
| All-strike request, before / after | 938,663 | 69.6 s / 58.9 s (first data 65.7 s / 55.8 s) |
| Per strike, 1 at a time | 938,663 | 95.9 s |
| Per strike, 8 at a time | 938,663 | **9.1 s** |

- **Exact:** rows, columns and the value digest equal the reference at both
  concurrencies. 226 strikes returned data and 6 had none (`NoData`).
- **Per-strike request times** (the full report stays local):

  | Concurrency | Median | p95 | Max |
  | --- | ---: | ---: | ---: |
  | 1 | 0.223 s | 1.33 s | 4.77 s |
  | 8 | 0.251 s | 0.92 s | 1.16 s |

  The service did not slow under 8 requests at once. The wall time at 8
  (9.1 s, at most 73 s of slot time for 232 requests, about 0.32 s mean)
  shows all 8 slots stayed busy. The floor is about the strike count × 0.25
  s / 8, whatever the window: the API takes one strike or all of them,
  nothing in between, and PRO allows 8 at once.
- **Decision:** the planner splits all-strike tick and 1 s quote requests
  per strike, at a scheduler concurrency of 2 or more. `split-probe` also
  runs `job()` and compares it exactly with the reference.

## `job()` live check (`opt-split-002`, 2026-11-05)

`split-probe` on main 6b90f38 sent W5b through `job()`. **The result was
exact:** 938,663 rows, with columns and value digest equal to the
reference. It took **14.1 s**, against references of 46.3 s and 38.1 s that
day, and against 9.1 s for the hand-made 8-way split the day before.

**The 5 s gap is head-of-line blocking in the job, not the strikes list or
the service:**
- **Not the list:** the job's first batch arrived at 0.17 s, and that
  includes the strikes-list request.
- **Not the service:** it was faster that day, with references of 38-46 s
  against 59-70 s.
- **The cause:** the job started a new part only after the head part had
  been read to the end, so a slow strike at the head left the other 7 slots
  idle.
- **Simulation** (232 parts, `opt-split-001` latencies, 8 slots):

  | Scheduling | Median |
  | --- | ---: |
  | Next part only after the head (the old job) | 18.4 s |
  | Work-conserving (the hand-made run) | 10.0 s |

  The observed times (14.1 s and 9.1 s) fall in the same order.
- **Fix:** the job now sends a part whenever a request slot is free, still
  reads parts in order, and keeps at most 4× the concurrency parts open (32
  at PRO). In the simulation a window of 1× gives 18.4 s, 2× 12.0 s, and
  4× 10.0 s, which is work-conserving.

**Verified live (`opt-split-003`, main 5f17c10):** `job()` returned the same
938,663 rows in **7.4 s**, with rows, columns and digest equal to the
reference.
- **Same speed band:** that day's references were 48.3 s and 39.5 s, close
  to `opt-split-002`'s 46.3 s and 38.1 s. So the fair comparison is 14.1 s
  against 7.4 s, a 1.9× gain, where the simulation predicted 1.83×.
- **The single request:** `job()` is 5.3-6.5× faster than the all-strike
  request on the same day.
- **The hand-made 9.1 s** (`opt-split-001`) ran on a slower day, with
  references of 58.9-69.6 s. It is not a like-for-like comparison.

## Memory bound under W5 (planned)

The target is that memory is bounded by configuration. We claim this for our
accounted buffers only, never for the whole process.

**The configured ceiling** (PRO, defaults):

| Buffer | Bound |
| --- | ---: |
| Open job parts: 32 (4× concurrency) × 2 batches × ≤ 8 MiB | ≤ 512 MiB |
| 8 decoders building a batch, ≤ 8 MiB each | ≤ 64 MiB |
| 8 HTTP/2 stream windows of 2 MiB | 16 MiB |
| One Parquet row group | 131,072 rows |

Real batches are far smaller. A per-strike part over two hours is about
30k rows.

**The test:** `options_memory.py` runs M1, M2 and M3 (the W5b shape over 15,
60 and 120 minutes, about 0.9M, 4M and 7.5M rows), each in its own process,
streamed with `to_parquet` and held as a frame. Bounded means the streamed
peak minus idle stays flat as rows grow, while the frame's grows.

**First run** (2026-11-05, main b403a06): the streamed delta was 57.8, 103.8
and 130.6 MiB for 0.94M, 3.0M and 5.1M rows. That is far below the frames
(240.8, 636.4 and 1,041.0 MiB) but **not flat**. It is not recorded as
bounded.

**Cause, measured offline** (`examples/job_memory.rs`: the fixture in a
child process, a counting allocator):
- **The Parquet writer alone grew linearly:** 42 MiB at 1M rows and 4.3 GiB
  at 140M.
- **Only through decimal statistics:** turning statistics off gave a flat
  25 MiB, and the Decimal128 columns accounted for all of the growth.
- **The mechanism:** `parquet` 60.1 converts a written batch's decimals into
  one shared buffer. Each row group's min/max statistics keep slices of it,
  so whole batches stay alive until the file is closed.
- **Not our read-ahead:** a single stream stayed at 22.6 MiB, and the job's
  read-ahead levels off.

**Fix** (Coordinator decision): no min/max statistics on decimal columns by
default, with an explicit opt-in. Offline after the fix:

| Test | Before | After |
| --- | ---: | ---: |
| The writer alone, 140M rows | 4,299 MiB | 25.7 MiB |
| The 232-part job, 600k rows per part (139M rows) | 1,005 MiB | 107.6 MiB, flat from 150k rows per part |

`parquet` 60.1.0 (2026-10-28) is the newest release, so no upstream fix
exists yet. The rerun of M1-M3, plus M4 (a whole day), is the live check.

**Rerun after the fix** (2026-11-05, main f520b09, Windows, PRO; all OK):

| Workload | Rows | Rows per part (average) | Parquet delta | Frame delta |
| --- | ---: | ---: | ---: | ---: |
| M1 | 0.94M | 4.0k | 62.1 MiB | 241.3 MiB |
| M2 | 3.02M | 13.0k | 98.8 MiB | 631.8 MiB |
| M3 | 5.12M | 22.1k | 107.3 MiB | 1,030.7 MiB |
| M4 (whole day) | 12.85M | 55.4k | 133.9 MiB | (not run) |

The frame grows linearly. The Parquet delta grows 2.2× for 13.7× the rows:
not flat. **Two things change at once from M1 to M4: part size and total
rows.** An offline test separates them (`examples/job_memory.rs`: the
fixture in a child process; Rust heap peak and Linux peak RSS). It holds the
part size fixed and multiplies the parts:

| Rows per part | Parts | Total rows | Peak heap | Peak RSS |
| ---: | ---: | ---: | ---: | ---: |
| 55,000 | 232 / 464 / 928 | 12.8M / 25.5M / 51.0M | 73.1 / 74.5 / 75.5 MiB | 151.7 / 144.3 / 153.3 MiB |
| 200,000 | 232 / 464 / 928 | 46.4M / 92.8M / 185.6M | 99.4 / 104.2 / 112.2 MiB | 229.6 / 241.1 / 252.7 MiB |

- **Part size sets the level:** the read-ahead holds bigger batches as parts
  grow, up to its cap.
- **Total rows add only Parquet's footer metadata:** about 13 KB per row
  group, held until the file closes. That is about 0.1 MiB per million rows
  with 131,072-row groups.
- **No allocator retention:** RSS follows the heap.
- **The live runs fit this:** the average part grew 14× from M1 to M4.

**The ceiling we state.** It covers our own buffers, never the whole
process:
- the read-ahead: up to 4 × concurrency parts × 2 batches, each at most
  65,536 rows or 8 MiB;
- the decoders: concurrency × 8 MiB;
- the stream windows: concurrency × 2 MiB;
- one Parquet row group in progress;
- **plus** Parquet footer metadata of about 13 KB per row group, which grows
  with the file (`row_group_rows` sets how fast).

**M5 result (`opt-mem-M5-parquet-001`, main 699f245): the prediction
failed.**
- **Measured:** 26,576,223 rows and 203 row groups in 38.3 s. The delta was
  **204.4 MiB**, against about 135 MiB predicted.
- **The excess:** +70.5 MiB for +13.7M rows, about 5 MiB per million rows
  (0.68 MiB per added row group), where the offline test gave about 0.1.
- **The target stays unmet.**

The difference is live-only so far. The Python `to_parquet` path is the same
code as the offline example, so the Python layer is ruled out. Still open:
- part sizes on each day;
- our buffers (heap) against the Windows allocator (RSS);
- steady accumulation against a higher plateau.

`live_job_memory` measures them live:
- the Rust heap against rows written, through the run;
- the process peak working set;
- batch sizes.

It runs on M4, M4b (06-13 alone) and M5. The Python runner repeats M4b.

## Scope and order

Measurement first. One PR at a time; final titles before green; rule 3 holds.
Official paths only (no direct FPSS). No publish or tag.

| Step | PR / run | Content |
| --- | --- | --- |
| A | this PR | Vendor facts above; this plan |
| B | live tooling PR, then one Coordinator batch | (i) the session probe: two logins, then one request each, recording the gRPC code; (ii) the incremental memory rerun, both clients; (iii) the PRO entitlement map of every option RPC (one bounded request each, flat files read for a few messages only). Text stays local; the Coordinator sends classes, codes and tiers |
| C | scheduler PR | Size concurrency from the login's highest tier (PRO → 8), with an explicit override; no per-minute limit on paid tiers; FREE keeps 1 and 20/min |
| D | baseline PR, then runs | W4 (1-minute and 1 s quotes for a chain), W5 (tick quote and trade for a liquid root day), W6 (Greeks) against the vendor package: synchronous, plus a threaded variant at the same concurrency. First runs are bounded (a smaller root or a few expirations), then scaled. Every request stays under about 1M rows |
| E | one PR per change, each with evidence | The fan-out hypothesis (1 vs N shards, 1 vs N connections: is there a per-stream send cap?); planner splits for option tick (per day, expiration or contract) under 1M rows; Parquet throughput; whether the Polars per-call cost and the coalescing thresholds matter at this volume. D-B applies to data-path changes |
| F | coverage PR | Live checks for every option endpoint through `query()` and the Python package, recorded in the generated coverage table |
| G | later | W9: option flat files to Parquet with bounded memory. W10 real time needs the Terminal: when we get there, list what the user must provide. W10 comes last |

## Targets (from the approved plan)

- **W5:** time to complete ≥ **7×** the synchronous vendor package at 8-way,
  and ≥ **1.0×** the threaded variant at the same concurrency.
- **W4 and W6:** at least as fast as the threaded variant, and ≥ C× the
  synchronous package minus scheduling losses (C = 8).
- **Memory:** bounded by configuration under W5. Reported as total peak RSS
  and the incremental figure, for both clients. Only our accounted
  allocations are claimed bounded.
- **CPU per row:** the P1 replay method, on option captures.

## Limits we keep

- Requests under about 1M rows; concurrency ≤ 8; no over-limit probes.
- First runs bounded, then scaled. Captures are private and live on the
  user's D: drive; check free space before each capture run (a liquid root's
  tick day is GBs).
- Only one flat file at a time, never the 38 GB option quote file without a
  separate go-ahead.
- Vendor error text stays in private local reports; committed files hold
  codes and classifications only.

[conc]: https://thetadata.net/docs/Articles/Data-And-Requests/Concurrent-Requests.html
[subs]: https://thetadata.net/docs/Articles/Getting-Started/Subscriptions.html
[sizing]: https://thetadata.net/docs/Articles/Data-And-Requests/Request-Sizing.html
[flat]: https://thetadata.net/docs/Flat-Files/Getting-Started.html
[stream]: https://thetadata.net/docs/Streaming/Getting-Started.html
