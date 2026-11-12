# ADR-0021: a per-call library: no retry, scheduler, rate limit or default deadline

Status: Accepted, 2026-11-12 (user decision), for `0.1.0-beta.2`.
Supersedes the scheduler, per-minute limiter and retry of the
[P1 design](../architecture/p1-design.md#5-query-engine-and-scheduler)
(decision D5) and of ADR-0010's beta.1 review; revises the client defaults.
Origin: **mixed**: the absence of retry, scheduling, rate limiting and default
timeouts follows the vendor package; the per-job concurrency default and the
distinct busy error are Rust enhancements.
Owner: praevis-thetadata-client and praevis-thetadata-py maintainers.

## Context

`0.1.0-beta.1` froze a client with an account-wide scheduler (the tier's
concurrency and a per-minute limiter), retry with backoff at open on
`RESOURCE_EXHAUSTED` and `UNAVAILABLE`, and default timeouts of 300 s overall
and 60 s idle, which the Python vendor-named methods inherited.

The vendor's Python package has none of these: its high-level methods take no
timeout, and it creates no retry/backoff, request scheduler or rate limiter
([research](../research/thetadata-1.0.12.md#transport-and-errors); the 1.0.13
to 1.0.17 changes add none). So a long call that completes in the vendor
package failed here after 300 s, and requests could wait on policy the caller
never asked for.

Coordinating many callers (fair scheduling, a shared read-ahead budget,
deadlines while queued, retry policy) is the job of the future shared data
service ([ROADMAP](../ROADMAP.md#out-of-scope-a-shared-data-service-for-many-consumers)),
not of a client used by one program.

## Decision

**Principle:** the library does what one caller asks, as fast as possible.
Anything that coordinates many callers, or applies policy over time, belongs
in the shared data service.

1. **No retry.** A request is sent once. The service's `RESOURCE_EXHAUSTED`
   (the account is busy) becomes a distinct error, `QueryError::Busy` (Python
   kind `"busy"`), so a caller or a service can decide to retry.
   `UNAVAILABLE` stays `Remote(Unavailable)`.
2. **No account-wide scheduler or rate limit.** `SchedulerPolicy`,
   `with_scheduler`, the admission slots and the per-minute limiter are
   removed. The account's tier stays available as information
   (`ThetaClient::tier()`).
3. **No default deadline.** The overall and idle timeouts default to none, as
   in the vendor package, and can be set per client (`ClientConfig`, Python
   `Client(request_timeout=…, idle_timeout=…)`) or per call
   (`RequestOptions`, Python `timeout=`/`idle_timeout=`). Without one, no
   `grpc-timeout` is sent. Login and HTTP auth timeouts are unchanged.
4. **A job's concurrency is its own.** `job()` keeps planning, splits and
   parallel parts with read-ahead. Its concurrency (1 to 64) defaults to the
   tier's documented limit (FREE 1, VALUE 2, STANDARD 4, PRO 8), so one job
   alone stays within the plan. Nothing coordinates jobs and queries running
   at the same time: together they can exceed the plan, and that is the
   caller's (or the service's) responsibility.
5. **In-process safety stays:** `EodPolicy::concurrent_streams` admission, the
   shared byte budget, decode limits and receive windows. They bound memory
   and transport, not the vendor's rate.

## Alternatives

- **Keep the scheduler, off by default.** Rejected: it keeps a second policy
  layer in the library that the service would duplicate, and an opt-in
  scheduler still cannot coordinate several processes.
- **Keep retry only for `RESOURCE_EXHAUSTED`.** Rejected: whether and when to
  retry depends on what else the caller is doing. A distinct `Busy` error
  gives the caller that choice without guessing.
- **Keep finite default timeouts.** Rejected: they turn calls that succeed in
  the vendor package into failures. Callers who want a bound set one.

## Consequences

- **A deliberate breaking change after the beta.1 freeze,** released as
  `0.1.0-beta.2` with release notes and a migration section.
- **Callers that relied on the scheduler** to stay within their plan across
  calls must now coordinate themselves (for example one job at a time, or a
  semaphore), or wait for the shared data service. A single job stays within
  the plan by default.
- **Callers that relied on retry** handle `Busy` themselves.
- **Calls can now run indefinitely** unless a timeout is set; this matches
  the vendor package.
- **The beta.1 evidence gaps "retry live" and "per-call timeout live"** become
  "removed" and "optional, caller-set".

## Supporting evidence

- [Vendor transport and errors](../research/thetadata-1.0.12.md#transport-and-errors).
- [Performance plan: Python library parallelism](../architecture/performance-plan.md)
  (the vendor has no scheduler, retry, rate limiter or concurrency).
- [ROADMAP: the shared data service](../ROADMAP.md#out-of-scope-a-shared-data-service-for-many-consumers).
