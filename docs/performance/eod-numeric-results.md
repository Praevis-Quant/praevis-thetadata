# Bounded numeric EOD results (2026-10-04)

Clean source: `ab8147d12aa1572afd0f0304c8a7aa0bdb3f9df3`.
The [experiment design and commands](eod-numeric.md) define the three engines
and metric boundaries. All modes use the same binary per platform, fixture
bytes, dependencies and exact-value oracle. The legacy decoder and mock source
remain unchanged from the original baseline. This is synthetic local delivery,
not vendor latency or an accepted complete EOD implementation.

Six sequential runs per platform: legacy, Table, numeric, numeric, Table,
legacy. Each has 3 warmups and 12 samples, eight columns, 10,000 rows except
small mixed (16 rows), four streams and three batches per stream. Windows runs
completed before Linux runs; no competing builds were running. Windows uses
Rust 1.99.0 MSVC/LLVM 23.1.1, Ryzen 7 2700 and 16 logical CPUs. Ubuntu under
WSL2 uses Rust 1.98.1 GNU/LLVM 22.1.8 and eight exposed CPUs; the executable is
on the native `/tmp` filesystem. Host OS context is unchanged from the
[original baseline](eod-results.md). Compare engines within each platform;
the two platforms are not a controlled OS comparison. No custom build flags
were supplied in the commands; reports do not capture every ambient setting.

## What the measurements support

- Timestamp-heavy numeric decode is about 3.0–4.0 times faster across NONE/ZSTD
  and both repetitions/platforms. NONE four-stream delivery is 7.3–8.2 times
  faster. This includes bypassing timestamp presentation; the compatibility
  adapter's results separately show the cost of retaining formatting.
- Numeric NONE timestamp allocation calls drop from 120,027 to 13; peak
  requested Rust heap drops from 6.117 to 2.232 MiB. C ZSTD allocations, stacks,
  transport buffers, allocator metadata and process RSS are not included.
- Large mixed numeric decode improves approximately 12–25% on these runs.
  End-to-end delivery benefits additionally from bounded offload, with two
  admitted jobs instead of all decoding executing on the consumer thread.
- There are material regressions. Large mixed NONE Table decoding is about
  5–7% slower on Windows and 32–42% slower on Linux. Linux small NONE delivery
  increases from 0.640–0.749 ms to 1.244–1.263 ms for numeric and
  1.299–1.441 ms for Table. Scheduling/validation overhead matters for tiny work.
- Table conversion reduces allocation calls but **increases peak requested
  Rust heap**: NONE timestamps rise from 6.117 to 7.401 MiB, and large mixed
  from 3.952 to 4.950 MiB. The numeric and formatted representations coexist
  during conversion. Account for that cost; do not advertise a universal
  memory improvement.
- Unrelated timer delay improves markedly under large timestamp load. On
  NONE timestamps, the p95 of each sample's maximum lateness falls from
  278–308 ms to 15.9 ms on Windows and 286–293 ms to 1.9–2.0 ms on Linux.
  This includes OS timer granularity and consumer work, not just decoder stalls.

These results support further work on the numeric path and bounded offload,
not merging an accepted market-data baseline yet. Keep the current policy
provisional. Next measure a safely bounded small-work inline threshold and
reduce Table adapter coexistence/preflight overhead after the remaining
resource/fault audits. A compressed byte count or untrusted `original_size`
alone cannot safely select inline work. Windows small ZSTD and Linux formatted
timestamp timings vary substantially; retain both runs and avoid a universal
speedup or latency threshold. With 12 samples, nearest-rank p95 is the maximum.

## Decode medians

Milliseconds per batch; each cell is **run 1 / run 2**. Input cloning and output
equality/destruction are outside this interval. The instrumented allocator still
performs an inactive thread-local check during timing; allocation counts come
from separate probes that include input cloning.

### Windows

| Case | Legacy Table | Bounded Table | Numeric |
| --- | ---: | ---: | ---: |
| Small mixed NONE | 0.020 / 0.019 | 0.020 / 0.020 | 0.015 / 0.015 |
| Small mixed ZSTD | 0.103 / 0.074 | 0.026 / 0.079 | 0.080 / 0.018 |
| Large mixed NONE | 12.245 / 12.228 | 12.873 / 13.112 | 9.168 / 9.148 |
| Large mixed ZSTD | 12.311 / 12.929 | 12.927 / 12.964 | 9.851 / 9.741 |
| Nulls NONE | 8.100 / 7.211 | 5.751 / 6.059 | 4.596 / 4.614 |
| Nulls ZSTD | 7.377 / 7.290 | 5.857 / 6.047 | 4.719 / 4.728 |
| Timestamps NONE | 22.701 / 22.469 | 22.557 / 22.184 | 7.221 / 7.092 |
| Timestamps ZSTD | 22.700 / 22.034 | 22.818 / 21.982 | 6.579 / 7.389 |
| Prices NONE | 10.428 / 9.590 | 8.769 / 8.928 | 7.419 / 7.451 |
| Prices ZSTD | 10.997 / 10.761 | 9.740 / 9.830 | 8.310 / 8.289 |

### Linux under WSL2

