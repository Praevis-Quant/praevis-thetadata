# Architecture decision records

Records identify their own date and status. “Accepted” records the implemented design; it does not imply a vendor has validated undocumented service contracts.

| ADR | Status | Decision |
| --- | --- | --- |
| [0001](0001-standalone-auth.md) | Accepted | Keep authentication independent of transport and applications |
| [0002](0002-auth-contract-and-credentials.md) | Accepted | Pin the observed wire contract and explicit credential precedence |
| [0003](0003-native-session-persistence.md) | Accepted | Store sessions in the native Windows/Linux credential store |
| [0004](0004-explicit-session-lifecycle.md) | Accepted | Separate local persistence from remote session validity |
| [0005](0005-verification-and-provenance.md) | Accepted | Use reproducible provenance, synthetic CI, and measured performance |
| [0006](0006-cli-runtime-lifecycle.md) | Accepted | Initialize a current-thread CLI runtime only for HTTP authentication |
| [0007](0007-first-market-data-slice.md) | Proposed | Start with one stock EOD history query through the reusable library |
| [0008](0008-typed-requests-and-presence.md) | Proposed | Separate typed validation/defaults from wire representation and preserve explicit presence |
| [0009](0009-market-data-values-and-schema.md) | Proposed | Preserve exact values, batch schemas, and explicit empty/partial-result semantics |
| [0010](0010-stream-lifecycle-and-resource-bounds.md) | Proposed | Define backpressure, terminal errors, timeouts, cancellation, and measured resource limits |
| [0011](0011-market-data-verification-and-artifact-retention.md) | Proposed | Use Rust protocol fixtures/benchmarks and recoverable, hash-verified source inputs |
| [0012](0012-requirements-and-research-traceability.md) | Proposed | Keep L1/L2 central, place substantial component L3 with its owner, and trace source to verification |

ADRs 0007-0012 define the proposed next design. They do not add an implemented
market-data baseline or approve all inventoried features. Each identifies the
requirements and verification still to derive. Start with 0007, then read
0008-0011 for behavioral contracts and 0012 for their requirements organization.

New decisions should include status/date, context, decision, alternatives, consequences, requirement links, and supporting evidence. Supersede a record when a decision changes rather than rewriting its historical rationale.
