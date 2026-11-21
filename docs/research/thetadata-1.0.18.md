# ThetaData Python 1.0.18: upstream delta research and proposal

**Status: adopted.** The user approved the [proposal](#proposal-for-the-users-decision)
on 2026-11-20: 1.0.18 is the compatibility baseline from `0.1.5`, with the
new defaults, the two corporate-action wrappers (not live-verified), the
login unchanged and the OHLC widening held back to `0.2.0`. Per
[AGENTS.md](../../AGENTS.md) and the [monitor policy](../upstream-monitor.md),
a new upstream release starts research; it does not authorise a baseline
bump. The scheduled monitor stays red until then.

## Trigger

The scheduled `Upstream Python compatibility` run 44401463653 failed on
2026-11-18: PyPI's latest stable `thetadata` is **1.0.18** (uploaded
2026-11-17 22:08 UTC) and the tracked `source_version` is 1.0.17.

## Inputs and method

| Version | Uploaded (UTC) | Wheel SHA-256 | Sdist SHA-256 | Attested publishing commit |
| --- | --- | --- | --- | --- |
| 1.0.17 (baseline) | 2026-10-30 | `a43a2fe0fc4896047b9287a9cbf725214386566e3adf930523e5f8aeabf81fea` | `048884e7dc28e304b668698b6a321717b6ec6525a7fef57cfa83ee84e148ff39` | `7d8def5003d30423ec75144e95b26f682b434464` |
| 1.0.18 | 2026-11-17 | `5ee4d60d81149437b306a2a152047a6303fe203a399ba44b452d8ddaaf39b94d` | `3381de43468d580da60a2ec6ee446553b459ac82c7b885fc6ef4a5984f45f75b` | `279f102caf79ddb350c57eae67db6376ca7c9cd9` |

