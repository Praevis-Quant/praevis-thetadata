# praevis-thetadata-core

Transport-independent value types shared by the praevis-thetadata clients:
exact prices, validated timestamps, generic tables and immutable numeric
batches.

> **Independent and unofficial.** This is an unofficial Rust project. It is not
> affiliated with, sponsored by or endorsed by Theta Data Inc.; “ThetaData”
> identifies the service it interoperates with. `0.1.0-beta.2` is a prerelease
> whose public API is **frozen for beta.1** ([scope](../../docs/releases.md#beta1-scope-record-user-2026-11-10)): no breaking
> change without a new prerelease version and release notes. It is **not** a
> full port of, or drop-in replacement for, the vendor Python package. The Rust version is
> independent of the tracked Python compatibility baseline (currently 1.0.17).

## What it does

- `Price` holds an exact integer mantissa and decimal exponent, with no
  floating-point conversion.
- `Timestamp` holds a validated epoch-millisecond instant and its source time
  zone (New York or UTC). It formats RFC 3339 on demand.
- `Value`/`Table` form the formatted compatibility representation; `BatchValue`
  and `DataBatch` form the flat, numeric delivery representation.
- Timestamps whose precision is finer than milliseconds are rejected as
  unsupported rather than truncated (`check_timestamp_precision`).

It has no HTTP, gRPC, async runtime, keyring or CLI dependencies.

## Install

Prereleases are never selected implicitly, so pin the version:

```toml
[dependencies]
praevis-thetadata-core = "=0.1.0-beta.2"
```

## Supported platforms

Pure Rust; any platform supported by Rust 1.88+ and its dependencies
(`chrono`, `chrono-tz`, `serde`, `thiserror`). CI covers Windows and Ubuntu.

## Example

```rust
use praevis_thetadata_core::{Price, Timestamp};

let price = Price::from_wire(12345, 8).unwrap().unwrap(); // mantissa 12345, scale 8
assert_eq!(price.to_string(), "123.45");
let utc = Timestamp::from_wire(0, 1).unwrap();
assert_eq!(utc.to_rfc3339(), "1970-01-01T00:00:00.000Z");
```

## License

Apache-2.0. The package includes `LICENSE` and `NOTICE`. Source, issues,
requirements and design records: <https://github.com/Praevis-Quant/praevis-thetadata>.
