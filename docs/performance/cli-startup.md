# CLI startup evaluation

Scope: roadmap recommendation #2; AUTH-L2-010, AUTH-L2-011, AUTH-L3-019 through AUTH-L3-024. Decision: [ADR-0006](../adr/0006-cli-runtime-lifecycle.md). These are synthetic local measurements, not vendor latency guarantees.

## Change and acceptance

Argument parsing, help/version, status, and logout now execute synchronously. Only HTTP authentication creates a current-thread Tokio runtime with I/O and timer drivers; the runtime ends before native persistence and output. The async auth library and its dependencies are unchanged. Existing CLI behavior, record format, and exit codes remain the functional acceptance criteria.

A runtime is necessary to drive the existing async HTTP client. A multithread worker pool is not necessary for this sequential command. This change removes startup work; it does not accelerate the server, TLS, or credential provider. Dependency-managed helper threads (for example DNS) remain possible.

## Reproduce a before/after comparison

Use two source checkouts, the same Rust toolchain/build environment, and release builds. The helper builds before copying a binary and records its source fingerprint, Git revision/dirty state, Rust toolchain, build command, and executable SHA-256. The source fingerprint includes workspace manifests/lockfile and Rust/protocol inputs. Build outputs are isolated under each checkout's `target/cli-benchmark/<os>/`, overriding an inherited shared `CARGO_TARGET_DIR`; this avoids stale top-level executables when switching checkouts and separates Windows/WSL caches. The comparator verifies each executable against its recorded hash before running it. The Rust crate version need not change for this experiment.

PowerShell, with the candidate checkout as the working directory and a baseline checkout at `artifacts/cli-startup/baseline-source`:

```powershell
python tools/build_cli_benchmark.py --source artifacts/cli-startup/baseline-source --output artifacts/cli-startup/before.exe
python tools/build_cli_benchmark.py --output artifacts/cli-startup/after.exe
python tools/compare_cli_startup.py --before artifacts/cli-startup/before.exe --after artifacts/cli-startup/after.exe --before-provenance artifacts/cli-startup/before.json --after-provenance artifacts/cli-startup/after.json --samples 100 --output artifacts/cli-startup/comparison-1.json
python tools/compare_cli_startup.py --before artifacts/cli-startup/before.exe --after artifacts/cli-startup/after.exe --before-provenance artifacts/cli-startup/before.json --after-provenance artifacts/cli-startup/after.json --samples 100 --output artifacts/cli-startup/comparison-2.json
```

On Linux, build the executables without `.exe`, then prefix each comparison command with `bash tools/with-test-keyring.sh`. That wrapper provides a private D-Bus session and disposable persistent keyring. Build and test first; do not compile or run unrelated workloads during measurements. Under WSL, additionally compare with `--stage-binaries`, which copies both executables byte-for-byte to the host temporary filesystem before timing and records that execution mode. Compare original-path and staged results separately.

The **Auth performance** workflow accepts an optional `baseline_ref` during manual dispatch. It checks out that baseline and the selected candidate ref separately, builds both on each Windows/Ubuntu runner, executes two comparisons, and uploads JSON reports. Without the input, its existing single-version baseline remains unchanged. A local branch must be pushed before it can be evaluated in hosted Actions.

Each comparison uses 100 measured samples per binary/case after five warmups, alternating before/after and after/before ordering. It removes inherited ThetaData credentials, proxy settings, and Tokio worker overrides, and uses a temporary working directory without credential files. The identity fixture is stopped before local-command measurements. Cases are auth/save, saved-session status, missing-session status (exit 3), absent-record logout, help, and version. Logout timing measures the idempotent absent case; the native persistence test separately verifies successful deletion. All created profiles are unique, and cleanup is attempted for every profile even after failure.

Reports retain raw samples, p50/p95, minima/maxima/means, runtime/host context, build provenance, median speedup ratios, and percentage reductions. Functional failures fail the run; a slower measurement remains a reported result, not a fabricated success or guessed SLA failure.

## Windows measurements: 2026-10-04

Host: Windows 11 build 26200, 16 logical processors, Python 3.12.10, rustc 1.99.0, release defaults, Windows Credential Manager. Before: clean commit `0caf272ac398b97862f53ec5277f2c2d61ffe550`; after: working implementation on `perf/cli-startup`. The original binary was built/preserved before editing the CLI; its sidecar records that clean state. Later benchmark reports correctly mark the working tree dirty because new tooling was present.

Executable SHA-256:

- Before: `18b233683951d2d75ef696b3c82560a91a1f04753effd2a196bdc7a8adc58d8a`
- After: `c445882d7a6498bfe71c434c13871570bd9a32b14353c899ba5a504da59fc703`

The table uses the confirmation run (`comparison-3.json`, 12:21:45 UTC); the final column also includes the first run (`comparison-1.json`, 12:18:21 UTC). Each contains 100 samples per binary/case. Values are milliseconds.

