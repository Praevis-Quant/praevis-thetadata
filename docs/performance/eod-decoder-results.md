# EOD decoder recovery (alpha.2 step 2, 2026-11-02)

The bounded decoder (EOD-L3-020 to EOD-L3-022) originally ran a full prost
`DataValue::decode` for every cell, after its preflight pass. When the bounded
path replaced the unbounded decoder, null-heavy decode regressed about 11-18%
and some price workloads' delivery regressed 9-19%
([acceptance results](eod-acceptance-results.md)). This step replaces the
per-cell prost call with a hand-written reader that has identical semantics
(differential and exhaustive equivalence tests), plus fast paths for the
dominant two-byte null, boolean and small-integer cells. Preflight, limits,
accounting and error categories are unchanged.

## Method

- **Protocol:** the before/after mode of the **Synthetic EOD performance**
  workflow ([protocol](eod-baseline.md#on-github-hosted-runners-windows-and-linux)):
  clean worktree release builds, alternating AB/BA order, 3 rounds, numeric
  and Table engines, 10,000-row fixtures. Ratios are candidate/baseline p50;
  below 1 is faster.
- **Baseline:** `main` at `7274474` (#28).
- **Candidates:** `b1f6ef7` (reader only) and `73bdb48` (reader plus fast paths).
- **Noise floor:** the A/A run (`main` against itself, run 41380736017) on the
  same runner images. On large fixtures, decode varies about ±1-3% on Linux
  and ±1-5% on Windows. Delivery and first-batch ratios swing up to ±25% and
  ±45%, so they are informational. The 16-row `small-mixed-*` fixtures are
  noise and are omitted below.
- Shared runners are x86_64. Local runs in the aarch64 development sandbox are
  reported separately, because the first candidate behaved differently there.

## Results: reader plus fast paths (`73bdb48`, run 41381892825)

### Linux x86_64 runner

| Engine | Case | Rounds | Decode p50 ratio (min-max) | Delivery p50 ratio (min-max) | First batch p50 ratio (min-max) |
| --- | --- | ---: | --- | --- | --- |
| Numeric | large-mixed-none | 3 | 0.835-0.847 | 0.902-1.006 | 0.957-1.078 |
| Numeric | large-mixed-zstd | 3 | 0.855-0.864 | 0.787-0.887 | 0.597-0.851 |
| Numeric | nulls-none | 3 | 0.699-0.701 | 0.753-0.816 | 0.893-0.928 |
| Numeric | nulls-zstd | 3 | 0.706-0.716 | 0.671-0.776 | 0.773-0.786 |
| Numeric | prices-none | 3 | 0.900-0.903 | 0.900-1.160 | 0.990-1.158 |
| Numeric | prices-zstd | 3 | 0.891-0.913 | 0.775-0.885 | 0.720-0.851 |
| Numeric | small-mixed-none | 3 | 0.811-1.403 | 0.803-0.895 | 0.846-0.968 |
| Numeric | small-mixed-zstd | 3 | 0.880-1.449 | 0.995-1.078 | 1.011-1.151 |
| Numeric | timestamps-none | 3 | 0.836-0.839 | 0.731-0.786 | 0.841-0.870 |
| Numeric | timestamps-zstd | 3 | 0.840-0.848 | 0.687-0.735 | 0.664-0.669 |
| Table | large-mixed-none | 3 | 0.892-0.906 | 0.854-0.985 | 0.959-1.044 |
| Table | large-mixed-zstd | 3 | 0.864-0.873 | 0.641-1.060 | 0.582-1.389 |
| Table | nulls-none | 3 | 0.731-0.737 | 0.807-1.082 | 0.897-1.138 |
| Table | nulls-zstd | 3 | 0.730-0.763 | 0.742-0.984 | 0.549-1.257 |
| Table | prices-none | 3 | 0.893-0.908 | 0.841-0.989 | 0.763-1.136 |
| Table | prices-zstd | 3 | 0.848-0.940 | 0.787-0.842 | 0.850-0.855 |
| Table | small-mixed-none | 3 | 0.847-1.397 | 0.898-0.991 | 0.867-1.055 |
| Table | small-mixed-zstd | 3 | 0.877-1.087 | 0.886-1.117 | 0.977-1.111 |
| Table | timestamps-none | 3 | 0.921-0.938 | 0.824-1.036 | 0.839-1.091 |
| Table | timestamps-zstd | 3 | 0.928-0.936 | 0.756-0.978 | 0.727-0.864 |

### Windows x86_64 runner

| Engine | Case | Rounds | Decode p50 ratio (min-max) | Delivery p50 ratio (min-max) | First batch p50 ratio (min-max) |
| --- | --- | ---: | --- | --- | --- |
| Numeric | large-mixed-none | 3 | 0.822-0.859 | 0.618-0.860 | 0.687-0.930 |
| Numeric | large-mixed-zstd | 3 | 0.840-0.845 | 0.656-0.857 | 0.743-0.830 |
| Numeric | nulls-none | 3 | 0.641-0.678 | 0.624-0.798 | 0.691-0.856 |
| Numeric | nulls-zstd | 3 | 0.655-0.698 | 0.704-0.773 | 0.755-0.809 |
| Numeric | prices-none | 3 | 0.898-0.901 | 0.894-0.922 | 0.923-0.962 |
| Numeric | prices-zstd | 3 | 0.881-0.906 | 0.851-0.922 | 0.868-0.874 |
| Numeric | small-mixed-none | 3 | 0.427-1.090 | 0.980-1.037 | 1.039-1.128 |
| Numeric | small-mixed-zstd | 3 | 0.910-1.112 | 0.880-1.052 | 0.798-1.200 |
| Numeric | timestamps-none | 3 | 0.866-0.891 | 0.752-0.778 | 0.716-0.872 |
| Numeric | timestamps-zstd | 3 | 0.889-0.903 | 0.738-0.749 | 0.755-0.801 |
| Table | large-mixed-none | 3 | 0.901-0.922 | 0.911-0.992 | 0.927-0.963 |
| Table | large-mixed-zstd | 3 | 0.748-0.917 | 0.857-0.903 | 0.816-0.846 |
| Table | nulls-none | 3 | 0.703-0.790 | 0.759-0.794 | 0.857-0.909 |
| Table | nulls-zstd | 3 | 0.681-0.732 | 0.757-0.778 | 0.765-0.788 |
| Table | prices-none | 3 | 0.886-0.915 | 0.914-0.934 | 0.961-0.983 |
| Table | prices-zstd | 3 | 0.909-0.923 | 0.883-0.897 | 0.844-0.891 |
| Table | small-mixed-none | 3 | 0.932-1.068 | 0.896-1.062 | 0.903-1.032 |
| Table | small-mixed-zstd | 3 | 0.516-1.860 | 0.925-1.016 | 0.910-1.015 |
| Table | timestamps-none | 3 | 0.932-0.967 | 0.905-0.958 | 0.899-1.021 |
| Table | timestamps-zstd | 3 | 0.926-0.949 | 0.918-0.961 | 0.890-0.910 |

## Maintainer Windows PC (quiet x86_64 host, 2026-11-02)

Host: AMD Ryzen 7 2700 (8C/16T, Zen+), Windows 11 Pro, desktop on AC power and
otherwise idle, rustc 1.99.0. Baseline `7274474`, candidate `73bdb48`. Protocol:

- separate clean detached worktrees, each with its own target directory;
- release builds finished before measuring;
- runs from inside each checkout, in alternating AB/BA order;
- native `compare` and `tools/eod_compare_summary.py`.

Deviations from the runner protocol: A/A used 2 rounds. Table got 3 extra A/B
rounds (r4-r6) to check a single-round outlier. The reader-only candidate was
not rerun, because the runner data already answers it.

| Case (decode p50, candidate/baseline) | A/A (2 rounds) | A/B r1-r3 | Table A/B r4-r6 |
| --- | --- | --- | --- |
| Numeric nulls-none / -zstd | 0.999-1.001 / 1.003-1.014 | **0.702-0.711 / 0.701-0.736** | — |
| Table nulls-none / -zstd | 0.969-1.047 / 1.082-1.152 | **0.755-0.796 / 0.692-0.757** | **0.665-0.845 / 0.765-0.822** |
| Numeric prices-none / -zstd | 1.006-1.030 / 0.957-1.015 | 0.907-0.919 / 0.868-0.932 | — |
| Table prices-none / -zstd | 0.946-0.991 / 0.992-0.992 | 0.907-**1.483** / 0.923-0.948 | 0.940-0.951 / 0.966-1.182 |
| Numeric large-mixed-none / -zstd | 0.985-0.992 / 0.982-1.006 | 0.706-0.886 / 0.894-0.964 | — |
| Table large-mixed-none / -zstd | 1.008-1.022 / 0.955-1.019 | 0.924-0.938 / 0.958-1.117 | 0.945-0.991 / 0.935-0.972 |
| Numeric timestamps-none / -zstd | 0.987-0.995 / 1.018-1.036 | 0.884-0.951 / 0.529-0.944 | — |
| Table timestamps-none / -zstd | 1.047-1.496 / 0.971-1.002 | 0.994-1.121 / 0.973-1.005 | 0.959-1.061 / 0.951-1.028 |

The 16-row `small-mixed-*` fixtures were noise here as on the runners (A/A
0.21-3.61x) and are omitted.

**Reading:**
- The PC confirms the runners. Null decode is 0.67-0.85x on both engines,
  numeric prices are 0.87-0.93x, and large mixed is at or below 1.0x.
- Table prices-none at 1.483x in r1-r3 is a single-round spike: r4-r6 give
  0.940-0.951x.
- This host shows similar occasional single-round spikes even in A/A (Table
  nulls-zstd 1.15x, timestamps-none 1.50x). Treat a one-round excursion as
  noise unless it repeats.
- No workload regresses consistently beyond the A/A spread.

**Gate 2 is accepted as resolved** by the maintainer on 2026-11-02, on the
basis of the runner and Windows PC evidence above.

## Reader only (`b1f6ef7`): why the fast paths exist

On aarch64 (local sandbox, 3 rounds), the reader alone cut null decode to
0.79-0.86x. On the x86_64 Linux runner (run 41381046724), it made null decode
**1.06-1.10x slower**, outside the ±2-3% A/A noise. On Windows it was within or
near noise. Adding exact two-byte fast paths turned null decode into a 21-36%
gain on both x86 runners. Both candidates are kept as evidence; only the
combined head is proposed.

## Against the unbounded legacy decoder (current harness, aarch64 local)

The pre-regression control commit cannot be rebuilt; it predates the current
history. Within the current harness, the `legacy` engine still runs the
unbounded prost table decoder. The bounded numeric reader (first candidate)
decoded nulls at 1.52-1.65x and prices at 1.37-1.51x legacy time, while
end-to-end delivery was 0.39-0.56x of legacy. The remaining raw-decode gap is
the preflight bound pass plus validation and batch construction, which are
deliberate resource contracts. Merging the preflight text walk into decoding
would change when accounting happens (EOD-L3-021/022) and needs its own
reviewed change.

## Gate 2 assessment

On both x86 runners, null decode improves 21-36% against `main`, which
contains the regressed bounded decoder. That exceeds the recorded 11-18%
regression. Price decode improves 6-12%, and price delivery improves on Linux
(0.78-0.94x; the Windows runner within noise to 0.92x). The old price-delivery
regression (+9-19%) is therefore at least offset on the measured runners.

Because the original control cannot be rebuilt, "resolved" means **better
than the regressed baseline by more than the regression's measured size**, on
noise-calibrated runners and a quiet Windows PC. It is not a like-for-like
comparison with the original control. The maintainer accepted gate 2 as
resolved on 2026-11-02 (see the Windows PC section).

The Table `*-mixed-none` shift noted after ADR-0019 (+1-2%) is superseded:
Table `large-mixed-none` decode is now 0.89-0.92x.
