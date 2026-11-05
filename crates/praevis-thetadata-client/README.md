# praevis-thetadata-client

Experimental gRPC client for ThetaData market data. It reuses a session from
`praevis-thetadata-auth` and offers raw RPC helpers plus a typed stock end-of-day
(EOD) stream with finite limits on its own accounted decode allocations.

> **Independent and experimental.** This is an unofficial Rust project. It is not
> affiliated with, sponsored by or endorsed by Theta Data Inc.; “ThetaData”
> identifies the service it interoperates with. `0.1.0-alpha.2` is an early
> prerelease: APIs may change between alphas, and it is **not** a full port of, or
> drop-in replacement for, the vendor Python package. The Rust version is
> independent of the tracked Python compatibility baseline (currently 1.0.17).

## Status and scope

- **Typed stock EOD** (`StockEodRequest`, `stock_eod_batches`, `stock_eod`):
  validated one-symbol requests, exact values, pull-based batches, explicit
  partial results, deadlines and cancellation. **Memory:** only the client's own
  accounted allocations are bounded: decoded batch sizes, per-batch accounted
  bytes and in-flight decode work. Total process memory is not bounded.
  Profiling showed that libzstd stays within the accounted budget, but
  transport buffering adds roughly one full encoded message per concurrently
  open stream. For large responses, lower `EodPolicy::concurrent_streams`. This is **experimental**: synthetic acceptance is extensive, but
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
- **Jobs and scheduling** (`ThetaClient::job`, `SchedulerPolicy`, unreleased P1
  work): a request is split by date within the vendor's limits (EOD at 366
  days, intraday history per day), and the parts are merged in order. Every
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
- **Raw helpers** call every descriptor RPC with wire query types. They are
  unsupported foundations: no typed validation, and no claim that any endpoint
  or entitlement works.
- Not included: flat-file downloads, real-time streaming or a dataframe API.
  Responses carrying a flat-file manifest are rejected.

## Install

Prereleases are never selected implicitly, so pin the version:

```toml
[dependencies]
praevis-thetadata-client = "=0.1.0-alpha.2"
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

## License

Apache-2.0. The package includes `LICENSE` and `NOTICE`. Source, issues,
requirements and design records: <https://github.com/Praevis-Quant/praevis-thetadata>.
