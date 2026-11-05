# Vendor Python baseline harness (performance plan C.1)

This harness runs the **official `thetadata` 1.0.17 package** to measure the
baseline every Praevis performance claim is compared against. It exists under
the single scoped exception in [AGENTS.md](../../AGENTS.md):

- It runs **only manually, by the Coordinator, on the user's machine**, with the
  user's own key from `THETADATA_API_KEY`. It never runs in CI or from `tools/`,
  and it is not part of the Cargo workspace or any package.
- It installs from the hash-pinned [`requirements.txt`](requirements.txt) into
  an isolated virtual environment (`--require-hashes`, wheels only). The
  `thetadata` wheel must match SHA-256
  `a43a2fe0fc4896047b9287a9cbf725214386566e3adf930523e5f8aeabf81fea`.
- It writes timings and counts only. Raw captures (with `--save-captures`) stay
  under the ignored `artifacts/` directory and are never committed or shared.

## Log guard: the session token never reaches a log

`thetadata` logs its whole auth response, including `sessionId`, at INFO, and
rejection bodies at ERROR (research: "Logging hazard"). Before the package is
imported, [`log_guard.py`](log_guard.py) silences the `thetadata` logger
hierarchy (above CRITICAL, no propagation, `NullHandler`). It also filters out
any record whose source file is inside the installed package, on the root
logger, on `logging.lastResort` and on every handler added later. The harness
self-tests the guard at startup and refuses to run if a fake token line would
be emitted. It also refuses to write a report that contains the account email
or the session token. The guard's own tests need no vendor package:

```powershell
.\.venv\Scripts\python -m unittest test_log_guard
```

## What it measures

Workloads come from [`workloads.json`](workloads.json); W1 and W2 are FREE-tier
(performance plan B). For each request it records:

| Mode | Measures |
| --- | --- |
| `e2e` | The public wrapper call returning a Polars and a pandas DataFrame: network + server + conversion |
| `drain` | The same request through the package's own generated gRPC stub, consuming the stream without conversion: network + server, time to first message, message count and bytes |
| `convert` | The drained responses replayed offline through the package's converter (Polars and pandas), repeated: CPU only |

Rounds alternate the order of `e2e` and `drain` (AB/BA). Network requests are
paced (default one per 3.5 s, under the documented FREE limit of 20/min). The
run stops at the first `RESOURCE_EXHAUSTED` or any status other than
`NOT_FOUND`, and keeps the results so far.

## Run it (Windows PowerShell, from the repository root)

Requires Python 3.12 or newer (`thetadata` 1.0.17 declares `>=3.12`).

```powershell
cd bench\python-baseline
py -3.12 -m venv .venv
.\.venv\Scripts\python -m pip install --require-hashes --only-binary=:all: --no-deps -r requirements.txt
.\.venv\Scripts\python -m unittest test_log_guard
# THETADATA_API_KEY must already be set in this shell (it is never printed).
.\.venv\Scripts\python baseline.py --workloads W1,W2 --rounds 3 --save-captures
```

If W2 stops on `INVALID_ARGUMENT` (expiration wildcard not accepted), rerun with
the fixed-expiration fallback: `--workloads W2b --rounds 3 --save-captures`.

Linux/macOS: `python3.12 -m venv .venv`, then `.venv/bin/python` in place of
`.\.venv\Scripts\python`.

## What to send back

The summary lines printed at the end and the JSON under
`artifacts/python-baseline/py-*.json`. Both contain timings, counts, sizes,
package versions and host information only, with no keys and no row values.
Keep the `.bin` captures private; their SHA-256 values are in the JSON.

## Regenerating the lock

```bash
uv pip compile --universal --generate-hashes --python-version 3.12 requirements.in -o requirements.txt
```

Keep `thetadata==1.0.17` unless a newer version is approved through the same
process.
