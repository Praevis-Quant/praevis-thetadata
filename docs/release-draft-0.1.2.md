# GitHub release draft: v0.1.2 (not published)

Status: **draft.** Publishing needs the user's approval
([checklist](crates-io-release-plan.md#012-publication-checklist-and-order-prepared-2026-11-14)).
Nothing is tagged or published yet.

**To fill in at publication:**
- the date;
- the commit;
- the exact wheel file names, from that commit's CI artifacts
  `wheel-ubuntu-latest` and `wheel-windows-latest`. The same two files go
  to PyPI and to this release.

---

**Title:** `v0.1.2`. Not a pre-release: mark it as the latest release.

**Body:**

> **Requests by name, added to `0.1.1`.** praevis-thetadata is an
> independent, unofficial client for ThetaData's market-data service. It is
> not affiliated with, sponsored by or endorsed by Theta Data Inc.

## Added

**`praevis_thetadata_client::dispatch`: build a typed request from its name
and parameters at runtime,** for a program that receives requests as data
(for example `{"endpoint": "option_history_eod", "params": {...}}`) and
would otherwise hand-write a match over every request type.
- **`dispatch(name, params, runner)`** accepts the vendor wrapper name or
  the RPC name, reads each parameter from your `Params` source, rejects
  unknown parameter names, and hands the typed request to your `Runner`.
- **`params(name)`** describes each parameter: its name, kind, whether it is
  required, and the vendor default, as a value (`default_value()`) and as
  written. Passing the default builds the same request as omitting it.
- **`ENDPOINTS`** lists every endpoint with its wrapper name (if any) and
  RPC name; **`stability(name)`** gives its stability.
- **As given, one request:** values are sent as given, as the vendor
  package sends them (the defaults' `HH:MM:SS` time form is canonical), and
  `dispatch` does not split a request. Run it as a job, or split it with
  `plan()` in your runner.
- **Stability is unchanged:** the 19 descriptor-only RPCs are reachable by
  their RPC names but stay unstable.

The `praevis-data` service tried it on `main` before publication; its
findings were fixed first.

**API:** additive. The new types are `#[non_exhaustive]`. Nothing that
compiled against `0.1.1` breaks. The Python package's calls now use the same
generated code, with no change to the Python API.

The full list, including unpublished tooling, is in
[CHANGELOG.md](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.2/CHANGELOG.md).

## Install

```toml
praevis-thetadata-client = "0.1"       # resolves 0.1.2
praevis-thetadata-stream = "=0.1.2"    # optional, real time, experimental: pin it
```

```text
cargo install praevis-thetadata-cli --locked
pip install --upgrade praevis-thetadata
```

The wheels are for Windows x86_64 and Linux x86_64 (glibc 2.28 or newer),
CPython 3.10 or newer. The known gaps of `0.1.0` still apply
([list](https://github.com/Praevis-Quant/praevis-thetadata/blob/v0.1.2/docs/releases.md#known-gaps-shipped-in-010)),
less the one fixed in `0.1.1`.

## Assets

| File | Platform |
| --- | --- |
| `praevis_thetadata-0.1.2-cp310-abi3-win_amd64.whl` | Windows x86_64, CPython 3.10+ |
| `praevis_thetadata-0.1.2-cp310-abi3-manylinux_2_28_x86_64.whl` | Linux x86_64, glibc 2.28+, CPython 3.10+ |
