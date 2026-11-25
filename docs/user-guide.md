# User guide: platforms, prerequisites, resources and compatibility

For `0.1.0`, the first supported release. The
[0.1.0 scope record](releases.md#010-scope-record-user-2026-11-12) says what is
supported and what is not, and lists the
[known gaps](releases.md#known-gaps-shipped-in-010).

## Supported platforms

| | Supported | Notes |
| --- | --- | --- |
| **Windows x86_64** | Yes | Tested in CI. Python wheel: `win_amd64`; CLI executable |
| **Linux x86_64, glibc 2.28 or later** | Yes | Tested in CI. Python wheel: `manylinux_2_28_x86_64`, which needs at most `GLIBC_2.28`; CLI executable |
| **Windows arm64; Linux aarch64 (glibc 2.28+); Linux musl (x86_64, aarch64)** | Built from `0.1.3` | Built and smoke-tested on their own runners at each release: the CLI runs `--version`; the wheel (`win_arm64`, `manylinux_2_28_aarch64`, `musllinux_1_2_*`) imports and plans `describe()` offline. The full test suite runs on x86_64 Windows and Linux only. The musl CLI is fully static |
| **macOS (Intel 10.12+, Apple silicon 11.0+)** | Built from `0.1.3` | As above (`macosx_10_12_x86_64`, `macosx_11_0_arm64`). CI also builds the CLI and runs the auth tests, including a Keychain save, read and delete, on macOS |

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

**Distribution:** the crates are on crates.io as `0.1.5` (`0.1.0` plus
planner fixes, requests by name, dependency updates and the 1.0.18
baseline). Pin the unstable `praevis-thetadata-proto` and the experimental
`praevis-thetadata-stream` exactly (`=0.1.5`). The Python package is on
PyPI as `praevis-thetadata` `0.1.5` (`pip install praevis-thetadata`). There are wheels only,
no source distribution, for the platforms in the table above. The same
wheels, prebuilt CLI executables for those platforms, `SHA256SUMS` and a
build-provenance attestation for each are attached to the GitHub release
([how to verify them](releasing.md#verifying-a-release)).

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
- **macOS:** the user's default Keychain. No setup; macOS may ask once to
  allow access when another program reads the item.
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
- `RequestOptions`, per call: a job's concurrency (by default the tier's
  documented limit, from FREE 1 to PRO 8), batch policy and timeouts (none
  unless set);
- `DecodeLimits` and the arrow `Limits` (`encoded_bytes` defaults to 64 MiB).

## Compatibility with the vendor Python package

**The same:**
- **Methods:** all 83 vendor `ThetaClient` methods, with the vendor 1.0.18
  names, argument order and defaults (one held back: `option_history_ohlc`'s
  `expiration` stays a date until `0.2.0`).
- **Credentials:** the same order (`creds_file`, `api_key`, email and
  password, the environment, `./creds.txt`).
- **Frames:** `dataframe_type` selects `"polars"` (the default), `"pandas"`
  or `"arrow"`.

**Different, by design:**

| Area | Vendor package | praevis-thetadata |
| --- | --- | --- |
| [Prices, Greeks, and implied volatility](#numeric-output-prices-greeks-and-implied-volatility) | Price cells become binary64 floats, with NaN for scale 0 | Price cells stay exact `decimal128(38, 10)`, with Arrow null for scale 0 |
| Timestamps | Python `datetime` objects, built through floating-point seconds; the dataframe library infers the column type | Exact millisecond Arrow timestamps with the zone in the column type (`America/New_York`, or `UTC` where sent) |
| A column null throughout a response | Inferred as `Null`, so joining results can fail | Registered endpoints keep the column's observed type ([registry](endpoints/schemas.json)) |
| No data (`NOT_FOUND`) | `NoDataFoundError` | `QueryFailed` with kind `"no_data"`. An unknown symbol and a range with no trading days are the same category. The service answers both identically (`err-notfound-001`), so check the symbol with `stock_list_symbols` or `index_list_symbols` if you need to know which |
| Ended session | — | `SessionEnded`, a subclass of `QueryFailed`. No automatic re-login |
| Long date ranges | Sent as one request | Split within the vendor's limits and merged in order (`plan=True`). `plan=False` sends as given |
| Timeouts | None: the high-level methods take no timeout ([research](research/thetadata-1.0.12.md#transport-and-errors)) | None by default either (from beta.2). Set `Client(request_timeout=…, idle_timeout=…)` for every call, or `timeout=`/`idle_timeout=` per call on `query`, `batches` and `to_parquet` |
| Retry and rate limits | None | None (from beta.2, [ADR-0021](adr/0021-per-call-library.md)). A busy account (`RESOURCE_EXHAUSTED`) raises `QueryFailed` with kind `"busy"`, not retried |
| Concurrency | The caller's threads; one request per call | A planned call runs its parts at once, up to its `concurrency` (by default the tier's documented limit: FREE 1 to PRO 8). Calls are not coordinated with each other: several at once can exceed the plan |
| Extras | — | `query`, `batches` (streaming) and `to_parquet` (written from Rust), with per-call `timeout`, `idle_timeout` and `concurrency` |

**Not covered:**
- **Real-time streaming:** the stream crate is experimental and needs an
  operator-run Theta Terminal.
- **Raw gRPC access:** low-level and unstable.
- **Full parity:** not claimed. Per-endpoint live evidence is in the
  [coverage table](endpoints/coverage.md).

### Numeric output: prices, Greeks, and implied volatility

**A `Decimal` price, strike, Greek, or implied volatility is a valid numeric
value.** Accept decimal columns in your schema and check missingness
separately. For model inputs that require floats, cast those columns
explicitly.

#### From the wire to the dataframe

- **Price cells** carry a signed 32-bit mantissa `m` and a scale `s`. For
  `s` from 1 through 19, the value is `m × 10^(s − 10)`. Scale 0 means
  missing, even if the mantissa is nonzero.
- **Number cells** carry signed 64-bit integers. Praevis returns Arrow
  `int64` for these cells. The official Python package passes their integer
  values to the dataframe constructor for type inference.
- In the observed options schemas, `strike`, quotes, trade prices, OHLC,
  `underlying_price`, Greeks (including higher orders), `implied_vol`,
  `bid_implied_vol`, `ask_implied_vol`, and `iv_error` are price cells
  where present. Counts, sizes, volume, exchange and condition codes,
  sequence numbers, and open interest are examples of number cells.
  Types are endpoint-specific. For example, observed option flat-file
  responses carry strikes and quoted prices as text. See the
  [observed endpoint schemas](endpoints/schemas.json).

**Praevis** stores price cells as Arrow fixed-point `decimal128(38, 10)`.
The smallest wire increment is `10^−9`, which fits in ten decimal places.
The largest magnitude is `2^31 × 10^9`. At Arrow scale 10 the stored integer
is `m × 10^s`, whose magnitude is at most `2^31 × 10^19` (29 decimal digits,
within the 38-digit precision). The decoder uses integer multiplication
and performs no rounding. Scale 0 becomes an Arrow null. Polars
preserves the decimal type, and the default pandas adapter preserves it
as an Arrow-backed column. [Arrow decoder](../crates/praevis-thetadata-arrow/src/lib.rs)

**The official `thetadata` 1.0.18 package** computes
`price.value * PRICE_TYPE_FACTORS[price.type]` before constructing the
dataframe. A nonzero scale produces a Python binary64 float. Scale 0
produces `float("nan")`. See `thetadata/client.py` in the
[versioned wheel](https://pypi.org/project/thetadata/1.0.18/#files) and the
[recorded wheel hash](research/thetadata-1.0.18.md).

Binary64 has 53 bits of significand precision. With IEEE 754
`roundTiesToEven`, each operation rounds to the nearest representable
value, with ties going to the even significand. The signed 32-bit mantissa
fits exactly. Fractional factors such as `0.1` and `0.01` are already
rounded to binary64, and multiplication can round the product again.
For a nonzero wire price, the resulting relative error against the exact
decimal is bounded by `2u + u²`, with `u = 2^−53` and both roundings in
the normal range. The result can differ by one ULP (unit in the last
place) from rounding the exact decimal once. [IEEE 754-2019, §§3.6, 4.3.1 and 4.3.3](https://doi.org/10.1109/IEEESTD.2019.8766229)

For example, `m = 3`, `s = 9` is exactly `0.3000000000` in Praevis. The
official factor `0.1` has binary64 bits `3fb999999999999a` and is slightly
above one tenth. The resulting product is one binary64 step above the
nearest binary64 to exact `0.3`. The official calculation can even yield
two float keys for the same decimal. `m = 30`, `s = 8` is also exactly
`0.3`, but multiplying by the stored `0.01` factor gives the lower float.

```pycon
>>> from decimal import Decimal
>>> from struct import pack
>>> exact = Decimal(3).scaleb(9 - 10)
>>> exact.quantize(Decimal("0.0000000001"))
Decimal('0.3000000000')
>>> pack(">d", 0.1).hex()
'3fb999999999999a'
>>> official = 3 * 0.1
>>> official, pack(">d", official).hex()
(0.30000000000000004, '3fd3333333333334')
>>> nearest = float(exact)
>>> nearest, pack(">d", nearest).hex()
(0.3, '3fd3333333333333')
>>> Decimal(30).scaleb(8 - 10) == exact
True
>>> alternate = 30 * 0.01
>>> alternate, pack(">d", alternate).hex()
(0.3, '3fd3333333333333')
>>> alternate == official
False

```

#### What changes when you cast to float64

- **Exactness.** Values such as `0.05`, `1.15` and `0.1` have no exact
  binary64 representation. A correctly rounded conversion of a nonzero
  wire value has relative error at most `2^−53 ≈ 1.11 × 10^−16`.
  PyArrow's decimal-to-float kernel uses an approximate C++ conversion.
  In PyArrow 26.0.1 on Linux aarch64, `pc.cast` on `decimal128(38, 10)`
  returned `1.1500000000000001` (bits `3ff2666666666667`) for exact
  `1.15`. The nearest float prints as `1.15` (bits `3ff2666666666666`).
  Across 437 test inputs, 41 differed from `float(Decimal(...))`, all
  by one ULP, with a maximum relative error against the exact input of
  about `1.35 × 10^−16`. The test crossed scales 1–19 with mantissas
  `−2147483648, −1000000007, −17, −15, −5, −3, −1, 0, 1, 2, 3, 5, 7, 10,
  11, 15, 17, 29, 50, 115, 123456789, 1000000007, 2147483647`. These are
  results for that version and sample.
  [Arrow C++ cast kernel](https://github.com/apache/arrow/blob/apache-arrow-26.0.1/cpp/src/arrow/compute/kernels/scalar_cast_numeric.cc)
  [Arrow C++ decimal conversion](https://github.com/apache/arrow/blob/apache-arrow-26.0.1/cpp/src/arrow/util/decimal.cc)
- **Equality and keys.** Different conversion or arithmetic paths can
  produce different floats for the same decimal, as in the example.
  Exact comparisons, joins, grouping and deduplication can then split
  one price across two keys. Rounding keys to a chosen precision can
  also merge distinct prices. Keep option strikes as decimal join keys.
- **Accumulation.** Rounding can accumulate or cancel, depending on the
  values and the order of operations. Even adding three tenths with
  ordinary binary64 additions gives `0.30000000000000004`. The decimal
  sum is exactly `0.3`. Sum prices or P&L
  in decimal with sufficient precision, or in integer minor units if
  the amounts are already exact multiples of those units (for example,
  cents).

  ```pycon
  >>> 0.1 + 0.1 + 0.1
  0.30000000000000004
  >>> sum([Decimal("0.1")] * 3)
  Decimal('0.3')

  ```

- **Round-trip and display.** Seventeen significant decimal digits are
  sufficient to print a finite binary64 value and recover the same
  binary64 value on a correctly rounded read. The other direction has
  a 15-significant-digit guarantee for decimal inputs in the normal
  binary64 range, when rounded correctly to binary64 and back to the
  original decimal precision. Wire prices have at most ten significant
  digits. With a correctly rounded conversion, Python's shortest
  round-trip display recovers the original wire price as decimal text,
  ignoring trailing zeros. For example, `repr(float(Decimal("0.1")))`
  is `'0.1'`, even though the float is slightly larger. A shortest
  representation of an approximate cast or arithmetic result can
  show extra digits, as in the examples above.
  [IEEE 754-2019, §5.12.2](https://doi.org/10.1109/IEEESTD.2019.8766229)
- **Missingness.** PyArrow casts retain the null bitmap. Polars
  `Float64` retains nulls, which `is_null()` detects separately from
  `is_nan()`. In pandas, Arrow-backed `float64[pyarrow]` keeps null as
  `pd.NA` and can distinguish it from a valid NaN. NumPy-backed pandas
  `float64` represents missing values as NaN, so `isna()` treats both
  alike.

[ADR-0009](adr/0009-market-data-values-and-schema.md) keeps money exact and
leaves conversion to the consumer. Binary64 uses eight bytes per value
compared with 16 for decimal128 and can be faster for vectorised maths.
Keep prices, strikes, P&L and their keys in decimal. For models that expect
floating point, cast only the analytical inputs, such as Greeks and
implied volatility.

```python
import polars as pl
import pyarrow as pa
import pyarrow.compute as pc

# Cast only columns used by the model.
delta = pc.cast(arrow_table["delta"], pa.float64())
model_frame = polars_frame.with_columns(pl.col("delta", "implied_vol").cast(pl.Float64))
model_df = pandas_frame.astype({"delta": "float64[pyarrow]", "implied_vol": "float64[pyarrow]"})
```

Read Parquet with its stored schema and inspect the field types before
casting. For example, use `pyarrow.parquet.read_table(path)` and
inspect `table.schema`. A stored decimal column stays decimal when read with PyArrow.
