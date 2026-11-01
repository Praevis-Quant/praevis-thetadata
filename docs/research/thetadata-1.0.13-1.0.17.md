# ThetaData 1.0.13 through 1.0.17: upstream delta research

Reviewed: 2026-11-01. This report compares each stable PyPI release after the
tracked **1.0.12** baseline with its predecessor. It separates observed
artifact behavior, documented vendor behavior and project inference, as in
[the 1.0.12 research](thetadata-1.0.12.md). No vendor code was installed,
imported or executed, and no production login or data request was made.
The compatibility baseline is still 1.0.12. This document does not change it;
see [the update plan](../upstream-update-1.0.17.md).

## Trigger

The scheduled [upstream monitor](../upstream-monitor.md) has failed daily since
**2026-10-09** (run 37975942902, reporting 1.0.13). The last success was on
2026-10-08. Every failing run exits 1 from `tools/check_upstream.py` with
"A newer upstream release requires research and compatibility review". The
2026-10-31 run (41226014421) reports latest stable **1.0.17**. The workflow,
checker and network are working as designed. The failure is the intended
alert, not a tooling fault. It will stay red until `source_version` is
deliberately updated by the plan's final step.

## Inputs and method

| Version | Uploaded (UTC) | Wheel SHA-256 | Sdist SHA-256 | Attested publishing commit |
| --- | --- | --- | --- | --- |
| 1.0.12 (baseline) | 2026-09-30 | `d9a5f4d19d0a6e7abc9861e8aee6bba3838e3551105babf76c4e361ff233d565` | `5c038a36f9b44a4b17dab63a352c5754f31a7a296d21230f96df9cecab8b7f87` | `a7912e4dfa72a985b03d4d75516568b230aabb92` |
| 1.0.13 | 2026-10-08 | `df9c7c7b3081c4afe34554468aa9ec4dd46ee532733cbe03ef545b556871a5c2` | `d51c5b5ce80f1ed048f1ce283ffcfc791b3a9e5e87b64fb3b3239c6e684efc2a` | `fe20e17f99aa6a26adbb1f521ac1637239ae6893` |
| 1.0.14 | 2026-10-09 | `7114e7f5ca09420d798b1fa5b87298c4723b51ee58292d7d18e7ede692c263e5` | `03b583a6c68abe57ca417acf7e0f33f65ed6635b361be9d1de3300940c8786b4` | `9dc295cbdf8b120dcfcd2392d44febb293405a29` |
| 1.0.15 | 2026-10-14 | `68104130571ac70a4bad7a9433e794a01ff81cc2e952e1f86513785e84533c17` | `15a5b37950aeba99f9f7e89bb45dbbbf260af7073865831facb932ccf6bc470c` | `46e346b05fe793b1ef82be9cc5d15a5cc6766b9f` |
| 1.0.16 | 2026-10-28 | `709fed6738f9450931ff405795b0783ea45604761467584473d85b1e485ab6b0` | `ca66e346f2e9fa1b1dccacdb8c20e515379a3a01e04398f76248a61013b81a51` | `226cceb174bb7e98e8868b91740fbfedb0546edd` |
| 1.0.17 | 2026-10-30 | `a43a2fe0fc4896047b9287a9cbf725214386566e3adf930523e5f8aeabf81fea` | `048884e7dc28e304b668698b6a321717b6ec6525a7fef57cfa83ee84e148ff39` | `7d8def5003d30423ec75144e95b26f682b434464` |

