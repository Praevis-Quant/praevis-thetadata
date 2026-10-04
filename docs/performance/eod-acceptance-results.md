# EOD fault handling and resource calibration (2026-10-04)

Selected implementation: `2eab7910cd7edbabdd25cc4615c483522257086c`.
Control: `51c84815d3ca58fced07352cada13d59c69a68e5`, whose production code is
unchanged from merged `dba6b9ce46fdb53ac7663d090f37a09c29873a3e`.
The control adds only the common benchmark/fixture, matching dependency metadata,
and an uncompiled frame-guard fingerprint placeholder. Both sources were clean.
The subsequent decoder-layout experiment was reverted; the selected production
source is unchanged. See the [acceptance matrix](../requirements/stock-eod-matrix.md)
for the per-requirement tests, error provenance and open verification clauses.

## Decision and material findings

- Integrate the correctness fixes as experimental work: local frame and ZSTD
  resource errors are distinguishable without trusting remote status text.
- Large slow-consumer calibration exposed an existing HTTP/2 progress failure.
  Eight concurrent streams with 100,000-row mixed batches timed out in both the
  unchanged control and initial candidate. The selected bounded stream/window
  policy completes every measured shape in both repetitions on Windows and WSL.
  A timeout is a failed workload, not a finite latency baseline or speedup ratio.
- Retain the existing decode limits, two jobs and offload-by-default policy.
  Add a 16-query admission limit and fixed 2 MiB stream / 34 MiB connection
  receive credit on a dedicated typed channel. Raw traffic has a separate channel.
- **Performance acceptance remains open.** Numeric null-heavy decode regresses
  about 11-18% across these Windows/Linux repetitions. Several price workloads
  also regress: Windows numeric NONE delivery rises about 9-19%, and Linux
  numeric ZSTD price delivery about 9-11%. These are retained costs, not hidden
  behind aggregate throughput. Large mixed/Table delivery is mostly similar or
  variable; this is not a universal speedup or a performance-approved baseline.
- The deliberate integration tradeoff is correctness and progress under finite
  resources, while carrying the measured decoder costs as an explicit next
  optimization task. Do not raise budgets to conceal stalls or remove validation
  to recover microbenchmark speed. Full EOD/PERF acceptance is not granted.

## Measurement method

The Rust executable performs all timing, fixture serving, equality checks and
allocation instrumentation. Report analysis does not form part of timing.
Windows uses Rust 1.99.0 MSVC/LLVM 23.1.1 on Ryzen 7 2700, 16 logical CPUs;
Ubuntu WSL2 uses Rust 1.98.1 GNU/LLVM 22.1.8 and eight exposed CPUs. Linux
executables run on native `/tmp` storage. Compare within each platform; this is
not a controlled OS comparison. Builds finished before measurements; Windows
runs finished before Linux runs. Existing ambient host noise is not eliminated.

Normal comparison: 10,000 rows (16 small), eight columns, four streams, three
batches each, three warmups and twelve samples. For numeric-offload, then
Table-offload, order is control 1, candidate 1, candidate 2, control 2. Both
engines use inline_bytes=0. Harness, fixture, lockfile and descriptor hashes
match within each pair; eight guarded comparisons passed. The v3 harness adds
consumer_delay_ms and rejects comparison with v2 or a different consumer delay.

Calibration: numeric 100,000 rows and Table 50,000 rows, eight streams, three
batches each, one warmup/four samples, with a 10 ms pause **while retaining one
batch per consumer**. Run numeric then Table, twice, on each platform. Output is
checked cell-for-cell. Delivery includes all pauses (including after the last
batch), transport, decode, consumer checks and destruction; no pause subtraction
or equal-row speedup comparison between these two calibration sizes is valid.

Direct decoding includes decompression, preflight, conversion and validation;
it excludes input cloning and output checks/drop. It compiles the same private
decoder source into the benchmark, whereas delivery calls the production client;
compiler context and allocator instrumentation can affect their relative costs.
The separate allocation probe includes input cloning and requested Rust heap,
not libzstd C allocation, transport, stacks, fragmentation or process RSS.
First-batch, per-batch wait, throughput and timer observations remain in the raw
reports. p95 is the maximum with both twelve and four samples; it is not a tail
latency guarantee. Small ZSTD decode times sometimes vary by several-fold.