The method is the one used for 1.0.13–1.0.17
([analysis](thetadata-1.0.13-1.0.17.md#inputs-and-method)):
- **Downloads:** all fourteen wheels and sdists from 1.0.12 to 1.0.18 came
  from the `files.pythonhosted.org` URLs in PyPI's JSON metadata, into
  ignored local storage. Each SHA-256 matched PyPI's digest. Nothing was
  installed, imported or executed.
- **Integrity:** the 1.0.18 wheel's `RECORD` verifies (13 hashed entries),
  and all nine Python modules are byte-identical to the sdist's.
- **Provenance:** PyPI's integrity API attests the wheel to
  `AXIOMXLLC/thetadata-python`, workflow `publish.yml`, environment `pypi`;
  the subject digest equals the wheel hash. The commit above is the one in
  the signing certificate. There is still no public Git repository to diff.
- **Tool:** `tools/upstream_delta.py generate` over 1.0.12–1.0.18 (static
  AST and descriptor analysis), then `upstream_delta.py check` and
  `research_python.py --check`: both pass. The generated records:
  [1.0.17 → 1.0.18](python-1.0.18-delta.md),
  [1.0.12 → 1.0.18 cumulative](python-1.0.12-to-1.0.18-delta.md),
  [inventory](python-1.0.18-inventory.json), [vendor summary](vendor-1.0.18.json),
  [catalogue](python-1.0.18-catalog.md), [delta data](python-1.0.13-1.0.18-delta.json).
- **Protocol:** `tools/extract_protocol.py`'s descriptor recovery, applied in
  memory to the 1.0.18 wheel, gives exactly the checked-in `thetadata.bin`
  (57,926 bytes, SHA-256 `35572458…5a58`, 100 RPCs).
- **Secondary evidence:** the vendor's public
  [OpenAPI schema](https://thetadata.net/docs/openapiv3.yaml), fetched
  2026-11-19: SHA-256 `c11bca35…f017a`, byte-identical to the 2026-11-01
  copy. It documents the Terminal REST surface, not gRPC.

## What changed from 1.0.17

**Protocol: nothing.** No RPC, message, field or enum changed; the
descriptor is byte-identical. **Dependencies and packaging: nothing**
(`Requires-Python >=3.12`; the same `Requires-Dist`; Apache-2.0).

**Python wrappers: 2 added, 12 changed, none removed** (81 → 83).

| Change | Where | Detail |
| --- | --- | --- |
| **New wrapper** | `corporate_action_dividend(symbol, start_date, end_date)` | For `GetCorporateActionDividend`, an RPC already in the descriptor (today a descriptor-only, unstable RPC here). Dividend events with an ex-date in the range; one row per payment, or its parts flagged `is_component` |
| **New wrapper** | `corporate_action_split(symbol, start_date, end_date)` | For `GetCorporateActionSplit` (likewise descriptor-only here). Split events in the range, with `before_shares`, `after_shares`, `split_ratio`, `event_code` |
| **Default flipped** | `perf_boost_intraday`: `False` → **`True`** | 9 trade-Greeks wrappers (`option_history_trade_greeks_*` and `option_history_binomial_trade_greeks_*`). Truthiness-guarded, so 1.0.18 now **sends `perf_boost_intraday = true` by default**, where 1.0.17 sent nothing. Per OpenAPI: current-day requests only, the underlying price looked up at 1-second rather than millisecond precision |
| **Default flipped** | `use_time_based_trade_quote`: `True` → **`False`** | `option_history_trade_quote`, `stock_history_trade_quote`. Truthiness-guarded, so 1.0.18 **stops sending `true`** and sends nothing by default. Per OpenAPI: current-day only; `true` matches trades to quotes by millisecond timestamp, otherwise to the last quote before the trade |
| **Type widened** | `option_history_ohlc`'s `expiration`: `date` → `Union[date, str]` | Now formatted with `_fmt_if_date`, so `*` (every expiration) is accepted |
| **Documented behaviour** | `option_history_ohlc`, and the trade-Greeks and option trade-quote docstrings | "Multi-day requests are limited to 1 month of data, **and must specify an expiration**" (the OHLC line is new in 1.0.18) |
| **Documented behaviour** | `option_history_trade_quote` | New: "Current-day requests with `expiration=*` only return trades matched with the last quote reported before the trade. They require `use_time_based_trade_quote=false`, ignore `exclusive`, and return a 400 error otherwise" |
| **Login request** | `ThetaClient.__init__` (+ `import importlib.metadata`) | The authentication request gains `clientName: "python"` and `clientVersion: "thetadata-python/<installed version>"` |

**Not changed:** the Greeks `version` parameter (default `'latest'`, values
unchanged; nothing touches `version='1'`), the Greeks EOD wrappers and their
"day by day" note, authentication otherwise, credentials, error classes,
conversion and every other wrapper.

## Impact on this project

**Today, on 1.0.17 parity, nothing breaks:** the protocol is identical, so
every request the library sends is still valid, and the live checks of
2026-11-18 (`weekly-20261118`, all clean) ran against the service after
1.0.18's release.

**What differs from the vendor package's new behaviour, if we stay on 1.0.17:**
- **Current-day trade Greeks:** a caller relying on defaults gets
  millisecond-precision underlying prices from us, 1-second from the vendor
  (`perf_boost_intraday`). Historical requests are unaffected.
- **Current-day trade-quote matching:** we send `use_time_based_trade_quote =
  true` by default (millisecond matching); the vendor now sends nothing.
  What an absent field means over gRPC was already an open question
  ([1.0.13–1.0.17, question 2](thetadata-1.0.13-1.0.17.md#open-questions-for-thetadata));
  the new docstring implies the service treats it as `false`.
  **Per the new docstring, current-day option trade-quote with
  `expiration=*` would get a 400 with our default** (it requires `false`);
  not observed. Historical requests are unaffected.
- **Corporate actions:** reachable today by RPC name through `dispatch` and
  the generated requests, but unstable and without vendor names.
- **OHLC with `expiration=*`:** our `OptionHistoryOhlc.expiration` is a
  `NaiveDate`, so `*` cannot be expressed. Our planner already sends option
  OHLC one day per request, so a multi-day `*` request would be split as
  the new docstring requires.
- **Login metadata:** we do not send a client name or version. Nothing
  indicates the service requires them.

## Proposal (for the user's decision)

**Recommendation: adopt 1.0.18 as the baseline**, in two parts so nothing
breaks in `0.1.x`:

**Part A, `0.1.5` (no API break; a documented behaviour change):**
1. **Move the baseline to 1.0.18:** manifest `source_version`, the 1.0.18
   dispositions ledger (carried forward, plus decisions for the two new
   wrappers), `research_python.py`'s current baseline, the monitor green.
2. **Follow the new defaults** in the generated requests, the dispatch
   metadata and the Python methods: `perf_boost_intraday` `true` on the nine
   trade-Greeks requests, and `use_time_based_trade_quote` unset on the two
   trade-quote requests. This changes current-day results for callers who
   rely on defaults, so the CHANGELOG and release notes say so plainly, with
   the explicit values that restore 0.1.4's behaviour.
3. **Add `corporate_action_dividend` and `corporate_action_split`** as vendor
   wrappers (Python methods, dispatch names); they become supported
   (`VendorWrapper`) only after the live checks below.
4. **Leave the login request unchanged.** Sending a client name and version
   is not required, and the assurance report states what the login sends;
   if the user wants parity, send `clientName: "praevis-thetadata"` and our
   version (a user decision, not a default).

**Part B, `0.2.0` (breaking, with the jiff dates):** widen
`OptionHistoryOhlc.expiration` to `Expiration`, so `*` works. It changes a
public field's type, which is why it waits for the minor version.

**Alternative:** stay on 1.0.17 and document the two default differences.
The monitor would then stay red until a later release is adopted, which the
policy discourages, so this is not recommended.

## Live checks before adopting (one request each; for the user's approval)

1. **`corporate_action_dividend`** (a symbol with known dividends) and
   **`corporate_action_split`** (a symbol with a known split): codes,
   columns and row counts, to register their schemas and entitlements.
2. **Trade Greeks, current day,** `perf_boost_intraday` absent against
   `true`: both answer, and how the underlying price differs.
3. **Trade-quote, current day,** `use_time_based_trade_quote` absent,
   `false` and `true`: whether absent behaves as `false` (open question 2),
   and the documented 400 for `expiration=*` with `true`.
4. **Option OHLC** with `expiration=*` over one day: answered.

Codes and counts only are recorded, as for every live check.

## Open questions for ThetaData

1. Over gRPC, does an absent `use_time_based_trade_quote` mean `false`
   (as 1.0.18's docstring implies) or `true` (the REST default)?
2. Will the REST defaults for `perf_boost_intraday` and
   `use_time_based_trade_quote` change to match the Python package?
3. What does the service do with the login's `clientName` and
   `clientVersion`?