| Operation | Before p50 | After p50 | Before p95 | After p95 | Median reduction across two runs |
| --- | ---: | ---: | ---: | ---: | ---: |
| Authentication + save | 31.875 | 30.112 | 35.328 | 44.045 | 5.5–6.0% |
| Persisted status | 22.205 | 19.858 | 49.352 | 22.870 | 10.6–10.9% |
| Missing status | 21.421 | 19.062 | 48.932 | 46.010 | 11.0–11.2% |
| Absent-record logout | 21.617 | 19.245 | 26.221 | 48.259 | 10.3–11.0% |
| Help | 18.941 | 17.238 | 20.463 | 19.060 | 9.0–9.7% |
| Version | 18.836 | 16.998 | 20.681 | 18.860 | 9.5–9.8% |

All measured medians improved in both runs, generally by about 1.7–2.4 ms. Tail latency did not consistently improve: auth/save and absent logout p95 worsened in the confirmation run, and version p95 worsened in the first run. This evidence supports a modest startup improvement, not a tail-latency guarantee.

An additional comparison (`comparison-2.json`) and the chronological post-change baseline overlapped compilation/test activity; retain those artifacts but exclude them from the controlled comparison. The original single-binary benchmark was also run before the source change and afterward. Its non-interleaved readings should not be used to subtract process/keyring costs or claim a causal speedup under changing host load.

Raw local reports, both executables, and provenance sidecars remain under ignored `artifacts/cli-startup/`. This checked-in evaluation preserves the key evidence; the hosted comparison workflow retains complete JSON reports for 30 days. Repeat comparisons on deployment hosts before introducing a latency target.

## Linux/WSL measurements: 2026-10-04

Host: Ubuntu/glibc 2.39 on WSL2 kernel 6.18.33.2, 8 logical processors, Python 3.12.3, rustc 1.98.1, release defaults, disposable GNOME Keyring/Secret Service. Compare within this platform; its toolchain and storage provider differ from Windows.

Executable SHA-256:

- Before: `c43caf36a7441537431d6dcc55f897fc60a0f1b602375ae0d821ddf1b97f5993`
- After: `505bdea33fb6e72d4a5ebf04b69cdb532da9612e3e007200c176ba02264be74f`

The initial shared build-cache attempt returned a stale candidate executable with the baseline hash. It was rejected before timing. Both versions were then built with isolated output directories; a fresh baseline rebuild reproduced its original hash exactly. The final candidate also passed the zero-worker-configuration startup check.

Executing from the Windows-mounted workspace produced large regressions, especially help/version (roughly 4.6 ms before versus 20–22 ms after). The fresh-build diagnostic reproduced this behavior. These reports remain in `linux-comparison-1.json`, `linux-comparison-2.json`, and `linux-isolated-comparison.json`; do not discard them or describe the change as universally faster on Linux.

To separate execution-location effects, both unchanged executables were copied to native Linux `/tmp` and compared twice with five warmups and 100 samples per binary/case. This is now reproducible with `--stage-binaries`. The table uses the second native-filesystem run; the range includes both runs. All times are milliseconds.

| Operation | Before p50 | After p50 | Before p95 | After p95 | Median reduction across two runs |
| --- | ---: | ---: | ---: | ---: | ---: |
| Authentication + save | 85.089 | 89.654 | 109.963 | 108.610 | -5.4–1.0% |
| Persisted status | 14.658 | 12.877 | 15.724 | 14.349 | 11.5–12.1% |
| Missing status | 13.521 | 11.821 | 15.487 | 13.645 | 12.2–12.6% |
| Absent-record logout | 13.988 | 12.294 | 15.512 | 13.596 | 11.5–12.1% |
| Help | 2.944 | 1.836 | 3.529 | 2.172 | 37.6–41.4% |
| Version | 2.801 | 1.685 | 3.254 | 2.020 | 39.8–40.7% |

Native-filesystem local-command medians improved consistently. Authentication/save did not: it improved about 1% in one run and regressed about 5.4% in the other. The run-to-run variation does not establish a reliable Linux authentication speedup. Executable location materially affected results in this environment; the underlying WSL/host behavior was not profiled. Hosted Ubuntu measurements and deployment-host measurements remain useful before acceptance, especially if executables will run from Windows-mounted storage.

The two full reports are retained in `linux-native-comparison-1.json` and `linux-native-comparison-2.json` under ignored `artifacts/cli-startup/`. They were produced by staging byte-identical executables before invoking the same comparator; the reusable flag was then added for future reproduction.

## Functional validation

Windows and Ubuntu/WSL workspace tests, documentation tests, Clippy, and native persistence tests passed. The added process tests exercise help/version, invalid arguments, and invalid profiles with `TOKIO_WORKER_THREADS=0`; the native process test covers successful authentication, persisted status, deletion, and idempotent logout under that setting. Local commands are also exercised with deliberately invalid inherited credential-file and auth-URL settings. Control-flow review establishes that no async runtime is constructed on local paths; the environment-based tests additionally detect a regression to the default worker pool.
