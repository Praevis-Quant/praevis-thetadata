# The Greeks `version` parameter

Researched 2026-11-15 from the vendor's published v3 documentation and the
pinned Python package (1.0.17), then checked live the same day by
`praevis-data` ([below](#observed-eod-greeks-differ-on-the-expiration-day)).
Asked by `praevis-data`: can a backtest pin the Greeks methodology?

## What it accepts and means

Every Greeks endpoint (snapshot, history, trade Greeks, EOD and their
binomial variants) takes `version`, a string defaulting to `latest`
(the Python wrappers' default too). The v3 operation pages list exactly two
values, `latest` and `1`, described as "used to adjust Greeks calculation
methodology". The difference is only **time to expiration (DTE)**, not the
pricing model:

| `version` | Time to expiration |
| --- | --- |
| `1` (legacy) | A fixed DTE of 0.15 on the expiration day; a whole-number DTE otherwise |
| `latest` (default) | Real time to expiration, down to a minimum of one hour. The Option Greeks article adds that this applies to contracts with fewer than 7 days left, computed from the quote timestamp; beyond 7 DTE a whole-number DTE is used |

**The model does not change with `version`:** Black-Scholes for all Greeks,
and a Leisen-Reimer binomial tree for the binomial endpoints (where
`binomial_steps`, default 101, and `exercise_style` apply).

**End-of-day Greeks** "always use a whole-number DTE, with 0.15 on the day
of expiration", per the article, which does not single out the binomial EOD
endpoint. That is the `1` behaviour, so on the documentation, `version`
should not change EOD Greeks. **The live comparison says otherwise** for
contracts expiring that day
([below](#observed-eod-greeks-differ-on-the-expiration-day)).

## Pinning it for repeatable backtests

- **`latest` is a moving name.** The documentation gives no number for the
  current methodology and no changelog of past ones. If the vendor adds a
  methodology, `latest` follows it, and the same request can return
  different Greeks.
- **`1` is the only value that names a fixed methodology,** but it is the
  legacy one: for contracts under 7 DTE it differs from the default
  (intraday and snapshot Greeks near expiry).
- **For EOD Greeks,** the two values are documented to compute the same
  way, but live they differ on the expiration day: `latest` does not keep
  the fixed 0.15 there. Sending `version="1"` pins the legacy methodology;
  it changes the result only for contracts expiring on the report day.
- **Either way, the computation inputs matter as much:** the rate
  (`rate_type`, default SOFR, or `rate_value`), the dividend
  (`annual_dividend` or `annual_dividend_amount`), the price source
  (`option_use_nbbo`, `underlyer_use_nbbo`, `use_market_value`), and for
  binomial Greeks `binomial_steps`, `exercise_style` and
  `implied_vol_method`. Their defaults have changed between package
  releases ([1.0.13-1.0.17 analysis](thetadata-1.0.13-1.0.17.md)). A
  repeatable request sends every one explicitly and records the values.
- **The library passes `version` through unchanged.** It is a plain text
  parameter, with the vendor default `latest`
  ([`dispatch::params`](../../crates/praevis-thetadata-client/src/dispatch/mod.rs)
  reports it).

## Observed: EOD Greeks differ on the expiration day

**Live, 2026-11-15, by `praevis-data`:** `option_history_greeks_eod`, SPY,
every expiration, for 2025-11-04, requested with `version='1'` and with
`version='latest'`, all else equal.
- **120 of 6,850 rows differed,** and all 120 were contracts expiring on
  2025-11-04 itself. Every other row was identical.
- **So:** for EOD Greeks, `latest` does not apply the documented fixed DTE
  of 0.15 on the expiration day, while `1` does, or at least the two
  differ there. Elsewhere they agree, as the documentation says.
- **One day and one root only.** Counts are recorded here, no values.

**Open questions for ThetaData:** what DTE does `latest` use for EOD Greeks
on the expiration day (the article says 0.15)? Will `latest` change
without notice, and will each methodology get a stable number?

## Sources

- v3 operation pages, for example
  [option_history_greeks_eod](https://thetadata.net/docs/operations/option_history_greeks_eod.html),
  [option_history_binomial_greeks_eod](https://thetadata.net/docs/operations/option_history_binomial_greeks_eod.html)
  and
  [option_history_greeks_all](https://thetadata.net/docs/operations/option_history_greeks_all.html):
  the `version` enum and its one-line meaning.
- The v3 [Option Greeks article](https://thetadata.net/docs/Articles/Data-And-Requests/Option-Greeks.html):
  the model, the 7-day rule, the legacy behaviour and the EOD rule.
- The 1.0.17 package: every Greeks wrapper defaults `version='latest'` and
  sends it only if truthy
  ([inventory](python-1.0.17-inventory.json)); its docstrings do not
  describe the values.
