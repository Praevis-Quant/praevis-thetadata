# Stock EOD acceptance matrix

2026-10-04; alpha.2 disposition 2026-11-02. This is an evidence matrix, not a
replacement for the canonical [L3 requirements](L3-stock-eod.md). Every ID is
retained below. Synthetic tests establish the listed cases; a passing row does
not imply undocumented vendor behavior. Full slice acceptance and ADR acceptance
remain separate decisions.

## Alpha.2 disposition (2026-11-02)

Every one of the 30 L3 clauses has an alpha.2 disposition. **Closed** means the
alpha.2 evidence in the table below is complete for the library contract.
**Closed, with deferral** means the alpha.2 part is complete and a named
hardening item remains in the [deferred block](../releases.md#deferred-from-alpha2),
which is a `0.1.0` exit gate. **Not applicable to alpha.2** was a maintainer
decision. Service behavior is never closed by synthetic evidence; it is
recorded as observed or unresolved under EOD-L3-029.

| Disposition | Clauses |
| --- | --- |
| Closed | 001, 002, 003, 004, 006, 007, 008, 009, 010, 011, 012, 013, 014, 015, 016, 017, 018, 020, 022, 024, 025, 026, 028, 030 |
| Closed, with deferral | 005 (trusted-TLS fixture, item 3); 021 (exhaustive corpus, item 2); 023 (C-allocator profiling, item 1; the lease release order, item 7, closed 2026-11-03); 027 (desktop A/A, item 6); 029 (unresolved service semantics listed in the [semantics table](stock-eod-evidence.md#observed-service-semantics-eod-l3-029)) |
| Not applicable to alpha.2 (D-E) | 019: exhaustive controlled-clock interleavings and the transport seam (item 4); its existing synthetic idle/deadline/cancellation tests pass |
| Open | None |

Since alpha.2, all closed on 2026-11-03:
- 019 (deferred item 4, alpha.3 A4);
- item 7 under 023 (alpha.3 A2);
- item 6 under 027 (alpha.3 A3: the desktop tolerances are final).

Closed clauses still carry the scope limits in their third column. For example,
EOD-L3-030 rejects non-millisecond timestamps while the vendor meaning of
`precision` remains unknown.

Test locations used below:

- **contracts**: [public numeric/Table acceptance tests](../../crates/praevis-thetadata-client/tests/eod_contracts.rs).
- **typed**: [typed wire/lifecycle fixtures](../../crates/praevis-thetadata-client/tests/eod_typed.rs).
- **lifecycle**: [stream implementation and deterministic worker/admission tests](../../crates/praevis-thetadata-client/src/eod.rs).
- **framing**: [local gRPC frame guard and fragmentation tests](../../crates/praevis-thetadata-client/src/framing.rs).
- **bounded**: [preflight/decompression tests](../../crates/praevis-thetadata-client/src/bounded.rs).
- **envelope**: [outer protobuf parser tests](../../crates/praevis-thetadata-client/src/envelope.rs).
- **core**: [exact prices/Table](../../crates/praevis-thetadata-core/src/lib.rs) and [numeric timestamps/batches](../../crates/praevis-thetadata-core/src/batch.rs).
- **harness**: [Rust release measurements and allocator tests](../../crates/praevis-thetadata-client/examples/eod_benchmark.rs).

## Requirement-by-requirement evidence

| Requirement | Implemented synthetic evidence | Scope or remaining gate |
| --- | --- | --- |
| [EOD-L3-001](L3-stock-eod.md) | Typed API doctest; typed and contracts exercise both public streams; raw fixture suite retained | Callers own runtime; no market-data CLI added |
| [EOD-L3-002](L3-stock-eod.md) | typed `validates_symbols_and_calendar_ranges`; contracts exact Unicode/case query round trip | Rejection occurs in constructor before an RPC can be constructed |
| [EOD-L3-003](L3-stock-eod.md) | Same tests cover leap day, reversed/equal dates and 0001/9999 serialization | No date inclusivity or market calendar promise |
| [EOD-L3-004](L3-stock-eod.md) | typed compares complete request params/query_info against supplied session and expected defaults | Fixture binds the exact generated RPC; descriptor integrity audited |
| [EOD-L3-005](L3-stock-eod.md) | contracts rejects HTTP before connection; stalled TLS fixture; session/channel reuse inspected; contracts `typed_queries_never_authenticate_again`: injected auth-connection counter stays at 1 across successful, failing (UNAUTHENTICATED) and cancelled typed queries on both APIs; the client never enables `native-store` (CI dependency-tree check) | Injected counters **closed 2026-11-02**. TLS success for alpha.2 rests on live PROD captures (`prod-aapl-alpha1-002`, `prod-*-a2-*`) plus the plain-HTTP rejection and stalled-handshake tests (D-D(b)); the synthetic trusted-TLS fixture is [deferred item 3](../releases.md#deferred-from-alpha2). Earlier note: Auth/storage calls are absent by inspection; dedicated injected counters and a trusted local TLS success fixture remain open |
| [EOD-L3-006](L3-stock-eod.md) | Both APIs: NONE/ZSTD equivalence, corrupt data, manifests and unknown compression; bounded concatenation/skippable/truncation tests | Synthetic wire compatibility; no manifest fetch |
| [EOD-L3-007](L3-stock-eod.md) | contracts tests Unicode/NUL/empty text, i64 extremes, booleans, null/unset; bounded tests unknown-only oneof | Unknown-only values intentionally collapse to null |
| [EOD-L3-008](L3-stock-eod.md) | contracts tests every scale 0-19 with i32 minimum/maximum, negative and zero mantissas; invalid scale fault; core rendering tests | Integer mantissa/exponent preserved; no floating point |
| [EOD-L3-009](L3-stock-eod.md) | core tests spring/fall DST boundaries and overflow/unknown zones; typed Table/numeric equivalence; core `timestamp_boundary_catalogue_matches_table_and_numeric`: epoch, 1 ms, New York spring-forward/fall-back edges, the 9999-12-31T23:59:59.999Z maximum, rejection of year 10000+ (previously formatted as non-RFC-3339 `+10000-...`, fixed 2026-11-02), chrono/i64/u64 limits and unknown zones, identical for Table and numeric | **Closed 2026-11-02** (platform-independent: chrono and chrono-tz are pure Rust with an embedded tz database). Earlier note: Complete platform-independent representable-domain boundary catalogue remains open |
| [EOD-L3-010](L3-stock-eod.md) | contracts preserves unusual headers, repeated/out-of-order rows; duplicate/ragged faults through both APIs | No sorting, deduplication or fixed OHLC schema |
| [EOD-L3-011](L3-stock-eod.md) | typed empty-before/between-schema and reordered schema; contracts schema-empty and zero-width faults | Existing schema identity is shared by numeric batches |
| [EOD-L3-012](L3-stock-eod.md) | contracts zero-message, schemaless/schema-empty completion and all remote codes before/after a batch | NOT_FOUND stays a distinct error; partial batches never imply complete history |
| [EOD-L3-013](L3-stock-eod.md) | contracts repeatedly polls each terminal fault/completion; one RPC per stream; lifecycle worker-entry counters | Both APIs fuse; no automatic replay |
| [EOD-L3-014](L3-stock-eod.md) | contracts explicit cancellation and dropped pending receive before/after delivery; lifecycle latched worker/queued-clone cancellation | Running work retains reservations; server-side cessation timing is not guaranteed |
| [EOD-L3-015](L3-stock-eod.md) | lifecycle actual-thread/no-prefetch counter and caller pause beyond idle budget; slow-consumer harness | Transport buffers and caller-owned retained batches remain outside the delivery slot |
| [EOD-L3-016](L3-stock-eod.md) | Full category/phase matrix below; safe status codes; local frame and ZSTD window provenance fixed | A transport status without locally established provenance remains a safe remote/status category |
| [EOD-L3-017](L3-stock-eod.md) | contracts sentinel payload/status/metadata, Display/Debug and absent source chains; contracts `credential_bearing_endpoint_urls_are_rejected_without_echo`: endpoint URLs with credentials, query or fragment are rejected before connection (typed `Configuration`, raw `Config`) with no echo; the shared `safe` check also asserts the absence of the session token and account email; `tests/eod_logs.rs` installs process-global TRACE `tracing` and `log` collectors and captures about 670 dependency lines (hyper connection events, h2 frames and HPACK decoding of a trailer carrying `secret: SENTINEL`) across authentication, a typed success and a remote failure; none contains the API key, session token, account email or SENTINEL text | Session/URL sentinel matrix and dependency log capture **closed 2026-11-02**. Earlier note: Public errors retain no raw status; dependency tracing capture and expanded session/URL sentinel matrix remain open |
| [EOD-L3-018](L3-stock-eod.md) | contracts all zero/MAX durations reject before TCP accept; stalled TLS; typed stalled initial headers; contracts `whole_query_deadline_is_sent_as_grpc_timeout_metadata` captures server-side `grpc-timeout` on both APIs (present, at most the configured 10 s and above 9 s at dispatch) | Timeout metadata capture **closed 2026-11-02**. Earlier note: Explicit timeout metadata capture remains open; dispatch sets remaining deadline by inspection |
| [EOD-L3-019](L3-stock-eod.md) | contracts idle/deadline/pending cancellation before/after delivery; typed empty trickle/caller pause; lifecycle schema/job admission expiry and expired-stream deadline priority; **paused-clock interleavings through the internal `Source` seam** (lifecycle, 7 tests): exact idle/deadline boundaries, simultaneous expiry, per-message idle reset including skipped empties, empty trickle, consumer pause, admission wait, offloaded decode and cancellation at each wait ([results](../performance/eod-clock-seam-results.md)) | **Closed 2026-11-03** (deferred item 4). The initial-header wait precedes the seam and stays covered by real-transport stalls (EOD-L3-018). Earlier note: not applicable to alpha.2 (D-E) |
| [EOD-L3-020](L3-stock-eod.md) | Both APIs: declared/encoded/frame/actual expansion errors; bounded exact/over byte tests; framing u32::MAX header with no payload | 1 KiB envelope allowance; no payload allocation is needed to reject an enormous frame announcement |
| [EOD-L3-021](L3-stock-eod.md) | bounded header/row/cell/text boundaries and checked arithmetic; harness manifest/tiny-string allocation probes; envelope copy limits; bounded `bounded_mutation_corpus_is_classified_and_mode_independent`: 20,000 deterministic mutations of three seed tables × NONE/ZSTD (40,000 envelopes) through the envelope parser and bounded decoder; every case accepted within limits or classified (Decode/Schema/Resource: header bytes, headers, rows, text bytes), and numeric and Table agree | **Bounded corpus closed for alpha.2** (D-C, 2026-11-02); the exhaustive adversarial corpus is a 0.1.0 exit gate. Earlier note: Requested Rust storage estimate, not RSS; exhaustive adversarial corpus remains open |
| [EOD-L3-022](L3-stock-eod.md) | Typed ZSTD window resource error; lifecycle schema partition, queued/running cancellation, retained leases, admission deadline and recovery | Existing finite reservation partition unchanged |
| [EOD-L3-023](L3-stock-eod.md) | All policy zeros/overflow/window range and impossible budgets rejected; measured offload default and bounded opt-in inline; larger slow-consumer runs; **calibrated 2026-11-02**: defaults kept by maintainer decision, effective ceilings measured (the 128 MiB accounted budget binds first), and limit-sized batches complete at the 16-stream cap on Windows/Linux runners ([calibration](../performance/eod-calibration.md)) | Transport/HTTP/2 and ZSTD C-allocator memory profiling is a **0.1.0 exit gate** (D-C, 2026-11-02); only the client's own accounted allocations are claimed bounded. Earlier note: Calibration is workload-specific; independent limit maxima need not all fit simultaneously; transport/C memory profiling remains open |
| [EOD-L3-024](L3-stock-eod.md) | Cargo-only mock, framing and decoder fixtures; Windows/Ubuntu CI runs contracts via workspace tests | No real credentials or vendor traffic |
| [EOD-L3-025](L3-stock-eod.md) | Compiled incremental consumer doctest; contracts exercises a failure after one batch and fusion; runnable `examples/eod_partial.rs` (`cargo run --example eod_partial`): explicit session and typed request, incremental consumption, rows labelled complete only after clean EOF and partial after a mid-stream failure; its tests run in `cargo test --workspace` on Windows/Linux | **Closed 2026-11-02**. Earlier note: Standalone example run against synthetic partial failure remains open |
| [EOD-L3-026](L3-stock-eod.md) | Rust harness NONE/ZSTD small/mixed/null/price/timestamp workloads; allocation probes and executor timer; `--extremes` limit-sized text/header workloads added (2026-11-02) | Slow consumer delay is explicit and retains one caller-owned batch per stream |
| [EOD-L3-027](L3-stock-eod.md) | Common-harness control/candidate builds; provenance, repeated alternating runs, raw samples and hashes; runner before/after workflow with A/A noise and approved tolerances (D-B, [policy](../performance/benchmark-policy.md)) | Informational comparisons; old v2 reports remain historical, incompatible with v3 |
| [EOD-L3-028](L3-stock-eod.md) | Repository and research audits retain the canonical requirements (151 at 2026-11-02), ADR origins (20), the 531 frozen 1.0.12 and 623 current 1.0.17 inventory items | Mechanical checks do not prove semantic completeness |
| [EOD-L3-029](L3-stock-eod.md) | **Closed 2026-11-02.** Seven authorized PROD/FREE captures, each recording version, environment, query, entitlement context, date and sanitized results, are kept separate from synthetic tests ([observations](../research/live-observations.md#alpha2-prod-service-semantics-runs-2026-11-02)). Numeric, Table and raw replay match value-for-value on all four data runs. Each open contract is classified as observed or unresolved in the [semantics table](stock-eod-evidence.md#observed-service-semantics-eod-l3-029) | Observations cover one FREE account on PROD only; they are not vendor guarantees. Unresolved: unknown symbol versus no-data, dividend adjustment, corrections, column meanings, calendar edge cases, exact FREE depth, paid tiers, STAGE, token lifetime |
| [EOD-L3-030](L3-stock-eod.md) | core precision unit tests; raw and bounded decoder tests over NONE/ZSTD and numeric/Table; contracts precision faults through the local gRPC service on both APIs, with and without delivered rows (passing) | Vendor meaning of `nanos`/`precision` unknown; no live response with non-millisecond precision observed. The possible ~1–2% Table-adapter decode shift on `*-mixed-none` (#18) is superseded by the step-2 decoder: Table `large-mixed-none` decode is 0.89–0.92× on the Windows and Linux runners ([results](../performance/eod-decoder-results.md)) |

## Fault category and phase matrix

Every applicable local data fault is injected through **both** numeric and
Table APIs, before delivery and after a valid batch. A later valid message is
also queued: it must never be delivered after the fault. Each error is followed
by three fused polls, then the same single-job client is reused for another
query to detect leaked admission. Initial remote failures and post-batch remote
failures cover every non-OK gRPC code. Constructors/connection establishment
precede stream creation, so an after-delivery injection is not meaningful for
those phases.

| Category | Injected cases | Phase / expected result |
| --- | --- | --- |
| Input | Symbol/date rejection | Constructor; Input; no request exists |
| Configuration | All positive limits set to zero, byte/schema arithmetic overflow, window bounds, job bounds, impossible pool, insecure transport, zero/MAX durations | Before connect; Configuration; listener observes no connection |
| Connection/TLS | TCP peer accepts but never completes TLS | Before query; Connection within local connect budget |
| Whole query | Header stall, empty trickle, admission stall, latched worker, caller pause, expired stream | Deadline, terminal; no expired batch delivery |
| Idle | Stalled next message before/after a batch | Idle, terminal; no-data is not substituted |
| Cancellation | Explicit cancel, dropped pending receive, queued/running work | Cancelled once then fused; worker retains reservations until exit |
| No data | NOT_FOUND initial/post-batch | NoData, distinct from successful empty EOF |
| Remote status | All other non-OK codes, including PermissionDenied, Unauthenticated, DeadlineExceeded, OutOfRange and ResourceExhausted | Remote(code); server-supplied text imitating local errors cannot change category |
| Decode | Malformed protobuf/ZSTD, timestamp overflow/zone, invalid price scale; truncated/invalid framing at guard unit boundary | Decode, terminal; payload/metadata omitted from public error |
| Schema | Duplicate/ragged/no-header row, changing/reordered schema | Schema, terminal |
| Unsupported | Unknown envelope compression, manifest; gRPC compression flag at guard boundary | Unsupported; no downloader or implicit compression negotiation |
| Resource | Declared/encoded/decompressed/frame bytes, header/row/cell/text limits, ZSTD window | Resource with static limit name; no status-text guessing |

Allocation ceilings and arithmetic overflow additionally have direct preflight
and allocation-instrumented tests. Fragmentation tests cover every split in a
coalesced valid-message/oversized-header sequence: the first message remains
deliverable, and the guard stops before tonic can reserve the oversized body.

## Error provenance design

The typed service wraps the response body with a five-byte gRPC header scanner.
It forwards borrowed Bytes slices at message boundaries, preserving backpressure
and avoiding payload copies. A per-query atomic marker records only locally
detected framing failures. Tonic keeps its own ceiling as a second check. The
marker is checked only when a status is returned; remote messages/metadata never
set it. Invalid flags/truncation are decode errors; gRPC compression is unsupported
(envelope ZSTD is supported). Pending slices retain transport-owned storage,
which is documented separately from the application allocation reservation.

ZSTD streaming now retains the numeric error result from `zstd_safe::DCtx`.
A narrow, documented scalar-only call to `ZSTD_getErrorCode` distinguishes
window-too-large and allocation failures from corrupt data. The same 64 KiB
output chunks, growth checks, window parameter and input/decoder drop boundary
remain in place. Concatenated and skippable frames remain supported; incomplete
or no-progress decoding fails rather than looping.

The inspected locked tonic 0.14.6 `src/codec/decode.rs` has SHA-256
`433ce180e5ee34a271f352ecad70aec31f87b5827ec31292d6a727004e6fe518`.
Its registry source records Git commit `6cb6056b5a748bc5a29bd48f4602dbc4e552bb7d`;
the guessed public version-tag URL returned 404, so it is not used as evidence.
References: [tonic source repository](https://github.com/hyperium/tonic),
[ZSTD streaming/error API](https://github.com/facebook/zstd/blob/v1.5.7/lib/zstd.h),
[ZSTD error codes](https://github.com/facebook/zstd/blob/v1.5.7/lib/zstd_errors.h).

## Concurrent transport correction

The first eight-stream, 100,000-row slow-consumer run exposed a pre-existing
stall in both the unchanged control and the initial candidate. This is retained
as a failed calibration, not omitted or converted into a latency sample. Unpolled
streams could consume the shared connection's receive credit while the two
admitted jobs waited for complete messages larger than a stream window.

`EodPolicy::concurrent_streams` now defaults to 16 (validated range 1-64).
Admission occurs before RPC dispatch and uses the whole-query deadline. A
dedicated typed channel advertises a fixed 2 MiB stream window and
`(concurrent_streams + 1) * 2 MiB` connection credit: 34 MiB by default.
Adaptive receive-window growth is disabled. Raw helpers have an independent
channel and cannot consume typed connection credit. The primary constructor API
connects eagerly; the other channel connects lazily. Session/auth behavior is
unchanged.

These windows are **flow-control credit**, not preallocated memory or a total
transport heap bound. HTTP/2/TLS metadata, socket buffers and already copied
tonic message storage are additional. Per-message copies remain in the existing
application reservation; caller-retained batches remain caller-owned. Paused
queries retain stream slots; cancellation/EOF/error releases transport before
the slot. An unpolled stream still requires drop/cancel/resumption for cleanup.

Lifecycle tests `paused_streams_cannot_exhaust_connection_credit_for_active_decode`
and `stream_admission_bounds_dispatch_and_releases_on_pending_drop` verify one
active 100,000-row decode beside seven unpolled streams, slot saturation before
dispatch, whole-query expiry, pending-drop release and replacement queries.
This supplements EOD-L3-015/019/022/023 and PERF-L3-005 evidence above. The raw
helper API retains its existing guarantees; simultaneous use may establish two
connections, with independent transport overhead.
