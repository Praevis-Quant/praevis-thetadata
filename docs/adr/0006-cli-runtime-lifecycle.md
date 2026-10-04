# ADR-0006: initialize the CLI runtime only for HTTP authentication

Status: Accepted. Date: 2026-10-04. Requirements: AUTH-L2-010, AUTH-L2-011, AUTH-L3-019, AUTH-L3-020, AUTH-L3-021, AUTH-L3-022, AUTH-L3-023, AUTH-L3-024.

## Context

The default Tokio main macro creates a multithread runtime before argument parsing, including for help, status, and logout. The CLI performs one HTTP authentication request at a time. Native persistence and command output are synchronous. Review measurements support investigating startup overhead, but a worker-count experiment is not evidence for the final implementation.

## Decision

Use synchronous `main` and command dispatch. Create a current-thread runtime with I/O/time drivers only around HTTP client construction and authentication. Drop it before saving the session or writing output. Keep the auth library async and independent of any runtime ownership policy; applications continue to choose their own runtime. Do not change workspace-wide Tokio features merely to optimize this one application.

Preserve the original release binary and build provenance before changing source. Compare release binaries with alternating AB/BA order on the same machine, synthetic local HTTP, and unique native-store profiles. Keep raw samples and hashes, and report median/p95 including regressions. Run separate repetitions; no guessed percentage threshold becomes a merge gate.

## Alternatives

Setting one worker still starts a background scheduler for synchronous commands. Using a current-thread main macro still initializes a runtime before parsing/local operations. A blocking HTTP client would change the dependency/HTTP execution model and is unnecessary for this narrow improvement. Runtime ownership remains in the application rather than the reusable auth library.

## Consequences

Status/logout/help avoid runtime initialization. Authentication retains asynchronous HTTP and timeouts without a worker pool. This does not guarantee zero operating-system/helper threads: dependencies may use blocking helpers such as DNS resolution. If future commands perform concurrent or CPU-heavy work, evaluate their runtime needs separately. No change to persistence semantics, wire protocol, or output is intended. See [measurements](../performance/cli-startup.md).
