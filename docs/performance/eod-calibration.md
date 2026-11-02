# EOD resource-default calibration (alpha.2 step 3, 2026-11-02)

**Decision (maintainer, 2026-11-02): keep the current defaults and document the
effective ceilings below** (EOD-L3-023). No budget was raised. Tolerances for
future comparisons are in the [benchmark policy](benchmark-policy.md).

## Defaults under calibration

| Limit | Default | Limit | Default |
| --- | ---: | --- | ---: |
| `encoded_bytes` | 64 MiB | `cells` | 1,000,000 |
| `decompressed_bytes` | 64 MiB | `text_bytes` (per cell) | 1 MiB |
| `headers` | 128 | `header_bytes` (per header) | 1024 |
| `rows` | 100,000 | `allocated_bytes` (accounted, per batch) | 128 MiB |
| `zstd_window_log` | 23 | `concurrent_streams` (query cap) | 16 |
| `concurrent_jobs` | 2 | `shared_bytes` (across jobs) | 512 MiB |
| `inline_bytes` | 0 (always offload) | | |

## Effective ceilings: the accounted budget binds first

The per-dimension limits are independent maxima; they were never meant to fit
together (EOD-L3-023). Measured with the harness's `--extremes` mode on
synthetic batches, the **128 MiB accounted-allocation budget is reached
before** the other limits combine. The budget covers the simultaneously live
wire, decompressed, decoded and output buffers.

| Batch | Numeric engine | Table engine |
| --- | --- | --- |
| 8 columns of 1 MiB text cells: 6 rows (48 MiB) | **rejected** (`Resource("allocation bytes")`) | **rejected** |
| Same, 4 rows (32 MiB) | **rejected** | **rejected** |
| Same, 3 rows (24 MiB) | accepted | accepted |
| 128 headers × 1024 bytes, 7,812 integer rows (999,936 cells) | accepted | **rejected** |
| Same, 3,906 rows (499,968 cells) | accepted | accepted |

Rejections happen during preflight and accounting, before the batch is
materialized. They are terminal resource errors, as for any oversized batch.
The Table adapter accounts more per cell, because it materializes formatted
values, so its cell envelope at 128 wide columns is roughly 0.5-1M cells.

For scale: every live PROD EOD batch observed so far is about 230-243 encoded
bytes and 269-275 decompressed bytes for one 16-column row
([live observations](../research/live-observations.md)). That is five to six
orders of magnitude below these ceilings.

## Runner measurements at the query cap (16 streams)

These are dispatches of the before/after workflow with the same commit on both
sides (A/A), so the ratios also measure noise at the cap.

- **Standard workloads:** run 41384835621, 3 rounds, `--streams 16`.
- **Extremes:** run 41384841519, 2 rounds, `--extremes --samples 4 --warmups 1 --streams 16`.
- Commit `6f399f0`; the later lint-only commit does not change behavior.
- Times are p50 per batch for decode, and for the whole 16-stream x 3-batch
  delivery. Memory is the harness's requested Rust heap for one decode (the
  allocation probe), not RSS or C allocations.

### Standard 10,000-row workloads, 16 concurrent streams

Every case completed on both runners. Decode p50 ranged over the 6 reports per
case:

| Workload | Linux numeric | Linux Table | Windows numeric | Windows Table | Requested heap (numeric / Table) |
| --- | --- | --- | --- | --- | --- |
| large-mixed (none / zstd) | 2.8-2.9 / 3.1-3.2 ms | 5.1-5.3 / 4.5-4.6 ms | 4.8-4.9 / 5.3-6.1 ms | 7.7-9.0 / 7.5-8.0 ms | 2.6-3.9 / 5.6-6.9 MiB |
| nulls | 1.6-1.7 / 1.7-1.8 ms | 2.1 / 2.2 ms | 3.0-3.1 / 3.2 ms | 3.7-3.8 / 3.8-4.8 ms | 2.2-2.8 / 4.8-5.4 MiB |
| timestamps | 3.3 / 3.3-3.4 ms | 10.0-10.1 / 10.1-10.2 ms | 5.2-6.0 / 5.3-5.8 ms | 16.8-19.4 / 17.0-18.0 ms | 2.2-2.8 / 7.8-8.3 MiB |
| prices | 3.8-3.9 / 4.0-4.1 ms | 4.3 / 4.5-4.6 ms | 5.9-6.1 / 6.6-6.7 ms | 6.6-6.9 / 7.3-8.0 ms | 3.2-5.8 / 5.8-8.4 MiB |

Whole-cap delivery (16 streams x 3 batches) took 46-425 ms on Linux and
81-694 ms on Windows. The Table adapter's timestamp formatting is the
slowest case. At the cap, A/A decode ratios stayed within the approved
tolerances: the every-round minimum was at most 1.008 on Windows and 1.005 on
Linux. Single-round spikes reached 1.25 on Windows (Table nulls-zstd); the
policy's minimum-ratio rule absorbs them.

### Limit-sized batches, 16 concurrent streams

| Case (decompressed size) | Linux decode, numeric / Table | Windows decode, numeric / Table | Delivery (16 x 3), Linux / Windows | Requested heap / peak live |
| --- | --- | --- | --- | --- |
| text-max-none (24 MiB) | 4.4-4.5 / 4.3-4.5 ms | 15.1-15.5 / 15.1-15.4 ms | 2.3-2.4 s / 2.7-3.3 s | 48.0 / 48.0 MiB |
| text-max-zstd (24 MiB) | 8.0-9.2 / 8.0-9.2 ms | 36.7-40.1 / 37.8-39.9 ms | 0.56-0.60 s / 1.16-1.33 s | 87.9 / 56.0 MiB |
| headers-max-none (2.5 MiB, 499,968 cells) | 19.1-19.4 / 23.1-23.5 ms | 22.3-23.1 / 33.8-34.9 ms | 0.61-0.74 s / 0.94-1.23 s | 14.1 / 14.1 MiB numeric; 29.6 / 27.0 MiB Table |
| headers-max-zstd | 21.7-22.2 / 25.7-26.1 ms | 26.6-27.6 / 33.7-34.0 ms | 0.50-0.61 s / 0.71-1.03 s | 19.5 / 15.6 MiB numeric; 35.0 / 27.0 MiB Table |

All limit-sized batches completed at the 16-stream cap on both runners. With
two decode jobs and the 512 MiB shared budget, admission bounds concurrency. At
most two batches are decoded at once, so the worst requested heap in flight is
about 2 x 88 MiB, plus caller-retained batches. ZSTD text batches have the
highest requested heap (88 MiB), because the compressed input, decompressed
buffer and strings are live together. That stays within the 128 MiB per-batch
budget.

## Not covered (unchanged scope)

- RSS, libzstd's C allocations, transport buffers and fragmentation are not
  measured. The requested-heap probe is a Rust-allocation estimate
  (EOD-L3-021).
- Profiling transport and C memory is proposed as not applicable to alpha.2
  (decision D-C, pending).
- Timings are shared-runner observations and remain informational, as the
  benchmark policy specifies.
