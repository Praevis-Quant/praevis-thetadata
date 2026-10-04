# ADR-0001: standalone authentication library

Status: Accepted. Date: 2026-10-03. Requirements: AUTH-L1-001, AUTH-L2-001, AUTH-L2-002.

## Context

Other Praevis-Quant projects need the same authentication. The Python client constructor combines credential discovery, an HTTPS login, gRPC connection setup, and dataframe choices. The current deliverable is an auth CLI, not a complete market-data application.

## Decision

Use a Cargo workspace with `thetadata-auth` for credentials, async HTTP login, opaque sessions, and persistence. `theta` owns environment/argument handling. `thetadata-client` consumes a session and owns gRPC; `thetadata-proto` owns recovered bindings; `thetadata-core` owns transport-independent data. Authentication does not import those three crates. Use `reqwest` with reusable clients; persistence remains a synchronous native-store API, which async applications can offload if needed.

## Alternatives

A single monolithic crate would couple every consumer to market data. Separate crates per asset class would add boundaries without independent auth or transport lifecycles. Requiring Theta Terminal would defeat the direct client use case.

## Consequences

Consumers can depend on auth alone and pass the resulting session to another transport. The existing market-data crates remain explicitly provisional. Configuration/discovery duplication belongs in the CLI rather than hidden library side effects. See [artifact and documentation evidence](../research/thetadata-1.0.12.md).
