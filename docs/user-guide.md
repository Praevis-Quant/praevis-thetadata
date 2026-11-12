# User guide: platforms, prerequisites, resources and compatibility

For `0.1.0-beta.1`. The [beta.1 scope record](releases.md#beta1-scope-record-user-2026-11-10)
says what is frozen and what is not.

## Supported platforms

| | Supported | Notes |
| --- | --- | --- |
| **Windows x86_64** | Yes | Tested in CI. Python wheel: `win_amd64` |
| **Linux x86_64, glibc 2.28 or later** | Yes | Tested in CI. Python wheel: `manylinux_2_28_x86_64`, which needs at most `GLIBC_2.28` |
| **macOS** | No | Not built or tested. Without the native store the Rust crates may compile, but nothing is claimed |
| **ARM (any OS)** | No | No wheels are built. The Rust crates are not tested on ARM in CI |

**Toolchains:**
- **Rust:** 1.88 or later (`rust-version`, checked in CI).
- **Python:** 3.10 or later (one abi3 wheel). CI installs that wheel into a
  clean environment and runs the package tests on Python 3.10, 3.11, 3.12
  and 3.13, on Windows and Linux.
- **Builds from source:**
  - Windows needs the Visual Studio C++ build tools.
  - Linux needs a C compiler, make and pkg-config: ZSTD always, and a
    vendored D-Bus library with the native store.
  - Neither Python nor `protoc` is needed for the Rust crates.

**Distribution:** the crates are on crates.io as prereleases, so pin the exact
version. The Python package is on PyPI as `praevis-thetadata` from
`0.1.0b1`, a pre-release, so install it with `pip install --pre praevis-thetadata`. There are wheels only
(Windows x86_64, and Linux x86_64 with glibc 2.28 or later) and no source
distribution. The same wheels are attached to the GitHub release.

## Credentials and the native store

**By default nothing is stored.** The session lives in memory, as in the
vendor package ([ADR-0020](adr/0020-in-memory-sessions-and-optional-persistence.md)).
- **The client and the Python package** use in-memory sessions only. They
  pull in no keyring or D-Bus dependency.
- **One session per account:** a new login ends the account's earlier
  session, whose requests then fail with `SessionEnded`. Log in once and
  share the client.

**The native store is optional:** the auth crate's `native-store` feature, and
the CLI's `auth --save`, `status` and `logout`. Its prerequisites:
- **Windows:** Credential Manager. No setup.
- **Linux:** Secret Service over the user's D-Bus session, backed by a
  persistent keyring such as GNOME Keyring.
  - The keyring must be unlocked.
  - Services and containers need an unlocked Secret Service under the same
    OS account. The [README](../README.md#build-and-authenticate) has the
    headless setup.
- **Other platforms:** persistence returns an explicit unsupported-platform
  error. In-memory authentication still works.

## Resource policy

**What is bounded:** the **Rust heap** on `query()`/`job()` and the stock EOD
stream is bounded by configuration (read-ahead, decoders, batch limits and
stream windows).

**Plus up to streams × `encoded_bytes`** for messages being assembled, held
only while they are in flight:
- **Worst case at PRO:** 8 × 64 MiB.
- **At observed message sizes:** about 3 MiB. The largest real message was
  0.36 MiB.
- **After the message:** an idle or stalled stream keeps nothing
  message-sized. This is tested
  ([closure](performance/eod-transport-buffering.md#closure-2026-11-10-under-decision-as-beta1-gate)).

**For streaming a job to Parquet,** the Rust heap is bounded by configuration
(read-ahead, batch limits, stream windows, one row group) plus Parquet footer
metadata of about 13 KB per row group, which grows with the file
([record](architecture/options-phase.md#memory-bound-under-w5-planned): heap
peaks of 103-182 MiB over 7 live runs).

**Not bounded:**
- total process memory (RSS), including the allocator, the Python
  interpreter and libraries;
- results you collect, such as `collect()`, `query()` in Python, or a
  dataframe;
- raw access (`stream_raw`), which uses plain tonic.

**The settings:**
- `SchedulerPolicy` concurrency: the tier's by default, from FREE 1 to PRO 8;
- `RequestOptions`, per call: concurrency, batch policy and timeouts;
- `DecodeLimits` and the arrow `Limits` (`encoded_bytes` defaults to 64 MiB).

## Compatibility with the vendor Python package

**The same:**
- **Methods:** all 81 vendor `ThetaClient` methods, with the vendor 1.0.17
  names, argument order and defaults.
- **Credentials:** the same order (`creds_file`, `api_key`, email and
  password, the environment, `./creds.txt`).
- **Frames:** `dataframe_type` selects `"polars"` (the default), `"pandas"`
  or `"arrow"`.

**Different, by design:**

| Area | Vendor package | praevis-thetadata |
| --- | --- | --- |
| Prices | Floating point (scale 0 becomes NaN) | Exact `decimal128(38, 10)`; a missing price is null. pandas frames are Arrow-backed |
| Timestamps | Python `datetime` objects, built through floating-point seconds; the dataframe library infers the column type | Exact millisecond Arrow timestamps with the zone in the column type (`America/New_York`, or `UTC` where sent) |
| A column null throughout a response | Inferred as `Null`, so joining results can fail | Registered endpoints keep the column's observed type ([registry](endpoints/schemas.json)) |
| No data (`NOT_FOUND`) | `NoDataFoundError` | `QueryFailed` with kind `"no_data"`. An unknown symbol and a range with no trading days are the same category. The service answers both identically (`err-notfound-001`), so check the symbol with `stock_list_symbols` or `index_list_symbols` if you need to know which |
| Ended session | — | `SessionEnded`, a subclass of `QueryFailed`. No automatic re-login |
| Long date ranges | Sent as one request | Split within the vendor's limits and merged in order (`plan=True`). `plan=False` sends as given |
| Concurrency | The caller's threads | A scheduler at the tier's documented limits, with retry only at open on `RESOURCE_EXHAUSTED`/`UNAVAILABLE` |
| Extras | — | `query`, `batches` (streaming) and `to_parquet` (written from Rust), with per-call `timeout`, `idle_timeout` and `concurrency` |

**Not covered:**
- **Real-time streaming:** the stream crate is experimental and needs an
  operator-run Theta Terminal.
- **Raw gRPC access:** low-level and unstable.
- **Full parity:** not claimed. Per-endpoint live evidence is in the
  [coverage table](endpoints/coverage.md).