## Normal-workload comparisons

Milliseconds; every cell is **run 1 / run 2**, nearest-rank median. All cases
are shown, including regressions. Full first-batch/p95/timer/allocation results
remain in the retained reports and guarded comparisons.

### Windows numeric-offload

| Case | Control decode | Candidate decode | Control delivery | Candidate delivery |
| --- | --- | --- | --- | --- |
| small-mixed-none | 0.015 / 0.015 | 0.015 / 0.015 | 1.132 / 1.017 | 1.017 / 1.150 |
| small-mixed-zstd | 0.018 / 0.079 | 0.018 / 0.018 | 1.430 / 1.651 | 1.356 / 1.444 |
| large-mixed-none | 8.756 / 8.842 | 8.606 / 8.666 | 77.573 / 78.287 | 79.651 / 77.914 |
| large-mixed-zstd | 9.787 / 9.546 | 9.659 / 9.332 | 61.764 / 61.834 | 63.182 / 63.126 |
| nulls-none | 4.630 / 4.625 | 5.210 / 5.155 | 38.488 / 38.353 | 39.750 / 38.509 |
| nulls-zstd | 4.852 / 4.771 | 5.378 / 5.312 | 30.992 / 31.969 | 32.765 / 32.231 |
| timestamps-none | 5.877 / 5.858 | 6.266 / 6.112 | 45.782 / 47.478 | 47.430 / 46.572 |
| timestamps-zstd | 6.001 / 5.970 | 6.298 / 6.233 | 38.274 / 39.146 | 40.885 / 40.961 |
| prices-none | 6.759 / 6.808 | 7.502 / 7.534 | 82.047 / 83.351 | 89.201 / 99.313 |
| prices-zstd | 7.553 / 7.594 | 8.334 / 8.321 | 50.971 / 51.174 | 55.593 / 54.318 |

### Windows table-offload

| Case | Control decode | Candidate decode | Control delivery | Candidate delivery |
| --- | --- | --- | --- | --- |
| small-mixed-none | 0.020 / 0.020 | 0.019 / 0.020 | 1.064 / 1.334 | 1.030 / 1.103 |
| small-mixed-zstd | 0.024 / 0.024 | 0.084 / 0.087 | 1.298 / 1.588 | 1.624 / 1.176 |
| large-mixed-none | 12.725 / 12.836 | 12.652 / 12.681 | 117.796 / 122.240 | 119.302 / 119.700 |
| large-mixed-zstd | 12.875 / 12.876 | 12.710 / 13.143 | 96.463 / 98.056 | 94.268 / 96.783 |
| nulls-none | 6.163 / 5.812 | 6.247 / 6.328 | 53.663 / 50.753 | 51.413 / 54.612 |
| nulls-zstd | 6.446 / 5.903 | 6.559 / 6.503 | 46.000 / 47.343 | 46.363 / 48.534 |
| timestamps-none | 21.807 / 20.962 | 21.024 / 20.753 | 174.904 / 183.638 | 175.939 / 178.842 |
| timestamps-zstd | 20.938 / 21.213 | 21.474 / 20.977 | 171.081 / 168.297 | 170.766 / 169.068 |
| prices-none | 8.181 / 8.220 | 8.700 / 8.636 | 94.775 / 94.796 | 106.770 / 105.129 |
| prices-zstd | 9.087 / 9.185 | 9.629 / 9.783 | 66.549 / 67.943 | 68.447 / 71.234 |

### Linux numeric-offload

