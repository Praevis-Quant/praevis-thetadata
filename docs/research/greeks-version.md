# The Greeks `version` parameter

Researched 2026-11-15 from the vendor's published v3 documentation and the
pinned Python package (1.0.17). No live requests. Asked by `praevis-data`:
can a backtest pin the Greeks methodology?

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
endpoint. That is the `1` behaviour, so on the
documentation, `version` should not change EOD Greeks. This is documented,
not observed: no run has compared the two values.

## Pinning it for repeatable backtests

- **`latest` is a moving name.** The documentation gives no number for the
  current methodology and no changelog of past ones. If the vendor adds a
  methodology, `latest` follows it, and the same request can return
  different Greeks.
- **`1` is the only value that names a fixed methodology,** but it is the
  legacy one: for contracts under 7 DTE it differs from the default
  (intraday and snapshot Greeks near expiry).
- **For EOD Greeks,** both values are documented to compute the same way,
  so sending `version="1"` pins the methodology without changing the
  result, as far as the documentation goes.
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

**Open question for ThetaData:** will `latest` change without notice, and
will each methodology get a stable number?

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
