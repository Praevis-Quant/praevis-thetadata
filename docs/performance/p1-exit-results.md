# P1 exit results (2026-11-04)

The exit criteria of phase P1 ([design §1](../architecture/p1-design.md#1-goal-and-exit-criteria))
were measured on the maintainer's machine:
- **Hardware and account:** a Ryzen 7 2700, Windows 11, the FREE account,
  PROD.
- **Code:** branch `p1/exit-measurement` at 2d37dee. The P1 live checks ran
  on 18c23c4; 2d37dee changed only the runners.
- **Runs:** all by the Coordinator, with every report secret-scanned clean.
  The reports hold timings, counts, statuses and schemas only; the raw files
  stay private under `artifacts/`.
- **Pacing:** every network request was 3.5 s apart, and the client's
  scheduler kept FREE's documented limits (1 concurrent request, 20 per
  minute).

## Summary

| # | Criterion | Status |
| --- | --- | --- |
| a | Every FREE-entitled endpoint returns correct exact data on live checks | **Met.** 10/10, with values equal to the legacy decoder |
| b | W1 time to complete ≤ the vendor Python package's | **Met on medians, within the service's noise** |
| c | CPU per million rows ≥ 10× better than Python with Polars (offline replay of the identical P0 captures) | **Met for the Rust decoder** (15.9× / 10.4×). Through the Python package into a Polars frame it is **9.0× / 9.2×**, just under 10× |
| d | Peak memory ≤ 0.5× Python's for the same result | **Not met.** 0.97× (W1) and 0.79× (W2) |
| e | Compatibility aliases add no measurable overhead; no per-row Python objects | **Met** (CI gates) |

## a. Live checks (runs `p1-checks-001`, `p1-pychecks-001`)

The 10 endpoints the [entitlement map](../endpoints/coverage.md) found
entitled ran through `query()`. Each was then sent a second time through the
raw stream and the legacy decoder, and the two results were compared value by
value. Prices were compared as `Decimal128(38, 10)`, and timestamps as the
legacy RFC 3339 text.

| Endpoint | Rows | Query / legacy | Values equal |
| --- | ---: | --- | --- |
| `stock_history_eod` | 1 | OK / OK | yes |
| `option_history_eod` | 452 | OK / OK | yes |
| `stock_list_symbols` | 26,949 | OK / OK | yes |
| `stock_list_dates` | 3,634 | OK / OK | yes |
| `option_list_symbols` | 15,840 | OK / OK | yes |
| `option_list_expirations` | 2,163 | OK / OK | yes |
| `option_list_strikes` | 232 | OK / OK | yes |
| `index_list_symbols` | 24,331 | OK / OK | yes |
| `index_history_eod` | 1 | OK / OK | yes |
| `interest_rate_history_eod` | 1 | OK / OK | yes |

The Python package's vendor-named methods returned the same 10 results with
the same row counts. The types are exact: `decimal128(38, 10)` prices and
`timestamp[ms, tz=America/New_York]` times.

## b. Live end to end (`p1-py-compare-002` against `py-20261104T112952Z` / `T113137Z`)

These are the vendor-named methods with one shared session, measured like the
vendor baseline. The values are medians in milliseconds per request.

| Workload | Ours, Polars | Ours, pandas | Vendor today, Polars | Vendor today, pandas | Vendor at P0 (Polars / pandas) |
| --- | ---: | ---: | ---: | ---: | --- |
| W1 (n = 30 ours, 10 vendor) | 1,327 | 1,563 | 1,603 | 1,295 | 1,810 / 1,865 |
| W2 (n = 3 ours, 1 vendor) | 1,789 | 1,637 | 2,335 | 2,537 | 8,972 / 8,236 |

**Met on medians, but within the service's noise.** Our W1 requests ranged
from 90 ms to 3,262 ms. The vendor's own medians moved by a factor of up to
3.9 between P0 and today. Network and server time dominate both clients; a
single row of medians cannot rank them.

## c. CPU per row (identical P0 captures, offline)

| Path | W1 (252 rows/request) | W2 (9,164 rows) | Against the vendor with Polars |
| --- | --- | --- | --- |
| Rust decoder ([replay](p1-columnar-results.md)) | 3.46 µs/row | 6.07 µs/row | **15.9× / 10.4×** |
| Python package, decode only (`_decode_frames`) | 3.66 µs/row | 6.49 µs/row | |
| Python package, decode plus Polars frame | 6.01 µs/row (1.51 ms) | 6.59 µs/row (60.4 ms) | **9.0× / 9.2×** |
| Python package, decode plus pandas frame | 7.67 µs/row (1.93 ms) | 6.72 µs/row (61.6 ms) | 9.5× / 11.9× against vendor pandas |
| Vendor, Polars (today) | 54.1 µs/row (13.62 ms) | 60.6 µs/row (555.7 ms) | |
| Vendor, pandas (today) | 72.8 µs/row (18.34 ms) | 79.8 µs/row (731.6 ms) | |

- **Rust:** the criterion as defined, CPU per million rows on the offline
  replay, is met.
- **Python into Polars:** it is just under 10×.
  - **W1:** the gap comes from a fixed cost of about 0.6 ms per call to build
    the Polars frame, which dominates a 252-row request.
  - **W2:** decoding is 98% of the time, so the frame is not the
    bottleneck.

## d. Peak memory (separate process per workload)

| Workload | Ours (`p1-mem-ours-*-002`) | Vendor (`py-20261104T112952Z` / `T113137Z`) | Ratio |
| --- | ---: | ---: | ---: |
| W1 | 110.5 MiB | 113.9 MiB | **0.97×** |
| W2 | 115.8 MiB | 147.0 MiB | **0.79×** |

**Not met: the criterion is ≤ 0.5×.** Most of each process is the interpreter
and the pyarrow, Polars and pandas imports that both clients load. The data
itself is small at these sizes: W2 is 2.5 MB on the wire. The criterion is
not redefined here.

**Proposal for a separate user decision:** add an incremental metric,
measured for both clients. It would be the peak minus an idle baseline taken
in the same process after imports and login, before the first request.
- **Not available from these runs.** The reports hold only one end-of-run
  peak per process, so this record does not estimate the figure.
- **Instrumentation is in place.** Both runners now record
  `import_peak_rss_bytes` at that point, so a rerun of the two memory commands
  would give the incremental figure for both clients.

## Full path over loopback (not client cost)

`columnar_benchmark --replay <captures> --loopback` serves the captures from
a gRPC server inside the same process. **Its totals are not client cost.** On
Windows, the in-process server and the gRPC transport cost 34.3 µs/row (W1)
and 35.7 µs/row (W2), one row per message (`transport-drain`). The wall time
each engine adds on top of the transport (`replay-loopback-ryzen-002`) is:

| Engine | W1 | W2 |
| --- | ---: | ---: |
| `query()` to Arrow | 0.49 µs/row | −0.02 µs/row (noise) |
| `query()` to Parquet | 3.88 µs/row | 0.67 µs/row |
| Typed stock EOD stream (bounded decoder) | 13.5 µs/row | 15.0 µs/row |

Decoding overlaps with receiving, so `query()` adds almost no wall time on
top of the transport. This is added wall time, not CPU; CPU per row is
section c.

## Finding: one session per account (confirmed by `opt-session-001`)

The first exit run (`p1-py-compare-001`, `p1-mem-ours-*` 001) made two
`Client`s, which each logged in. The first-logged-in client then failed
about 118 ms into every first request, while the second succeeded. The rerun
shared one session (`existing_authorized_client`, the vendor's mechanism) and
had no failures.

**Confirmed on 2026-11-05** by the session probe `opt-session-001` (Windows,
PROD, main 8aaa445): log in as A, log in again as B, then one request on
each. A failed with **`UNAUTHENTICATED`** (gRPC 16) about 120 ms in, with no
message; B returned its row. On the historical gRPC path, **a new login ends
the account's earlier session.** This contradicts ThetaDataDx's README, which
says separately logged-in market-data clients coexist
([review](../research/thetadatadx-review.md#watch-items)).

The client now reports this as a typed error: `QueryError::SessionEnded` from
Rust's `query()` and `job()`, and `SessionEnded` (a `QueryFailed`) in Python, "session is no longer
valid; another login may have ended it". It is not retried, and the client
never logs in again by itself. Share one session instead: clone the Rust
client, or pass `existing_authorized_client` in Python.
