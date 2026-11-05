# Rust and Python performance comparison plan

Status: selected planning work, 2026-10-04; harness and measurements not yet
implemented. No Rust-versus-Python speedup is established. This plan supports
the [release execution chart](../releases.md#immediate-execution-order) and
PERF-L3-001/007/008; it does not grant EOD acceptance or change auth behavior.

## Comparator and current evidence

Interpret the vendor Python script as a small driver of the reviewed
`thetadata` **1.0.17** package's `stock_history_eod`, using default Polars output
and a separately reported Pandas run. A different user script needs its own
source hash and workload definition. Do not substitute the older Terminal
client, a rewritten Python decoder, or only Pandas for the vendor default.

The comparator pin moved from 1.0.12 to 1.0.17 with the compatibility baseline
([plan decision D4](../upstream-update-1.0.17.md)). The `stock_history_eod`
wrapper, its request messages, `_convert_response_stream`, the date/time
helpers and the declared dependencies are unchanged from 1.0.12 to 1.0.17
([cumulative record](../research/python-1.0.12-to-1.0.17-delta.md)). The
[artifact research](../research/thetadata-1.0.12.md#conversion-and-output)
therefore still describes the measured path: full-stream accumulation followed
by dataframe construction. Python 1.0.16+ ignores the new timestamp
`nanos`/`precision` fields, while Rust rejects non-millisecond precision
([ADR-0019](../adr/0019-timestamp-precision.md)). Comparator fixtures use
millisecond timestamps, where both produce the same instants.
Rust provides incremental numeric batches and a Table adapter. Python converts
prices to floats; Rust retains exact prices. These output and lifecycle
differences must be visible in the report. Current vendor
[usage documentation](https://thetadata.net/docs/Python-Library/Getting-Started.html)
also describes Polars as the default, Pandas as an option, and client session
reuse (checked 2026-10-04). The versioned artifact remains the comparator authority.

| Available evidence | What it establishes | What it does not establish |
| --- | --- | --- |
| [Rust numeric results](eod-numeric-results.md) | Legacy/Table/numeric implementation costs on retained fixtures | A comparison with executing vendor Python |
| [Fault and resource calibration](eod-acceptance-results.md) | Concurrent progress fix and retained null/price regressions | Full EOD/PERF acceptance or a universal speedup |
| [Live observations](../research/live-observations.md) | Narrow PROD interoperability and offline replay | Controlled local performance or broad service semantics |
| Python comparison in this plan | Work selected for implementation | Timings, wins or approved performance claims |

## Work packages and dependencies

| Step | Deliverable | Exit condition |
| --- | --- | --- |
| Pin and isolate | Recover the exact 1.0.17 wheel using the hash-verified procedure in [upstream-source.md](../research/upstream-source.md) (section "Releases 1.0.13–1.0.17 and the current 1.0.17 baseline"); lock Python and transitive dependency versions/hashes, including protobuf, grpcio, zstandard, Polars, Pandas and dotenv | Compatible environment reproduces the artifact's imports; declared-versus-generated dependency gaps recorded; artifact code unchanged |
| Establish execution scope | Add a narrowly scoped benchmark exception to the repository's static-research policy when implementing this harness | Reviewed local-only benchmark workflow with synthetic credentials, loopback endpoints and no ambient credential files/environment; ordinary builds and research checks still do not import vendor code |
| Share inputs | Extend the Rust fixture tooling to export identical serialized response sequences for both clients; expose those through the same local gRPC fixture | Identical response/schema/compression hashes, request validation, row/batch counts and terminal status |
| Verify outputs | Implement independent expected-value checks and documented normalization for both dataframe backends and both Rust representations | Every measured case passes; unsupported or semantically different cases appear as such, never as successful speedups |
| Measure baseline | Run release Rust and unchanged Python in isolated processes on each platform before optimizations | Raw samples, dependency/build/host provenance and per-case validity checks retained |
| Optimize and repeat | Profile Rust null/price regressions against the retained Rust control, then rerun the cross-language suite | Correctness/resource contracts preserved; repeat-run results and regression decisions reviewed |
| Publish evidence | Generate charts and a scoped conclusion from retained reports | Claim identifies version, platform, workload, output, metric and variability; release notes link the report |

[AGENTS.md](../../AGENTS.md) currently says never to import or execute the vendor
package during research. This documentation change does not execute it or
silently override that policy. The benchmark implementation must reconcile that
instruction explicitly for its isolated comparison runner. The wheel remains
in ignored storage; normal Rust builds and credential-free Rust checks retain
their current dependencies. Do not patch vendor conversion code to make it faster
or slower. Record any harness adapter and whether it bypasses a public API.

## Comparisons to run

| Experiment | Measured boundary and equivalent work | Interpretation |
| --- | --- | --- |
| Full-result EOD delivery | Ready authenticated clients and channels; request dispatch through complete materialized results, with all rows retained by both consumers; Python Polars/Pandas separately, Rust collected numeric/Table separately | Complete-result API costs with explicit representation differences; Rust Table is not a Polars dataframe |
| Same consumer task | Dispatch through the same specified aggregate over all rows, such as valid-close count, min/max and a defined sum, with a validated scalar result; use normal vectorized operations for the Python dataframe | End-to-end task comparison; include Python dataframe construction and Rust streaming consumption; explain the architectural difference |
| Buffered conversion | Identical in-memory encoded response sequence through complete output materialization, separately from transport | Local decompression/decoding/conversion costs; calling a private Python converter must be labelled a diagnostic, not public API latency |
| First useful output | Request through first consumable Rust batch; Python public API through dataframe return | Availability comparison only; Python offers no public incremental batch, so its first-batch metric is N/A |
| Cold invocation and auth | Fresh process through synthetic HTTP login and agreed result, measured separately from ready-session queries | Import/startup/login cost; use in-memory sessions in both. Report Rust CLI native save as a separate feature cost |
| Memory scaling and slow consumer | Increasing total batches with fixed batch size, then matched concurrency and caller retention | Separate collected-result and streaming-task results; default memory-policy failures remain failures, not throughput samples |

Do not compare a Rust first batch with a complete Python dataframe as if both
completed the same amount of work. Do not describe a numeric-batch win as a
drop-in dataframe win. PyO3/dataframe export performance is outside the current
release scope. Keep required Rust validation enabled, and disclose differences
in validation and error behavior instead of disabling checks for a timing win.

## Fixture and correctness matrix

Use the constructed sixteen-column EOD schema plus the existing small/large,
null-heavy, price-heavy, timestamp-heavy and text-heavy synthetic shapes.
Run NONE and ZSTD, single and multiple batches, and matched concurrent queries.
Separate representative EOD sizes from deliberately oversized stress cases;
100,000-row fixtures are resource tests, not a claim about a typical EOD query.
Cover empty results, partial failures and malformed boundaries in correctness
checks; report their outcomes separately from successful-query throughput.

Check row count, column order, null masks, booleans, strings, integers, prices,
timestamp instants/zones and completion independently of timing. Define the
normalization before running: a missing price may be Python NaN and Rust Null;
Python floats need a stated rounding/error rule against exact source values.
Use exactly representable fixtures for the first common aggregate and add
explicit rounding/large-integer/timestamp boundary cases separately. Never
round both outputs indiscriminately or use a checksum alone as proof of equality.
Differences in empty schemas or rejected domains remain explicit exclusions.

Preflight and post-run correctness checks use the same fixtures as timed runs.
Keep output inspection outside API-latency timing; include the aggregate itself
in the consumer-task timing. Retain outputs consistently until the timing stop,
then drop them outside that interval in both implementations. Record these
boundaries in the report so collection, destruction or validation cannot drift.

## Measurement and claim rules

1. Use one host at a time, release Rust, and the pinned Python environment.
   Measure Windows and Linux separately; place WSL binaries on native Linux
   storage. Record OS/CPU/RAM, toolchains, dependency lock, source/wheel/fixture/
   executable hashes, power settings, worker/thread counts, concurrency and
   resource policies. Default backend threading is the primary consumer case;
   matched thread budgets are a separately labelled diagnostic.
2. Separate setup, cold invocation, warm delivery and offline conversion.
   Use a common server with identical behavior and fresh independent client
   processes. Avoid live network latency in local-performance claims. Keep
   fixture-server resource usage separate from client measurements.
3. Warm up each case, alternate Rust/Python order, retain raw samples and repeat
   complete runs. A starting protocol is five warmups and thirty samples per
   case in each of three independent runs; calibrate from observed noise before
   selecting tolerances. Record any protocol adjustment before the comparison
   used for a claim. Do not selectively discard slow samples or pick the best run.
4. Report latency p50/p95, rows/cells/bytes per second, total task duration and
   peak process resident memory/Windows working set using the same OS metric
   for both languages. Show idle process memory and absolute peak, sampling
   method/interval and limitations. Use separate memory/profile runs where
   instrumentation affects timing. Python tracemalloc and the existing Rust
   allocation probe omit native costs and are not comparable total-memory gauges.
5. Define latency speedup as Python time / Rust time for the same experiment;
   identify the statistic and report repeat-run variability or an uncertainty
   interval with its method. Values below one are regressions. A timeout, OOM,
   resource rejection or incorrect output is a failed case, never a speedup ratio.
6. Calibrate per-workload acceptance tolerances under PERF-L3-008 before accepting
   optimizations. Claim improvement only where the effect exceeds measured
   uncertainty and correctness passes. Include no-change and losing cases.
   Rust regression acceptance and beating Python are independent decisions.

The report should contain a per-workload latency/speedup chart with a 1x
reference, a peak-process-memory versus total-rows chart, and a first-output
versus full-completion chart. Facet by platform, compression and output/task;
include sample counts and variability. Commit generated SVG/PNG charts and a
compact evidence table with raw-report locations/hashes and reproduction commands.
Retain full reports under ignored benchmark artifacts following the existing
retention policy. Do not draw result bars until measurements exist.

A defensible conclusion is limited to the tested version and task: which Rust
interface completed which workload faster or with less memory, on which host,
by how much, and with which tradeoffs. The outcome may be mixed. Broad claims
about every vendor endpoint, live-service speed, or all Python programs would
require evidence outside this plan.
