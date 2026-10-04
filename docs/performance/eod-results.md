# Initial synthetic EOD results (2026-10-04)

This is a **before-change baseline**, not an optimization or live-service result.
Production client/core/decoder code is unchanged. See the
[methodology and commands](eod-baseline.md) for measurement boundaries.

Source: clean commit `f45bed207833d27c9c6b7832aa5b8d12a63d8a0a`.
Two repetitions per platform, each with three warmups and twelve samples per
case. Eight columns; 16 rows for small mixed, 10,000 for every other shape.
Delivery uses four concurrent streams, three batches per stream, one client
runtime thread and one independent mock server thread. Builds finished before
measurement; platform runs were sequential. No vendor account was used.

| Environment | Compiler | Host/runtime context |
| --- | --- | --- |
| Windows, OS build 10.0.26200.0 | Rust 1.99.0, MSVC, LLVM 23.1.1 | Ryzen 7 2700 host; 16 available logical CPUs; native Windows executable |
| Ubuntu on WSL2, kernel 6.18.33.2-microsoft-standard-WSL2 | Rust 1.98.1, GNU, LLVM 22.1.8 | Same physical host; 8 CPUs exposed to WSL; executable/build output on native Linux `/tmp` filesystem |

These are independent platform baselines, **not a controlled Windows-versus-
Linux comparison**: compiler, scheduler, virtualized environment and CPU
availability differ. Default Cargo release builds were used. Future comparisons
must also verify matching build flags/environment; the report fingerprints do
not automatically validate every ambient build flag or host configuration.

## Decoder measurements

Median milliseconds per single batch, shown as **run 1 / run 2**. These exclude
input clone and returned-output equality/destruction. Peak requested Rust heap
is from the separate allocation probe, including input cloning; values matched
across these runs/platforms. C ZSTD memory and process RSS are excluded.

| Shape / encoding | Windows decode ms | WSL decode ms | Peak requested Rust heap (MiB) |
| --- | ---: | ---: | ---: |
| Small mixed / NONE | 0.021 / 0.022 | 0.017 / 0.017 | 0.007 |
| Small mixed / ZSTD | 0.101 / 0.050 | 0.022 / 0.022 | 0.127 |
| Large mixed / NONE | 12.657 / 12.770 | 10.058 / 9.461 | 3.952 |
| Large mixed / ZSTD | 12.958 / 14.380 | 9.918 / 9.897 | 4.290 |
| Null-heavy / NONE | 7.778 / 7.777 | 6.398 / 6.573 | 3.141 |
| Null-heavy / ZSTD | 7.926 / 7.858 | 6.650 / 6.693 | 3.317 |
| Timestamp-heavy / NONE | 23.078 / 23.023 | 17.666 / 21.511 | 6.117 |
| Timestamp-heavy / ZSTD | 23.036 / 22.436 | 20.516 / 18.362 | 6.216 |
| Price-heavy / NONE | 10.445 / 10.414 | 8.111 / 8.457 | 4.143 |
| Price-heavy / ZSTD | 12.077 / 11.516 | 8.863 / 8.676 | 4.817 |

Repeat variability is material: Windows small ZSTD differs by roughly 2x and
WSL uncompressed timestamps by about 22%. Twelve samples/two repetitions do
not calibrate an SLA; nearest-rank p95 with twelve observations is the maximum
observation. Retain raw samples and rerun AB/BA comparisons for an actual
optimization rather than selecting the most favorable baseline.

## Consumer delivery and responsiveness

Selected run-1 observations below include mock transport and consumer work.
First batch is the earliest result across four streams. Delivery finishes all
twelve batches, checking every returned cell and dropping each Table.

| Case | Windows first / complete delivery (ms) | WSL first / complete delivery (ms) | Windows / WSL timer-lateness p95 (ms) |
| --- | ---: | ---: | ---: |
| Small mixed / NONE | 0.512 / 0.975 | 0.347 / 0.635 | 14.598 / 1.730 |
| Large mixed / NONE | 29.324 / 224.674 | 27.648 / 205.099 | 129.317 / 129.093 |
| Null-heavy / ZSTD | 9.316 / 113.257 | 7.368 / 93.438 | 97.341 / 93.352 |
| Timestamp-heavy / NONE | 28.874 / 356.575 | 29.367 / 330.952 | 329.378 / 312.038 |

