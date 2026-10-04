# ADR-0005: synthetic verification and source provenance

Status: Accepted. Date: 2026-10-03. Requirements: AUTH-L2-008, AUTH-L2-009.

## Context

The upstream wheel is the compatibility input but the original archive and extraction must not enter Git. CI needs reproducible builds, native persistence coverage, and a performance starting point without live credentials or undocumented account limits.

## Decision

Commit Rust code, recovered descriptors, hashes/inventory, upstream license, requirements, and ADRs. Ignore the original ZIP/directory, wheel files, credentials, and generated artifacts; enforce exclusions in CI. A regular build consumes checked-in descriptors and never downloads or imports the Python package.

Use pinned GitHub Action revisions and read-only workflow permissions. Run locked builds, tests, formatting, Clippy, and native-store process tests on Windows and Ubuntu. Performance runs use a localhost identity fixture and disposable native-store profile; archive latency summaries with revision/platform/sample context. Initial measurements are informational and do not gate merges on a guessed SLA.

## Alternatives

Live login in every PR would require secrets and mix service latency/account policy into correctness checks. Timing only credential parsing would miss process startup and actual native-store overhead. Committing the entire vendor extraction violates the explicit repository boundary.

## Consequences

Synthetic tests prove client contract handling, not live compatibility. Performance results cannot be advertised as vendor throughput. Source regeneration is an explicit maintenance task requiring a separately obtained artifact. Upstream licensing and the private repo's publication policy remain separate from correctness; no license for newly authored Rust is chosen here.
