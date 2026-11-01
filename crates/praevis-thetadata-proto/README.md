# praevis-thetadata-proto

Generated protobuf message types and tonic gRPC client/server bindings for
ThetaData's direct market-data service. The definitions were recovered
statically from the vendor Python distribution: the checked-in descriptor
covers Python **1.0.17**, with **100 RPCs**.

> **Independent and experimental.** This is an unofficial Rust project. It is not
> affiliated with, sponsored by or endorsed by Theta Data Inc.; “ThetaData”
> identifies the service it interoperates with. `0.1.0-alpha.1` is an early
> prerelease: APIs may change between alphas, and it is **not** a full port of, or
> drop-in replacement for, the vendor Python package. The Rust version is
> independent of the tracked Python compatibility baseline (currently 1.0.17).

## What it contains

- `endpoints` and `beta_endpoints` modules generated at build time from the
  checked-in `schema/thetadata.bin` descriptor set. Building needs **neither
  Python nor `protoc`**.
- `FILE_DESCRIPTOR_SET` holds the descriptor bytes, and `schema/manifest.json`
  records the source version, source hashes and RPC list.
- Generated bindings are raw wire types. A generated method does **not** mean
  the service supports that endpoint for your account, or that this project
  supports it. For authenticated requests and decoded results, use
  `praevis-thetadata-client`.

Struct literals of generated messages can break when the upstream schema adds
fields (for example, `ZonedDateTime` gained `nanos`/`precision` in 1.0.16).
Prefer `..Default::default()`.

## Supported platforms

Any platform supported by Rust 1.88+, `prost` and `tonic`. CI covers Windows and Ubuntu.

## License

Apache-2.0. The package includes `LICENSE` and `NOTICE`, plus `LICENSE.upstream`, the Apache-2.0 license of the upstream Python distribution from which the protocol definitions were recovered. Source, issues,
requirements and design records: <https://github.com/Praevis-Quant/praevis-thetadata>.
