# praevis-thetadata (Python)

Python bindings for the independent praevis-thetadata Rust client: a thin
pass-through over Arrow.

> **Independent and unofficial.** This is an unofficial project. It is not
> affiliated with, sponsored by or endorsed by Theta Data Inc.; “ThetaData”
> identifies the service it interoperates with. `0.1.0` is the first
> supported release: the vendor-named methods plus `query`, `batches` and
> `to_parquet`, with no breaking change without a new minor version and
> release notes. It is not full parity with the vendor package.

**Install:** `pip install praevis-thetadata`. Wheels exist for
Windows x86_64 and Linux x86_64 (glibc 2.28 or newer), Python 3.10 or newer;
CI tests the wheel on Python 3.10, 3.11, 3.12 and 3.13 on both platforms.
There is no macOS or ARM wheel and no source distribution. The same wheels
are attached to the GitHub release.

- Queries run in Rust (planning, transport and decoding), with the GIL
  released. As in the vendor package, there is no retry, scheduler, rate
  limit or default timeout ([ADR-0021](../../docs/adr/0021-per-call-library.md)).
- Results are Arrow record batches shared through the Arrow PyCapsule
  interface (`__arrow_c_stream__`, `__arrow_c_array__`). `pyarrow.table(r)`,
  `r.to_polars()` and `r.to_pandas()` import them without copying and without
  one Python object per row.
- Prices are exact `decimal128(38, 10)`; timestamps are millisecond
  timestamps with their time zone.

```python
import datetime

import praevis_thetadata as pt

# Vendor-compatible: the same credential order and method names as the
# vendor's ThetaClient, returning a Polars (default), pandas or Arrow frame.
client = pt.Client(dataframe_type="polars")
frame = client.stock_history_eod("AAPL", datetime.date(2024, 1, 2), datetime.date(2024, 12, 31))

# Enhanced: Arrow results, streaming batches and Parquet from Rust.
result = client.query("stock_history_eod", symbol="AAPL",
                      start_date="2024-01-02", end_date="2024-12-31")
frame = result.to_polars()

for batch in client.batches("option_history_eod", symbol="SPY", expiration="*",
                            start_date="2024-06-14", end_date="2024-06-14"):
    ...  # pyarrow.record_batch(batch), polars.DataFrame(batch)

client.to_parquet("stock_history_eod", "aapl.parquet", symbol="AAPL",
                  start_date="2020-01-02", end_date="2024-12-31")
```

`query`, `batches` and `to_parquet` also take per-call keyword options:
`timeout` and `idle_timeout` in seconds (none unless set), and `concurrency`
for planned jobs (1 to 64; by default `client.default_concurrency`, the
tier's documented limit). `Client(request_timeout=…, idle_timeout=…)` sets
timeouts for every call, the vendor-named methods included. Calls are not
coordinated with each other, so several at once can exceed the plan. A busy
account raises `QueryFailed` with kind `"busy"`; it is not retried.

`to_parquet` keeps memory flat however large the file. Decimal (price)
columns get no min/max statistics; every other column does.
`decimal_statistics=True` writes them for price pushdown in readers, at a
memory cost that grows with the file. The default works around an upstream
`parquet` bug
([apache/arrow-rs#12027](https://github.com/apache/arrow-rs/issues/12027),
fixed by [#12028](https://github.com/apache/arrow-rs/pull/12028) but not yet
released).

Parameter names are the vendor package's. `pt.describe(...)` shows the wire
query without sending it, and how many date parts `plan` makes. With `plan`
(the default), an all-strike tick or 1 s `option_history_quote` is also split
per strike when it runs, after one strikes-list request; `describe` does not
count those parts. `plan=False` sends the request as given
([request timing](../../docs/performance/request-timing.md)).

**Vendor-named methods.** All 81 vendor `ThetaClient` methods exist with the
vendor 1.0.17 signatures (names, order and defaults; checked against the pinned
inventory), plus a keyword-only `plan`. Each forwards every argument to the
same native query and returns the client's `dataframe_type`. Differences, by
design: prices stay exact (`decimal128(38, 10)`, Arrow-backed in pandas), and
timestamps keep their time zone.

**Partial results and cancellation.**
- **Partial results:** `query()` is all-or-error: if the stream fails, it
  raises `QueryFailed` and keeps no rows. To keep what arrived before a
  failure, iterate `batches()`. The rows are complete only if the iteration
  ends without an error.
- **Cancellation:** `cancel()` a batch iterator, or drop it. That stops its
  requests and frees their slots for the next request.
- **Runnable example:** [`examples/consumer_patterns.py`](examples/consumer_patterns.py),
  against a synthetic loopback service. CI runs it.

**Credentials** follow the vendor and CLI order (ADR-0020): explicit
`creds_file`, `api_key`, or `email` and `password`; otherwise
`THETADATA_API_KEY`, `THETADATA_EMAIL` and `THETADATA_PASSWORD`,
`THETADATA_CREDENTIALS_FILE`, and finally `./creds.txt`. Blank values fall
through; nothing is logged.

Build locally with [maturin](https://www.maturin.rs/):
`maturin develop --release` from this directory, inside a virtual environment.

## Request timing

Large option requests can wait a minute or more for their first row, and a second login ends the first session. See [where the time goes](https://github.com/Praevis-Quant/praevis-thetadata/blob/main/docs/performance/request-timing.md).