| Case | Control decode | Candidate decode | Control delivery | Candidate delivery |
| --- | --- | --- | --- | --- |
| small-mixed-none | 0.013 / 0.013 | 0.015 / 0.015 | 1.289 / 1.238 | 1.236 / 1.265 |
| small-mixed-zstd | 0.018 / 0.018 | 0.018 / 0.018 | 1.328 / 1.275 | 1.313 / 1.399 |
| large-mixed-none | 7.882 / 8.177 | 8.266 / 8.462 | 61.964 / 58.678 | 61.974 / 62.818 |
| large-mixed-zstd | 8.232 / 8.136 | 8.553 / 8.843 | 58.848 / 62.910 | 64.684 / 61.978 |
| nulls-none | 4.346 / 4.313 | 4.991 / 5.070 | 31.463 / 31.642 | 32.382 / 31.204 |
| nulls-zstd | 4.467 / 4.531 | 5.208 / 5.244 | 27.007 / 27.047 | 27.124 / 26.747 |
| timestamps-none | 5.486 / 5.486 | 5.789 / 5.901 | 41.983 / 42.026 | 42.739 / 41.754 |
| timestamps-zstd | 5.828 / 5.723 | 5.884 / 6.046 | 37.491 / 36.648 | 35.771 / 36.591 |
| prices-none | 6.263 / 6.351 | 6.709 / 6.818 | 71.413 / 69.730 | 71.896 / 71.794 |
| prices-zstd | 6.517 / 6.604 | 7.133 / 7.245 | 39.502 / 39.833 | 43.008 / 44.333 |

### Linux table-offload

| Case | Control decode | Candidate decode | Control delivery | Candidate delivery |
| --- | --- | --- | --- | --- |
| small-mixed-none | 0.017 / 0.017 | 0.018 / 0.018 | 1.325 / 1.308 | 1.323 / 1.482 |
| small-mixed-zstd | 0.022 / 0.023 | 0.023 / 0.025 | 1.342 / 1.383 | 1.398 / 1.629 |
| large-mixed-none | 12.711 / 12.199 | 12.888 / 13.517 | 100.202 / 101.237 | 105.391 / 96.862 |
| large-mixed-zstd | 10.347 / 10.376 | 10.882 / 11.195 | 86.343 / 86.261 | 86.914 / 83.241 |
| nulls-none | 5.012 / 4.867 | 5.545 / 5.614 | 58.598 / 53.682 | 56.230 / 56.305 |
| nulls-zstd | 5.032 / 4.959 | 5.664 / 5.743 | 49.532 / 49.364 | 50.634 / 48.865 |
| timestamps-none | 20.681 / 21.676 | 22.194 / 18.077 | 168.057 / 167.324 | 172.225 / 170.983 |
| timestamps-zstd | 18.025 / 18.000 | 18.225 / 18.404 | 165.924 / 157.567 | 169.649 / 158.470 |
| prices-none | 6.844 / 6.872 | 7.223 / 7.398 | 75.775 / 74.324 | 78.815 / 75.956 |
| prices-zstd | 7.109 / 7.213 | 7.770 / 7.616 | 46.280 / 47.228 | 49.789 / 50.421 |

## Larger concurrent/slow-consumer observations

Milliseconds, **run 1 / run 2**; peak requested Rust heap is a per-decode probe
in MiB. Numeric and Table use different row counts as stated above. The timer
column is p95 of each sample's maximum 1 ms timer lateness, including OS timer
granularity and consumer work. It is not a decoder-only stall measurement.

### Windows numeric-offload: 100,000 rows, eight streams

| Case | Delivery median ms | Peak Rust heap MiB | Timer p95 ms |
| --- | --- | --- | --- |
| small-mixed-none | 46.328 / 46.573 | 0.005 / 0.005 | 15.130 / 14.564 |
| small-mixed-zstd | 46.647 / 46.820 | 0.005 / 0.005 | 15.562 / 14.960 |
| large-mixed-none | 1584.486 / 1540.489 | 26.036 / 26.036 | 19.365 / 16.369 |
| large-mixed-zstd | 1170.102 / 1174.699 | 27.169 / 27.169 | 19.625 / 18.986 |
| nulls-none | 809.687 / 809.459 | 21.554 / 21.554 | 19.094 / 17.361 |
| nulls-zstd | 654.457 / 639.243 | 22.311 / 22.311 | 17.496 / 17.442 |
| timestamps-none | 1017.839 / 1039.039 | 22.317 / 22.317 | 17.928 / 17.698 |
| timestamps-zstd | 826.432 / 831.092 | 26.311 / 26.311 | 17.447 / 17.781 |
| prices-none | 1728.377 / 1757.331 | 31.567 / 31.567 | 17.735 / 17.659 |
| prices-zstd | 1054.755 / 1046.642 | 34.311 / 34.311 | 17.122 / 17.876 |

