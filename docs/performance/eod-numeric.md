# Bounded numeric EOD experiment

Status: implemented experiment; full EOD acceptance remains open. No live vendor
traffic or account is required. Raw helpers still use the retained decoder.
The [Windows/Linux results](eod-numeric-results.md) retain both gains and
regressions, raw-report/binary hashes and the final measured source revision.
The subsequent [resource audit](eod-resource-audit.md) supersedes the initial
envelope/accounting/admission and always-offload policy described below. This
document retains the first experiment's design and measurement context.

## Design and provisional policy

`stock_eod_batches(StockEodRequest)` delivers immutable-schema `DataBatch`
values in flat row-major storage. Numeric timestamps preserve epoch milliseconds
and a validated source zone; exact prices never pass through floating point.
`stock_eod` uses the same engine and explicitly converts batches to the existing
Table representation. It formats RFC 3339 timestamps on the decode worker.

The decoder preflights the wire without constructing a prost DataTable. It
counts headers, rows and cells, checks widths and text limits, then materializes
one bounded prost DataValue at a time into a single cell vector. This retains
prost's repeated/unknown/oneof semantics while removing intermediate row/value
vectors. Repeated headers reuse the stream's immutable Arc schema after equality
validation. Header decoding and duplicate checks still run for each batch; no
buffer cache or zero-copy claim is made.

| Setting | Experimental default | Reason / boundary |
| --- | --- | --- |
| Encoded / decompressed payload | 64 MiB each | Retains baseline byte allowance; independent actual expansion check |
| gRPC message allowance | Encoded ceiling + 1 KiB | Finite outer envelope allowance, separate from DataTable preflight |
| Headers / bytes per header | 128 / 1,024 | Generous for synthetic 8-column batches; no claimed vendor maximum |
| Rows / total cells | 100,000 / 1,000,000 | Covers benchmark range while bounding tiny-wire null expansion |
| UTF-8 bytes per text occurrence | 1 MiB | Applies even to subsequently overwritten protobuf fields |
| Accounted storage per admitted batch | 128 MiB | Includes simultaneously live buffers, flat cells and optional Table conversion |
| ZSTD window | 8 MiB (`window_log_max = 23`) | Explicit decoder window ceiling; 16 MiB extra workspace allowance |
| Concurrent receive/decode slots | 2 per client and its clones | Admission occurs before receiving ResponseData; no unbounded offload queue |
| Shared accounted reservation | 512 MiB | Per-batch worst-case reservation plus separately held schema leases |
| Decode scheduling | One blocking job per requested batch | Protects current-thread executor; small-message overhead is measured |

Call `ThetaClient::with_eod_policy` to validate overrides before connection.
Defaults are provisional, not an accepted tuning policy or a process-RSS limit.
Admission can reduce concurrency below the job setting when memory is scarce.
Idle receivers occupy slots; there is no promise of independent-stream fairness.
Whole-query deadlines bound admission waits and caller pauses. Idle time starts
only when the stream actually waits for a message. Delivery is pull-driven with
no application prefetch. Transport-internal buffering is separate.

Requested Rust storage is conservatively accounted before materialization:
wire capacity plus wire length for retained text, cell capacity, header/Arc/sort
storage and transient per-cell decoding. Table mode additionally accounts for
the old and new cell vectors, row vectors, header copies and 64 bytes of
formatted timestamp storage per cell. Decompression checks old plus replacement
capacity before growth, input capacity and the ZSTD allowance. Allocator metadata,
fragmentation, thread stacks, tonic/h2 buffers and exact libzstd allocator usage
are outside these estimates. The outer ResponseData prost decode is still owned
by tonic; audit adversarial envelope allocation before claiming complete
EOD-L3-021/022 compliance. An envelope byte ceiling alone is not that proof.

Reservations move into blocking work and its returned result. Dropping a pending
call cancels transport consumption; already-running work can finish while
retaining its reservation. Schema leases persist with the stream and any active
worker references. Delivered batches belong to the caller and are outside the
library budget. Multiple independently constructed clients have separate pools.

## Common-harness comparison

The extended Rust harness has `--engine legacy`, `--engine table` and
`--engine numeric`. The first compiles the unchanged original decoder and uses
the original raw helper. The other two use the candidate decoder and public
typed stream. Every mode checks all values against the same independent fixture
oracle. Numeric expectations are derived from the oracle's formatted timestamp
instants/zones before measurement; numeric consumers compare numeric values.

Legacy versus Table compares identical output representations. Numeric versus
legacy measures the benefit of bypassing presentation work as well as the layout
change; it must not be described as identical formatting work performed faster.
Direct decode excludes input clone and result equality/drop. End-to-end delivery
includes the corresponding exact-value consumer. Allocation probes include input
clone and Rust allocations only; they do not measure C ZSTD allocations or RSS.

Run one release binary per platform, no competing builds, with defaults of
10,000 rows, 12 samples, 3 warmups, four streams and three batches per stream:

```text
cargo build --release --locked -p praevis-thetadata-client --example eod_benchmark
eod_benchmark run --engine legacy  --output artifacts/eod/legacy-1.json
eod_benchmark run --engine table   --output artifacts/eod/table-1.json
eod_benchmark run --engine numeric --output artifacts/eod/numeric-1.json
eod_benchmark run --engine numeric --output artifacts/eod/numeric-2.json
eod_benchmark run --engine table   --output artifacts/eod/table-2.json
eod_benchmark run --engine legacy  --output artifacts/eod/legacy-2.json
eod_benchmark compare --baseline artifacts/eod/legacy-1.json --candidate artifacts/eod/table-1.json --output artifacts/eod/table-comparison.json
eod_benchmark compare --baseline artifacts/eod/legacy-1.json --candidate artifacts/eod/numeric-1.json --output artifacts/eod/numeric-comparison.json
```

Use the executable in `target/release/examples` (with `.exe` on Windows), or an
isolated Linux target directory. Reports embed compiler, source/fixture/harness,
lockfile, descriptor and executable fingerprints, source revision/dirtiness and
raw samples. Both implementations use the same binary/toolchain/dependencies;
this controls build differences rather than comparing incompatible v1 reports.
The original [baseline evidence](eod-results.md) remains unchanged. Version 2
reports cannot be compared directly with version 1: rerun both engines using
the common harness. Forward/reverse runs describe variability, not a calibrated
SLA or statistical confidence bound. Twelve-sample p95 is the maximum sample.

## Remaining acceptance work

Audit the outer protobuf envelope, allocator/capacity assumptions, shared-pool
saturation and cancellation during active decode with deterministic injection.
Complete header/deadline/empty-trickle fault tests, all requirement boundary
cases, explicit no-auth/storage activity assertions and transport diagnostics.
Use repeated measurements to approve defaults and assess an inline small-batch
threshold, a dedicated worker pool or buffer reuse before changing policy.
Live service schema, entitlements and date semantics remain unverified.