| Case | Legacy Table | Bounded Table | Numeric |
| --- | ---: | ---: | ---: |
| Small mixed NONE | 0.017 / 0.017 | 0.017 / 0.018 | 0.014 / 0.014 |
| Small mixed ZSTD | 0.022 / 0.022 | 0.023 / 0.023 | 0.019 / 0.019 |
| Large mixed NONE | 9.490 / 9.333 | 12.509 / 13.244 | 8.283 / 8.167 |
| Large mixed ZSTD | 9.824 / 9.934 | 10.548 / 10.675 | 8.213 / 8.352 |
| Nulls NONE | 6.223 / 6.327 | 5.024 / 4.885 | 4.264 / 4.186 |
| Nulls ZSTD | 6.036 / 6.233 | 5.130 / 4.971 | 4.410 / 4.302 |
| Timestamps NONE | 19.969 / 20.856 | 22.910 / 17.867 | 5.450 / 5.266 |
| Timestamps ZSTD | 18.762 / 18.252 | 16.752 / 18.330 | 5.703 / 5.653 |
| Prices NONE | 8.149 / 8.113 | 6.978 / 6.799 | 6.502 / 6.284 |
| Prices ZSTD | 8.968 / 8.561 | 8.473 / 7.144 | 6.678 / 6.589 |

## Delivery medians

Milliseconds for four streams × three batches, including loopback serving,
transport, decoding, the corresponding exact-value consumer and output drop.
Connections/authentication/fixture setup are outside timing. First-batch and
per-batch wait distributions are also retained in the raw reports.

| Platform / NONE case | Legacy Table | Bounded Table | Numeric |
| --- | ---: | ---: | ---: |
| Windows small mixed | 1.082 / 1.034 | 1.023 / 1.010 | 1.168 / 1.146 |
| Windows large mixed | 206.645 / 209.947 | 117.290 / 113.113 | 76.873 / 74.904 |
| Windows timestamps | 344.161 / 340.076 | 178.232 / 174.303 | 46.829 / 46.710 |
| Windows prices | 199.105 / 182.888 | 89.631 / 97.295 | 82.932 / 86.881 |
| Linux small mixed | 0.749 / 0.640 | 1.299 / 1.441 | 1.263 / 1.244 |
| Linux large mixed | 190.479 / 191.235 | 79.977 / 91.570 | 57.565 / 58.837 |
| Linux timestamps | 315.902 / 314.994 | 159.106 / 173.088 | 42.000 / 38.463 |
| Linux prices | 143.752 / 147.230 | 71.421 / 68.349 | 65.663 / 60.434 |

## Retained artifacts

Files below are ignored under `artifacts/eod/`. All twelve reports record clean
source `ab8147d`; eight guarded comparisons passed provenance/fixture/configuration
checks. Earlier `numeric-*` files are exploratory measurements of `0b7aec4`,
before the deadline-priority correction, and are not substituted for these results.
The original `windows-final-*`/`linux-final-*` baseline evidence remains intact.

| File | SHA-256 |
| --- | --- |
| bounded-windows.exe | `631e456b7ea9862d7a9c77149847b2cd8a643c32cb9d693e530027e142fa12a9` |
| bounded-linux | `55849fcf04a31e9cd40a016454de08dd2bc807548043aa9511dd171fd543aff3` |
| bounded-windows-legacy-1.json | `57d891a99c3532a9dfea6a175f05d07ba83bac6c8b2e74316885b729ef44c2cb` |
| bounded-windows-legacy-2.json | `5201d1a571baa763b77f50e775fa1736ce4e69d338f079797ab6a5315d779ef6` |
| bounded-windows-table-1.json | `2105040f6a964ac019f67127f2574257bd36096459270eccfe481b515658dd3d` |
| bounded-windows-table-2.json | `0ebd54e7aae40e965d55ce73816a085c85d514d4b966f70183972f2d586b83cd` |
| bounded-windows-numeric-1.json | `cd269ab534f7692434be63d106e186c218661968e44f99638133969433171c5b` |
| bounded-windows-numeric-2.json | `834e0426cffc37c2a17b5c2d18d73954fbc9e7494c02a72d39a357120e95357d` |
| bounded-linux-legacy-1.json | `52c096081ed84aac2b092fbbebbb2098b91b09031249b13a1d26a2304aac1c75` |
| bounded-linux-legacy-2.json | `f1a7219b7e18bc5a0a38465b9e1f1023010f9807ccda4ccfbfa8e151216bc198` |
| bounded-linux-table-1.json | `7fd8a061b5a0229d199854071a1f5acaea59e85f34e58fe7e4c5e962e37e1f12` |
| bounded-linux-table-2.json | `bb9410b806ec5dc2e9c7500ecfff0bf9085e19ec87cf22600e67a02198adda5e` |
| bounded-linux-numeric-1.json | `1c8e02e2537eaa1dd35fa099668dea20e2d9cb4779151713f12d48e3f02d377b` |
| bounded-linux-numeric-2.json | `7c9f134adf24704099ab6b5fa510bf84f8212afc926a6ef887ab266b554cb22d` |
| bounded-windows-table-comparison-1.json | `6daa8070cd16928b19c3ce249a9e6bba8beaca5f932cc715732a2e4826bbd219` |
| bounded-windows-table-comparison-2.json | `ea870494ed881a877ea9eb4517dcfa8fe84babfd1ba6bba46790135a34f745a7` |
| bounded-windows-numeric-comparison-1.json | `d54668cbe1a3b52278f3b0e4dea38443daff13ecaaaa58affdcd44206b7ebac4` |
| bounded-windows-numeric-comparison-2.json | `9f7162a2b4f438e7c63b03bf686e102a7440d8e1c4018a72b98d6e447ace1f99` |
| bounded-linux-table-comparison-1.json | `8a06b0387b8fa8ec7c621222b6faece02e9a2c7906025e3a8475c02b6e3f8ec1` |
| bounded-linux-table-comparison-2.json | `732f874e639a1e77bc6b7f9718d00b5a257677acafd9e40e933af7cb9dfaa1d1` |
| bounded-linux-numeric-comparison-1.json | `40ef9074c107fcc45f07f15049bc63079e32415119b6aadd1bab30d293a830ff` |
| bounded-linux-numeric-comparison-2.json | `c4db6350f04920981b21824140c48d19fd84f6cdd1a0de2953470770d88bbd25` |
