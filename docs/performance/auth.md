# Authentication performance baseline

Scope: AUTH-L3-018. Initial results are observations, not an SLA or a regression gate.

For the before/after CLI runtime experiment, comparison tooling, and measured results, see [CLI startup evaluation](cli-startup.md).

`tools/benchmark_auth.py` measures two user-visible operations using the actual release CLI:

1. `auth`: process startup, credential discovery from a synthetic environment key, localhost HTTP request/response, JSON parsing, native session save, and JSON output.
2. `auth status`: a separate process with no login credentials, native session load, and JSON status output. The identity fixture is stopped before these measurements.

Each case has two warmup iterations and 30 measured iterations by default. The report includes min, mean, median/p50, nearest-rank p95, max, sample count, timestamp, platform, Python version, Git revision, executable SHA-256, and backend. The script verifies the response and checks that synthetic secrets never appear in CLI output. A unique profile is removed in `finally`. Linux runs inside a disposable keyring; Windows uses a unique synthetic credential entry.

```powershell
cargo build --release --locked -p praevis-thetadata-cli
python tools/benchmark_auth.py --binary target/release/praevis-thetadata.exe --samples 30 --output artifacts/auth-windows.json
```

```bash
cargo build --release --locked -p praevis-thetadata-cli
bash tools/with-test-keyring.sh python3 tools/benchmark_auth.py \
  --binary target/release/praevis-thetadata --samples 30 --output artifacts/auth-linux.json
```

The **Auth performance** workflow runs on pushes to `main` and manual dispatch, on Windows and Ubuntu. It uploads the JSON summaries for 30 days. Runtime/functional failures fail the workflow; latency variation does not. Reports remain build artifacts, not committed credentials or benchmark output.

## Interpretation

The fixture deliberately uses localhost HTTP, not production HTTPS. Results include process scheduling and native keyring work, but exclude internet latency, TLS handshakes to ThetaData, server queues, subscription limits, and market-data throughput. Compare equivalent runners/build profiles/sample counts. A slow hosted runner is not evidence of a library regression. Do not subtract these figures from live measurements as though they were a stable constant.

Before setting a performance requirement, collect multiple revisions/runs, inspect dispersion, define a reference machine and scenario, and agree on a tolerance. Future measurements can separate HTTP client reuse, same-process auth, cold/warm keyring access, and failure paths. Live-service load testing requires an explicit vendor/account policy decision; it is not part of these workflows.
