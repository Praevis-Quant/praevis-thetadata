# Architecture decision records

Records identify their own date and status. “Accepted” records the implemented design; it does not imply a vendor has validated undocumented service contracts.

| ADR | Status | Origin | Decision |
| --- | --- | --- | --- |
| [0001](0001-standalone-auth.md) | Accepted | Rust enhancement | Keep authentication independent of transport and applications |
| [0002](0002-auth-contract-and-credentials.md) | Accepted | Mixed | Pin the observed wire contract and explicit credential precedence |
| [0003](0003-native-session-persistence.md) | Accepted | Rust enhancement | Store sessions in the native Windows/Linux credential store |
| [0004](0004-explicit-session-lifecycle.md) | Accepted | Rust enhancement | Separate local persistence from remote session validity |
| [0005](0005-verification-and-provenance.md) | Accepted | Mixed | Use reproducible provenance, synthetic CI, and measured performance |
| [0006](0006-cli-runtime-lifecycle.md) | Accepted | Rust enhancement | Initialize a current-thread CLI runtime only for HTTP authentication |
| [0007](0007-first-market-data-slice.md) | Proposed | Mixed | Start with one stock EOD history query through the reusable library |
| [0008](0008-typed-requests-and-presence.md) | Proposed | Mixed | Separate typed validation/defaults from wire representation and preserve explicit presence |
| [0009](0009-market-data-values-and-schema.md) | Proposed | Mixed | Preserve exact values, batch schemas, and explicit empty/partial-result semantics |
| [0010](0010-stream-lifecycle-and-resource-bounds.md) | Proposed | Mixed | Define backpressure, terminal errors, timeouts, cancellation, and measured resource limits |
| [0011](0011-market-data-verification-and-artifact-retention.md) | Proposed | Rust enhancement | Use Rust protocol fixtures/benchmarks and recoverable, hash-verified source inputs |
| [0012](0012-requirements-and-research-traceability.md) | Proposed | Rust enhancement | Keep L1/L2 central, place substantial component L3 with its owner, and trace source to verification |
| [0013](0013-library-boundaries.md) | Proposed | Rust enhancement | Keep cohesive libraries, optional storage and inward adapter dependencies |
| [0014](0014-performance-first-data-path.md) | Proposed | Rust enhancement | Make numeric batch delivery primary and measure consumer-facing performance |
| [0015](0015-upstream-and-enhancement-contracts.md) | Proposed | Rust enhancement | Protect local contracts independently of upstream compatibility updates |
| [0016](0016-optional-batch-transforms.md) | Proposed | Rust enhancement | Add optional bounded local filtering/projection without inventing vendor pushdown |

ADRs 0007-0016 define proposed designs and linked requirements; runtime
implementation/acceptance remains planned. Read 0007-0012 for the selected EOD
contract, then 0013-0016 for architecture, performance and extension policy.
0014 adds numeric delivery while retaining the Table contract of 0009.
Origin is independent of status; [the origin register](../requirements/origins.md)
names the protected policy for every mixed record and every requirement.

New decisions should include status/date, context, decision, alternatives, consequences, requirement links, and supporting evidence. Supersede a record when a decision changes rather than rewriting its historical rationale.