The monitor measures maximum lateness of 1 ms sleeps on the same consumer
runtime, including OS timer resolution, decoding and exact-value consumer
work. Small Windows cases show timer granularity despite sub-millisecond
delivery; this is not evidence of a 15 ms decoder stall. Large-case results
motivate bounded scheduling experiments, but do not isolate decoder blocking
from consumer work or establish an offload speedup.

## Findings and next experiments

- The compressed null fixture is only **84 payload bytes**, yet the probe peaks
  at **3.317 MiB** requested Rust heap. Shape/allocation and ZSTD window checks
  remain necessary; compressed-byte limits cannot establish a memory bound.
- Timestamp-heavy cases perform **120,027 allocation/reallocation calls** under
  NONE versus **40,027** for null-heavy data. Numeric timestamp delivery is a
  useful next experiment. Different wire shapes also contribute; these numbers
  do not isolate formatting cost by themselves.
- Compare numeric/compatibility representations with equivalent result checks,
  then measure bounded inline/offload policies, retained buffer capacity and
  concurrent/slow-consumer workloads. Keep correctness and limit rejection in
  the experiment. No resource defaults or regression thresholds are selected
  solely from these ten synthetic shapes.

An exploratory fixture at `8a6ad76` showed roughly 43 ms Linux first-batch
latency for small messages while decoding took about 20 microseconds. Explicit
TCP_NODELAY on accepted mock sockets removed that delay (about 0.35 ms in the
retained baseline). This is a fixture transport correction, not a production
decoder optimization. Exploratory reports/binaries remain separately named
under ignored artifacts; their fixture fingerprint differs and the comparator
rejects pairing them with these results.

## Retained evidence

All paths below are relative to ignored `artifacts/eod/`. Raw reports include
every sample, source/compiler/configuration information and their executable
hash. These local artifacts are not required for CI; commands and the source
revision reproduce the experiment. The manual workflow retains independently
generated runner artifacts for 30 days, not these workstation samples.

| File | SHA-256 |
| --- | --- |
| `windows-final-1.json` | `38e7b73d7ac14137f601ab86badc08c467c95db594efb4882b76f5c223fd312e` |
| `windows-final-2.json` | `c07832bc9920ced2e3e4e325389019725407343231994f4ae5ae28c6d7d4dc0f` |
| `linux-final-1.json` | `594088993efafbfb28975d612c2047a20d0d7e21efe826a8022184536674f248` |
| `linux-final-2.json` | `a9aee6af3c1bca3354af601dbd00e5b830f4bf27f41fe6e0e9e979aa4f557472` |
| `windows-final-repeatability.json` | `64f8aadc535a98bdca71cedb77d0b119a0107bd1a54194fb7eed406aabc9169d` |
| `linux-final-repeatability.json` | `0396ebe2aa2d8d2d98f91162329c11cc598a10dc5dc63f6078e7003f066a417f` |
| `baseline-final-windows.exe` | `e18979861394cc9f548a43790fa2f3fec555f6aaa88c1837a88d27e47c5fcc4a` |
| `baseline-final-linux` | `8e52b45fdfc1c4f46355049dc08e8d8c704ecc3c4b50838ad50ac2cb38d727a9` |

Production decoder fingerprint:
`e41564a322ef1a46ad02ef6f7dfd41ae3c402e5fd12f6b8f47b425415e6b2212`.
The fixture source fingerprint is
`446f3a8c7d46cd36c2b777dff46f489aa1f7733dfb3649629cc2794acc4c1275`.
Harness fingerprint:
`1050a4b3250133d1e8284d7892da2e9eb73ddfbe443a4cb56cc767a3ce27ba5d`.

Evidence is partial for EOD-L3-024/026/027 and PERF-L3-001/007/008. No numeric
API, calibrated regression gate, complete heap/RSS bound or live compatibility
has been established. The current planned feature/ADR status remains accurate.
