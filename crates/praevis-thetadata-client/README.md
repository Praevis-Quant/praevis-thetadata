# praevis-thetadata-client

Experimental gRPC client for ThetaData market data. It reuses a session from
`praevis-thetadata-auth` and offers raw RPC helpers plus a typed, bounded stock
end-of-day (EOD) stream.

> **Independent and experimental.** This is an unofficial Rust project. It is not
> affiliated with, sponsored by or endorsed by Theta Data Inc.; “ThetaData”
> identifies the service it interoperates with. `0.1.0-alpha.1` is an early
> prerelease: APIs may change between alphas, and it is **not** a full port of, or
> drop-in replacement for, the vendor Python package. The Rust version is
> independent of the tracked Python compatibility baseline (currently 1.0.17).

## Status and scope

- **Typed stock EOD** (`StockEodRequest`, `stock_eod_batches`, `stock_eod`):
  validated one-symbol requests, exact values, pull-based batches, explicit
  partial results, deadlines, cancellation and finite resource limits. This is
  **experimental**: synthetic acceptance is extensive, but live service
  semantics are only partly verified.
- **Raw helpers** call every descriptor RPC with wire query types. They are
  unsupported foundations: no typed validation, and no claim that any endpoint
  or entitlement works.
- Not included: flat-file downloads, real-time streaming or a dataframe API.
  Responses carrying a flat-file manifest are rejected.

## Install

Prereleases are never selected implicitly, so pin the version:

```toml
[dependencies]
praevis-thetadata-client = "=0.1.0-alpha.1"
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
