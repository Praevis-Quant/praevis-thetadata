# praevis-thetadata-client

gRPC client for ThetaData market data: generated typed requests for all 100
RPCs, exact Arrow `query()` and `job()` streams, a tier-aware scheduler and
Parquet output, plus the typed stock end-of-day (EOD) stream. It reuses a
session from `praevis-thetadata-auth`.

> **Independent and unofficial.** This is an unofficial Rust project. It is not
> affiliated with, sponsored by or endorsed by Theta Data Inc.; “ThetaData”
> identifies the service it interoperates with. `0.1.0-beta.1` is a prerelease
> whose public API is **frozen for beta.1**, except raw access
> (`raw_client`, `stream_raw`, `RawStream`), which stays low-level and unstable
> ([scope](../../docs/releases.md#beta1-scope-record-user-2026-11-10)): no breaking
> change without a new prerelease version and release notes. It is **not** a
> full port of, or drop-in replacement for, the vendor Python package. The Rust version is
> independent of the tracked Python compatibility baseline (currently 1.0.17).

## Status and scope

- **Typed stock EOD** (`StockEodRequest`, `stock_eod_batches`, `stock_eod`):
  validated one-symbol requests, exact values, pull-based batches, explicit
  partial results, deadlines and cancellation.
  - **Memory:** the Rust heap on this stream and on `query()`/`job()` is
    bounded by configuration:
    - decoded batch sizes, accounted bytes and decode work;
    - **plus up to one message per open stream while it is being
      assembled.** That's streams × `encoded_bytes` in the worst case. The
      client assembles messages itself, so an idle or stalled stream keeps
      nothing message-sized, and real messages measured at most 0.36 MiB.
  - **Not bounded:** total process memory (the allocator, libraries).
  - **Not covered:** `stream_raw`, which uses plain tonic and keeps up to
    one message per open stream for the stream's lifetime.

  This is **experimental**: synthetic acceptance is extensive, but
  live service semantics are only partly verified.
- **Query streams** (`ThetaClient::query`, `endpoints::*`, unreleased P1
  work): any of the 100 generated typed requests, streamed as exact Arrow
  `RecordBatch`es through the `praevis-thetadata-arrow` decoder. Rows from many
  messages are coalesced, and the first rows are emitted as soon as they
  arrive. A column's Arrow type is fixed by its first value; a column with no
  value yet is `Null`, and `collect()` gives it its final type. The stock EOD
  stream's deadlines, idle timeout, admission, cancellation and partial-result
  rules apply. Only the FREE-entitled endpoints have been seen working live
  (`docs/endpoints/coverage.md`).
- **Parquet output** (feature `parquet`, on by default): `to_parquet` on query
  and job streams. The file appears only once the whole stream is written.
  Memory stays flat however large the file. For that, decimal (price)
  columns are written without min/max statistics: the `parquet` writer would
  otherwise hold each row group's decimal statistics as slices of whole
  batches until the file closes. Every other column keeps its statistics.
  `ParquetOptions::decimal_statistics` opts in, at a memory cost that grows
  with the file. This works around an upstream bug
  ([apache/arrow-rs#12027](https://github.com/apache/arrow-rs/issues/12027),
  fixed by [#12028](https://github.com/apache/arrow-rs/pull/12028) but not
  yet released).
- **Jobs and scheduling** (`ThetaClient::job`, `SchedulerPolicy`, unreleased P1
  work): a request is split by date within the vendor's limits (EOD at 366
  days, intraday history per day), and the parts are merged in order.
  All-strike tick and 1 s option quote requests are also split per listed
  strike (one extra `option_list_strikes` request) when the scheduler runs
  at least 2 requests at once. The service waits about 60 s before answering
  them whole, and the split returned identical rows 6.4-7.6× sooner at 8
  ([request timing](../../docs/performance/request-timing.md)). A job sends
  a part whenever a request slot is free and reads parts in order. It keeps
  at most 4× the concurrency parts open, each buffering at most 2 batches,
  so a slow part does not hold back the others.
- **Per-call options** (`query_with` and `job_with`, `RequestOptions`):
  - batching;
  - an overall timeout per request (per part in a job). **It covers time
    queued for a scheduler slot**, so a busy scheduler cannot hold a caller
    past its budget; the request fails with `Deadline`;
  - an idle timeout between messages;
  - for jobs, a concurrency of at most the scheduler's, with the read-ahead
    window scaling with it.

  Each option left unset uses the client's setting. Every
  `query()` is admitted by the client's scheduler. By default it follows the
  account's tier from the login response (`ThetaClient::tier`, the highest
  subscription): FREE's documented 1 concurrent request and 20 per minute;
  VALUE 2, STANDARD 4 and PRO 8 concurrent requests with no per-minute limit.
  `with_scheduler` overrides it. Retry happens only on `RESOURCE_EXHAUSTED`
  and `UNAVAILABLE` at open.
- **One session per account** (confirmed live, `opt-session-001`): a new
  login for the account ends its earlier session, whose requests (on every
  query path, the stock EOD streams included) then fail with
  `QueryError::SessionEnded` (gRPC `UNAUTHENTICATED`). It is not retried
  and the client never logs in again by itself. Log in once and clone the
  client (Python: `existing_authorized_client`).
- **Errors** (`QueryError`, reviewed for beta.1 on 2026-11-11). Each
  category, with when it happens and the evidence for it:

  | Category | When | Evidence |
  | --- | --- | --- |
  | `Input`, `Configuration` | Invalid request or options, rejected before any network activity | Synthetic tests |
  | `Connection` | The channel or transport failed | Synthetic tests |
  | `Deadline`, `Idle` | The local whole-request budget (queue time included) or the idle gap between messages ran out | Synthetic tests |
  | `Cancelled` | The caller cancelled | Synthetic tests |
  | `NoData` | The service answered `NOT_FOUND` | **Live (stock EOD, FREE, 2026-11-02):** an unknown symbol and a range with no trading days (a weekend, a holiday) both answer `NOT_FOUND` at the headers, so both are `NoData`. **They are indistinguishable** (`err-notfound-001`, main b37e62b, Windows, 2026-11-11): the same code, the same details and the same message. To tell them apart, check the symbol yourself, for example with `StockListSymbols`/`IndexListSymbols` (Python: `stock_list_symbols`, `index_list_symbols`). A stream that ends cleanly with no rows is **not** `NoData`. In a job, empty parts are skipped, and the job is `NoData` only if every part was |
  | `SessionEnded` | `UNAUTHENTICATED`: the session ended | **Live** (`opt-session-001`): a second login for the account ends the first session |
  | `Remote(code)` | Any other gRPC status, with its code | **Live:** `PermissionDenied` for every tier refusal (it is never `NoData`), and `InvalidArgument` for one flat-file date. `Internal` was seen once, unexplained. `ResourceExhausted` and `Unavailable` are retried at open first |
  | `Decode`, `Schema`, `Unsupported`, `Resource` | The response broke a local rule: malformed data, a column changing type, an unsupported feature (a flat-file manifest), or a size limit | Synthetic tests and the adversarial corpus |

  **Python:** these become `QueryFailed(kind, detail, grpc_code)`, with
  `kind` the snake-case category (`"no_data"`, …). `SessionEnded` has its
  own subclass.
- **Raw helpers** call every descriptor RPC with wire query types. They are
  unsupported foundations: no typed validation, and no claim that any endpoint
  or entitlement works.
- Not included: flat-file downloads, real-time streaming or a dataframe API.
  Responses carrying a flat-file manifest are rejected.

## Install

Prereleases are never selected implicitly, so pin the version:

```toml
[dependencies]
praevis-thetadata-client = "=0.1.0-beta.1"
```

## Supported platforms

Windows and Linux are tested in CI. The client has no platform-specific code.
It uses `praevis-thetadata-auth` without its optional `native-store` feature, so
it pulls in no keyring or D-Bus dependencies. Pass it an in-memory `Session`.
Requires a Tokio runtime supplied by the caller.

## Example

```rust,no_run
use praevis_thetadata_client::{ClientConfig, NaiveDate, Session, StockEodRequest, ThetaClient};

async fn eod(session: Session) -> Result<(), Box<dyn std::error::Error>> {
    let client = ThetaClient::with_session(ClientConfig::default(), session).await?;
    let request = StockEodRequest::new(
        "AAPL",
        NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(),
        NaiveDate::from_ymd_opt(2026, 10, 5).unwrap(),
    )?;
    let mut stream = client.stock_eod_batches(request).await?;
    while let Some(batch) = stream.next_batch().await? {
        for row in batch.rows() {
            let _ = row; // process now; earlier rows are partial if a later call fails
        }
    }
    Ok(()) // clean end of stream: the query is complete
}
```

**Runnable examples,** in the repository (not in the package). Both run
against a synthetic loopback server, with no credentials:
- **`query_consumer`** (`query()` and `job()`):
  - **Partial results:** keep the rows delivered before a failure, and call
    them complete only after `Ok(None)`. `collect()` is all-or-error.
  - **Cancellation:** drop a `JobStream` mid-way. Its requests are cancelled
    and its scheduler slot is free at once.
- **`eod_partial`:** the same partial-result pattern on the stock EOD stream.

`cargo run -p praevis-thetadata-client --example query_consumer`

## License

Apache-2.0. The package includes `LICENSE` and `NOTICE`. Source, issues,
requirements and design records: <https://github.com/Praevis-Quant/praevis-thetadata>.
