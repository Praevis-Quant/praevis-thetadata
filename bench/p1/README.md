# P1 exit runners

Manual only. Run by the Coordinator on the user's machine, never in CI (CI
runs `test_p1.py` offline). The key comes from `THETADATA_API_KEY`, or the
other ADR-0020 sources, and is never printed or stored. Reports hold timings,
counts, statuses, schemas and peak memory: no row values, and the writer
refuses a report that contains a credential. Reports and captures stay under
the ignored `artifacts/`. Requests are paced 3.5 s apart, as in P0, and the
client's scheduler keeps FREE's documented limits (1 concurrent request, 20
per minute).

| Script | Measures | Criterion |
| --- | --- | --- |
| `cargo run ... --example live_p0 -- checks` | The 10 FREE-entitled endpoints through `query()`, each compared value by value with the legacy decoder on a second request | a |
| `live_checks.py` | The same 10 endpoints through the Python package's vendor-named methods | a |
| `compare.py` | W1/W2 through the Python package, like the [vendor baseline](../python-baseline/README.md): end to end (Polars, pandas) and conversion only from the baseline's captures | b, c (Python), d |
| `options_ours.py` | Options phase D2: whole W4-W6 workloads through the vendor-named methods into Polars, synchronously and threaded on one client (`--threads 1,8`); the vendor counterpart is `../python-baseline/options_vendor.py` | — |
| `option_checks.py` | Options phase step F: the 47 non-flat-file option endpoints through the vendor-named methods, with the same requests as `live_p0 option-checks`; status, rows and column types | — |
| `options_memory.py` | Options phase: one W5-shaped job (M1-M3, growing windows) in one mode per process, `to_parquet` (streamed) or a Polars frame (held), with peak memory, to test "bounded by configuration" | — |
| `columnar_benchmark --replay <captures> --loopback` | The captures through the full Rust path: gRPC on localhost, `query()` and Parquet | c |

**One session per account (confirmed by `opt-session-001`).** A new login
ends the account's earlier session, whose requests then fail with
`UNAUTHENTICATED` (`SessionEnded`). The runners log in once. `compare.py` shares one session between its Polars
and pandas clients (`existing_authorized_client`), as the baseline does.
Failures record the gRPC code (`grpc_code`).

**Loopback figures are not client cost.** `--loopback` includes the
in-process server and the gRPC transport, which dominate on Windows. The
`transport-drain` engine sends the same messages and receives them without
decoding. Each engine's `client_us_per_row`, its median minus the drain, is
the wall time the client adds on top of the transport. Decoding overlaps
with receiving, so this is not CPU time; CPU per row is the decoder-only
replay (`--replay` without `--loopback`).

`../python-baseline/baseline.py` now also records `peak_rss_bytes`. Both
runners also record `import_peak_rss_bytes`, read after imports and login and
before the first request. It is for a proposed incremental-memory metric, not
for criterion d. For
criterion d, run each workload in its own process for both clients.

## Setup (once, PowerShell, repository root)

```powershell
python -m venv .venv-p1
.\.venv-p1\Scripts\python -m pip install --require-hashes -r crates\praevis-thetadata-py\requirements-test.txt
.\.venv-p1\Scripts\maturin build --release --locked -m crates\praevis-thetadata-py\Cargo.toml -o dist
.\.venv-p1\Scripts\python -m pip install --no-index --find-links dist praevis-thetadata
```

The commands are in the P1 exit PR description and the results record.
