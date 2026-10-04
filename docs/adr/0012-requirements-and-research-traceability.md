# ADR-0012: central outcomes, component detail, and source traceability

Status: Proposed. Date: 2026-10-04. Owner: Praevis-Quant maintainers.
Requirements: [EOD-L2-012](../requirements/L2-stock-eod.md) and
[EOD-L3-028](../requirements/L3-stock-eod.md);
existing AUTH requirement IDs and definitions remain unchanged.

## Context

The initial research had 531 inventory items and 26 feature dispositions;
the selected EOD slice adds a 27th disposition without removing inventory. A large
catalogue can still lose behavior during implementation if requirements and
tests do not trace back to it. Duplicating L1/L2/L3 trees in every crate would
split cross-component outcomes and create conflicting definitions. Placing
all future detail in one file would eventually obscure component ownership.

## Proposed decision

Adopt the hybrid organization described in [the documentation index](../README.md):

| Level / record | Canonical location | Responsibility |
| --- | --- | --- |
| L1 product/user outcomes | `docs/requirements/` | Workspace/product owner |
| L2 system behavior and boundaries | `docs/requirements/` | Cross-component architecture owner |
| Cross-component L3 | `docs/requirements/` | Named collaborating component owners |
| Substantial independent component L3 | `crates/<name>/docs/requirements/` or `apps/<name>/docs/requirements/` | Owning component |
| ADRs | `docs/adr/` | Decisions, alternatives, consequences, status |
| Source observations/dispositions | `docs/research/` | Versioned evidence and explicit gaps |

For the first market-data slice, keep its L1/L2 and initial L3 centrally while
the contract spans auth, client, proto, and core. Name the implementation owner
on each L3 row. Move independent details beside a crate only when their size
and ownership justify it; preserve IDs and update the central index. Do not
create empty requirement files just to mirror directories.

Each requirement has one permanent canonical ID, parent links, owner,
verification method, and evidence/status. Preserve distinctions between
planned verification, inspection, passing synthetic tests, and live evidence.
Never renumber, reuse retired IDs, or mark a product feature complete because
one generated function exists. The repository checker now audits canonical
definitions, duplicates, references, and immediate-level parents in AUTH and
new namespaces. These structural checks do not prove implementation or test
coverage and do not approve a market-data baseline.

Trace selected work along this chain:

```text
versioned source item -> research/disposition -> ADR -> L1 -> L2 -> L3
                                                       -> implementation
                                                       -> verification evidence
```

Links must resolve in both directions through the central index and feature
ledger. Keep unselected items visible as planned/unresolved or explicitly
excluded with rationale; a narrower first slice must not silently delete the
remaining wrappers/RPCs from compatibility accounting. Preserve the original
research and verbatim performance review; add corrections/reconciliations.

## Alternatives

- Full L1/L2/L3 trees in each crate duplicate outcomes and obscure ownership
  of authentication, wire mapping, and output behavior across crates.
- A single permanent monolithic specification is easy initially but makes
  substantial component-specific detail harder to maintain independently.
- Treating the roadmap or generated inventory as requirements bypasses
  deliberate scope/default/error decisions and acceptance criteria.

## Consequences and next requirements work

The EOD slice now has [defined L1/L2/L3 contracts](../requirements/L1-stock-eod.md),
linked to ADRs 0007-0011 and source items, with explicit planned verification
and structural parent/reference checks. Next link implementation and passing
evidence, and close the measured resource-policy gate. These proposed ADRs
remain design records until reviewed and implemented according to the local
ADR status convention; they do not change the accepted auth baseline.

## Supporting evidence

- [Existing requirements organization](../README.md#requirements-organization).
- [Feature ledger](../research/python-1.0.12-dispositions.json) and
  [coverage matrix](../research/python-1.0.12-catalog.md).
- [Current repository checker](../../tools/check_repository.py), which should
  not be mistaken for a complete feature-to-test traceability checker.
