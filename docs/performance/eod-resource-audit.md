# EOD resource and cancellation audit

Scope: follow-up to the [numeric experiment](eod-numeric.md), 2026-10-04.
This records reviewed code paths and synthetic verification, not live service
validation or acceptance of every stock EOD requirement.

Follow-up: the [acceptance matrix](../requirements/stock-eod-matrix.md) and
[resource-calibration results](eod-acceptance-results.md) supersede the open
local-frame/ZSTD-window error-classification gap below. They also add bounded
query admission and dedicated HTTP/2 receive credit after larger concurrent
workloads exposed a progress failure. Full acceptance and measured decode
regressions remain open; the original audit is retained as evidence.

## Findings and changes

| Finding | Change | Verification |
| --- | --- | --- |
| Generated outer ResponseData decoding could allocate a manifest tree before rejecting it | Typed EOD uses a narrow tonic codec: one size-limited owned frame, followed by a bounded envelope parser. Manifest presence is rejected without decoding its contents. Unknown fields are skipped; repeated payload/scalar/message behavior follows prost. | Envelope duplicate/merge/malformed/overhead tests; allocation probe proves a 10,000-entry manifest rejection allocates only the input frame clone; typed RPC fixtures still verify complete wire requests |
| Raw byte length undercounted tiny String capacities and geometric growth | Account twice wire length plus eight bytes per cell, conservative transient cell storage and existing header/Table allowances | One-byte-string allocation probe measures actual peak, then lowers the budget below it and verifies rejection before materialization |
| Compressed input could remain live during decoded table construction | Explicitly drop the ZSTD decoder and compressed input before table preflight/materialization | Ownership inspection plus allocation/peak reports; malformed/expansion/window tests retained |
| Schema leases could consume the last available batch reservation | Partition the shared budget into fixed batch slots and a separate schema pool. Round reservations up and total budget down to KiB. | Minimum-budget test saturates schema storage while a batch slot remains available; excess acquisition fails and permits recover |
| Running cancellation had only indirect coverage | Per-pool test-only worker latch holds actual decode work at a deterministic boundary | Cancelled pending calls and local deadlines return no batch while the worker retains its job/schema leases; another cloned client cannot start work; reservations release after worker exit |
| Every tiny message paid blocking-task scheduling costs | Offer opt-in inline decoding only for NONE responses whose actual entire envelope is at most 4 KiB; default 0 retains offload. Compressed messages always offload. | Same-harness numeric/Table versus forced-offload measurements; limits and output validation remain identical |
| Inline empty batches could form a ready loop | Release the batch slot and yield when skipping a schemaless empty response | Empty-message trickle still reaches the whole-query deadline rather than resetting it indefinitely |

Tests live in [the envelope module](../../crates/thetadata-client/src/envelope.rs),
[typed lifecycle module](../../crates/thetadata-client/src/eod.rs),
[typed fixture suite](../../crates/thetadata-client/tests/eod_typed.rs) and
[allocation-instrumented Rust harness](../../crates/thetadata-client/examples/eod_benchmark.rs).
The worker latch is compiled only for unit tests; production and release
benchmark builds contain no latch, lock, wait or hook branch.

## Allocation accounting boundary

The codec receives a complete frame after tonic's configured encoded ceiling
plus 1 KiB check. It makes one exact-capacity owned copy. Policy validation
requires this to fit the per-batch reservation. The envelope parser borrows
fields during scanning, enforces the 1 KiB non-payload allowance and checks
frame capacity plus the final payload copy before allocating that copy. It
releases the frame before entering DataTable decoding. This also bounds
repeated or unknown envelope fields without materializing their object trees.

NONE moves its payload into preflight. ZSTD accounts input, decoder allowance,
and old plus replacement output capacity while growing; compressed input and
decoder are dropped before table construction. Table preflight accounts wire
capacity, retained text capacity, flat cells, transient cell decode, headers,
Arc conversion/duplicate sorting, and optional coexistence with formatted Table
storage. Checked arithmetic rejects overflow. The allocation tests instrument
requested Rust heap, including reallocation calls; they do not prove an RSS cap.

The shared pool reserves a complete per-batch ceiling for each admitted slot.
The slot is acquired before receiving an owned frame and retained through
decode, conversion, abandoned worker completion, error cleanup or ownership
transfer to the caller. Schema leases are separate and shared with workers.
The number of slots is the smaller of configured concurrency and the number
that fits while leaving at least one schema reservation. Idle schemas cannot
consume slot capacity. Defaults allocate two 128 MiB batch reservations and
the remaining 256 MiB to schema reservations, within 512 MiB total.

Transport buffers (tonic/h2/TLS), allocation metadata/fragmentation, thread
stacks, caller-retained batches and exact libzstd C allocations remain separate.
The ZSTD window is capped at 8 MiB with a conservative additional 16 MiB
workspace allowance; it is not a custom C allocator enforcement mechanism.
Transport frame copies still execute in the codec on the polling thread;
large envelope parsing, decompression and table work execute on bounded workers.
Separate client constructions have separate pools. Raw helpers bypass these
typed-path guarantees. This is not a total-process memory limit.

## Scheduling and evidence limits

`EodPolicy::inline_bytes` defaults to **0** (bounded offload). Consumers can
opt in with 4096; values over 4096 are rejected before connecting. The entire
actual frame must fit,
and the decoded compression algorithm must be NONE. Neither compressed size
nor `original_size` grants eligibility. At most a few KiB of parser input is
handled inline; preflight and allocation checks still precede materialization.
An expired query cannot deliver an inline result. A polled pending future is
still terminal when dropped. Empty inline batches explicitly yield.

The benchmark adds `table-offload` and `numeric-offload` engines; each shares
its decoder, codec, fixture, output oracle and binary with the corresponding
`table`/`numeric` engine, which explicitly selects 4096 for this experiment.
Only scheduling differs. Run each pair in A/B/B/A order on
Windows and Linux. Do not compare old fixture/harness reports as though they
were this scheduling experiment. Original baseline and numeric reports remain
historical evidence.

The [scheduling results](eod-scheduling-results.md) show a Linux/WSL small-batch
gain but inconsistent Windows results. That does not justify a portable default
change or an OS-specific policy based only on one WSL host. The final default
remains 0; the measured, bounded inline policy is available explicitly. Tests
also observe the actual decode thread: small NONE work can stay inline, while
a tiny ZSTD frame with a forged small size hint still runs on a worker. A caller
pause longer than idle_timeout does not trigger prefetch or consume idle time.

The audit closes the identified application-buffer accounting and active-worker
cancellation gaps with the listed tests. Full EOD acceptance still needs the
complete requirement-by-requirement fault matrix, policy calibration over
larger/concurrent consumer workloads, transport-buffer profiling, and remaining
error-classification review. Tonic can report local frame rejection through the
same safe status-code interface as remote errors; the typed API deliberately
does not guess provenance from server-controllable status text. Window failures
likewise remain safe decode errors. These distinctions must be reconciled
before claiming exhaustive EOD-L3-016 acceptance. Live vendor semantics remain
unverified. No auth/store/CLI behavior or publishing configuration changes.
