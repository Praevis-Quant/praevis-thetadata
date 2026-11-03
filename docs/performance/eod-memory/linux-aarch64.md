# EOD client memory profile

Platform: linux aarch64. Method: Linux /proc/self/status: VmHWM (reset via clear_refs) and RssAnon.
Default policy: 2 jobs, 16 streams, 512 MiB shared budget, 128 MiB accounted per batch, ZSTD window log 23.

## libzstd decompression context (`ZSTD_sizeof_DCtx`)

| Window log | Context size (MiB) | Client accounting: window + 16 MiB (MiB) |
| ---: | ---: | ---: |
| 10 | 0.10 | 16.00 |
| 15 | 0.22 | 16.03 |
| 17 | 0.59 | 16.12 |
| 20 | 1.47 | 17.00 |
| 23 | 8.47 | 24.00 |

## Peak growth over an idle connected client (MiB)

| Workload | Mode | Engine | Streams | Batches | Rust heap peak | OS resident peak | OS private peak | Resident minus Rust heap | Time (ms) |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| large-mixed | none | numeric | 1 | 8 | 5.4 | 8.1 | 8.4 | 2.7 | 30 |
| large-mixed | none | numeric | 16 | 128 | 49.7 | 55.1 | 57.1 | 5.4 | 279 |
| large-mixed | none | table | 1 | 8 | 7.7 | 10.7 | 11.0 | 3.0 | 52 |
| large-mixed | none | table | 16 | 128 | 54.4 | 62.2 | 63.7 | 7.9 | 562 |
| large-mixed | zstd | numeric | 1 | 8 | 3.2 | 2.9 | 4.0 | -0.2 | 36 |
| large-mixed | zstd | numeric | 16 | 128 | 9.7 | 10.5 | 11.3 | 0.8 | 313 |
| large-mixed | zstd | table | 1 | 8 | 5.2 | 6.5 | 5.9 | 1.3 | 53 |
| large-mixed | zstd | table | 16 | 128 | 13.7 | 15.8 | 15.1 | 2.1 | 534 |
| nulls | none | numeric | 1 | 8 | 4.5 | 5.6 | 5.6 | 1.1 | 16 |
| nulls | none | numeric | 16 | 128 | 41.5 | 41.3 | 43.1 | -0.2 | 129 |
| nulls | none | table | 1 | 8 | 6.8 | 7.8 | 7.8 | 1.0 | 26 |
| nulls | none | table | 16 | 128 | 46.9 | 46.9 | 48.7 | -0.0 | 244 |
| nulls | zstd | numeric | 1 | 8 | 2.4 | 3.3 | 2.7 | 0.9 | 15 |
| nulls | zstd | numeric | 16 | 128 | 4.9 | 6.3 | 5.7 | 1.3 | 108 |
| nulls | zstd | table | 1 | 8 | 4.5 | 5.4 | 4.8 | 0.8 | 25 |
| nulls | zstd | table | 16 | 128 | 9.3 | 10.5 | 9.9 | 1.2 | 231 |
| prices | none | numeric | 1 | 8 | 6.6 | 9.7 | 10.7 | 3.1 | 34 |
| prices | none | numeric | 16 | 128 | 60.9 | 69.6 | 71.5 | 8.7 | 307 |
| prices | none | table | 1 | 8 | 7.9 | 11.3 | 12.2 | 3.4 | 43 |
| prices | none | table | 16 | 128 | 63.5 | 75.1 | 77.1 | 11.6 | 420 |
| prices | zstd | numeric | 1 | 8 | 3.9 | 3.1 | 4.1 | -0.8 | 37 |
| prices | zstd | numeric | 16 | 128 | 8.0 | 7.6 | 8.4 | -0.4 | 303 |
| prices | zstd | table | 1 | 8 | 4.5 | 5.4 | 4.8 | 0.9 | 39 |
| prices | zstd | table | 16 | 128 | 9.3 | 10.5 | 9.9 | 1.2 | 348 |
| timestamps | none | numeric | 1 | 8 | 4.7 | 6.4 | 6.4 | 1.7 | 26 |
| timestamps | none | numeric | 16 | 128 | 44.3 | 45.6 | 46.9 | 1.4 | 214 |
| timestamps | none | table | 1 | 8 | 9.9 | 12.1 | 12.1 | 2.2 | 125 |
| timestamps | none | table | 16 | 128 | 54.6 | 60.7 | 62.7 | 6.1 | 1316 |
| timestamps | zstd | numeric | 1 | 8 | 2.4 | 3.4 | 2.9 | 1.1 | 25 |
| timestamps | zstd | numeric | 16 | 128 | 4.9 | 6.6 | 6.0 | 1.6 | 187 |
| timestamps | zstd | table | 1 | 8 | 7.4 | 9.0 | 8.5 | 1.6 | 126 |
| timestamps | zstd | table | 16 | 128 | 15.1 | 17.7 | 17.2 | 2.6 | 1418 |
| text-24mib | none | numeric | 1 | 2 | 73.9 | 96.5 | 95.3 | 22.6 | 101 |
| text-24mib | none | numeric | 16 | 32 | 502.5 | 566.1 | 567.4 | 63.6 | 1030 |
| text-24mib | none | table | 1 | 2 | 73.6 | 95.6 | 97.3 | 22.0 | 107 |
| text-24mib | none | table | 16 | 32 | 485.7 | 573.5 | 574.6 | 87.8 | 956 |
| text-24mib | zstd | numeric | 1 | 2 | 56.0 | 46.5 | 47.9 | -9.5 | 41 |
| text-24mib | zstd | numeric | 16 | 32 | 112.3 | 97.6 | 98.6 | -14.7 | 243 |
| text-24mib | zstd | table | 1 | 2 | 56.0 | 47.0 | 46.2 | -9.0 | 40 |
| text-24mib | zstd | table | 16 | 32 | 112.3 | 96.1 | 97.4 | -16.2 | 283 |

Largest OS resident peak growth: 573.5 MiB (text-24mib none table x16).

## Stream-count sweep: text-24mib, none, numeric, 2 messages per stream (MiB)

| Streams | Batches | Rust heap peak | OS resident peak | Per stream (resident) |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 2 | 73.4 | 96.0 | 96.0 |
| 2 | 4 | 147.9 | 172.5 | 86.2 |
| 4 | 8 | 187.6 | 249.0 | 62.3 |
| 8 | 16 | 301.5 | 378.5 | 47.3 |
| 16 | 32 | 499.2 | 563.9 | 35.2 |

## Message-size sweep: 16 streams, none, numeric, 2 messages per stream (MiB)

| Message | Rust heap peak | OS resident peak | Rust heap per stream |
| --- | ---: | ---: | ---: |
| text-8mib | 177.2 | 209.0 | 11.1 |
| text-16mib | 344.6 | 402.0 | 21.5 |
| text-24mib | 507.3 | 586.4 | 31.7 |

## Stream-cap sweep: 16 concurrent queries, text-24mib, none, numeric (MiB)

`EodPolicy::concurrent_streams` limits how many queries are dispatched at once.

| concurrent_streams | Rust heap peak | OS resident peak | Time (ms) |
| ---: | ---: | ---: | ---: |
| 16 | 494.8 | 618.0 | 969 |
| 4 | 196.2 | 275.6 | 892 |
| 2 | 147.6 | 243.6 | 894 |
| 1 | 74.0 | 147.1 | 1115 |
