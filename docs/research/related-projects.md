# Related project watchlist

Reviewed 2026-10-04. These projects inform comparison and research; they do not
replace the pinned Python 1.0.12 compatibility baseline or prove vendor behavior.

## ThetaDataDx

- Repository: [userFRM/ThetaDataDx](https://github.com/userFRM/ThetaDataDx).
- Reviewed revision: [`5750dbda087abc717359d19607bed06c516ee487`](https://github.com/userFRM/ThetaDataDx/tree/5750dbda087abc717359d19607bed06c516ee487).
- Sources: pinned [README](https://github.com/userFRM/ThetaDataDx/blob/5750dbda087abc717359d19607bed06c516ee487/README.md),
  [Rust manifest](https://github.com/userFRM/ThetaDataDx/blob/5750dbda087abc717359d19607bed06c516ee487/thetadatadx-rs/Cargo.toml)
  and [Apache-2.0 license](https://github.com/userFRM/ThetaDataDx/blob/5750dbda087abc717359d19607bed06c516ee487/LICENSE).
- README advertises Rust/Python/TypeScript/C++, 64 typed endpoints, direct access,
  flat files, events, separate market-data/streaming clients and dataframe adapters.
  These are project claims, not functionality or performance independently tested here.
- Rust package `thetadatadx-rs` exports library name `thetadatadx`. The manifest
  declares 0.5.1 while README installation prose also mentions a 0.1.0 reset;
  verify registry/release evidence before selecting a comparison version.

| Watch area | Our follow-up |
| --- | --- |
| Transport and session separation | Compare evidence for direct event access with ADR-0018's Terminal track before selecting an event implementation. Keep direct gRPC, HTTP and FPSS contracts distinct. |
| Dataframe/binding ergonomics | Review ownership, copies and cancellation when selecting Arrow/PyO3 work; benchmark claimed zero-copy behavior independently. |
| Endpoint coverage | Map exact methods/parameters against our 531-item inventory; counts alone cannot establish parity or omissions. |
| Reliability | Investigate reconnect/gap and account-concurrency claims with vendor evidence and mocks before adopting policies. |
| Packaging | Review per-language package naming and archive-target boundaries while preparing our own packages. |

No code was copied, dependency added, remote project executed or performance
comparison run. Any future reuse needs file-level license/NOTICE review and
attribution. Keep this watchlist separate from the scheduled PyPI version gate.

At each release-planning review, check repository activity/releases and relevant
changes since the recorded revision; append a dated revision and material findings.
Before selecting event, binding or dataframe work, review the corresponding area
again. Treat resulting ideas as candidate enhancements with our own ADRs,
requirements and measurements. No automatic GitHub notification subscription or
new CI workflow was created.
