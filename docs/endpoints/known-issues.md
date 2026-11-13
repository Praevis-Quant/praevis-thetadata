# Known issues: request shapes the service refuses

Service behaviour that the library has to plan around, each with the
observation that showed it. Codes and counts only; never row values or the
service's message text. The emulator plays each one
([`model.json`](../../crates/praevis-thetadata-emulator/model.json),
`refusals_by_shape`), and the drift check reports when a live run stops
seeing it.

## Option Greeks EOD over every expiration: one day per request

**Observed** on 2026-11-13, about 17:45 UTC. It came from the first ingest of
the private data service (`praevis-data`): Windows, options PRO, with the
Python package `0.1.0b2` against the live service.

| Request (SPY, expiration `*`) | Range | Answer |
| --- | --- | --- |
| `option_history_greeks_eod` | 2026-11-02 to 11-06 (5 days) | **refused, `InvalidArgument`** |
| `option_history_greeks_eod` | 2026-11-05 to 11-06 (2 days) | **refused, `InvalidArgument`** |
| `option_history_greeks_eod` | 2026-11-06 (1 day) | 12,952 rows |
| `option_history_eod` | 2026-11-02 to 11-06 (5 days) | 63,626 rows |

The refusals held with and without `strike='*'`, `right='both'`,
`rate_type='sofr'` and `version='latest'`.

**What went wrong in `0.1.0`:**
- **The planner kept the whole range:** `job()` (and Python's planned calls
  and `describe`) split this RPC only at 366 days, so a multi-day wildcard
  request went out as one part, and the service refused it.
- **Why the live checks missed it:** they use single days.

**Fixed for `0.1.1`:**
- **Per day:** the planner splits `OptionHistoryGreeksEod` per day when the
  expiration is `*`. One expiration keeps the 366-day split, since no
  refusal was seen there.
- **The rule is per request:** `Endpoint::split` is a new provided method
  of the sealed trait, defaulting to `SPLIT`, so nothing that compiled
  before breaks.
- **Weekends:** the per-day parts are calendar days, and empty ones are
  skipped, as with the intraday splits.
- **Tests:** a planner test and an emulator test; the scenarios run it in
  CI against the emulator, and live in the scheduled run.
- **The workaround on `0.1.0`:** one request per day, or a single expiration.

**The same shape, untested (not inferred, so not split):** these RPCs also
plan at 366 days and accept expiration `*`, but no run has sent them `*`
over more than one day.

| RPC | Status | The check that would settle it |
| --- | --- | --- |
| `OptionHistoryBinomialGreeksEod` | **Unverified** | one request, SPY, expiration `*`, two weekdays |
| `OptionAtTimeTrade` | **Unverified** | the same |
| `OptionAtTimeQuote` | **Unverified** | the same |
| `OptionHistoryEod` | **Verified fine:** 5 days answered whole | — |

Every other option history RPC already plans one day per request.
