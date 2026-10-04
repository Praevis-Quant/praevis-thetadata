# Architecture decision records

All records below are accepted project decisions for the initial auth baseline (2026-10-03). “Accepted” records the implemented design; it does not imply a vendor has validated undocumented service contracts.

| ADR | Decision |
| --- | --- |
| [0001](0001-standalone-auth.md) | Keep authentication independent of transport and applications |
| [0002](0002-auth-contract-and-credentials.md) | Pin the observed wire contract and explicit credential precedence |
| [0003](0003-native-session-persistence.md) | Store sessions in the native Windows/Linux credential store |
| [0004](0004-explicit-session-lifecycle.md) | Separate local persistence from remote session validity |
| [0005](0005-verification-and-provenance.md) | Use reproducible provenance, synthetic CI, and measured performance |

New decisions should include status/date, context, decision, alternatives, consequences, requirement links, and supporting evidence. Supersede a record when a decision changes rather than rewriting its historical rationale.
