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
| Messages being assembled, one per open stream, while in flight (item 8, closed 2026-11-10) | ≤ 8 × 64 MiB; about 3 MiB at observed sizes |
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

**Upstream status (checked 2026-11-07):**
- **Reported and fixed:** the bug is
  [apache/arrow-rs#12027](https://github.com/apache/arrow-rs/issues/12027),
  fixed by [apache/arrow-rs#12028](https://github.com/apache/arrow-rs/pull/12028)
  (merged to main b1614b8 on 2026-11-03).
- **Not released:** 60.1.0 is still the latest.
- **Repro:** the Coordinator checked it independently with a standalone repro:
  - With statistics on, 60.1.0 holds about 1 MiB per decimal column per row
    group.
  - Only precision above 18 is affected. Precision 18 or less is stored as
    INT64. Our prices are precision 38.
  - Git main 8330865 stays flat.
- **What we do:** we wait for a release and do not switch to a git
  dependency ([ROADMAP](../ROADMAP.md)).

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

**Diagnosis (main 57cac8f, Windows, PRO; all runs OK).** The samples of
heap against rows written show:
- **No accumulation:** the heap returns to 0.4-0.6 MiB after every run.
  Between peaks it moves between about 15 and 97 MiB with no upward trend,
  on both days of M5.
- **The peak is a stall:** each peak falls where the rows stop arriving for
  seconds while one slow request holds the head. The other open parts keep
  receiving and fill their buffers (the bounded read-ahead).
- **Same spot, different runs:** M4 stalled about 3.0M rows into 06-14 (5 s).
  M5 stalled at the same point of its second day, 06-14 (3.9 s), and peaked
  there. M5's first day did not repeat M4b's early stall (97.6 MiB against
  172.0).
- **Heap and RSS agree:** the peak working set above idle was 155-188 MiB
  against heap peaks of 155-182 MiB, so the Windows allocator adds little.

**Repeats** (heap peak above idle; process peak working set):

| Workload | Rows | Run 1 | Run 2 | Run 3 |
| --- | ---: | ---: | ---: | ---: |
| M4 (one day) | 12.85M | 154.9 MiB (RSS 176) | 103.3 MiB (RSS 138) | 107.2 MiB (RSS 133) |
| M4b (the other day) | 13.73M | 172.0 MiB (RSS 166) | | |
| M5 (both days) | 26.58M | 182.4 MiB (RSS 199) | 163.8 MiB (RSS 179) | 109.9 MiB (RSS 137) |

M5's lowest run (109.9 MiB) matches M4's (103.3 MiB) with twice the rows.
**The "M5 excess" was the spread of a stall-driven peak**, read from single
runs. That is why the prediction above failed, and why the repeats were run.

**Result: memory bounded by configuration under W5, met for the streaming
path** (`job()` to Parquet, the Rust heap):
- **Observed:** heap peaks of **103-182 MiB** above idle over 7 live runs of
  up to 26.6M rows. The process peak working set was 133-199 MiB (idle about
  11 MiB).
- **Configured worst case:**
  - the read-ahead: 4 × concurrency parts × 2 batches of at most 65,536 rows
    or 8 MiB (32 × 2 × 8 MiB = 512 MiB at PRO);
  - the decoders: 8 × 8 MiB;
  - the stream windows: 8 × 2 MiB;
  - messages being assembled, one per open stream, only while in flight:
    up to 8 × `encoded_bytes` (64 MiB), about 3 MiB at observed sizes;
  - one Parquet row group in progress.

  Peaks reach this only if one request stalls long enough for every other
  open part to fill.
- **Correction (2026-11-08): one term was missing.** tonic's per-RPC decode
  buffer keeps up to one encoded message per open stream (item 8, at most
  `encoded_bytes`, 64 MiB by default). That is up to 8 × 64 MiB = 512 MiB
  more in the worst case at PRO. The observed peaks above include it,
  because the counting allocator sees it, and real messages are far smaller.
  **Resolved 2026-11-10 (item 8 closed):**
  - since option 2a (#129), the client assembles messages itself, and an
    idle or stalled stream keeps nothing message-sized (#130);
  - the term is now part of the stated bound, only while messages are in
    flight: up to 8 × `encoded_bytes`;
  - real messages are at most 0.36 MiB (`opt-msgsize-001`), so about 3 MiB
    ([closure](../performance/eod-transport-buffering.md#closure-2026-11-10-under-decision-as-beta1-gate)).
- **Not bounded:**
  - Parquet footer metadata, about **13 KB per row group**, held until the
    file closes. It grows with the file (about 0.1 MiB per million rows at
    131,072-row groups).
  - The Python process baseline (interpreter, pyarrow, Polars), the
    allocator and the OS. The Python runner's RSS deltas were 62-204 MiB.
  - Holding results as frames grows with the rows (241 MiB to 1,031 MiB for
    M1-M3).
- **A tighter guarantee** would need a byte budget on the read-ahead, at the
  cost of the stall overlap that took the 232-part job from 14.1 s to 7.4 s.
  It is not proposed without a measured need.

## Step F: option endpoint live checks (`opt-checks-001`, 2026-11-05)

**The run:** main 838febb, Windows, options PRO, at 10:02 ET. Snapshots
used expiration 2026-11-05, and the flat files used 2026-11-04. Exit 0, scan
clean.
- **Rust:** all 47 non-flat-file requests returned OK through both `query()`
  and the legacy decoder, with equal row counts.
  - **Values equal:** the 33 history, at-time and list requests, and the
    open interest snapshot.
  - **13 snapshots differ** (ohlc 22/193 rows, trade 25/193, quote, market
    value and every Greeks snapshot 201/201). The two decoders read two live
    requests seconds apart in a moving market, so this neither shows nor
    rules out a decoder difference.
  - **Next:** a same-response replay (one response decoded both ways). Until
    then those snapshots are recorded as *rows equal*, not value-verified.
- **Python:** all 47 OK, with the same rows as Rust.
- **Snapshots** are entitled during market hours. The map's no-data was the
  expired contract and the closed market.
- **Flat files** (bounded probes, one message each):

  | File | First message | Bytes | Manifest |
  | --- | ---: | ---: | --- |
  | Option EOD | 0.3 s | 232,570 | no |
  | Option open interest | 0.1 s | 79,761 | no |
  | Option trade/quote | 0.3 s | 220,393 | no |

  The map's `InvalidArgument` was the same-day date. The option quote file
  (about 38 GB) was not probed.

**Same-response replay (`opt-same-001`, main f05c0e9, about 10:55 ET,
market open; exit 0, scan clean):**
- **Method:** each request sent once, and every message decoded by the
  columnar decoder and the legacy decoder.
- **All 17 equal on identical bytes.**
  - **The 14 snapshots:** one message each. Ohlc and trade had 195 rows; the
    others 201. They are now recorded as value-verified.
  - **The 3 flat files:** first message only (8,192 rows each). They decode
    as our tables.
- **So:** the snapshot differences in `opt-checks-001` were the market
  moving between two requests, not the decoders.

**What this means for step G (flat files to Parquet):**
- **Data, not a manifest:** over direct gRPC, a flat file streams as
  ordinary `ResponseData` messages, not a manifest pointing elsewhere. The
  vendor package sends them through the converter it uses for every other
  endpoint.
- **So G can reuse the existing path:** `query()`, the columnar decoder and
  `ParquetSink`. One file is one request, with no date or strike split. The
  single-stream path measured 22.6 MiB offline.
- **Still unverified** (decoding as our tables is now confirmed on the first
  message, `opt-same-001`):
  - whether a manifest arrives later in the stream (the decoder would stop
    with `Unsupported`);
  - the real row counts and file sizes.
- **Order:** the 50 MB open interest file, then the 150 MB EOD file, then
  the 1.2 GB trade/quote file, each measured for memory. Full downloads are
  a change from "flat-file probes never pull bulk data" and need the
  Coordinator's go-ahead. The 38 GB quote file needs the user's.

## Step G: option flat files to Parquet (2026-11-05)

`live_job_memory --flat-file` on main e3cd65c (Windows, options PRO, file
date 2026-11-04), the three approved files in order. All OK, scan clean, and
each Parquet file deleted after measuring.

| File | Rows | Time | Heap peak | Peak RSS (idle 11.4) | Parquet | Row groups |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Option open interest | 2,022,241 | 1.7 s | 13.7 MiB | 30.0 MiB | 12.2 MB | 16 |
| Option EOD | 2,030,667 | 3.9 s | 23.9 MiB | 43.1 MiB | 30.0 MB | 16 |
| Option trade/quote | 13,777,618 | 38.1 s | 33.1 MiB | 66.3 MiB | 297.8 MB | 106 |

- **The stream is data throughout:** every file decoded to the end with no
  manifest and no error.
- **What was verified, exactly:** values were compared with the legacy
  decoder on the first message only (`opt-same-001`); the whole files were
  decoded end to end.
  - **Why not the whole file:** a whole-file comparison would need another
    full download, and the legacy decoder would hold every row in memory
    (gigabytes for trade/quote). A streaming digest comparison is possible
    if wanted.
- **The sizes are in different units:** the vendor's 1.2 GB for trade/quote
  is the **CSV** size from their flat-file page. For 13.8M rows of 25 text
  fields that is about 87 bytes per row, which matches. Our 297.8 MB is
  ZSTD-compressed columnar Parquet, about 21.6 bytes per row, roughly 4×
  smaller. We did not record the gRPC bytes on the wire.
- **The default timeout is enough:** the largest file took 38.1 s, well
  inside the client's default 300 s request timeout. The vendor's "about 3
  minutes" is for its Terminal REST path. The runner's 1800 s was not
  needed, and the library default stays.
- **Memory:** one stream with no split peaked at 14-33 MiB of heap, and the
  heap after each run was 0.1-0.2 MiB.
- **Still not run:** the option quote file (about 38 GB) needs the user's
  go-ahead.

## Step F: how the checks run

- **Rust, `live_p0 option-checks`:** sends the PRO map's 51 option requests
  again.
  - **History, at-time and list requests** (2024-06-14, expiring
    2024-06-21) go through `query()`, and their values are compared with the
    legacy decoder on a second request.
  - **Snapshots** use `--snapshot-expiration`, a SPY expiration listed
    today, during US market hours.
  - **Flat files** use `--flat-file-date`, a past trading day within the
    vendor's 7 calendar days. They stay **bounded probes**, as in the map:
    one message of at most 1 MiB. They record whether the reply is a file
    manifest or data. **The option quote file (about 38 GB) is left out**
    unless `--include-option-quote-flat-file`, which needs the user's
    explicit go-ahead.
- **Python, `bench/p1/option_checks.py`:** the same 47 non-flat-file requests
  through the vendor-named methods: status, rows and column types.
- **Recording:** the results go into the generated coverage table.

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
| G | later | W9: option flat files to Parquet with bounded memory. W10 real time needs the Terminal: when we get there, list what the user must provide. W10 comes last. **W10 status (2026-11-10):** option quote latency measured live on Windows at 10, 100 and 1,000 contracts, p99 24-32 µs ([ADR-0018 W10 record](../adr/0018-explicit-connection-adapters.md)); the live full trade stream run remains |

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