### Windows table-offload: 50,000 rows, eight streams

| Case | Delivery median ms | Peak Rust heap MiB | Timer p95 ms |
| --- | --- | --- | --- |
| small-mixed-none | 46.642 / 46.611 | 0.008 / 0.008 | 14.707 / 14.686 |
| small-mixed-zstd | 46.597 / 46.633 | 0.008 / 0.008 | 14.563 / 15.566 |
| large-mixed-none | 1086.330 / 1102.629 | 24.748 / 24.748 | 24.445 / 30.245 |
| large-mixed-zstd | 884.271 / 876.768 | 24.748 / 24.748 | 22.947 / 22.941 |
| nulls-none | 543.325 / 532.909 | 22.507 / 22.507 | 18.595 / 19.411 |
| nulls-zstd | 470.258 / 467.089 | 22.507 / 22.507 | 19.580 / 18.046 |
| timestamps-none | 1671.094 / 1649.404 | 37.003 / 37.003 | 37.582 / 32.008 |
| timestamps-zstd | 1566.414 / 1571.802 | 37.003 / 37.003 | 32.027 / 33.701 |
| prices-none | 1119.766 / 1088.525 | 22.507 / 22.507 | 18.723 / 19.050 |
| prices-zstd | 685.978 / 701.962 | 22.507 / 22.507 | 18.091 / 18.406 |

### Linux numeric-offload: 100,000 rows, eight streams

| Case | Delivery median ms | Peak Rust heap MiB | Timer p95 ms |
| --- | --- | --- | --- |
| small-mixed-none | 36.252 / 36.462 | 0.005 / 0.005 | 1.587 / 1.709 |
| small-mixed-zstd | 35.982 / 34.282 | 0.005 / 0.005 | 1.745 / 1.673 |
| large-mixed-none | 1121.948 / 1108.844 | 26.036 / 26.036 | 17.609 / 13.063 |
| large-mixed-zstd | 934.430 / 937.382 | 27.169 / 27.169 | 9.977 / 7.074 |
| nulls-none | 607.745 / 643.738 | 21.554 / 21.554 | 9.268 / 8.279 |
| nulls-zstd | 517.270 / 516.532 | 22.311 / 22.311 | 5.753 / 6.373 |
| timestamps-none | 820.887 / 836.615 | 22.317 / 22.317 | 8.089 / 8.361 |
| timestamps-zstd | 744.340 / 702.779 | 26.311 / 26.311 | 6.154 / 6.244 |
| prices-none | 1342.211 / 1343.654 | 31.567 / 31.567 | 21.508 / 35.328 |
| prices-zstd | 839.361 / 868.652 | 34.311 / 34.311 | 6.838 / 8.468 |

### Linux table-offload: 50,000 rows, eight streams

| Case | Delivery median ms | Peak Rust heap MiB | Timer p95 ms |
| --- | --- | --- | --- |
| small-mixed-none | 34.058 / 35.734 | 0.008 / 0.008 | 1.951 / 1.739 |
| small-mixed-zstd | 35.859 / 34.837 | 0.008 / 0.008 | 1.760 / 1.791 |
| large-mixed-none | 842.959 / 862.683 | 24.748 / 24.748 | 13.145 / 14.008 |
| large-mixed-zstd | 710.419 / 743.817 | 24.748 / 24.748 | 12.785 / 12.590 |
| nulls-none | 443.765 / 437.784 | 22.507 / 22.507 | 10.286 / 7.581 |
| nulls-zstd | 389.327 / 409.297 | 22.507 / 22.507 | 8.992 / 7.198 |
| timestamps-none | 1450.283 / 1490.943 | 37.003 / 37.003 | 26.147 / 27.289 |
| timestamps-zstd | 1397.440 / 1367.856 | 37.003 / 37.003 | 23.451 / 23.110 |
| prices-none | 744.063 / 737.021 | 22.507 / 22.507 | 19.288 / 16.275 |
| prices-zstd | 502.310 / 481.550 | 22.507 / 22.507 | 4.911 / 4.688 |

