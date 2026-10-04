# ADR-0017: Manual live verification and private response replay

Status: Accepted for the verification workflow, 2026-10-04.
Origin: Rust enhancement. Owner: workspace/client maintainers.
Requirements: [LIVE contracts](../requirements/live-verification.md).
This accepts a testing policy, not live interoperability or the EOD baseline.

## Context

Synthetic fixtures verify local contracts but cannot establish that an account
can authenticate to the actual service or that actual EOD envelopes satisfy our
assumptions. The user requests optional credentialed checks outside CI and real
responses that can be compared with the mocks. Performance remains a primary
product objective; capture instrumentation need not run on the library hot path.

## Decision

Provide a separately feature-gated Rust developer example with explicit capture
and offline replay commands. Require a manually chosen environment, bounded
query, credential source and live consent. Sources are an explicit
`--api-key-env` opt-in to process `THETADATA_API_KEY`, an API-key file, or an
email/password file, with no discovery/fallback. Environment support was added
at the user's request on 2026-10-04; the auth library remains explicit-input-only.
Select PROD for initial integration;
STAGE is documented as a second independent connection, not a test sandbox.
Authenticate once, hold a session in memory, perform at most three sequential
EOD requests without retries and leave native persistence untouched.

Capture only known market-data envelope fields and their compressed payloads
into private ignored local artifacts. Retain ordered hashes, protocol/build
provenance and safe terminal categories. Keep auth bodies, request identity,
headers/trailers, status text and manifests with URLs out of artifacts. Captured
payloads remain unreviewed/private; this selection is not automatic sanitization
or permission to redistribute data. Real credentials never enter mock replay.

Replay verified complete captures through the existing loopback service and
compare raw, numeric and Table outputs. Keep independent synthetic edge/fault
oracles. Fail mismatches; record whether separate live calls may have observed
different service revisions. Preserve original captures and create new replay
reports. Promote actual data into public fixtures only after explicit contents
and redistribution review; prefer constructed reproductions when permission is
unestablished. Record those reproductions as synthetic.

The existing CI workflows remain credential-free and unchanged. The live tool
refuses execution when `CI` is set. The [release plan](../releases.md) requests a
maintainer-operated live record when claiming live compatibility; contributors
do not need an account to build/test the project. Failures or unavailable access
remain open release evidence, not silently skipped successes.

## Alternatives

Adding secrets or a manual live job to GitHub Actions would violate the selected
outside-CI policy. Capturing in the production decoder would add unnecessary
hot-path work. A Python comparison harness would add a runtime dependency and
still need independent semantic evidence. Treating STAGE as a free sandbox
would assume a vendor contract absent the documentation.

## Consequences

No new product dependency or capture callback is introduced. The manual tool is
checkout-only and must be excluded or made self-contained during package work.
Captures preserve known protobuf envelopes, not byte-exact transport frames or
unknown envelope fields. Initial replay requires completed captures; partial
captures support investigation, with dedicated failure-replay tooling deferred.
Live account/entitlement/ordering/date claims remain unresolved until recorded.

See [the runbook and vendor references](../live-verification.md),
[ADR-0011](0011-market-data-verification-and-artifact-retention.md) and
[release milestones](../releases.md).
