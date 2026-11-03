# EOD benchmark policy and regression tolerances (PERF-L3-008)

Status: **approved by the maintainer on 2026-11-02 (decision D-B)**. The desktop thresholds were tightened to the runner values on 2026-11-03 after its three-round A/A ([record](eod-desktop-aa-results.md)), as D-B provided. Applies to
EOD decoder, layout, scheduling and other data-path changes from alpha.2 on.
Functional and resource-boundary failures always fail CI. Timings never run in
ordinary CI; they come from the dispatch-only before/after workflow or a
maintainer host that follows the same protocol.

## Designated workloads and metric

- **Gating workloads:** the eight 10,000-row synthetic workloads of the
  `eod_benchmark` harness, `{large-mixed, nulls, timestamps, prices} x {none, zstd}`,
  on both the numeric and Table engines (EOD-L3-026).
- **Gating metric:** **decode p50** (decompression, protobuf, conversion and
  validation). It is the only metric whose A/A spread is tight enough to gate on.
- **Informational metrics:** delivery, first-batch and per-batch wait p50/p95,
  executor lateness, allocation counts. They are reported with every
  comparison but not gated, because their shared-runner A/A spread reaches
  ±23-33%.
- **Excluded from gating:** the 16-row `small-mixed-*` workloads. Their decode
  takes microseconds, and A/A ratios range from 0.21x to 3.6x. They remain
  functional smoke cases.
- **Calibration workloads** (`--extremes`, see the
  [calibration record](eod-calibration.md)) must complete or be rejected
  exactly as documented. Their timings are informational.

## Designated hosts and measured noise

Each comparison runs both sides on one host, from clean commits, in
alternating AB/BA order, with warmups inside each run
([protocol](eod-baseline.md)). Below are A/A runs
(same commit on both sides), decode p50 on the gating workloads:

| Host | A/A evidence | Largest single-round deviation | Largest shift present in every round | Delivery, largest single round |
| --- | --- | ---: | ---: | ---: |
| GitHub `ubuntu-latest` (x86_64) | run 41380736017, 3 rounds | 3.4% | 2.0% | 23% |
| GitHub `windows-latest` (x86_64) | run 41380736017, 3 rounds | 13.4% (one spike) | 0.2% | 26% |
| Maintainer desktop (Ryzen 7 2700, Windows 11, idle) | 2 rounds, 2026-11-02 | 49.6% (one spike) | 8.2% | 33% |
| Maintainer desktop, repeated | [3 rounds, 2026-11-03](eod-desktop-aa-results.md), `82d8a41` | 127.8% (one spike) | 0.3% | 59% |

Single-round spikes occur on every host, including the quiet desktop. A shift
present in **every** round is what distinguishes a real change from noise.

## Tolerances (D-B, approved 2026-11-02)

Ratios are candidate/baseline decode p50, from at least **three** alternating
rounds per engine.

| Host | Regression: **minimum** ratio over rounds exceeds | Improvement: **maximum** ratio over rounds is below |
| --- | ---: | ---: |
| `ubuntu-latest` | **1.05** | **0.95** |
| `windows-latest` | **1.05** | **0.95** |
| Maintainer desktop | **1.05** (since 2026-11-03; 1.10 provisionally before) | **0.95** (since 2026-11-03; 0.90 provisionally before) |

Rationale:

- Using the minimum or maximum across rounds requires the change in every
  round, which absorbs the single-round spikes seen in all A/A runs.
- Each threshold sits above that host's largest every-round A/A shift: 2.0%
  and 0.2% on the runners, and 0.3% on the desktop in its three-round A/A
  (8.2% in the earlier two-round run).
- The desktop thresholds were first wider (1.10/0.90), because its A/A had
  only two rounds and an 8.2% every-round shift. Its three-round A/A on
  2026-11-03 had no every-round shift above 0.3%, so they were tightened to
  the runner values ([deferred item 6](../releases.md#deferred-from-alpha2), closed).

Decision rules:

1. A regression beyond tolerance on any gating workload **blocks performance
   acceptance** until it is explained and either fixed or explicitly approved
   by the maintainer as a measured per-workload tradeoff.
2. A single-round excursion (any round beyond tolerance, while the minimum
   stays within it) triggers three more rounds of that engine before any
   conclusion. The extra rounds are reported, not substituted.
3. An improvement claim needs the maximum ratio below the improvement
   threshold, on at least one runner **and** with the other platform reported.
   Architecture matters: aarch64-only wins are not claimed for x86 (see the
   [decoder record](eod-decoder-results.md)).
4. On the informational metrics, a minimum ratio above **1.30** on a runner is
   flagged for review, but does not block.
5. Every comparison retains raw reports, the summary table, commits, host,
   toolchain and the A/A reference it is read against. Regressions are
   reported, never omitted.

## Re-baselining

Changes to the harness, fixtures or lockfile change the provenance hashes, and
the native comparator then refuses to compare across them. In that case, rerun
an A/A at the new commit before the next A/B. The `--extremes` calibration
mode (2026-11-02) changes the harness hash; standard-workload comparisons
before and after it are not mixed.
