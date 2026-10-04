# ADR-0016: optional local filtering and projection

Status: Proposed. Date: 2026-10-04. Owner: core/client maintainers.
Origin: **Rust enhancement**; not a Python 1.0.12 query feature or service promise.
Requirements: [EXT-L2-003 and EXT-L3-005 through EXT-L3-007](../requirements/enhancements.md).

## Context

Consumers can benefit from selecting rows/columns before expensive presentation
or Python transfer. The EOD request has only symbol and dates: there is no
evidence of server-side arbitrary filtering/projection. Its business column
schema is still unverified.

## Proposed decision

Offer opt-in Rust batch transforms over validated numeric batches, separate
from request wire types and the baseline unfiltered stream. Initially select
explicit column names and typed column/constant comparisons, including an
explicit is-null predicate and conjunction. Build the plan once per schema;
reject missing/duplicate projection names and incompatible cell types. A normal
comparison on null does not select the row. Preserve matching row order and
the caller's selected column order; do not sort, deduplicate or round prices.
Validate predicate inputs even on rows an earlier condition would reject so
optimization does not change type-error behavior. No string-expression engine
or implicit coercions are required for the first transform.

Compile pure operations without coupling core to a network/runtime. Initially
keep core batch operations and a client stream adapter; extract a transform
crate only if scope/reuse justifies it. Apply predicates before projection;
predicates may use columns not returned. Unselected transforms do no work.
Keep one-batch bounded state, parent backpressure, errors, deadlines and
cancellation. If a batch filters to zero rows, return an empty batch with the
projected schema rather than falsely reporting EOF or silently fetching ahead.
No callback into Python per row on the native fast path.

Filtering after transport does not save vendor bandwidth or claim to avoid
all decoding. A future pushdown optimization needs an evidenced server field,
equivalent null/type semantics, and separate compatibility fixtures. Keep
sorting, joins, caches, analytics and window aggregation on the roadmap until
their memory and semantics are independently defined.

## Alternatives and consequences

Caller-side iteration already permits custom filtering but repeats validation
and schema lookup. A small optional batch adapter provides a reusable contract.
SQL/dataframe engines or filtering in generated requests add dependencies and
scope before a demonstrated need. Benchmark no-op and selective cases; more
functionality is not automatically faster. Filtering remains planned, with
no fixed EOD column vocabulary or implementation promised by this ADR.
