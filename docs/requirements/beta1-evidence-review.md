# Beta.1 requirement-to-test and evidence review (2026-11-11)

One row per frozen public area of the
[beta.1 scope](../releases.md): its synthetic tests and its live evidence.
Gaps are listed explicitly below the table, not hidden in counts.

**Live** means a manual, approved run against the real service. Each run is
on Windows unless stated, on the account of that date:
- the FREE account until 2026-11-04;
- options PRO, with stocks and indices FREE, from then on.

CI runs every synthetic test on Windows and Linux.

| Area | Public surface | Synthetic tests (CI) | Live evidence |
| --- | --- | --- | --- |
| **Authentication** | `AuthClient`, `AuthConfig`, `Credentials`, `Session`, `Environment`, `User` | `praevis-thetadata-auth/src/tests.rs` (mock HTTP: API key, email/password, rejection, no secret in output); CLI `tests/no_save.rs`, `startup.rs` | PROD login on 2026-11-01 ([alpha.1 gate](../research/live-observations.md#alpha1-prod-exit-gate-2026-11-01)). Tier read from the login response (options PRO = 3, FREE = 0, 2026-11-04). One session per account (`opt-session-001`) |
| **Native store** (optional) | `native-store` feature; CLI `auth --save`, `status`, `logout` | `store.rs` tests. CLI `tests/persistence.rs` on a real Windows Credential Manager, and on Linux Secret Service (`tools/test-linux-persistence.sh`), in CI | Windows `--save`/status/logout lifecycle (alpha.1 gate) |
| **Core values** | `Price` (with `numeric_cmp`/`numeric_eq`), `Timestamp`, `TimeZone`, `Value`, `Table`, `DataBatch` | `praevis-thetadata-core/src/lib.rs`, `batch.rs` unit tests and doctests; the adversarial corpus (`client/src/corpus.rs`) | Stock EOD values on PROD, byte-identical across three dates ([semantics](stock-eod-evidence.md#observed-service-semantics-eod-l3-029)). Millisecond-only timestamps observed (ADR-0019) |
| **Arrow decoder** | `ColumnarDecoder`, `Limits`, `ColumnKind`, the Arrow type mapping | `arrow/tests/decoder.rs` (13); the differential oracle against the legacy decoder in the adversarial corpus (`client/src/corpus.rs`); the D-B decode benchmark gate | Values equal to the legacy decoder for 57 RPCs: 33 on a second request, 24 on the same response (`opt-checks-001`, `opt-same-001`, `sir-checks-001`, …) |
| **Parquet sink** | `ParquetSink`, `ParquetOptions`, `ParquetSummary`, `to_parquet` on query and job streams | `arrow/tests/parquet.rs` (5), `parquet_memory.rs`; `client/tests/output.rs` (whole file or no file) | Three option flat files decoded whole to Parquet (#102). Job to Parquet: 7 runs, Rust heap peaks of 103-182 MiB ([W5 record](../architecture/options-phase.md#memory-bound-under-w5-planned)) |
| **`query()`** | `query`, `query_with`, `QueryStream` (`next_batch`, `collect`), `unify`, `BatchPolicy` | `client/tests/query.rs` (9): coalescing, partial rows before an error, idle/cancel terminal, null-column typing. `query_consumer` example tests | Every live value check above goes through `query()` |
| **`job()` and the planner** | `job`, `job_with`, `JobStream`, `plan` | `client/tests/job.rs` (13): split rules, order, empty parts, slot release on drop, concurrency caps | `job()` returned the same rows as single requests (`opt-split-003`). The W5 job runs |
| **Scheduler** | `SchedulerPolicy`, `Tier`, `with_scheduler` | `scheduler.rs` unit tests; `job.rs`: tier default, retry only at open | PRO concurrency 8 used in the W5 and options runs. FREE pacing respected in the FREE map |
| **Per-call options** | `RequestOptions` (batch, timeout including queue time, idle timeout, concurrency) | `job.rs`: the timeout covers queued time, the idle override, concurrency checks. The examples' 5 s timeout | None specific (see the gaps) |
| **Generated endpoints** | `endpoints::*` (100 requests), `Endpoint` (sealed), `ALL`, `EndpointInfo` | `tools/test_generate_endpoints.py`; `client/tests/endpoints.rs` (8): wire mapping against the vendor wrappers; `generate_endpoints.py --check` | See the [coverage table](../endpoints/coverage.md). Of 81 vendor wrappers: **60 value-checked live**, **20 seen only as tier refusals**, **1 gated, not probed** |
| **Errors** | `QueryError` (and `EodError`), Python `QueryFailed`/`SessionEnded` | `query.rs` (opening errors), `eod_contracts.rs`, `eod_typed.rs` | `NOT_FOUND`, `PERMISSION_DENIED`, `UNAUTHENTICATED` and `INVALID_ARGUMENT` observed ([client README](../../crates/praevis-thetadata-client/README.md), Errors). An unknown symbol and no trading days are identical at the service: the same code, details and message (`err-notfound-001`) |
| **Memory bound** | The stated Rust-heap bound (item 8) | `eod_memory -- bound` (#130, the memory-profile workflow); `parquet_memory.rs` | Largest real message 0.36 MiB (`opt-msgsize-001`). The W5 heap runs |
| **Python, vendor-named methods** | 81 `Client` methods with the 1.0.17 signatures, credential order, `dataframe_type` | `test_vendor.py` (signatures against the pinned inventory); `test_credentials.py`; `test_api.py` | Python OK on every entitled method in `opt-checks-py-001` and `sir-checks-py-001`, with status matching Rust on every refusal |
| **Python, `query`/`batches`/`to_parquet`** | `Result`, `BatchIterator` (`cancel`), Arrow export, per-call keywords | `test_arrow.py`, `test_api.py`, `test_gates.py` (the D.2 overhead gates), `test_examples.py` (loopback partial results and cancellation) | `to_parquet` live in the options memory runs (`opt-mem-M*`, for example `opt-mem-M5-parquet-001`) |
| **Install and build** | The published archives and the wheel | The packaging rehearsal (seven consumers outside the workspace, CLI install); clean-venv wheel install; MSRV 1.88 ([record](../crates-io-release-plan.md)) | alpha.3 installed from crates.io and the release URL, on Windows and Linux x86_64 |

## Gaps

None of these is hidden by the counts above.

**Endpoints:**
- **20 vendor wrappers have no data-path evidence.** They are refused on
  this account: stock and index snapshot, history (except EOD), at-time,
  and the stock and index flat files. Their decoding is the generic path,
  checked on other RPCs, not on their own responses.
- **One wrapper was never probed:** the stock quote flat file, gated
  because even a probe may start a very large file.
- **17 descriptor-only RPCs were never probed** (futures, dividends). The
  2 that were probed answered `NOT_FOUND`. All 19 are unstable and outside
  the vendor surface.
- **Paid stock and index tiers, and the STAGE environment:** never run.
  The VALUE (1) and STANDARD (2) tier numbers in the login response are
  inferred from order, not observed.

**Behaviour:**
- **Retry:** `RESOURCE_EXHAUSTED` and `UNAVAILABLE` were never observed
  live, so the retry has only synthetic evidence. Rate limits are not
  probed, by policy.
- **`RequestOptions` timeouts** have no live run of their own. Live runs
  used the defaults.
- **Both gaps have a live check prepared** (2026-11-12, user-approved):
  `live_p0 retry-timeout`, with one bounded over-limit burst for the retry.
  This review will record its result.
- **`Remote(Internal)`:** seen once on a second request, unexplained.

**Platforms:**
- **Live runs were on Windows only,** apart from the stream crate's Linux
  run, which is pending and outside the freeze. Linux is covered by CI's
  synthetic tests and by the alpha.3 install checks.
- **Python 3.10 and 3.11:** the wheel claims them, but CI tests only 3.12.
  **Closed after beta.1 (2026-11-12):** CI now installs the wheel into a
  clean environment and runs the package tests on Python 3.10, 3.11, 3.12
  and 3.13, on Windows and Linux.

**Data semantics, unresolved** ([stock EOD semantics](stock-eod-evidence.md#observed-service-semantics-eod-l3-029)):
- dividend and other corporate-action adjustment;
- later corrections;
- early closes;
- the meaning of `created` and `last_trade`.
