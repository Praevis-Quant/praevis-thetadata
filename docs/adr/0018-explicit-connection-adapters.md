# ADR-0018: Explicit direct and Terminal connection adapters

Status: Proposed, 2026-10-04. Origin: Rust enhancement.
Owner: workspace/client maintainers.
Requirements: [CONN contracts](../requirements/connections.md),
[LIVE subscription evidence](../requirements/live-verification.md).

## Context

The Python-derived Rust implementation authenticates over hosted HTTPS and queries
MDDS using direct gRPC. The vendor's Terminal diagram describes another path:
a Java gateway exposing local REST and WebSocket APIs. Similar endpoint names
do not make their transport, error, value-format or lifecycle contracts identical.
Subscription levels are also independent of connector selection.

## Proposed decision

Keep the current direct path self-contained and Java-free. Add an optional REST
adapter for an operator-managed Theta Terminal v3 first. Select connection mode
explicitly; never silently switch transports when authentication, permissions
or decoding fails. Do not mix HTTP status numbers with gRPC codes or assume that
JSON/CSV prices preserve the same scale and precision as numeric protobuf values.

Keep optional JAR process supervision in application/tooling code, separate from
the transport-independent core and auth. Require an explicitly supplied JAR/config,
record the effective version/hash, distinguish an attached existing process from
one started by this application, bound readiness and shutdown, and protect logs
and credentials. Do not silently download/update/redistribute the vendor JAR.
Choose module/crate packaging when the adapter requirements are implemented;
do not reserve a speculative crate or preemptively fragment the client workspace.

Add the WebSocket event adapter under its own subscription lifecycle. Finite
gRPC batch delivery does not implement reconnects, event gaps, subscription
acknowledgements, multiplexing or slow-consumer policy for real-time streams.

Maintain manual entitlement evidence per product/tier/connector/endpoint/query.
Expected permission rejection is a separate assertion from successful data
delivery. Missing account access remains not-run; mock status coverage cannot
establish paid-tier interoperability. This extends testing policy, not a local
entitlement engine or a promise to purchase accounts.

## Alternatives

Requiring Terminal for every user would add Java/startup/configuration costs to
the working direct path. A universal endpoint URL switch would hide incompatible
protocols and status domains. Automatically falling back after permission denial
would obscure authorization failures and could duplicate requests.

## Consequences

Each adapter needs explicit value/error/resource contracts and its own synthetic
and manual checks. Shared request/value abstractions can be reused only where
semantics are proven. Benchmark each delivery path separately; a vendor gateway
performance claim does not demonstrate lower latency for our Rust consumers.
REST/JAR/WebSocket work remains planned, not implemented by this ADR.

See [connection inventory, diagram and sources](../connections.md),
[subscription verification](../subscription-testing.md), and
[library boundaries](0013-library-boundaries.md).
