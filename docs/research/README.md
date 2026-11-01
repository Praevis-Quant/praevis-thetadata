# Research index

Evidence about the upstream ThetaData Python package and related work. The
compatibility baseline is the one recorded as `source_version` in
[the protocol manifest](../../crates/praevis-thetadata-proto/schema/manifest.json),
currently **1.0.12**. Research is evidence, not approved scope; see
[the documentation index](../README.md) for requirements and ADRs.

## Python 1.0.12 baseline (frozen)

These files are preserved byte-for-byte. `python tools/research_python.py --check` checks the current baseline and, once a newer baseline is adopted, also this frozen set, plus additions-only compatibility between the two.

| File | Content |
| --- | --- |
| [thetadata-1.0.12.md](thetadata-1.0.12.md) | Analysis: authentication, transport, conversion, presence, protocol, flat files and open contracts |
| [python-1.0.12-catalog.md](python-1.0.12-catalog.md) | Generated catalogue of every inventory item, with feature dispositions and traceability |
| [python-1.0.12-inventory.json](python-1.0.12-inventory.json) | Full source/protocol inventory of the 1.0.12 wheel |
| [python-1.0.12-dispositions.json](python-1.0.12-dispositions.json) | Feature decision ledger: supported, planned, excluded or unresolved |
| [vendor-1.0.12.json](vendor-1.0.12.json) | Original artifact hashes, wrapper-to-RPC map and OpenAPI comparison |
| [upstream-source.md](upstream-source.md) | Publishing provenance, sdist equivalence and artifact recovery |

## Python 1.0.13–1.0.17 (proposed update, not yet the baseline)

The generated files are checked offline by `python tools/upstream_delta.py check`.

| File | Content |
| --- | --- |
| [thetadata-1.0.13-1.0.17.md](thetadata-1.0.13-1.0.17.md) | Analysis: per-release and cumulative changes, documented server-side behavior, Rust impact and open questions |
| [python-1.0.12-to-1.0.17-delta.md](python-1.0.12-to-1.0.17-delta.md) | Generated cumulative change record, 1.0.12 → 1.0.17 |
| [python-1.0.13-delta.md](python-1.0.13-delta.md) | Generated change record, 1.0.12 → 1.0.13 |
| [python-1.0.14-delta.md](python-1.0.14-delta.md) | Generated change record, 1.0.13 → 1.0.14 |
| [python-1.0.15-delta.md](python-1.0.15-delta.md) | Generated change record, 1.0.14 → 1.0.15 |
| [python-1.0.16-delta.md](python-1.0.16-delta.md) | Generated change record, 1.0.15 → 1.0.16 |
| [python-1.0.17-delta.md](python-1.0.17-delta.md) | Generated change record, 1.0.16 → 1.0.17 |
| [python-1.0.13-1.0.17-delta.json](python-1.0.13-1.0.17-delta.json) | Machine-readable deltas, artifact identities and per-item history |
| [python-1.0.17-catalog.md](python-1.0.17-catalog.md) | Generated catalogue of every 1.0.17 item, with its status since 1.0.12 |
| [python-1.0.17-inventory.json](python-1.0.17-inventory.json) | Full source/protocol inventory of the 1.0.17 wheel |
| [vendor-1.0.17.json](vendor-1.0.17.json) | 1.0.17 summary in the shape of `vendor-1.0.12.json` |

The update decisions and steps are in [the 1.0.17 update plan](../upstream-update-1.0.17.md).

## Other research

| File | Content |
| --- | --- |
| [architecture-performance-review.md](architecture-performance-review.md) | Crate assessment and performance findings |
| [live-observations.md](live-observations.md) | Reviewed live-service observation records |
| [related-projects.md](related-projects.md) | Related project watchlist |
