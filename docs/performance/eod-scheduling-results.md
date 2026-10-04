# Bounded EOD scheduling results (2026-10-04)

Measured clean source: `d0d925a2477af209e8d492d9dfaeaba9940eafff`.
This follows the [resource audit](eod-resource-audit.md). The decision is to
keep `EodPolicy::inline_bytes = 0` as the portable default and offer 4096 as
an explicit optimization. Linux/WSL small numeric delivery improved markedly;
Windows results were mixed. One WSL host does not justify an OS-specific default.

## Method and decision boundary

Each platform used one release binary, identical fixture bytes and exact-value
oracles, three warmups, twelve samples, eight columns, 10,000 rows (16 for small
mixed), four streams and three batches per stream. Run order was Table offload,
Table inline, numeric offload, numeric inline, numeric inline, numeric offload,
Table inline, Table offload. Windows finished before Linux; builds and timing
did not overlap. Windows used Rust 1.99.0 MSVC/LLVM 23.1.1, Ryzen 7 2700 and
16 logical CPUs. Ubuntu WSL2 used Rust 1.98.1 GNU/LLVM 22.1.8, eight exposed
CPUs and an executable on native `/tmp` storage. These are within-platform
comparisons, not a controlled comparison between operating systems.

The `table` and `numeric` benchmark engines explicitly select 4096; their
`-offload` counterparts select 0. Only actual complete NONE frames within the
threshold decode inline. ZSTD and larger frames always offload. Each pair uses
the same decoder, envelope codec, validation, resource limits and output oracle.
Direct decode timing does not exercise scheduling and must not be interpreted
as an algorithmic improvement between paired engines. Delivery includes local
transport, decoding, exact-value consumer checks and output destruction; setup
and authentication are outside timing. See [metric definitions](eod-baseline.md).

The measured revision defaulted to 4096. After reviewing these results, the
default was changed to 0; the benchmark's explicit settings preserve the
meaning of these measurements. No source revision or artifact is relabeled.
The changed harness/fixture fingerprints prevent treating earlier baseline or
numeric reports as equivalent scheduling comparisons. No latency gate is set.

## Small uncompressed batches

Milliseconds, **run 1 / run 2**, using the harness's nearest-rank median.
First-batch latency is separate from completion of all twelve delivered batches.

| Platform | Engine | First batch | Delivery |
| --- | --- | --- | --- |
| windows | table-offload | 0.563 / 0.793 | 1.028 / 1.253 |
| windows | table | 0.660 / 0.662 | 1.147 / 1.139 |
| windows | numeric-offload | 0.728 / 0.721 | 1.106 / 1.114 |
| windows | numeric | 0.807 / 0.608 | 1.313 / 0.999 |
| linux | table-offload | 0.820 / 0.468 | 1.609 / 1.295 |
| linux | table | 0.431 / 0.589 | 0.728 / 0.898 |
| linux | numeric-offload | 0.558 / 0.696 | 1.304 / 1.641 |
| linux | numeric | 0.403 / 0.364 | 0.634 / 0.569 |

Linux numeric delivery decreases from 1.304/1.641 ms to 0.634/0.569 ms, about
51-65% lower (2.1-2.9 times faster). Linux Table delivery also improves.
Windows numeric and Table results change direction between repetitions;
neither establishes a reliable portable improvement. Raw samples, including
outliers, remain in the reports. With twelve samples, nearest-rank p95 is the
maximum, so these runs cannot establish tail-latency guarantees.

## Controls and variability

These controls always use workers under both policies. Delivery medians in ms,
**run 1 / run 2**; differences therefore do not establish an inline-policy effect.

| Platform | Case | Table offload | Table inline policy | Numeric offload | Numeric inline policy |
| --- | --- | --- | --- | --- | --- |
| windows | small-mixed-zstd | 1.927 / 1.873 | 1.861 / 1.775 | 1.555 / 1.611 | 1.648 / 1.635 |
| windows | large-mixed-none | 119.418 / 126.398 | 126.657 / 126.012 | 82.636 / 78.564 | 80.439 / 79.025 |
| windows | timestamps-none | 177.497 / 182.919 | 178.702 / 182.388 | 46.132 / 46.754 | 46.878 / 47.690 |
| linux | small-mixed-zstd | 1.677 / 1.465 | 1.579 / 1.621 | 1.406 / 1.444 | 1.381 / 1.346 |
| linux | large-mixed-none | 137.412 / 105.363 | 162.344 / 99.666 | 66.303 / 66.206 | 66.885 / 64.729 |
| linux | timestamps-none | 172.314 / 171.332 | 314.772 / 174.450 | 42.996 / 42.697 | 40.706 / 44.216 |

Linux Table timestamp delivery includes a 314.772 ms median versus 172.314 ms
in the corresponding offload run, despite both using offload for this case.
Large mixed Table timing also varies substantially. Retain this variability;
do not select only favorable runs or attribute it to small-work scheduling.
The reports include direct decode, first-batch, batch-wait, allocation and
executor timer-delay observations for all NONE/ZSTD shapes. The guarded
comparison files summarize direct decoding; the delivery tables above use the
raw reports and are the relevant evidence for this scheduling decision.

## Reproduction and retained artifacts

Build the measured revision in an isolated checkout using the recorded
toolchain. Copy its release executable before measuring. For each platform,
run the eight engines in the order above, with run suffixes 1 then 2:

