# EOD lease release order (alpha.3 A2, deferred item 7, 2026-11-03)

An offloaded decode job holds a job permit, which reserves the per-batch
allocation ceiling, and a clone of its stream's schema lease. A worker
abandoned by cancellation or a deadline used to drop its result tuple in
field order on the blocking thread. That released the job permit **before**
the schema lease. Whoever acquired the freed permit could briefly observe the
old lease still held. It was not a leak, but it was the root cause of the
cancellation-test race fixed in the test by #34. A `JobLeases` struct now
fixes the order by field declaration on every path: schema lease first, job
permit last. The cancellation test again asserts the released lease
immediately, with no bounded wait.

## Deterministic evidence

Local runs on aarch64, 4 cores, with 12 concurrent loops × 300 runs of
`cancelled_running_work_retains_budget_and_queued_clones_cannot_oversubscribe`
and the strict assertion:

| Build | Failures |
| --- | ---: |
| `JobLeases` (schema lease, then job permit) | 0 / 3600 |
| Fields swapped (job permit, then schema lease) | 4 / 3600 |

## Benchmark protocol

- **Workflow:** before/after mode of the **Synthetic EOD performance**
  workflow ([protocol](eod-baseline.md#on-github-hosted-runners-windows-and-linux)):
  alternating AB/BA rounds, 10,000-row fixtures, on GitHub `ubuntu-latest`
  and `windows-latest`.
- **Commits:** baseline `main` at `7484236` (#38); candidate `abb725a`. The
  candidate's `crates/` and `Cargo.lock` are identical to the merged commit.
- **Engines:** numeric, Table, numeric-offload and Table-offload. The offload
  engines use the default policy (`inline_bytes` 0), which is the path changed here.
- **Run 41590775490:** 3 rounds of all four engines. Windows showed
  single-round decode excursions above 1.05, up to 1.371, on numeric,
  numeric-offload and Table-offload, while every minimum stayed within
  tolerance.
- **Run 41592117911:** under [policy rule 2](benchmark-policy.md#tolerances-d-b-approved-2026-11-02),
  3 more rounds of those three engines on both OSes. The extra rounds are
  combined with the first three, not substituted.

## Result against the D-B tolerances

These are decode p50 ratios (candidate/baseline) over the eight gating
workloads; the 16-row cases are excluded. A regression requires some
workload's **minimum** over rounds to exceed 1.05. An improvement requires a
**maximum** below 0.95.

| OS | Engine | Rounds | Largest per-workload minimum | Smallest per-workload maximum | Outcome |
| --- | --- | ---: | --- | --- | --- |
| Linux | Numeric | 6 | 1.000 (nulls-zstd) | 1.000 (prices-none) | Neutral |
| Linux | Numeric-offload | 6 | 0.998 (timestamps-zstd) | 1.000 (prices-none) | Neutral |
| Linux | Table | 3 | 1.001 (timestamps-zstd) | 0.996 (prices-none) | Neutral |
| Linux | Table-offload | 6 | 0.997 (nulls-none) | 0.996 (prices-zstd) | Neutral |
| Windows | Numeric | 6 | 0.998 (large-mixed-none) | 1.007 (timestamps-none) | Neutral |
| Windows | Numeric-offload | 6 | 0.999 (prices-none) | 1.015 (timestamps-none) | Neutral |
| Windows | Table | 3 | 0.999 (prices-zstd) | 0.998 (nulls-zstd) | Neutral |
| Windows | Table-offload | 6 | 0.995 (large-mixed-zstd) | 1.044 (nulls-zstd) | Neutral |

No delivery or first-batch minimum exceeds the 1.30 review flag. The Windows
excursions did not recur in every round, matching the single-round spikes of
the [A/A runs](benchmark-policy.md#designated-hosts-and-measured-noise).
**Conclusion:** no measurable performance effect, within tolerance on both
runners. Raw reports are retained as the run artifacts.
