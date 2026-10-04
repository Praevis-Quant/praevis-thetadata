# Connection adapter requirements

Status: Defined for future adapter work; current direct functionality retained.
Date: 2026-10-04. Origin: Rust enhancement throughout.
Decision: [ADR-0018](../adr/0018-explicit-connection-adapters.md).
Reference: [connection inventory and diagram](../connections.md).

| ID | Owner | L1 outcome |
| --- | --- | --- |
| CONN-L1-001 | workspace/client | Consumers can deliberately select supported connection paths without confusing direct service access, local Terminal queries and real-time event subscriptions or adding Java to the direct path. |

| ID | L1 parents | Owner | L2 behavior |
| --- | --- | --- | --- |
| CONN-L2-001 | CONN-L1-001 | client/core | Keep connector selection, value formats, error domains, resource/lifecycle behavior and verification explicit per direct gRPC, Terminal REST and Terminal WebSocket adapter. |
| CONN-L2-002 | CONN-L1-001 | applications | Optional Terminal supervision remains separate from connecting to an operator-managed process and from core/auth/direct-client dependencies. |

| ID | L2 parents | Owner | L3 contract | Planned verification |
| --- | --- | --- | --- | --- |
| CONN-L3-001 | CONN-L2-001 | client | Use explicit connector configuration and supported URI/protocol validation; preserve direct defaults and no Java dependency. Do not fallback/retry through another connection after permission, authentication, transport or decode failure. | Configuration rejection, zero-fallback request counters and direct dependency/build checks. |
| CONN-L3-002 | CONN-L2-001 | client/core | Preserve source error domain/code and distinguish local failures; define explicit Terminal HTTP/WebSocket mappings. Before sharing a value API, verify response-format precision, nulls, timestamps, schema and completion against the direct contracts. | Per-adapter synthetic status/data corpus, precise-value comparisons and manually observed service examples. No implicit HTTP 471/gRPC 7 equivalence without adapter mapping. |
| CONN-L3-003 | CONN-L2-002 | applications | Initially attach to an explicitly configured operator-started Terminal. Future supervision accepts an explicit JAR/config, records effective version/hash, bounds readiness/shutdown, protects credentials/logs and stops only owned processes. No silent download/update or redistribution. | Fake process/loopback lifecycle tests; missing/wrong Java/JAR/version, port conflict, readiness timeout and attached-process preservation. |
| CONN-L3-004 | CONN-L2-001 | client | Define WebSocket acknowledgement, multiplexing, disconnect/re-subscription, gap/order, cancellation and bounded slow-consumer behavior independently from finite query streams before implementing event support. | Deterministic event server with faults, queue/resource measurements and optional entitled manual checks. |
| CONN-L3-005 | CONN-L2-001 | workspace/client | Document current/planned connector support, endpoints/defaults and source evidence; keep subscription checks per transport. Measure decoding and consumer delivery for each adapter without assuming vendor throughput claims apply. | Connection diagram/index review, LIVE evidence matrix and same-host before/after benchmarks when adapter code is selected. |