```text
cargo build --release --locked -p thetadata-client --example eod_benchmark
eod_benchmark run --engine numeric-offload --rows 10000 --samples 12 --warmups 3 --streams 4 --output scheduling-PLATFORM-numeric-offload-1.json
eod_benchmark run --engine numeric --rows 10000 --samples 12 --warmups 3 --streams 4 --output scheduling-PLATFORM-numeric-1.json
eod_benchmark compare --baseline scheduling-PLATFORM-numeric-offload-1.json --candidate scheduling-PLATFORM-numeric-1.json --output scheduling-PLATFORM-numeric-comparison-1.json
```

Repeat comparisons for Table and both repetitions. These are Rust executables,
with Rust fixtures and allocator instrumentation; no Python timing harness is
involved. Retained local files are under ignored `artifacts/eod/`. All sixteen
reports record a clean measured revision; all eight guarded comparisons passed.
The following SHA-256 inventory includes reports, comparisons and both binaries.
These hashes identify retained local evidence, not a claim that ignored files
are distributed with a clone. Reproduction generates new timing samples.

| Artifact | SHA-256 |
| --- | --- |
| `scheduling-linux` | `ba7ff793d8e8edf82fcc16a4dac7c33b0799171a8c5fad28124015471c48c4ec` |
| `scheduling-linux-numeric-1.json` | `556eb82855e68365b12a104cb7f4f40d717d332b3fad7f3c6c4adcd26661d13a` |
| `scheduling-linux-numeric-2.json` | `2ae7cf9df18bb1347838c235fd755a19e2aae67353271be6f44106b9878fb7cc` |
| `scheduling-linux-numeric-comparison-1.json` | `7fe0c9771a04ebd275fa2c24c3bc3755df59c12523c7f02cd423fd3f8dc51a0d` |
| `scheduling-linux-numeric-comparison-2.json` | `6bf2c626ba53c908bc06a14515f62591983c0adbf46a3e3415f1a1f18b186ca5` |
| `scheduling-linux-numeric-offload-1.json` | `4edd49a7dd53cf13b5bfb618fcfe55d3fb4deb07519455fd63207ce6e9da414a` |
| `scheduling-linux-numeric-offload-2.json` | `5e6d21bb5e8d619df24c864c1ae02fc19df6ec89a29262792244ae26c25feab4` |
| `scheduling-linux-table-1.json` | `9590c8a0e8f84b8f4c06021a70bb82a68b0fcd62220c06dcf4ee108ae33fcc0d` |
| `scheduling-linux-table-2.json` | `4460082a8af58e180861a39acb4660a7326db49dd33a1f77d78f78131033076a` |
| `scheduling-linux-table-comparison-1.json` | `2217710b1e8c76f5f28584aa44df2fa6c2896cca18abe42bbd085b4b4b20c002` |
| `scheduling-linux-table-comparison-2.json` | `ff10c22ec3ed010ee61244f6df09d0853d13177b3c98d4887bc7b31d645c2a58` |
| `scheduling-linux-table-offload-1.json` | `c505d310c78694d217fb1e4e5b9e4bc8c71382057d5240abcd5551f440cb7bda` |
| `scheduling-linux-table-offload-2.json` | `37d192b52dbca622b1b9a06227260d082418d98fc0bd5b7eab6bc848d02982d7` |
| `scheduling-windows-numeric-1.json` | `426936684fe72d3fbc347607e4e3652306c33ce0d1c62dc1c45719de8a5cb8e0` |
| `scheduling-windows-numeric-2.json` | `030bee2e94dd8471fb8919c6f87b5a1ef1bc945fef2d368bc79d5cb55f3ae38a` |
| `scheduling-windows-numeric-comparison-1.json` | `65b617173aa590d024cc39ee1def83c40b6b501470be9c8f1f9ed8e801375744` |
| `scheduling-windows-numeric-comparison-2.json` | `bd3d9cfc536c903ad6d50873f0a4b27933a99f557ffb0ecfd93371de9604ab51` |
| `scheduling-windows-numeric-offload-1.json` | `7660c7bc543bf6cfdaaa8abd9b2b3389e9d08fa256d2063bd228e9d638a72536` |
| `scheduling-windows-numeric-offload-2.json` | `0f7a95521e524418c59b7a2cb6a3e51f1c498f9e0d227a4f5579b40c181afc94` |
| `scheduling-windows-table-1.json` | `1597d0ca2e84cff16a449dd14ea906ff47727ecbf1a33d2a3d0d1c5695f76b1b` |
| `scheduling-windows-table-2.json` | `b28dc44378fc2a3810c8a78863bcf6e1b384fa05bcf555aeb39f32822613ee4d` |
| `scheduling-windows-table-comparison-1.json` | `f0c931e68f468d11be3306d1c9ca9d031e943124ef5c9171a8d9793a89511028` |
| `scheduling-windows-table-comparison-2.json` | `04d0c5c9cbce28cf4fa7c35b5489e72c8725387018b62872e40b7565202b7a1d` |
| `scheduling-windows-table-offload-1.json` | `4fd99c81d3719474b7b0a549b34de77d55f4861a91f67e735056ff28912044d6` |
| `scheduling-windows-table-offload-2.json` | `c2229dbfbaec95a96143f3f67561a9602e81b98adb32afcec8bc77036aa24d15` |
| `scheduling-windows.exe` | `5148f41a7532e3de7d94cff3d84fdafeeefec4f42bb28077269b1ed6c6a96616` |
