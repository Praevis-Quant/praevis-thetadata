# Upstream update plan: Python 1.0.12 → 1.0.17

Status: **approved 2026-11-01 (D1–D6 as recommended, with the D3 flat-file correction below); complete (steps 1–5 landed)**. Date: 2026-11-01. Each step's
Status line records what has landed. This document follows the
[upstream update template](upstream-update-template.md) and
[ADR-0015](adr/0015-upstream-and-enhancement-contracts.md). Evidence is in
[the 1.0.13–1.0.17 research](research/thetadata-1.0.13-1.0.17.md) and
[the generated delta](research/python-1.0.13-1.0.17-delta.json).

## Template record

| Field | Record |
| --- | --- |
| Baselines | Old: 1.0.12, wheel `d9a5f4d1…d565`, descriptor `b5b7ba02…0373`, 82 RPCs. New: 1.0.17, wheel `a43a2fe0…1fea`, regenerated descriptor `35572458…5a58`, 100 RPCs. Rust revision: base of this branch. The canonical manifest changes in step 3. |
| Source delta | Additive only: 18 RPCs (1 stock flat file, 17 futures), 36 messages, enum `Endpoints.TimestampPrecision`, `ZonedDateTime.nanos`/`precision`, 6 optional request fields (76 additions across 43 existing query messages), 1 wrapper and 6 wrapper parameters, and a Python expiration-format bug fix. No removals or renumbering. Auth, dependencies, conversion and the stock EOD request are unchanged. |
| Compatibility impact | The stock EOD request wire (EOD-L3-004) is unchanged. The response timestamp schema changed under EOD-L2-003 and EOD-L3-009. Raw generated bindings grow, and the `ZonedDateTime` struct is source-breaking for literals. |
| Enhancement impact | Exact values (EOD-L3-009, PERF-L2-001): sub-millisecond data is currently dropped silently. Typed presence (ADR-0008) is reinforced by Python's new false-cannot-be-sent cases. Numeric delivery and bounds are otherwise unaffected. Auth and native store are unaffected (AUTH-L3-003 shape unchanged). |
| Reconciliation | Adapt at the decode boundary (ARCH-L3-003). Preserve the deliberate deviations already recorded: no truthiness omission in typed requests, and strict zone/price validation. The timestamp policy is a new decision (ADR-0019, step 4). |
| Correctness | The full CI suite runs at each step. A trial build against the 1.0.17 descriptor compiles every library and binary; test targets fail only with `E0063` on `ZonedDateTime` struct literals. A search finds five such literals, all in client tests. |
| Performance | Existing synthetic fixtures never set fields 3/4, so the encoded bytes are identical. Step 3 should be performance-neutral. Step 4 adds a per-timestamp check and needs PERF-L3-007/008 before/after evidence. |
| Open evidence | Meaning of `nanos`/`precision`, and which RPCs emit them; gRPC defaults for absent booleans; futures support and entitlements; the snapshot and Greeks server-default changes. See [research questions](research/thetadata-1.0.13-1.0.17.md#open-questions-for-thetadata). |
| Acceptance | Pending maintainer review of this plan and the decisions below. |

## Decisions needed

| # | Decision | Options | Recommendation |
| --- | --- | --- | --- |
| D1 | Policy for non-millisecond timestamps (`precision = NANOSECOND`, non-zero `nanos`, or an unknown precision value) in the Table and numeric decoders | (a) Reject as a typed unsupported-response error until semantics are confirmed. (b) Truncate silently to milliseconds, as Python does. (c) Carry nanoseconds in `Timestamp`, which is a public API change that depends on unverified semantics. | **(a)**. It matches "unsupported content shall fail explicitly" (EOD-L2-003) and exact values. It is reversible to (c) once ThetaData confirms the semantics. The raw proto still exposes the fields. |
| D2 | Update target | Step through each version, or move directly to 1.0.17 while retaining the per-version evidence | **Directly to 1.0.17**. The per-version record already exists in the research document. |
| D3 | Disposition of new surfaces | Futures: unresolved or planned. `stock_flat_file_quote`: planned or excluded. | **Futures: `unresolved`** (no wrapper, docs or schema, as for corporate actions). ~~Flat-file quote: `planned` with the other flat files~~ **Corrected 2026-11-01: flat-file quote stays `unresolved` with the other flat files** (see the D3 correction below). New optional request fields take their existing endpoint dispositions. |
| D4 | Python comparator pin in [the comparison plan](performance/python-comparison.md) | Keep 1.0.12, or move to 1.0.17 | **1.0.17 after step 3**. The `stock_history_eod` and conversion code is identical, so the timings are comparable and the comparison targets the current vendor release. Keep 1.0.12 only if comparator work starts before step 3 lands. |
| D5 | Release placement | Before or after `0.1.0-alpha.1` | **Before alpha.1**. Otherwise the first published `praevis-thetadata-proto` ships a stale 82-RPC schema and needs a breaking follow-up immediately. |
| D6 | Research artifact layout | Overwrite the 1.0.12 inventory, or keep it frozen and add a 1.0.17 set | **Keep 1.0.12 frozen** (EOD-L3-028 and ADR-0012 forbid rewriting preserved research). Add `python-1.0.17-*` files and have the checker validate both. |

**D3 correction (2026-11-01).** The recommendation above assumed the seven
1.0.12 flat-file wrappers were `planned`. The 1.0.12 ledger actually records them
as `unresolved`: Python ignores the manifest and has no downloader. The tooling
puts `stock_flat_file_quote` under the same `flat-files` feature key, so the
maintainer chose option (a). The flat-file quote stays `unresolved` with its
siblings, and futures are `unresolved`. The 1.0.17 ledger records exactly this.

## Breaking and behavior-visible changes

1. **Source-breaking (proto crate):** `Endpoints::ZonedDateTime` gains
   `nanos: u32` and `precision: i32`. Any struct literal without
   `..Default::default()` stops compiling. In this workspace that affects only
   test support: `crates/praevis-thetadata-client/tests/support/mod.rs`,
   `tests/eod_observed_shape.rs` and `tests/eod_contracts.rs`. The crates are
   unpublished `0.1.0`, so no external consumer breaks. Record it in the
   CHANGELOG anyway.
2. **Raw API growth:** the generated client gains 18 methods. This is not a
   claim of support (see releases.md: generated bindings ≠ supported features).
3. **Runtime behavior (only if D1 = a):** a response containing a
   nanosecond-precision timestamp changes from silent truncation to a terminal
   unsupported-response error. Today no fixture or recorded live observation
   contains one.
4. **Vendor server-side semantics changed independently of any client version**
   (documented, unverified): snapshots look back 7 days; Greeks EOD defaults
   to the NBBO midpoint; binomial Greeks price European index roots as European.
   None of this affects stock EOD. Record it in subscription and live
   observations before any snapshot or Greeks slice is selected.

## PR-sized steps

Each step is one PR. Every step runs `python tools/check_repository.py`,
`python tools/research_python.py --check`, the Python tool tests,
`cargo fmt --check`, `cargo clippy -D warnings` and `cargo test --workspace`
on Windows and Linux.

### Step 1: research and plan (this PR)

- Add the analysis document and `tools/upstream_delta.py` with its offline tests.
  Add the generated 1.0.17 research set: per-release and cumulative change
  records, delta JSON, `python-1.0.17-inventory.json`, `vendor-1.0.17.json` and
  `python-1.0.17-catalog.md`. Keep the 1.0.12 files frozen, as D6 proposes.
  Add this plan, and index everything in docs/README.md and
  docs/research/README.md.
- No baseline, descriptor or Rust change. The monitor stays red by design.
- The new catalogue shows carried-forward 1.0.12 dispositions and marks new
  items `undecided`. Recording decisions waits for D3 and step 3.

### Step 2: version-parameterized research tooling (no baseline change)

Status: implemented in the step-2 PR.


- `tools/research_python.py`: fold in the inventory builder from
  `upstream_delta.py`, which already reproduces the 1.0.12 inventory exactly,
  so there is one implementation. Derive the paths (`python-<v>-inventory.json`,
  dispositions, catalogue, report) from the manifest `source_version`. Treat
  `vendor-1.0.12.json` plus the 1.0.12 inventory, catalogue and dispositions
  as **frozen historical evidence**: check their hashes, but stop validating
  them as the current baseline. Make the "original wrapper mapping" and
  "unmatched RPCs" checks relative to the frozen set plus the reviewed delta,
  so that additions are allowed and removals are not.
- Map the new RPCs to a new `futures` feature key in `decision_key` (it
  currently falls through to `wire-schema`). Keep `corporate-actions` as it is.
- `tools/extract_protocol.py`: read descriptors straight from a hash-verified
  wheel (`--archive`, as `research_python.py` already does) instead of an
  extracted directory, and take `source_version` from the wheel METADATA
  instead of the hard-coded `"1.0.12"`.
- Tests: extend `test_research_python.py` so that a removed or renumbered
  field, or a dropped wrapper between frozen and current, fails. Add a
  round-trip test for `extract_protocol.py` on a synthetic wheel.
- Requirements: EXT-L3-002 and EOD-L3-028 (verification evidence only; no new IDs).

### Step 3: move the baseline to 1.0.17 (behavior-neutral)

Status: implemented in the step-3 PR.


- **proto:** regenerate `schema/thetadata.bin` with the step-2 tool.
  `manifest.json` becomes `source_version: "1.0.17"`, with `source_sha256`
  `_proto/endpoints_pb2.py` = `379104ca0510bc41575a7e7e3763e11c7018a444fdb126f672e497ef0f23a34d`
  and `_proto/v3grpc/endpoints_pb2.py` = `11cdd93caa8db12b903cb0883e801c6a043e9171fab42b0feb416c523768ea2a`,
  `descriptor_sha256` = `35572458782193a20b112e4b62c9179ca380d6f9dfd919b79494de584a3f5a58`
  and 100 `methods`. Update the `src/lib.rs` doc comment.
- **client tests:** add `..Default::default()` (or explicit
  `nanos: 0, precision: 0`) to the five `ZonedDateTime` literals. No
  library-code change is expected; the trial build confirmed this.
- **auth:** change the comment on the public client identifier to "Python
  1.0.12–1.0.17 (unchanged)". The value and request shape stay the same
  (AUTH-L3-003).
- **core / CLI:** no change.
- **Research:** make the step-1 `python-1.0.17-inventory.json` the checked
  current inventory. Add `python-1.0.17-dispositions.json` (27 carried forward
  plus `futures`, per D3) and re-render the catalogue with traceability links.
  Add the five attested commits to `upstream-source.md`, plus a recovery
  procedure for the 1.0.17 wheel.
- **PyPI checker:** no code change. `check_upstream.py` reads `source_version`
  and will report tracked 1.0.17 = latest 1.0.17, exit 0. Its synthetic tests
  use literal versions and stay valid. Run the workflow once with
  `workflow_dispatch` after merging and record the result in
  `upstream-monitor.md`.
- **Performance:** confirm that the EOD benchmark fixtures are byte-identical
  and record a PERF-L3-007 same-host before/after smoke comparison
  (all four engines). No re-baseline is expected.
- **Requirements:** EXT-L3-002, EXT-L3-003, EXT-L3-004, EOD-L3-024 and
  EOD-L3-028. Add origin-register entries only if new IDs are created.

### Step 4: timestamp precision policy (D1)

Status: implemented in the step-4 PR as EOD-L3-030 and ADR-0019.


- New **ADR-0019** "Timestamp precision under the 1.0.16 response schema",
  classified `mixed` in `origins.json`. Upstream supplies the fields; Rust
  protects exactness and explicit failure. It refines ADR-0009 (millisecond
  RFC 3339) and ADR-0014 (numeric epoch milliseconds) without rewriting them.
- New EOD L3 requirement(s), using the next free IDs with EOD-L2-003 as the
  parent. Do not reinterpret EOD-L3-009. Specify handling of `precision`
  MILLISECOND/NANOSECOND/unknown and of non-zero `nanos` under millisecond
  precision. Add matrix rows to `stock-eod-matrix.md` and the open question to
  `stock-eod-evidence.md`.
- **core:** extend `Timestamp::from_wire` and `timestamp()` (or add
  `_with_precision` variants to keep the existing helpers source-compatible)
  to accept the precision fields. Implement the D1 choice.
- **client:** pass `nanos`/`precision` from `decode.rs` (Table) and
  `bounded.rs` (numeric). Map rejection to the existing unsupported-response
  category from EOD-L3-016, with no payload or secret in the errors (EOD-L3-017).
- **Tests:** NONE/ZSTD fixtures covering MILLISECOND with nanos 0 (accepted,
  identical output), MILLISECOND with nanos > 0, NANOSECOND, and an unknown
  precision (2). Cover both decoders, partial-stream termination after
  delivered rows (EOD-L3-012), and the mock gRPC server (EOD-L3-024).
- **Performance:** PERF-L3-007/008 timestamp-heavy fixture comparison on
  Windows and Linux. Report the per-cell branch cost.

### Step 5: documentation and comparator alignment

Status: implemented in the step-5 PR. Follow-ups, including the ADR-0019
Table-decode observation and the missing Windows benchmark, are tracked in the
[ROADMAP](ROADMAP.md#respond-to-upstream-compatibility-changes).


- Apply the documentation list below that the earlier steps did not cover.
  Move the Python comparator pin to 1.0.17 (D4).
- Record the vendor server-side semantic changes as pending observations in
  [live observations](research/live-observations.md) and the
  [subscription matrix](subscription-testing.md). They need no code.

## Documentation update list

| Document | Change | Step |
| --- | --- | --- |
| README.md | Baseline "1.0.12" → "1.0.17 (originally derived from 1.0.12)"; protocol provenance becomes "81 methods; 100 RPCs including 2 corporate-action and 17 futures RPCs without wrappers"; link the new research | 3 |
| docs/README.md | Index rows for the research, delta, plan and ADR-0019 | 1, 4 |
| docs/ROADMAP.md | "Respond to upstream compatibility changes": record the 1.0.17 update. Add a futures research candidate (unresolved). Note that future typed option, snapshot and trade-quote slices must expose the new optional fields with explicit presence | 3 |
| docs/releases.md | "Generated bindings for 82 RPCs" → 100. Place the baseline update before alpha.1 (D5) | 3 |
| docs/performance/python-comparison.md | Comparator pin 1.0.12 → 1.0.17 (D4) | 5 |
| docs/upstream-monitor.md | "currently 1.0.12" → 1.0.17; add the latest live validation | 3 |
| docs/requirements: stock-eod-matrix, stock-eod-evidence, L3-stock-eod, origins.json/md | New timestamp-precision requirement(s), matrix rows, evidence and origin classification | 4 |
| docs/research: python-1.0.17-*, vendor-1.0.17.json, per-release delta records, research index | Generated 1.0.17 set (done here) | 1 |
| docs/research: upstream-source.md, python-1.0.17-dispositions.json | Attested commits, wheel recovery, dispositions and traceability | 3 |
| docs/adr: README, new 0019 | Precision decision; cross-reference from 0009 and 0014 | 4 |
| CHANGELOG.md | Baseline update, source-breaking proto change, timestamp policy | 3, 4 |
| crates/praevis-thetadata-proto/src/lib.rs, auth client-id comment | Version references | 3 |

## Out of scope

Typed support for futures, the new flat file or any new optional parameter.
Those follow the release train (0.2.0–0.4.0) and need their own ADRs and
requirements. This plan does not implement the Rust changes, and it does not
merge or modify PR #13.