## Policy rationale and accounting limits

The largest measured per-decode requested Rust heap was **37.003 MiB**;
the largest decompressed fixture was **13.256 MiB**. These observations
support retaining headroom, not proving every permitted shape fits or deriving
a process-memory bound. Count/byte/allocation ceilings apply together: reaching
one permitted maximum does not waive the others. A formatted 100,000-row batch
can exceed allocation policy even when the row limit permits it.

| Policy | Retained/selected default | Rationale and limit of evidence |
| --- | --- | --- |
| Encoded and decompressed bytes | 64 MiB each | Retain finite headroom above measured fixtures; actual expansion is checked, not trusted hints |
| Rows / cells / headers | 100,000 / 1,000,000 / 128 | Numeric calibration reaches the row ceiling and 800,000 cells; header and cell/text maxima still need representative shape calibration |
| Per text / header bytes | 1 MiB / 1 KiB | Independently boundary-tested; current mixed fixtures are short-text, not proof of optimal text-heavy defaults |
| Accounted allocation per batch | 128 MiB | Covers wire/output coexistence and conservative estimates; requested-heap probe excludes transport and C allocation |
| ZSTD window | 8 MiB plus 16 MiB workspace allowance | Typed window-limit rejection; C workspace allowance is conservative, not allocator enforcement |
| Decode jobs / shared application budget | 2 / 512 MiB | Finite jobs and retained reservations pass queued/running cancellation; budgets are ceilings, not eager allocations |
| Concurrent typed queries | 16, overrides 1-64 | Eight-stream calibration and saturation/cancellation tests; 16 is a conservative starting cap, not a measured optimum across 16/64 active queries |
| HTTP/2 stream / connection credit | 2 MiB / 34 MiB | Connection credit = (query cap + 1) * stream credit, adaptive growth disabled; prevents paused queries starving active ones on the typed connection |
| Inline scheduling | 0; explicit option up to 4096 | Retains the prior portable decision; this experiment forces offload |

HTTP/2 credit is neither allocated storage nor an RSS ceiling. Buffered HTTP/2
frames, tonic copies, TLS/socket state, C allocations and caller-held batches
require separate profiling. A separate raw channel protects typed flow control;
using both APIs may establish two connections. The constructor's primary channel
connects eagerly and the other lazily. No auth/refresh/store behavior changes.

## Retained exploratory evidence

Initial candidate `fbfb3c20aa95bffe9d50f8c4e787b8f17651813d` completed eight normal
Windows reports before the slow-consumer stall. The unchanged control reproduced
the stall. `acceptance-stall-observations.json` explicitly records transcript
observations, not raw benchmark timing; `acceptance-windows-control-stall.log`
retains the reproduction error. Neither failed run produced a completed JSON
measurement report. The selected source then passed all 24 Windows/Linux reports.

A subsequent function-isolation experiment at
`792ac197d0d2e932010ee49335d7941775e958b8` moved the ZSTD loop/scratch buffer behind
a non-inlined function boundary. Eight normal Windows reports and two larger
confirmation reports passed, but numeric null NONE decode still regressed
11-13%, and timestamp decode regressed roughly 8-10%. It did not solve the
observed regression and was reverted. Its reports and Windows executable remain
retained; the compiled Linux variant was not measured. No causal compiler/layout
explanation is claimed from this unsuccessful hypothesis.

## Reproduction and artifact retention

