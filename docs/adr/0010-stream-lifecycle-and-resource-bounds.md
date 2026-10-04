# ADR-0010: explicit stream completion, failure, and resource limits

Status: Proposed. Date: 2026-10-04. Owner: thetadata-client maintainers.
Requirements: [EOD-L2-005 through EOD-L2-009](../requirements/L2-stock-eod.md),
[EOD-L3-012 through EOD-L3-023](../requirements/L3-stock-eod.md);
preserves AUTH-L1-004 and the explicit session lifecycle.

## Context

The wire API is a finite server-streaming query service. Python fully
accumulates results and maps gRPC NOT_FOUND to NoDataFoundError, but exposes
no high-level deadlines or cancellation controls. Rust's foundation yields
batches and limits compressed/decompressed bytes, but its byte limit is not
a total heap bound, decoding runs on the async executor, and complete
cross-batch failure behavior is not yet baselined.

## Proposed decision

Use a pull-based async stream: the caller requests the next batch, with no
implicit full-result collection or background prefetch queue. Reuse a supplied
session and channel. Require an explicit caller action to authenticate again;
do not silently log in, refresh, replay a failed query, or revoke credentials.
Dropping the stream stops local consumption and cancels the transport as far
as its API allows; it does not prove that the vendor stopped work immediately.

Define a terminal state: after successful completion, cancellation, or a
terminal error, the stream cannot resume yielding new batches. Distinguish
invalid request/configuration, connection/TLS failure, deadline/idle timeout,
NOT_FOUND, other gRPC statuses, decode/shape failure, unsupported response,
and resource-limit failure. Preserve safe status codes and diagnostic context
without echoing session tokens, server payloads, or credential-bearing URLs.
Do not translate permission failures, corrupt data, or successful zero rows
into no-data. Previously delivered batches are partial if termination fails.

Use separate connect, whole-query, and between-batch idle budgets, with local
enforcement rather than relying only on a server timeout header. Cancellation
and failure cleanup must be testable without real credentials. Do not add an
automatic retry policy until its duplicate/partial-result and account-policy
consequences have a separate decision.

Retain encoded/decompressed byte checks and require row/cell/allocation
accounting before calling the output memory-bounded. Specify enforcement
points before large allocations, checked arithmetic, a ZSTD window ceiling,
and concurrency accounting across cloned clients that share a budget. A
row count checked only after protobuf allocation is insufficient. Exact
default ceilings and the decoding strategy must be selected from fixtures
and measurements at the [EOD-L3-023 acceptance gate](../requirements/stock-eod-evidence.md)
before accepting the implementation; the existing 64 MiB setting
is an encoded/decompressed ceiling only.

Keep at most the next requested batch in the library's delivery path. CPU
offload, if selected by measurements, must have bounded jobs and shared
admission control; already-running blocking work may outlive cancellation.
Do not introduce unlimited spawn_blocking calls or claim cancellation frees
all CPU/memory immediately. The current synchronous decoder remains an
explicit implementation gap, not a validated responsiveness guarantee.

## Alternatives

- Accumulating all results matches Python's dataframe interface but removes
  caller backpressure and scales memory with query size.
- Unbounded prefetch/offload increases parallelism while allowing unlimited
  queued work and complicating cancellation.
- Retrying any error or silently reauthenticating can duplicate delivered
  rows and apply undocumented session/account assumptions.

## Consequences and requirements to derive

Fixtures need successful multi-batch completion, empty completion, NOT_FOUND,
permission/other statuses, a failure after the first batch, malformed input,
each resource limit, local timeout, cancellation, and repeated next calls
after termination. Measure allocations/peak memory and executor responsiveness
before accepting stronger bounds or an offload design. No latency SLA or
vendor throughput guarantee follows from this proposal.

## Supporting evidence

- [Transport/error analysis](../research/thetadata-1.0.12.md#transport-and-errors)
  and [conversion analysis](../research/thetadata-1.0.12.md#conversion-and-output).
- [Current stream implementation](../../crates/thetadata-client/src/lib.rs).
- [Explicit auth lifecycle](0004-explicit-session-lifecycle.md).
- [Preserved performance recommendations](../ROADMAP.md#original-review-and-recommendations-verbatim).
