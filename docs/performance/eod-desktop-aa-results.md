# Maintainer desktop 3-round A/A (alpha.3 A3, deferred item 6, 2026-11-03)

D-B approved the desktop tolerances as provisional (1.10/0.90) because its
first A/A had only two rounds. They were to be tightened after a three-round
A/A on the same host. This record is that run; the tolerances it sets are
in the [benchmark policy](benchmark-policy.md#tolerances-d-b-approved-2026-11-02).

## Method

- **Host:** AMD Ryzen 7 2700 (Zen+, 8 cores/16 threads), Windows 11 Pro,
  desktop on AC power, otherwise idle.
- **Toolchain:** rustc 1.99.0.
- **Source:** `main` at `82d8a41` (#41), a clean detached worktree, release build.
- **Procedure:** each of 3 rounds runs the same binary twice (a, then b), then
  the harness's native `compare`, then `tools/eod_compare_summary.py`. Ratios
  are b/a p50; any deviation from 1 is host noise.
- Run and reported by the Coordinator.

## Results (decode p50, numeric and Table engines)

| Engine | Case | Rounds | Decode p50 (min-max) | Delivery p50 (min-max) | First batch p50 (min-max) |
| --- | --- | ---: | --- | --- | --- |
| Numeric | large-mixed-none | 3 | 0.985-1.001 | 0.977-1.015 | 0.974-1.024 |
| Numeric | large-mixed-zstd | 3 | 0.981-1.008 | 0.983-1.022 | 0.984-1.001 |
| Numeric | nulls-none | 3 | 0.979-1.010 | 0.976-1.053 | 0.958-1.016 |
| Numeric | nulls-zstd | 3 | 0.983-1.041 | 0.981-1.007 | 0.972-1.014 |
| Numeric | prices-none | 3 | 0.999-1.006 | 0.978-1.061 | 0.974-1.019 |
| Numeric | prices-zstd | 3 | 0.985-1.003 | 0.981-1.011 | 0.985-1.024 |
| Numeric | timestamps-none | 3 | 0.992-0.997 | 0.995-1.059 | 0.984-1.069 |
| Numeric | timestamps-zstd | 3 | 0.997-1.032 | 0.614-1.019 | 0.675-1.022 |
| Table | large-mixed-none | 3 | 0.987-1.015 | 1.001-1.011 | 0.982-1.014 |
| Table | large-mixed-zstd | 3 | 0.984-1.011 | 1.002-1.022 | 0.964-1.047 |
| Table | nulls-none | 3 | 0.976-1.018 | 1.004-1.328 | 1.007-1.320 |
| Table | nulls-zstd | 3 | 0.974-2.278 | 0.971-1.593 | 1.017-1.408 |
| Table | prices-none | 3 | 0.993-1.011 | 0.979-0.999 | 0.980-0.993 |
| Table | prices-zstd | 3 | 0.995-1.020 | 0.985-1.013 | 1.009-1.049 |
| Table | timestamps-none | 3 | 0.996-1.449 | 0.970-1.105 | 0.991-1.143 |
| Table | timestamps-zstd | 3 | 1.000-1.004 | 1.001-1.009 | 0.997-1.008 |

The 16-row `small-mixed-*` cases are excluded from gating. Their decode
ratios were 0.968-1.019.

**Gating view:**
- The largest per-workload minimum is **1.000** (Table `timestamps-zstd`). The
  smallest per-workload maximum is **0.997** (Numeric `timestamps-none`).
- So no shift is present in every round: the every-round A/A shift is **0.0%**
  upward and **0.3%** downward.
- Single-round decode spikes reached **2.278** (Table `nulls-zstd`) and
  **1.449** (Table `timestamps-none`). These are larger than the earlier
  two-round run's 49.6% spike, and the min/max-over-rounds rule absorbs them.

## Decision

The desktop's every-round shift is now as small as the runners' (2.0% and
0.2%). The tolerance rationale of D-B, a threshold above the host's largest
every-round A/A shift, therefore supports the runner tolerances on the
desktop too: **1.05 for regression and 0.95 for improvement, no longer
provisional**. This applies the tightening condition the user approved in D-B.