Use the same locked dependencies, host/toolchain and common v3 harness for both
revisions. Build in separate target directories and retain executable copies
before timing. Run from the corresponding source checkout so Git provenance
identifies the code under test. WSL needs GIT_DIR/GIT_WORK_TREE set explicitly
for the Windows-created control worktree; the retained Linux script does this.

```text
cargo build --release --locked -p thetadata-client --example eod_benchmark
eod_benchmark run --engine numeric-offload --rows 10000 --samples 12 --warmups 3 --streams 4 --output normal.json
eod_benchmark run --engine numeric-offload --rows 100000 --samples 4 --warmups 1 --streams 8 --consumer-delay-ms 10 --output slow-numeric.json
eod_benchmark run --engine table-offload --rows 50000 --samples 4 --warmups 1 --streams 8 --consumer-delay-ms 10 --output slow-table.json
eod_benchmark compare --baseline before.json --candidate after.json --output comparison.json
```

Repeat normal runs A/B/B/A for both APIs, then both slow-consumer workloads twice.
Do not compare different rows/delays or v2 reports. Files below are retained under
ignored `artifacts/eod/`; a source clone does not distribute these local artifacts.
The control checkout is retained there for reproducibility. SHA-256 identifies
original files; reproducing timing creates new samples and hashes.

