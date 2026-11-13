# 0.1.0 release-candidate evidence

The evidence planned in the
[0.1.0 scope record](../releases.md#release-candidate-evidence-for-010).
**The candidate is `main` at `78c95dd`, before the version bump.** Its code
is beta.2's (`3bf6266`); only documents changed since then. Before release,
the published code changed only in packaging (see the
[0.1.0 checklist](../crates-io-release-plan.md#010-publication-checklist-and-order-prepared-2026-11-13)).
- (a) the W5c speed comparison against the vendor package: **done
  2026-11-13**, below.
- (b) the live option-checks report: **passed 2026-11-13**, below.

## (a) W5c speed comparison (2026-11-13)

**Why it was re-run:** the [D2 ratios](options-d2-results.md) were measured
with the account-wide scheduler that beta.2 removed
([ADR-0021](../adr/0021-per-call-library.md)). This checks that the speed
gate still holds, with a job's own concurrency.

**Conditions:**
- **Host and service:** the maintainer's Ryzen 7 2700, Windows 11, options
  PRO, PROD, 04:08-04:12 UTC (US off-hours). The Terminal was stopped
  during the runs.
- **Workload:** W5c, SPY tick trades on 2024-06-14, one request per
  expiration (12 expirations, 265,034 rows).
- **Runners:**
  - the vendor package 1.0.17 (`options_vendor.py`);
  - our Python package (`options_ours.py`), from a wheel built at
    `78c95dd`;
  - our Rust `query()` (`live_p0 workload`).
- **Three cycles, each running every runner once,** with 1 then 8 threads.
  The middle cycle ran in reverse order, so no runner always went first
  (D2's caveat).
- **Every run returned 265,034 rows,** with all 12 requests OK.
- **Single runs per cycle, off-hours.** These are observations.

**Time to complete (seconds):**

| Runner | Cycle 1 | Cycle 2 (reversed) | Cycle 3 | Median |
| --- | ---: | ---: | ---: | ---: |
| Vendor, sync | 18.78 | 19.42 | 19.00 | 19.00 |
| Vendor, 8 threads | 9.33 | 9.44 | 9.74 | 9.44 |
| Ours Python, 1 | 15.83 | 10.41 | 12.60 | 12.60 |
| Ours Python, 8 | 1.76 | 2.04 | 1.65 | **1.76** |
| Rust, 1 | 12.45 | 12.30 | 11.24 | 12.30 |
| Rust, 8 | 2.15 | 2.30 | 2.08 | **2.15** |

**Against the targets (medians):**

| Target | Python, 8 | Rust, 8 | D2 (Python / Rust: sequential; interleaved) | Status |
| --- | ---: | ---: | --- | --- |
| W5 ≥ 7× the synchronous vendor package at 8-way | 10.8× | 8.8× | 9.8× / 11.0×; 9.4× / 8.4× | **Met** |
| W5 ≥ 1.0× the vendor threaded at the same concurrency | 5.4× | 4.4× | 3.3× / 3.7×; 3.4× / 3.0× | **Met** |

**Even in the worst pairs, the gate holds.** Our slowest run at 8 against
the vendor's fastest:
- **Python:** 2.04 s against 18.78 s synchronous (9.2×) and 9.33 s
  threaded (4.6×).
- **Rust:** 2.30 s against 18.78 s (8.2×) and 9.33 s (4.1×).

**One request at a time,** ours is 1.5× the synchronous vendor package
(Python 12.60 s and Rust 12.30 s, against 19.00 s).

**Peak RSS per process:**
- **The vendor package:** 232.9-235.7 MiB, of which 101.7-102.0 MiB at
  import.
- **Our Python package:** 149.3-156.8 MiB, of which 65.1-65.4 MiB at
  import.
- **The Rust runner** does not record memory.

**Reading:**
- **The speed gate holds without the scheduler.** A job's own concurrency
  (beta.2) gives the same result.
- **Against the threaded vendor package,** 5.4× is above D2's 3.3-3.7×.
  These are single runs per cycle, off-hours, so this is not claimed as an
  improvement: only that the gate holds.
- **The vendor package was faster too:** 19.0 s synchronous, against D2's
  25.7-27.5 s. The service was likely quicker off-hours, so absolute times
  are not comparable with D2. Only the ratios are.
- **At 8, our Python package was slightly faster than `live_p0`'s Rust
  workload in all three cycles** (1.65-2.04 s against 2.08-2.30 s).
  **Unexplained.** The two runners measure differently: they build
  different outputs and time different boundaries.

## (b) Live option-checks report

**Both halves pass.** Windows, options PRO (stocks and indices FREE), PROD,
on `main` at `62b947c`. That is the release candidate plus the test
infrastructure and docs merged since; no published code differs from the
candidate's except in packaging. Both runs were during US market hours on
2026-11-13, with the snapshot expiration 2026-11-13 (SPY). Counts and
classifications only.

**Python, `bench/p1/option_checks.py`** (`rc010-opt-checks-py-001`,
14:45-14:46 UTC): **all 47 option wrappers answered OK with rows.**

| Group | Rows per request |
| --- | --- |
| Lists | 15,858 / 502 / 2,170 / 232 / 4,028 |
| Snapshots | 156-171 |
| History: EOD, open interest, Greeks EOD | 226 |
| History: OHLC | 846 |
| History: quote, Greeks | 1,356 |
| History: trade, trade Greeks | 172 |
| At-time: trade / quote | 73 / 226 |

**Rust, `live_p0 option-checks --same-response`**
(`rc010-opt-checks-002`, 16:31 UTC, `--flat-file-date 2026-11-12`, the
Terminal stopped during the run): **all 50 option RPCs returned data.**
That is every option RPC except the gated quote flat file.
- **Equal values:** the columnar and legacy decoders gave equal values on
  the same response, for every RPC.
- **Flat files:** the three bounded flat-file probes (EOD, open interest,
  trade-quote) read one message of 8,192 rows each, equal both ways.
- **No refusals and no errors.**

**One operator error, not a library one.** The first Rust attempt
(`rc010-opt-checks-001`, 14:45 UTC) stopped at startup.
- **The cause:** the operator's script passed `--snapshot-expiration` as
  `20261113`, while `live_p0` expects ISO dates. It panicked
  (`live_p0.rs:375`, "workload dates are ISO") before sending any request.
- **The re-run** with `2026-11-13` is the result above.

**Follow-ups after the release (not in 0.1.0):**
- `live_p0` should reject a malformed date with exit code 2 and a clear
  message instead of panicking.
- It should accept or reject `yyyymmdd` consistently with
  `option_checks.py`, which accepted `20261113`.

`scripts/live/run.sh` had the same `yyyymmdd` date. It is fixed in its own
pull request.

**Reading: the live report passes, so the 0.1.0 gates are met.**
- **(a):** the speed re-check holds without the scheduler.
- **(b):** every option RPC on the declared surface returned data, with
  equal values through both decoders and the Python package.