All twelve artifacts were downloaded from the `files.pythonhosted.org` URLs
listed in the [PyPI JSON metadata](https://pypi.org/pypi/thetadata/json), into
ignored local storage. Each SHA-256 matched PyPI's published digest. The
1.0.12 wheel also matched the committed inventory's `artifact_sha256`. No
release is yanked and there are no pre-releases in this range. Every wheel's
`RECORD` covers all 14 members exactly once, and every hashed entry verifies.
In each version all nine Python modules in the wheel are byte-identical to the
corresponding files in the sdist. The sdists contain no extra Python modules.

PyPI's [integrity API](https://docs.pypi.org/api/integrity/) attests every
wheel to `AXIOMXLLC/thetadata-python`, workflow `publish.yml`, environment
`pypi`. The statement subject digest equals the wheel hash. The commit column
is the 40-hex revision embedded in each Sigstore signing certificate. The
same extraction applied to 1.0.12 reproduces the commit already recorded in
[upstream-source.md](upstream-source.md). As before, this consumed PyPI's
verified record and did not independently re-verify the certificate chain.
`api.github.com/repos/AXIOMXLLC/thetadata-python` still returns **404**, so
there is still no public Git tree to compare.

Comparison is static, using [tools/upstream_delta.py](../../tools/upstream_delta.py).
It reuses the AST helpers in `research_python.py` (definitions, parameters,
defaults and `query.*` bindings with their guarding conditions) and parses
serialized descriptors from `AddSerializedFile` literals with
`google.protobuf`. Its deterministic output is committed as
[python-1.0.13-1.0.17-delta.json](python-1.0.13-1.0.17-delta.json). Each entry
compares one release with the previous one. Regenerate it with:

```bash
python -m pip install -r tools/research-requirements.txt
python tools/upstream_delta.py --output docs/research/python-1.0.13-1.0.17-delta.json \
  path/thetadata-1.0.12-py3-none-any.whl=d9a5f4d1...  # one WHEEL=SHA256 per version, oldest first
```

The committed delta JSON was regenerated twice from the same inputs and was
byte-identical both times. Its SHA-256 is
`3ecf4939eb2dcb9ea161685b47fbdf977f7eea2b9b9e088288622caa19755cb9`.

The descriptor-set procedure in `tools/extract_protocol.py`, applied in memory
to the 1.0.12 wheel, reproduces the committed `thetadata.bin` hash
`b5b7ba02…0373` exactly. Applied to 1.0.17, it yields descriptor SHA-256
`35572458782193a20b112e4b62c9179ca380d6f9dfd919b79494de584a3f5a58`
(57,926 bytes, 100 RPCs). That is the expected manifest value after the update.

Secondary evidence comes from the current public
[OpenAPI schema](https://thetadata.net/docs/openapiv3.yaml), fetched
2026-11-01 with SHA-256
`c11bca35976517c8694427d21c32d4aa4398860df3249205c2334b2aa9ef017a`. It is
API version 3.0.0 with 86 operations. It describes the Terminal REST surface,
not gRPC. It is used only for parameter semantics and defaults that the
wheel does not document.

## Summary across 1.0.13–1.0.17

| Area | Change from 1.0.12 to 1.0.17 |
| --- | --- |
| Authentication | **None.** Constructor, credential discovery, auth URL/header constants, `_read_creds_from_file`, environment reads, imports and error classes are unchanged in every release. |
| Package metadata / dependencies | **None.** Only `Version` changed in METADATA; the long description and its examples are unchanged. Requires-Dist, Requires-Python and License are unchanged. Only the WHEEL generator changed in 1.0.16, hatchling 1.32.4 → 1.33.0. |
| Response conversion | **No Python code change.** `_convert_response_stream`, `_fmt_time` and `_fmt_if_date` are unchanged. The shared response schema changed in 1.0.16; see below. |
| RPCs | 82 → **100**. Added `GetStockFlatFileQuote` (1.0.14) and 17 `GetFuture*` RPCs (1.0.16). None removed or renamed, and no request/response/streaming shape changed. |
| Python wrappers | 80 → **81**. Added `stock_flat_file_quote` (1.0.14). The futures RPCs have **no** wrapper. Nothing was removed or renamed. |
| Messages / enums | 176 → 212 messages; 2 → 3 enums. Added `Endpoints.TimestampPrecision`. Existing fields were only added: none were renumbered, retyped or removed. |
| Stock EOD (selected slice) | **No change** to the `stock_history_eod` wrapper, `StockHistoryEodRequest(Query)`, `QueryInfo` or its RPC. The **response** `ZonedDateTime` message gained fields in 1.0.16. |

<a id="release-1-0-13"></a>

## 1.0.13 (2026-10-08): snapshot recency, Greeks price source, trade-quote matching

**Protocol.** 25 request-query messages gained one explicit-presence
(`proto3_optional`) bool. There were no other descriptor changes.

| New field (number varies by message) | Messages | Python wrapper parameter and default |
| --- | --- | --- |
| `current_day_only` | 21 snapshot queries: stock (ohlc, trade, quote, market_value); index (ohlc, price, market_value); option (ohlc, trade, quote, open_interest, market_value, greeks all/first/second/third/implied_volatility, binomial greeks all/first/second/third) | `current_day_only: Optional[bool] = False` |
| `option_use_nbbo` | `OptionHistoryGreeksEod`, `OptionHistoryBinomialGreeksEod` | `option_use_nbbo: Optional[bool] = True` |
| `use_time_based_trade_quote` | `StockHistoryTradeQuote`, `OptionHistoryTradeQuote` | `use_time_based_trade_quote: Optional[bool] = True` |

**Wrappers.** 25 wrappers each gained one trailing keyword parameter. Positional
compatibility is preserved. Each follows the established pattern:
`if X is not None: query_parameters["X"] = str(X)` for the metadata map, and
`if X: query.X = X` for the wire field. The `NoDataFoundError` message now
includes the new argument.

**Behavior-visible consequences (observed in the artifact):**

- With default arguments, Python 1.0.13+ now **sends `option_use_nbbo=true`**
  and **`use_time_based_trade_quote=true`**. 1.0.12 sent neither. The metadata
  map also gains a `"False"`/`"True"` string for every new parameter on every
  affected call.
- **Python cannot send an explicit `false`** for these three booleans. The
  truthiness guard omits the field, so the server sees "absent" and applies
  its own default. The OpenAPI schema documents `option_use_nbbo` and
  `use_time_based_trade_quote` as **default true**. On REST, `false` selects
  the last trade price and quote-time matching respectively. Through Python's
  gRPC path, therefore, `option_use_nbbo=False` probably still yields NBBO
  pricing. That is an inference from the guard and the documented default,
  not a verified service behavior. This is the same presence trap recorded in
  [the 1.0.12 analysis](thetadata-1.0.12.md#date-and-presence), and it
  reinforces ADR-0008's explicit-presence decision.

**Documented vendor behavior (wrapper descriptions; unverified):**

- Snapshots now return the latest message per symbol/contract **from the past
  7 days**. Outside market hours they return the most recent session, and
  expired contracts are removed. 1.0.12 said the snapshot cache resets at
  midnight ET and weekends may return nothing. `current_day_only=true`
  restores same-day-only results, applied before `min_time` according to
  OpenAPI. This is a **server-side semantic change** that affects all clients
  regardless of version.
- Greeks EOD now says it defaults to the midpoint of the option's closing NBBO.
  1.0.12 said the closing option trade price was used. This is again a server
  default change, visible to any client that omits the field.
- Trade-quote: current-day matching uses the last quote reported before the
  trade unless `use_time_based_trade_quote` is set. Historical matching is
  unchanged.

<a id="release-1-0-14"></a>

## 1.0.14 (2026-10-09): stock 1-minute quote flat file

**Protocol.** Added the `StockFlatFileQuoteRequest` envelope (`query_info`
field 1, `params` field 2), `StockFlatFileQuoteRequestQuery { date = 1 (string) }`
and server-streaming RPC `GetStockFlatFileQuote` returning
`Endpoints.ResponseData`. The generated stub, servicer and experimental
helpers were added accordingly.

**Wrapper.** `stock_flat_file_quote(self, date: datetime.date)` follows the
existing seven flat-file wrappers exactly. It formats `date.strftime('%Y-%m-%d')`
and passes the stream through the ordinary table converter. There is no
download, manifest or disk handling. That makes eight flat-file wrappers in total.

**Documented vendor behavior (unverified):** one CSV-style row per minute in
which a new NBBO quote arrived, grouped by symbol. Rows carry
`interval_ms_of_day` and `quote_ms_of_day`, and callers must forward-fill
missing minutes. Data is available for the last 7 calendar days and requires a
Professional stock subscription. The
[1.0.12 flat-file analysis](thetadata-1.0.12.md#flat-files-and-corporate-actions)
applies unchanged: the Rust decoder rejects manifests, and flat files remain
outside the selected scope.

<a id="release-1-0-15"></a>

## 1.0.15 (2026-10-14): dollar dividend input for Greeks

**Protocol.** 29 option Greeks request-query messages gained
`optional double annual_dividend_amount`: history, history trade, snapshot,
binomial and binomial-trade variants, including EOD and implied volatility.

**Wrappers.** The same 29 wrappers gained
`annual_dividend_amount: Optional[float] = None`, with the usual guards. The
existing `annual_dividend` (yield) parameter is retained.

**Consequences.** An explicit `0.0` is omitted on the wire, because it is
falsy, but present in the metadata map. The descriptions are unchanged.
OpenAPI documents the field as dollars per share per year, converted to a
yield per tick. It must be non-negative, and **cannot be combined with a
non-zero `annual_dividend`**. Python validates neither rule locally.

<a id="release-1-0-16"></a>

## 1.0.16 (2026-10-28): futures RPCs, binomial exercise style, nanosecond timestamps

**Protocol: shared response schema.** This is the most consequential change in the range.

```text
message ZonedDateTime {            // package Endpoints
  uint64 epoch_ms = 1;             // unchanged
  TimeZone zone = 2;               // unchanged
  uint32 nanos = 3;                // NEW, implicit presence
  TimestampPrecision precision = 4;// NEW, implicit presence
}
enum TimestampPrecision { MILLISECOND = 0; NANOSECOND = 1; }   // NEW
```

`ZonedDateTime` is the timestamp variant of `DataValue`, used by **every**
response table, including stock EOD. The descriptor carries no comments, so
the meaning of `nanos` is **unverified**. It could be the sub-millisecond
remainder (0–999,999) or the nanosecond-of-second, and `epoch_ms` might be
truncated or rounded. Which RPCs or entitlements emit `NANOSECOND`, if any,
is also unverified.

Python 1.0.16/1.0.17 does **not** read either field. `_convert_response_stream`
still builds a datetime from `epoch_ms / 1000` and casts Polars timestamp
columns to milliseconds, so sub-millisecond data is dropped silently. The
Rust workspace behaves identically today without any code change. Prost
discards unknown fields 3/4 inside `DataValue::decode` in both the Table
decoder (`decode.rs`) and the numeric decoder (`bounded.rs`). Nothing fails;
precision is silently lost.

**Protocol: futures.** There are 17 new server-streaming RPCs, each with a
`Future…Request` envelope and query message (34 messages):

| RPC | Query fields |
| --- | --- |
| `GetFutureListSymbols` | `optional type` |
| `GetFutureListDates` | `request_type`, `repeated symbol` |
| `GetFutureSnapshotOhlc` / `Trade` / `Bbo` / `MarketValue` | `repeated symbol`, `optional min_time` |
| `GetFutureHistoryEod` / `Settlement` | `symbol`, `start_date`, `end_date` |
| `GetFutureHistoryOhlc` / `Bbo` | `symbol`, optional `date`, `date_type`, `start_time`, `end_time`, `start_date`, `end_date`, `interval` |
| `GetFutureHistoryTrade` | `symbol`, optional `date`, `date_type`, `start_time`, `end_time`, `start_date`, `end_date` |
| `GetFutureHistoryTradeBbo` | the same as trade, plus `optional exclusive` |
| `GetFutureAtTimeTrade` / `Bbo` | `symbol`, `start_date`, `end_date`, `time_of_day` |
| `GetFutureFlatFileEod` / `Bbo` / `TradeBbo` | `date` |

There is **no Python wrapper, no OpenAPI operation and no public
documentation** for futures in the artifact or the schema reviewed. Like the
two corporate-action RPCs in 1.0.12, these exist only as descriptor and
generated stubs. They introduce new vocabulary (`bbo`, `settlement`,
`date_type`, `request_type`) with no defined semantics.

**Protocol: binomial Greeks.** 13 binomial Greeks queries (history, history
trade and snapshot, all orders plus EOD) gained `optional string exercise_style`.

**Wrappers.** The same 13 wrappers gained `exercise_style: Optional[str] = None`.
Descriptions now state that the server prices European-style index roots
(SPX, SPXW, XSP, NDX, RUT, VIX) as European and every other root as American
when the field is omitted. OpenAPI lists the values `american` and `european`.
This is a **documented server-side default change** for European roots. In
1.0.12 the description said early exercise was allowed at every node.

<a id="release-1-0-17"></a>

## 1.0.17 (2026-10-30): implied-vol source and an expiration bug fix

**Protocol.** 9 binomial Greeks queries (history all/first/second/third/EOD and
snapshot all/first/second/third) gained `optional string implied_vol_method`.
The 4 binomial **trade**-Greeks queries did not.

**Wrappers.**

- The same 9 wrappers gained `implied_vol_method: Optional[str] = 'own_quote'`.
  Because the default is truthy, Python **always sends `own_quote`** unless the
  caller passes `None` or `''`. OpenAPI documents the values `own_quote`
  (default) and `out_of_the_money`. The latter requires `strike=*`,
  `right=both`, and an `interval` on history requests; Python does not check
  these locally. The descriptions are unchanged.
- **Bug fix:** 9 interval-based history Greeks wrappers (`option_history_greeks_*`
  all/first/second/third/implied_volatility and `option_history_binomial_greeks_*`
  all/first/second/third) now build `ContractSpec.expiration` with
  `_fmt_if_date(expiration)` and annotate `Union[datetime.date, str]`.
  Previously they called `expiration.strftime(...)`, so a string expiration
  such as a wildcard raised `AttributeError` before any RPC. The output for a
  `date` argument is unchanged.

## Not changed (checked explicitly)

- `stock_history_eod`, its request messages and RPC path. Field numbers 1–3 and
  `QueryInfo` (including `query_parameters`) are unchanged.
- `Price`, `DataValue` oneof members, `DataTable`, `ResponseData`,
  `CompressionDescription`, `FlatFileManifest`, `AuthToken`, `ContractSpec`.
- Exception types and the `NOT_FOUND` → `NoDataFoundError` mapping. The
  messages only gained new arguments.
- Generated-module scaffolding apart from additions; `thetadata/__init__.py`;
  the bundled license.

## Impact on the Rust workspace (inference; see the plan)

| Crate | Impact |
| --- | --- |
| proto | Regenerating the descriptor adds 18 RPCs, 36 messages and one enum, and two fields to the public generated `ZonedDateTime`. This is **source-breaking for Rust struct literals.** A trial build against the 1.0.17 descriptor compiles every library and binary. Test targets fail only with `E0063: missing fields nanos and precision`, from five literals in client tests. |
| core / client | The decoders must deliberately handle or reject `precision = NANOSECOND`/non-zero `nanos`. The silent-truncation default conflicts with the "exact values" and "fail explicitly" contracts (EOD-L2-003, EOD-L3-009). |
| client (requests) | The typed EOD request is unchanged. New optional fields matter only to future typed endpoints, and they must use explicit presence rather than copying Python's truthiness guards. |
| auth / CLI | None. The auth request shape and client identifier are unchanged. |

## Open questions for ThetaData

1. What do `ZonedDateTime.nanos`/`precision` mean: the remainder after
   `epoch_ms`, or nanosecond-of-second? Is `epoch_ms` truncated? Which RPCs
   and tiers emit `NANOSECOND`? Can stock EOD ever emit it?
2. Over gRPC, does an absent `option_use_nbbo`/`use_time_based_trade_quote`
   mean `true`, as on REST? Is Python's inability to send `false` intentional?
3. Are the futures RPCs supported for direct gRPC clients, and with which
   entitlements, result schemas and symbology?
4. Is the 7-day snapshot lookback, and the NBBO-midpoint default for Greeks
   EOD, effective for all clients from the 1.0.13 release date?
5. Is there a public source repository or tag for the attested commits?
