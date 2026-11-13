# ADR-0022: an end-to-end emulator, dual-target scenarios and a scheduled live run

Status: Accepted, 2026-11-13 (user decision; its choices confirmed by the
Coordinator the same day). Does not block `0.1.0`.
Origin: **Rust enhancement**: test infrastructure of this project. The vendor
package has no emulator.
Owner: praevis-thetadata maintainers; the Coordinator owns the live schedule.

## Context

Too much evidence waits on market hours and the user's machine:
- the live option checks, the stream latency runs and the release-candidate
  report;
- every behaviour found so far live, such as a second login ending the
  first, or `NOT_FOUND` for an unknown symbol and for an empty range.

The synthetic coverage that exists is scattered:
- the client's loopback gRPC fixture (`tests/support`);
- the `loopback_server` example (for the Python tests);
- the stream crate's one-connection fake Terminal (`tests/loopback.rs`);
- per-test fixtures.

Each covers a few RPCs and a few behaviours. None is driven by what the live
runs recorded, so nothing says when a model has drifted from the service.

## Decision

Four parts, each landed as its own pull requests in this order.

### 1. An emulator: `praevis-thetadata-emulator`

**What it is:** a dev-only workspace crate (`publish = false`): a library
for Rust tests, plus a binary for Python tests and manual use. It absorbs
the loopback gRPC fixture, the `loopback_server` example and the fake
Terminal. Their tests move onto it, staying green at each step.

**Documented interfaces only.** The subscriber agreement forbids reverse
engineering:
- **gRPC:** all 100 RPCs of the checked-in descriptor, served through the
  proto crate's generated server stubs;
- **HTTP login,** including the tier fields;
- **the Terminal's WebSocket `/v1/events`:** quotes, the full trade stream,
  heartbeats, `REQ_RESPONSE` (including `SUBSCRIBED` for an unsubscribe)
  and the close handshake;
- **the Terminal REST calls we use** (today, the one list request of the
  login-conflict probe).

Nothing from the Terminal JAR's internals, and no FPSS.

**Driven by repository data:**
- **Responses:**
  - synthetic values generated from
    [`schemas.json`](../endpoints/schemas.json) (column names and kinds),
    deterministic per request, in both ZSTD and NONE framing;
  - **no real market data in the repository, ever.**
- **Refusals:** plan-tier profiles from [`status.json`](../endpoints/status.json).
  The emulator runs as FREE or as options PRO (stocks and indices FREE),
  refusing what the profile's recorded classification refused, with the
  same gRPC code and the tier named.
- **Sizes and timings:** from recorded evidence, each cited by its run ID:
  - the message-size histogram (`opt-msgsize-001`);
  - first-row delays, such as about 60 s for all-strike quotes.

  A time factor scales the delays down; CI runs it small.
- **Behaviours:**
  - `RESOURCE_EXHAUSTED` above the account's concurrency;
  - a second login ending the first (`UNAUTHENTICATED`, `SessionEnded`);
  - `NOT_FOUND`, identical for an unknown symbol and an empty range;
  - flat-file streaming;
  - the Terminal REST's 478 after an external login.

**The model lives in one committed file** in the crate. It holds only
codes, classifications, tiers, sizes and timings, each with the run that
observed it:
- **schemas** come from `schemas.json`;
- **tiers** come from `status.json`.

### 2. A dual-target scenario suite

**The same end-to-end scenarios run against `--target emulator|live`,** in
Rust and in Python:
- `query`, `job` and Parquet output;
- the vendor-named methods;
- the error kinds, and cancellation;
- the stream's subscribe, unsubscribe and close.

**They assert properties, not exact values:**
- row counts above zero;
- schemas equal to the registry;
- error kinds;
- no drops.

**The emulator target runs in CI on every pull request,** on Linux and
Windows. The live target never runs in CI.

### 3. A drift check

**A live run records what it observed:**
- schemas;
- gRPC codes;
- refusal tiers;
- timings.

**A check diffs that against the emulator's model.** Every difference is
reported as a finding for a person to review. **The model is never updated
automatically.** A change to it is a reviewed pull request citing the run.

### 4. A live procedure and schedule

**`docs/live-runbook.md` and one command** that runs the live target and
writes one private report under the ignored `artifacts/`. They are
generalised from the Coordinator's ad-hoc scripts (`market_run.sh`,
`close_run.sh`, `wsl_inner.sh`, `rc_live.sh`) and committed in the
repository. The run has two parts:
- **Market hours:** snapshots and stream latency.
- **Any time:** historical requests. The stream checks may use the dev
  replay feed off-hours.

**Schedule (user decision):** weekly during market hours, **and** before
every release.
- **The Coordinator owns the scheduling:** a Windows task plus a Cloudroom
  reminder, as today. The repository documents it.
- **Live runs stay manual or Coordinator-run,** never in CI. The approval
  rules in [AGENTS.md](../../AGENTS.md) still apply.

**What the weekly run is approved to include (user, 2026-11-13):**
- **Included:** the read-only option checks and the stock, index, calendar
  and interest-rate checks the user already approved, including their
  bounded flat-file probes (at most 1 MiB each).
- **Excluded:**
  - no requests above the account's concurrency;
  - no gated flat files (the option quote and stock quote files);
  - no full flat-file downloads.

AGENTS.md is updated in part 4 to record this as a standing approval for
the scheduled and pre-release runs.

## Consequences

- **The 20 wrappers without data-path evidence stay unverified.** Of the
  81 vendor wrappers, 60 have a registered schema. For the other 21 there
  is nothing observed to emulate:
  - under a profile that is entitled to them, the emulator answers
    `UNIMPLEMENTED` and names the gap;
  - it never invents a schema.

  **The emulator is a regression net for what has been observed.** It is
  not evidence of service behaviour that was never seen.
- **Live evidence is still required for releases.** The emulator replaces
  waiting, not the live report.
- **Each run's checks** follow the model as of that commit; drift is found
  by the scheduled live run.
- **No new dependencies are expected:**
  - tonic's server, from the proto crate's generated stubs;
  - tokio and `tokio-stream` (already a client dev-dependency);
  - prost, zstd and `serde_json`;
  - tungstenite's server-side handshake.

  Anything else is asked for first.
- **CI cost:** the Python job must build the emulator binary as well as
  the wheel, kept to one build per job. The scenario-suite pull request
  reports the added CI time. If it is large, it proposes the full set on
  one OS and a smoke subset on the other.

## Pull-request order (rule 3, one step per pull request)

1. This ADR.
2. **The emulator:**
   1. the gRPC side: all 100 RPCs, login, the profiles and the model file.
      `loopback_server` moves onto it.
   2. the behaviours: concurrency, sessions, `NOT_FOUND`, flat files and
      the time factor. The client's test fixture (`tests/support`) moves
      onto it here, with the scripted behaviours its tests need (delays,
      failing the first requests, pauses), so they are not ported twice.
      *Reordered from step 1 (Coordinator, 2026-11-13).*
   3. the Terminal WebSocket and REST. The stream crate's fake Terminal
      moves onto it.
3. **The scenario suite:** Rust and Python, with the emulator target in CI.
4. **The drift check.**
5. **The runbook and the live command,** with the AGENTS.md standing
   approval.
