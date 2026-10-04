# Live-service observation records

No live run has been completed by the initial manual-runner implementation.
Synthetic tests remain synthetic evidence. Use this template after a manually
authorized run; retain sensitive responses only under private ignored artifacts.
See the [runbook](../live-verification.md) and [release gates](../releases.md).

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
