# Subscription-aware manual verification

The account was **FREE**, as reported by the account holder on 2026-10-04 and reconfirmed by `praevis-thetadata auth --json` on PROD on 2026-11-01.
Since 2026-11-04 it is **options PRO, with stocks and indices FREE**. Per-RPC
results for both accounts are in the [endpoint coverage table](endpoints/coverage.md).
The [PROD AAPL EOD observation](research/live-observations.md) passed. A separately
reported Python `client.stock_snapshot_quote(symbol=["AAPL"])` request returned
gRPC `PERMISSION_DENIED`, with a message stating that VALUE access was required.
The manual `probe-stock-quote` command reproduces this query with the Python
default venue `nqb` and absent `min_time`. The Rust PROD probe reproduced code 7
at response headers, with zero batches; the expected-denial assertion passed.
See [retained observation and provenance](research/live-observations.md).

## Evidence matrix

Track **data product × declared tier × connection × endpoint × query shape**.
Stocks, options and indices can have separate entitlements; do not treat one
successful login or an opaque subscription JSON object as a universal access grant.
FREE, VALUE, STANDARD and PRO are test-profile labels, not local authorization
logic. The service remains authoritative for each actual request.

| Product/profile | Positive cases | Negative and boundary cases | Current evidence |
| --- | --- | --- | --- |
| Stocks / FREE | Small supported EOD query; exact values and clean EOF | Endpoint requiring VALUE or higher; historical date coverage; permission denial versus no-data | PROD EOD success on 2026-10-04 (1.0.12 baseline) and 2026-11-01 (1.0.17 baseline, `prod-aapl-alpha1-002`, identical values); 2026-11-02 (`*-a2-001`): AAPL four-day range, MSFT and NVDA June 2024 succeed; weekend, holiday and unknown symbol return `NOT_FOUND` (5) before data; **AAPL August 2020 returns `PERMISSION_DENIED` (7) before data, so FREE history depth excludes it** ([semantics](requirements/stock-eod-evidence.md#observed-service-semantics-eod-l3-029)); AAPL stock snapshot quote denial reproduced in Rust on both dates, code 7 before data (`prod-free-quote-alpha1-001`); CLI auth on PROD reports FREE (subscriptions 0). **2026-11-10 (`sir-checks-001`, stocks FREE):** list symbols, list dates and EOD entitled, values equal on the same response, Python OK; every snapshot, history (except EOD) and at-time RPC refused before data, naming VALUE or STANDARD; the stock EOD and trade-quote flat files refused, naming PRO |
| Stocks / VALUE | Selected EOD, quote/OHLC and delayed snapshot cases once implemented | STANDARD/PRO-only operations; delay/resolution/date boundaries | Unavailable: no paid account evidence |
| Stocks / STANDARD | Selected trade/trade-quote and real-time snapshot cases once implemented | PRO resolution/history/subscription boundaries | Unavailable: no paid account evidence |
| Stocks / PRO | Selected tick/history/streaming cases once implemented | Invalid query/contract, no-data and resource boundaries still apply | Unavailable: no paid account evidence |
| Options / each tier | Independently select endpoints and explicit contracts after requirements exist | Greeks/order, resolution/history, subscription and venue boundaries | Unverified; do not infer option permissions from stock results |
| Indices / each tier | Independently select symbols and history/snapshot cases | Tier/venue-specific access and date boundaries | Vendor documentation has conflicting FREE-index statements. **2026-11-10 (`sir-checks-001`, indices FREE):** list symbols, list dates and EOD entitled, values equal on the same response, Python OK; every snapshot, OHLC/price history and at-time RPC refused before data, naming VALUE or STANDARD; the index EOD flat file refused, naming PRO. Paid index tiers unverified |
| Calendar and interest rate | Today, four dates and two years; one rate history | A tier the service names without a product | `GetCalendarOnDate` was refused on the all-FREE account, naming VALUE (`p1-entitlements-001`). **2026-11-10 (`sir-checks-001`, options PRO, stocks and indices FREE):** every calendar request and the rate history entitled, values equal on the same response, Python OK. Which product's tier the calendar needs is unknown |
| Flat files / events | Separate profile for each product and connection when implemented | Manifest/download authorization and subscription limits/errors | Options PRO: see the [coverage table](endpoints/coverage.md). Stocks and indices FREE: the index EOD, stock EOD and stock trade-quote flat files are refused naming PRO (`sir-checks-001`); the stock quote flat file is gated and not probed |

This is a test-selection plan, not a complete static copy of vendor permissions.
The [vendor subscription page](https://thetadata.net/docs/Articles/Getting-Started/Subscriptions.html)
provides candidate endpoint boundaries, but contains inconsistencies: FREE limits
are described as both 20 and 30 requests/minute, and the index general-access and
EOD tables disagree about FREE access. Do not turn those contradictions into
hardcoded requirements. Our FREE results (January and June 2024 allowed, August 2020 denied)
narrow but do not settle the page's differing descriptions of historical coverage. Record discrepancies
and use modest sequential checks; never probe throttling or subscription limits
by load-testing the public service without a separate agreed plan. The one
agreed plan so far is a single bounded burst of 10 tiny requests on options PRO
(`live_p0 retry-timeout`, user-approved 2026-11-12; see AGENTS.md).

## Cases required as each slice is selected

For every supported endpoint/transport and available account profile, keep a
reviewed case record with the precise query, declared product/tier, evidence for
the expectation, expected status/phase, resource budgets and actual result.
Require both allowed-data and expected-denial cases where meaningful. Run the
same case after an account upgrade using a fresh in-memory authentication, and
retain the earlier result. Do not mutate a record to relabel a FREE run as paid.

| Outcome | Test treatment |
| --- | --- |
| Expected nonempty data and clean EOF | Pass only after value/shape checks and full completion; record rows and capture/replay evidence |
| Expected permission denial | Pass the **denial assertion** only for the configured code and phase; label it expected-denial, never data success |
| Expected denial but actual success | Mismatch; review changed entitlement/query assumptions; do not discard the unexpected result |
| Expected data but PERMISSION_DENIED | Failure/blocker for that profile and query; do not turn it into no-data, skip or retry |
| Other status, transport error or timeout | Failure/inconclusive service result, not an expected permission denial |
| Data then denial | Partial failure with delivered-batch count; never acceptable as a pre-data authorization rejection |
| Account/tier or adapter unavailable | Explicit not-run coverage gap; synthetic tests do not fill the live evidence cell |

Preserve numeric gRPC/HTTP codes and safe local categories; do not parse vendor
error text to identify a tier or choose a retry/upgrade action. Collect private
response envelopes when present. For denied calls, capture only safe status,
phase, query and provenance, with no account payload, error text or request token.
The user-approved one-run maps are the only exceptions (see `AGENTS.md`):
- the FREE entitlement map;
- the options PRO map;
- the stock, index, calendar and interest-rate run.

Their private, ignored reports keep truncated error text for a person to
classify locally. Only the classification and gRPC code are committed.
Use the existing independent mock error matrix for repeatable all-code coverage.

## Execution and release policy

All real calls remain optional, manual and outside CI. Credentials come from an
explicit environment key/file selection; no automatic discovery, shared CI
secrets, paid subscription purchase or account mutation. No credential should
appear in a case definition. The runner supports EOD capture/replay and a bounded
[stock quote denial probe](live-verification.md). Add endpoint-specific probes
as cases are selected, rather than running all 100 RPCs.

Python 1.0.13–1.0.16 descriptions document **service-side** changes, all
unverified here ([research](research/thetadata-1.0.13-1.0.17.md)):
- Snapshots return the latest message from the past 7 days, unless `current_day_only` is set.
- Greeks EOD defaults to the closing NBBO midpoint.
- Current-day trade-quote matching changed.
- Binomial Greeks price European index roots as European.

None of these affects stock EOD. When a snapshot, Greeks or trade-quote case
is selected, record the observed behavior and entitlement context rather than
assuming the documented defaults. Python cannot send `false` for
`option_use_nbbo` or `use_time_based_trade_quote`.

The account holder can run the same suite after upgrading or supply authorized
accounts with distinct tiers. Full paid-tier verification cannot be completed
with a FREE-only account. Each release must name verified profiles and not-run
cells; add requirements and cases before expanding the advertised surface.

Terminal REST, Terminal WebSocket and direct gRPC get separate case records and
error expectations. Connecting to the JAR does not bypass entitlement checks.
See [connection boundaries](connections.md), [release milestones](releases.md)
and [LIVE requirements](requirements/live-verification.md).
