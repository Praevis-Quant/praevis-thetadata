# Live-service observation records

The first bounded PROD run passed on 2026-10-04, as recorded below.
Synthetic tests remain synthetic evidence. Retain sensitive responses only under
private ignored artifacts; this record contains schema/counts/provenance, not rows.
See the [runbook](../live-verification.md) and [release gates](../releases.md).

## First PROD stock EOD observation (2026-10-04)

The account holder authorized PROD and explicitly selected the existing Windows
user-level `THETADATA_API_KEY`. Its value was loaded into the runner's process
environment without displaying or persisting it; no credential file was created.
The user also reported successful Python access, but this run did not execute
Python or perform an independent Python-versus-Rust value comparison.

| Field | Observed result |
| --- | --- |
| Run and time | Private local run `prod-aapl-env-001`; started `2026-10-04T22:56:45.846381600+00:00` |
| Environment/query | PROD; AAPL; start and end both `2024-01-02` |
| Source | Clean `53af2f6daaad44edb1108f0c57cd78819740fe6d`; Windows native execution |
| Upstream/protocol | Python baseline 1.0.12; descriptor SHA-256 `b5b7ba02cebfb7ec7d817af12012dd52dcd9a23ad0ce02fe04cce299d1310373` |
| Authentication/lifecycle | Authentication succeeded; one in-memory session reused; three sequential EOD calls, no retries or native-store writes |
| Captured response | One 232-byte re-encoded envelope; ZSTD algorithm 1; declared uncompressed size 269 bytes; clean EOF |
| Values | One row, 16 columns: eight integers, six exact prices and two timestamps; no null/text/boolean cells in this sample |
| Live comparison | Numeric and Table queries matched the captured reference's ordered exact-value digest |
| Offline comparison | Raw helper, numeric and Table replay matched on Windows and Ubuntu WSL; Linux replay used only synthetic loopback identity, not the real credential |
| Outcome | Passed for this query/account/environment/time; no package publication or full EOD/PERF acceptance |

Observed column order:
`created`, `last_trade`, `open`, `high`, `low`, `close`, `volume`, `count`,
`bid_size`, `bid_exchange`, `bid`, `bid_condition`, `ask_size`, `ask_exchange`,
`ask`, `ask_condition`.

The existing synthetic fixture uses eight deliberately synthetic column names
and covers nulls, text, booleans, integer extremes, prices and timestamps. This
live response exercises a different, sixteen-column shape through the same mock
transport on replay. Matching output adds real-envelope interoperability evidence;
shared core conversions still prevent this comparison from being an independent
value oracle. Keep existing exact-value synthetic tests and add constructed
endpoint-shaped cases when selecting broader schema coverage; do not hardcode
this one response as a guaranteed vendor schema.

The equal-date request returned one row. This alone does not establish general
date inclusivity, ordering across days, adjustments, token lifetime, entitlements
for other endpoints or sustained service performance. Null/price benchmark
regressions and all remaining acceptance-matrix clauses remain open.

| Retained provenance | SHA-256 |
| --- | --- |
| Windows executable | `d7f9caee29e7a59825191c84e638cb887ee7870ac642799c5f3b516218280a98` |
| Compiled source set | `54e4467d9b5e83e55d1d5de519784bea50e5b6a1e5259f95b7bfc7fc5ddedb2d` |
| Ordered semantic values | `069776c82a9d9491db01239e7bc8e0876a4385626173b397a5dbccdc1859eb43` |
| `batch-000.pb` | `9ac7178571cc281cc77dd73e81d485d72dcd168173087aacc87aa614ba376764` |
| `manifest.json` | `91af50e0f6d9c651a187ae55c722020427a29b647f97c4212d68912d9a1c017b` |
| `live.json` | `06f16cbe75ab54fde78e2057223a40ec42030b9a029c295d08960d9116a5b631` |
| Windows `replay-1791154606600321600-18384.json` | `33fb8bdc9a8f621f03266b2bed71c6d52f3bff202eab0fbd15daa1b0128314d3` |
| Linux `replay-1791154672930618841-521378.json` | `bcbc0b2731cde9c52eeed9ff1d0c8330f66d1ea8198c4e3623126ae8179c8667` |

All response files remain private and unreviewed for redistribution. No real
rows were promoted into public fixtures. The compile-time source hash and
capture file hashes are retained in every comparison record.

## Template for a reviewed run

| Field | Record |
| --- | --- |
| Date, reviewer, environment | UTC time and PROD/STAGE; no account email or user identifier |
| Client/protocol | Commit, clean/dirty, compiled source/executable hash, Rust/platform, tracked upstream version and descriptor hash |
| Query and scope | Exact symbol/start/end; expected entitlement described without copying account/subscription payloads |
| Private evidence | Local run ID and reviewed hashes; no downloadable public link to private data |
| Authentication/transport | Passed or safe failure code; no token, credential, headers or raw error text |
| Response structure | Ordered headers, observed value kinds, compression modes, batch/row counts and terminal state after private contents review |
| Exact-value checks | Price scales/mantissas, integers, nulls, timestamp zones; comparisons and any separate-call differences |
| Service semantics | Observed date endpoints/order/duplicates/adjustments/empty behavior; distinguish observation from a vendor guarantee |
| Synthetic comparison | Existing tests covering each shape; missing cases and clearly labelled constructed reproductions |
| Requirement/ADR impact | Affected IDs, upstream versus Rust enhancement implications, unresolved questions and follow-up |
| Release decision | Scope this evidence supports; remaining gaps; no universal entitlement, stability or performance claim |
| Public fixture review | Permission/evidence if sharing real rows, sensitive-content inspection, transformations and source/result hashes; otherwise private capture plus synthetic reproducer |

A successful query establishes only the observed account/environment/query/time.
Unexpected fields or schema differences initiate research; they do not authorize
silently changing local validation, price precision, lifecycle or resource policy.
