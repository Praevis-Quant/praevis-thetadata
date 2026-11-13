# 0.1.0 release-candidate evidence

The evidence planned in the
[0.1.0 scope record](../releases.md#release-candidate-evidence-for-010).
**The candidate is `main` at `78c95dd`, before the version bump.** Its code
is beta.2's (`3bf6266`); only documents changed since then.
- (a) the W5c speed comparison against the vendor package: **done
  2026-11-13**, below.
- (b) the live option-checks report: **to come.**

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

To come: the Rust `option-checks --same-response` run and the Python
`option_checks.py` run, during US market hours on 2026-11-13.