| Artifact | SHA-256 |
| --- | --- |
| `acceptance-final-linux-after` | `3438055fb58dd6ae4bd15a17659e08c896f33e953f4913ca134dcdbd5fdf50f2` |
| `acceptance-final-linux-numeric-offload-after-1.json` | `f3b9ab4eef28768f8ad70cb5dd34c77a9f6775ecc3fee8ebc92c64584ad8e546` |
| `acceptance-final-linux-numeric-offload-after-2.json` | `b0d07374779fc0ce0f0b3139aea49bdc9ad07be747ddacf2de219124b4f0eab4` |
| `acceptance-final-linux-numeric-offload-before-1.json` | `0c790fec79c1dc6502dd9633ed29d6ed4c664dddf5cec9dc83d894ccdc95027e` |
| `acceptance-final-linux-numeric-offload-before-2.json` | `fd093a87d83a3f4dfe152a50548e94bf48959c336935985e6d9f4b42b353db44` |
| `acceptance-final-linux-numeric-offload-comparison-1.json` | `18b3dd4fc8312b881bb4a893983e7d5c5e14749d162a3a1234a75e34cfab16e0` |
| `acceptance-final-linux-numeric-offload-comparison-2.json` | `be97a5ca75b023d5ab398347bb4d7a7f20d5b3ad2d602fb84297348f7f879a29` |
| `acceptance-final-linux-slow-numeric-offload-1.json` | `3e2f00ff86fea3fb6b45010bcced1f44777d3fdc692deda3b28862bd1c804b01` |
| `acceptance-final-linux-slow-numeric-offload-2.json` | `1c69027c04df959de4e63d8a49b0db772480cee1951a3822f814c9772d340d00` |
| `acceptance-final-linux-slow-table-offload-1.json` | `8138ff0d3d56676c3c22d8783ad82eb3d701aa2430bf99b337bc5c7859383a47` |
| `acceptance-final-linux-slow-table-offload-2.json` | `f31ba625ee32797e3d17f08fda225e79af95165504cc5982765f129e1e70f010` |
| `acceptance-final-linux-table-offload-after-1.json` | `2f5b8eb1552cd784276509d0b67f44bf3ab25440a81b91a854254ce22c4de904` |
| `acceptance-final-linux-table-offload-after-2.json` | `a6ecb208fc91b2e2e4f3ed99ba671c3597dd46eb3b3af0878a641cfdf543598f` |
| `acceptance-final-linux-table-offload-before-1.json` | `ff60c610fe934eb7d1a92a93e149fd060751fb08c4fdcdb6b0410c81acae5208` |
| `acceptance-final-linux-table-offload-before-2.json` | `235e18af2221aad05a22875856724d9df5e325d482ca79048a2b3399cb7b9ba5` |
| `acceptance-final-linux-table-offload-comparison-1.json` | `30e6df98c00d0daefc6224fdc46760bae09c4a2f444e0a1a4ab6cb9909c81d8e` |
| `acceptance-final-linux-table-offload-comparison-2.json` | `df13502ab8083abaaa9bd7fde36da36b8694c4d046a53db232b3dc9df902eb19` |
| `acceptance-final-windows-after.exe` | `a8de0a2465b5ab602be1caea6adc9b6cb301171290cf0cd98dfef0df6fb25bbc` |
| `acceptance-final-windows-numeric-offload-after-1.json` | `d85f2dd5f18b333a8fb59c2d266ea524f88e85630dbb6a73d2f4d783c7816a59` |
| `acceptance-final-windows-numeric-offload-after-2.json` | `7ca3e2679e9813a47f190fd4f092ffdd4a42eccbc723e882de502ff4032a1187` |
| `acceptance-final-windows-numeric-offload-before-1.json` | `8ee860ad96253fcfa985df87d46e1bd5ee53b7537af8a6f380967307393c3c17` |
| `acceptance-final-windows-numeric-offload-before-2.json` | `bb43b2df3a309945d1051e2698d8adfaf15a9f264396bc9d9d17301cd7f98a58` |
| `acceptance-final-windows-numeric-offload-comparison-1.json` | `cdd61ebaf7026e5f65f2307aeca24b6cd5d7f2ee25378f70f0cc0f474b0ba280` |
| `acceptance-final-windows-numeric-offload-comparison-2.json` | `81a297490fa3a73d94dc9a7b1ae56472be99a9b36248ca4547b50d5d27a17443` |
| `acceptance-final-windows-slow-numeric-offload-1.json` | `70ec64be34834dcb08784dd8ea058f8dfecb825bd0a5059dabcf5a52892fca71` |
| `acceptance-final-windows-slow-numeric-offload-2.json` | `7b6c654b8a024f3b5cc51ddb00aeecb1e4633729b5aeba04cd814854671d8c31` |
| `acceptance-final-windows-slow-table-offload-1.json` | `4fec7887afa66a3bf01917eb2beb0b3c9ce37ffed8d0e001a342cd90276dd89c` |
| `acceptance-final-windows-slow-table-offload-2.json` | `c40270f3d3bc15ae68c9a28798e831900082be825b9b69930a9b466b607f9570` |
| `acceptance-final-windows-table-offload-after-1.json` | `8079455efcb07a32e5cb2d6ccd3b310f51a9339adfa6b2f0d58fd299501b28f0` |
| `acceptance-final-windows-table-offload-after-2.json` | `27902aa702740b41f0e98652edbed1cddaf945ac93c3f7d617693e4f9a962aa3` |
| `acceptance-final-windows-table-offload-before-1.json` | `803cfe0ca11547e5bb853ffd1cbc7770fd3a8a0855f1b0dcc9f4962b20ad6c62` |
| `acceptance-final-windows-table-offload-before-2.json` | `28921d805a676e2832fd11dd8a2d7750955b1633dfb68951d918f43e70be5f22` |
| `acceptance-final-windows-table-offload-comparison-1.json` | `c46167dbc34dc4dde7530e62cf99265b4ffb47d8286487ec570665a9548c9f89` |
| `acceptance-final-windows-table-offload-comparison-2.json` | `04daa17fd240f29038f5f35f49784dbd77a98d7e157eb94331a7f08dddf3171b` |
| `acceptance-isolated-windows-after.exe` | `7b37d4b60c5fbef8c362a5d6b1c75f483e9f9c58a4c18394a8bb561a5a8f3a15` |
| `acceptance-isolated-windows-numeric-offload-after-1.json` | `fd3d35ad67f1fbdf641ba7f6717f5deb2e0ddf8edc3cb4b2ab021e4aebbdb044` |
| `acceptance-isolated-windows-numeric-offload-after-2.json` | `0eeada844eeff515d79b5906b7de9231daa0cecc9693550b0bd90de452751493` |
| `acceptance-isolated-windows-numeric-offload-before-1.json` | `73bda3dc0613c557ce645600dcbac9f705a431eefaab07cc3bbac4ee0884a6a6` |
| `acceptance-isolated-windows-numeric-offload-before-2.json` | `75e41a1d1ed0169a0df08ece46eb6bc2a539f690c7b25428cd8265ded6d281c9` |
| `acceptance-isolated-windows-slow-numeric-offload-1.json` | `01a16ed86ca2c506a07d6a3152f95301edaa8d70bcb544c2ef574d14be95830f` |
| `acceptance-isolated-windows-slow-table-offload-1.json` | `b7930c6c192bb5123095449fcb64730920ed896aa22f82491a2836dc8cdc7537` |
| `acceptance-isolated-windows-table-offload-after-1.json` | `d28d77171fd666772827d7a3cc4f4887f7884bbc1cdc788ff97deed76bec5433` |
| `acceptance-isolated-windows-table-offload-after-2.json` | `9a60cd5ea594d089a62a1ac91b058f449dd46989714461430ec25808436b5acf` |
| `acceptance-isolated-windows-table-offload-before-1.json` | `35313171d0b507f8a20de8a724d8bb01b2773f6b98b38016bb1217cac631ad06` |
| `acceptance-isolated-windows-table-offload-before-2.json` | `721fd87870b0bdf883454be590e3a545ad95d98b681fe62232f1ffa0cba0f8af` |
| `acceptance-linux-before` | `64a4b18db7c7645e2d18ec1a8d113ce158efa6ddfa8f8c646b47cfa6d91ba5ed` |
| `acceptance-stall-observations.json` | `598624f500e13c0a1061399256be5ec061222a598d26e76864247e6ae387fef4` |
| `acceptance-windows-after.exe` | `9b12ecaf696b97d92a3ad22e352015e4e217e21b9919694bc83e04dbd9ca4f46` |
| `acceptance-windows-before.exe` | `689a4acfe4371ad1e25d813b0eb67f440bb28b449c570d4e0f8972a2c2550a8e` |
| `acceptance-windows-control-stall.log` | `a8341c9f58589d219f3beeac7f6cfcb1feeea3117e234f0666da61ae7149b965` |
| `acceptance-windows-numeric-offload-after-1.json` | `175dd1408848fcfc2a6669ef8f326a064496356f2be3b97325f0f4cd730d015d` |
| `acceptance-windows-numeric-offload-after-2.json` | `ee5206340a72bfafabd3d4dfe4f1618517297da836e93bc567a6c55a21cb8959` |
| `acceptance-windows-numeric-offload-before-1.json` | `1b6b9188eb4e920d982e5bd8a119e8f61c5da3eab225808f7253654efe00b5be` |
| `acceptance-windows-numeric-offload-before-2.json` | `2eecb463de9439f51ec6d73b954a3c13c563ade1ab20a4e07bfb4f7c538f5dd6` |
| `acceptance-windows-table-offload-after-1.json` | `6bc4622e7c01de29c9b89aa61439d0e7971d4bab0e9b2efa50e8310fd06fe2ac` |
| `acceptance-windows-table-offload-after-2.json` | `7f8455fea2c4861e6ad8e439496748900c79a10f623ee67bbd4b71279b85f7da` |
| `acceptance-windows-table-offload-before-1.json` | `564e7f231c9a0af676c6c16c46509235659f5dbeeee2bf5205a49aea14de6a65` |
| `acceptance-windows-table-offload-before-2.json` | `de66e22e430d0ea0d02aebc641f51c69402205aeec8957a42df998473d806dc4` |
| `run-acceptance-final-linux.sh` | `8a7f30474c5173f54cad3c8d17b5947daedaf9f72240ff30f037f96de7565d8d` |
| `run-acceptance-final-windows.ps1` | `04de6bb22bd4221dd0289b552ba57767e9588dd69424fe4bfc1448a7a25f64b9` |
| `run-acceptance-isolated-windows.ps1` | `f77b1aabaf6be96c8842da80075d65cab762dc00981aee5f8e183d86507c8cd2` |
