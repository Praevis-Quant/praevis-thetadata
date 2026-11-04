# P1 design: breadth and columnar on FREE

Status: **approved by the user on 2026-11-04, with every recommendation**
(decisions in §10). Implementation follows the PR sequence in §9, one PR at a
time under rule 3. This implements phase P1 of the approved
[performance plan](performance-plan.md#e-phased-roadmap-tied-to-tiers), with the
design adjustments measured in [P0](../performance/p0-results.md#6-p1-design-adjustments-from-this-evidence).

## 1. Goal and exit criteria

Every FREE-entitled endpoint is reachable, typed, from Rust and Python. Data
comes back as **exact Arrow columns**, streamed in coalesced batches, with
Parquet output. The P1 exit criteria from the plan, unchanged:

| # | Criterion | How it is measured |
| --- | --- | --- |
| a | Every FREE-entitled endpoint returns correct exact data on live checks | One polite live check per endpoint (§8), exact-value comparison with the legacy decoder |
| b | W1 time to complete ≤ the vendor Python package's | `live_p0`-style interleaved runs; distributions, not single medians (P0 §2) |
| c | CPU per million rows ≥ 10× better than Python with Polars | Offline replay of the **identical** P0 captures on the user's Ryzen. Measured floor: ~2.5 µs/row from libzstd, so the target means ≤ ~5.5 µs/row there |
| d | Peak memory ≤ 0.5× Python's for the same result | `eod_memory`-style process counters on both clients |
| e | Compatibility aliases add no measurable overhead, and no per-row Python objects are created | The D.2 gates in the plan |

Headline metrics reported with every result: **time to first row**, CPU per
row, peak memory, and total time with its spread.

## 2. Components and crates

| Component | Where | Notes |
| --- | --- | --- |
| Generated typed requests | `praevis-thetadata-client`, module `endpoints` (generated, committed) | All 100 RPCs; §3 |
| Columnar decoder | New crate **`praevis-thetadata-arrow`** (depends on core and `arrow`) | Keeps `arrow` out of auth, core and proto. The client depends on it behind a default-on `arrow` feature |
| Query engine (streams, coalescing, scheduler) | `praevis-thetadata-client` | Replaces the per-endpoint stream; the existing admission, deadlines and cancellation are kept |
| Parquet sink | `praevis-thetadata-arrow`, feature `parquet` | Streaming writer, one row group per batch |
| Python package | New crate **`praevis-thetadata-py`** (`pyo3`, built with maturin), module `praevis_thetadata` | Outside the default workspace build if wheels need special CI; §6 |
| Legacy and bounded decoders | Unchanged | Kept as **oracles** for the Arrow decoder's tests, and as the existing API until it is retired |

Dependencies: `arrow` (features `ffi`), `parquet`, `pyo3` (approved as a set,
decision 3). No other new dependency. The Arrow C Stream export is implemented
with `arrow::ffi_stream` and a `pyo3` capsule, not an extra crate.

## 3. Generated typed requests

**Inputs:**
- the protocol descriptor (100 RPCs, request fields and presence);
- the 1.0.17 catalogue (wrapper names, signatures, defaults and the
  per-field assignment conditions recorded in `python-1.0.17-inventory.json`).

**Output:** a committed, reviewable `endpoints.rs`. For each RPC:
- a request struct with typed fields: dates as `NaiveDate`, times as
  `NaiveTime`, enums for `right`, `venue`, `interval` and `request_type` where
  the catalogue documents values, otherwise strings;
- defaults identical to the vendor wrapper;
- **presence rules reproduced**: a field is sent only under the vendor's
  condition. The "explicit `False` cannot be sent" quirk is reproduced in the
  compatibility layer and offered correctly in the enhanced API.

The generator is a tool in `tools/` (Python, standard library, reading the
committed research JSON). CI checks the generated file is up to date, like the
research check. Hand-written code is kept only for cross-cutting validation
(symbols, date ranges, contract specs).

The 17 futures and 2 corporate-action RPCs have no vendor wrapper. They are
generated as raw typed requests with an `unstable` marker and no
compatibility alias.

## 4. Columnar decoder

**Schema.** Column names come from the headers of the first message, compared
**in place**, byte for byte, on later messages: no `String` allocation per
message. A changed header list is a schema error, as today.

**Types**, fixed by the first non-null value of each column:

| Wire `DataValue` | Arrow type | Rule |
| --- | --- | --- |
| text | `Utf8` | Per-batch dictionary encoding was dropped on 2026-11-04 (agreed): it would change a column's type between batches. A fixed, opt-in choice per query may come later |
| number (int64) | `Int64` | |
| price (`value`, `scale`) | **`Decimal128(38, 10)`** | unscaled = `value × 10^scale` (exponent = scale − 10). Every wire scale 1-19 maps exactly: at most 2.1×10^28 < 10^38. Scale 0 is null, as today |
| timestamp | **`Timestamp(Millisecond, tz)`** (see decision D1) | tz = `America/New_York` for zone 0, `UTC` for zone 1. A column mixing zones is a schema error. ADR-0019 still rejects non-millisecond precision |
| boolean | `Boolean` | |
| null / unset | null in the column's type | A column that is null until its first value is backfilled with nulls; an all-null column is `Null`. Between batches, the only schema change is `Null` becoming typed; `collect()` gives earlier batches the final type (agreed 2026-11-04) |

A value of a different kind than its column's type is a schema error, never a
silent coercion.

**Per-message path** (P0 adjustments 2 and 3):
- **Inline by default.** A message below a threshold (64 KiB encoded) is
  decoded on the receiving task; only larger messages go to the blocking pool,
  as today.
- **No per-message allocation once warm.** A per-stream (or per-worker)
  scratch buffer for decompression, the existing hand reader parsing straight
  into the Arrow builders, and reused builders.
- **Limits and accounting:** `DecodeLimits`, preflight and the adversarial
  corpus are kept. The corpus gains a third oracle: Arrow output equals the
  legacy decoder's exact values.

**Coalescing (P0 adjustment 1).** Rows from many messages are appended to the
same builders. A batch is emitted when any threshold is reached:
- `batch_rows` (default 65,536);
- `batch_bytes` (default 8 MiB, accounted);
- `max_batch_latency` (default 50 ms since the first buffered row);
- end of stream.

The **first batch flushes after the first message** by default, so time to
first row equals the vendor's first-message time.

## 5. Query engine and scheduler

- **One generic stream:** `client.query(request) -> QueryStream`, yielding
  `RecordBatch`, with `.collect()` to a table and `.to_parquet(path)`. The
  stock-EOD typed API stays as a thin wrapper until it is retired.
- **Planner:** splits jobs into requests within vendor limits: EOD spans ≤ 366
  days (P0 §5), and per-day splits for tick data (vendor sizing guidance). It
  merges results in order, across shards, into one stream.
- **Scheduler:**
  - a semaphore sized to the tier's concurrency;
  - a per-minute limiter (FREE default 20/min, the documented minimum; P0
    observed no limit at 40/min);
  - retry with jittered exponential backoff only on `RESOURCE_EXHAUSTED` and
    `UNAVAILABLE`; never on `NOT_FOUND`, `PERMISSION_DENIED` or
    `INVALID_ARGUMENT`;
  - cancellation of a job cancels its queued and running requests.
- **Errors:** `EodError` generalizes to `QueryError`, with the same categories
  (an alias keeps old code compiling during the alpha).
- **Option 2a (item 8):** the framing body already assembles each message.
  Decoding inline from that assembled frame is the natural place, so 2a is
  absorbed here rather than merged separately, and it is benchmarked with the
  rest.

## 6. Python package (decision 5: a thin pass-through)

- **Package and module:** `praevis-thetadata` on PyPI (to confirm, D3), module
  `praevis_thetadata`. Built with maturin; `abi3` wheels for CPython ≥ 3.10
  (to confirm, D4).
- **Results** expose `__arrow_c_stream__`, so pyarrow, Polars and pandas
  import zero-copy, plus `.to_polars()`, `.to_pandas()`, `.to_arrow()` and
  `.batches()` (a streaming iterator). All network and decode work runs with
  the GIL released.
- **API:**
  - the enhanced API is primary (`client.history.stock_eod(...)`, `*_batches`,
    `*_to_parquet`, concurrency/deadline/cancel arguments);
  - vendor-named methods are **generated aliases** forwarding to the same
    native call, with vendor signatures and defaults plus optional enhanced
    keywords;
  - `dataframe_type='polars'|'pandas'` maps to the zero-copy conversion.
- **Credentials:** same order as the CLI (ADR-0020). The key is never logged.
  A log guard like the harness's is applied to our own logger.
- **Gates (plan D.2):** alias overhead within noise and ≤ 5 µs per call;
  Python allocations per million rows ≈ 0 (constant at 1M and 10M rows).
- **CI:** builds the wheel and runs its unit tests (no network) on Windows and
  Linux. The Python test dependencies (pyarrow, polars, pandas) are pinned with
  hashes, like the baseline harness, but are never the vendor package.

## 7. Testing and benchmarks

- **Generated coverage table** (lighter process): endpoint, typed builder,
  decoder test, live-check status. Generated and committed; replaces
  per-clause matrices for new endpoints.
- **Decoder:**
  - differential tests: Arrow equals the legacy decoder, on the corpus, on
    synthetic vendor-shaped fixtures (16/20 columns, one row per message,
    ZSTD), and on tables with every value kind;
  - property tests for coalescing: any split of messages gives identical
    batches when concatenated.
- **Benchmarks:**
  - a CI benchmark of vendor-shaped one-row-per-message workloads, gated by
    the D-B rules;
  - offline replay of the user's private P0 captures on the Ryzen (Coordinator
    only), for criterion c;
  - live distributions for criterion b.

## 8. FREE entitlement map (needs approval)

Criterion a needs to know which endpoints FREE allows. Proposed:
- **one tiny request per endpoint family**, sequential, paced at 3.5 s, about
  35 requests in total;
- each result recorded as data, `NOT_FOUND`, `PERMISSION_DENIED` or another
  code;
- stop at the first `RESOURCE_EXHAUSTED`.

This is **not** concurrency probing; concurrency stays at ≤ 3, as instructed.

## 9. Sequence of PRs (each green under rule 3 before the next)

Reordered on 2026-11-04 at the user's request: the FREE entitlement map moves
up to directly after the generator, because which endpoints FREE allows
decides which generated endpoints get live checks in the later PRs.

1. The endpoint generator, generated typed requests and coverage table.
   Raw-stream access only; no decode change.
2. **The FREE entitlement map** (§8, approved): one tiny request per endpoint
   family through the generated requests. Results go into
   `docs/endpoints/status.json` and the coverage table.
3. The `praevis-thetadata-arrow` columnar decoder, with differential and corpus
   tests and the CPU benchmark (no API yet).
4. The query engine: coalescing streams, planner, scheduler and the
   `QueryError` generalization, with a before/after benchmark (D-B).
   **Split on 2026-11-04 (agreed):** 4a is the generic `query()` stream with
   coalescing, inline decoding from the assembled frame (option 2a) and
   `QueryError`; 4b is the planner and scheduler. In 4b, retry happens only
   at open, before any row is delivered, so a retry cannot duplicate rows.
   The typed stock EOD stream stays unscheduled until it becomes a wrapper
   over `query()`.
5. The Parquet sink.
6. The Python package, compatibility aliases and the D.2 gates (CI wheels).
   **Split on 2026-11-04,** like PR 4: 6a is the native core (all endpoints by
   name, the Arrow PyCapsule hand-off, streaming, Parquet, the gates and CI
   wheels); 6b is the generated vendor-named aliases with vendor signatures
   and `dataframe_type`, plus the full ADR-0020 credential order.
7. Live checks of the FREE-entitled endpoints (from PR 2's map), and the P1
   exit measurement: replay plus live runs, and a results record.

## 10. Decisions (approved 2026-11-04)

| # | Decision |
| --- | --- |
| D1 | **Timestamps in milliseconds** (`Timestamp(Millisecond, tz)`) now; nanoseconds once a nanosecond feed is verified |
| D2 | **A separate `praevis-thetadata-arrow` crate** |
| D3 | **PyPI name `praevis-thetadata`.** The Coordinator found it unregistered on 2026-11-04; checking does not reserve it |
| D4 | **Minimum Python 3.10, `abi3` wheels** |
| D5 | **FREE scheduler defaults to the documented limits:** 1 concurrent request, 20 per minute. The observed 3 and 40/min are available only as an explicit opt-in |
| D6 | **FREE entitlement map approved** (§8): about 35 single requests, 3.5 s apart. The concurrency probe above 3 is **deferred** to a later user decision; it is not run |

The proposed options, as recorded:


- **D1, timestamp unit:** `Timestamp(Millisecond)` now (exact for all observed
  data; no year-2262 limit; no conversion cost), or the plan's
  `Timestamp(Nanosecond)` now (one type for the future nanosecond feed, but a
  year-2262 bound and a ×10^6 multiply per value)? **Recommendation:**
  milliseconds now, nanoseconds when a nanosecond feed is verified.
- **D2, crate layout:** a separate `praevis-thetadata-arrow` crate (above), or
  Arrow types inside the client? **Recommendation:** the separate crate.
- **D3, Python distribution name:** `praevis-thetadata`?
- **D4, minimum Python version:** 3.10 (`abi3`, broadest), or 3.12 like the
  vendor package?
- **D5, FREE scheduler defaults:** concurrency 1 (documented) or 3 (observed);
  per-minute limit 20 (documented) or off? **Recommendation:** documented
  values by default, observed values only as an explicit opt-in.
- **D6:** approve the FREE entitlement map (§8).
