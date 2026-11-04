# praevis-thetadata (Python)

Python bindings for the independent praevis-thetadata Rust client: a thin
pass-through over Arrow.

> **Independent and experimental.** This is an unofficial project. It is not
> affiliated with, sponsored by or endorsed by Theta Data Inc.; “ThetaData”
> identifies the service it interoperates with. It is not published yet.

- Queries run in Rust: the scheduler (FREE's documented limits by default),
  transport and decoding, with the GIL released.
- Results are Arrow record batches shared through the Arrow PyCapsule
  interface (`__arrow_c_stream__`, `__arrow_c_array__`). `pyarrow.table(r)`,
  `r.to_polars()` and `r.to_pandas()` import them without copying and without
  one Python object per row.
- Prices are exact `decimal128(38, 10)`; timestamps are millisecond
  timestamps with their time zone.

```python
import praevis_thetadata as pt

client = pt.Client.from_env()  # reads THETADATA_API_KEY
result = client.query("stock_history_eod", symbol="AAPL",
                      start_date="2024-01-02", end_date="2024-12-31")
frame = result.to_polars()

for batch in client.batches("option_history_eod", symbol="SPY", expiration="*",
                            start_date="2024-06-14", end_date="2024-06-14"):
    ...  # pyarrow.record_batch(batch), polars.DataFrame(batch)

client.to_parquet("stock_history_eod", "aapl.parquet", symbol="AAPL",
                  start_date="2020-01-02", end_date="2024-12-31")
```

Parameter names are the vendor package's. `pt.describe(...)` shows the wire
query without sending it.

Build locally with [maturin](https://www.maturin.rs/):
`maturin develop --release` from this directory, inside a virtual environment.
