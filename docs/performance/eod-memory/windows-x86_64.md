# EOD client memory profile

Platform: windows x86_64. Method: Windows K32GetProcessMemoryInfo: working set and private bytes, 1 ms sampler.
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
| large-mixed | none | numeric | 1 | 8 | 5.4 | 6.2 | 6.6 | 0.8 | 49 |
| large-mixed | none | numeric | 16 | 128 | 49.7 | 52.1 | 52.9 | 2.4 | 467 |
| large-mixed | none | table | 1 | 8 | 7.7 | 9.2 | 9.4 | 1.5 | 86 |
| large-mixed | none | table | 16 | 128 | 54.3 | 57.5 | 57.9 | 3.2 | 787 |
| large-mixed | zstd | numeric | 1 | 8 | 3.2 | 3.8 | 3.8 | 0.6 | 48 |
| large-mixed | zstd | numeric | 16 | 128 | 9.7 | 10.8 | 12.3 | 1.1 | 411 |
| large-mixed | zstd | table | 1 | 8 | 5.2 | 6.6 | 6.4 | 1.4 | 80 |
| large-mixed | zstd | table | 16 | 128 | 13.8 | 14.9 | 15.4 | 1.1 | 735 |
| nulls | none | numeric | 1 | 8 | 4.5 | 5.0 | 5.2 | 0.5 | 29 |
| nulls | none | numeric | 16 | 128 | 42.1 | 42.2 | 44.2 | 0.1 | 266 |
| nulls | none | table | 1 | 8 | 6.8 | 7.1 | 6.8 | 0.3 | 46 |
| nulls | none | table | 16 | 128 | 44.8 | 45.4 | 47.9 | 0.6 | 440 |
| nulls | zstd | numeric | 1 | 8 | 2.3 | 3.3 | 3.9 | 0.9 | 27 |
| nulls | zstd | numeric | 16 | 128 | 4.9 | 6.2 | 7.9 | 1.3 | 233 |
| nulls | zstd | table | 1 | 8 | 4.5 | 5.2 | 6.0 | 0.7 | 42 |
| nulls | zstd | table | 16 | 128 | 9.2 | 8.5 | 10.5 | -0.7 | 400 |
| prices | none | numeric | 1 | 8 | 6.6 | 7.1 | 6.9 | 0.6 | 56 |
| prices | none | numeric | 16 | 128 | 60.9 | 61.5 | 62.0 | 0.6 | 629 |
| prices | none | table | 1 | 8 | 7.9 | 8.6 | 8.7 | 0.7 | 74 |
| prices | none | table | 16 | 128 | 62.8 | 63.3 | 64.4 | 0.5 | 795 |
| prices | zstd | numeric | 1 | 8 | 3.8 | 4.6 | 5.4 | 0.8 | 48 |
| prices | zstd | numeric | 16 | 128 | 8.0 | 8.9 | 10.9 | 1.0 | 417 |
| prices | zstd | table | 1 | 8 | 4.5 | 5.7 | 5.7 | 1.2 | 65 |
| prices | zstd | table | 16 | 128 | 9.3 | 9.7 | 11.7 | 0.5 | 605 |
| timestamps | none | numeric | 1 | 8 | 4.7 | 5.4 | 5.2 | 0.7 | 53 |
| timestamps | none | numeric | 16 | 128 | 44.2 | 46.3 | 46.8 | 2.1 | 419 |
| timestamps | none | table | 1 | 8 | 9.9 | 11.5 | 11.4 | 1.6 | 176 |
| timestamps | none | table | 16 | 128 | 52.9 | 57.1 | 59.3 | 4.2 | 1647 |
| timestamps | zstd | numeric | 1 | 8 | 2.3 | 3.7 | 3.9 | 1.3 | 42 |
| timestamps | zstd | numeric | 16 | 128 | 4.9 | 6.4 | 8.0 | 1.4 | 368 |
| timestamps | zstd | table | 1 | 8 | 7.4 | 9.3 | 9.3 | 1.9 | 178 |
| timestamps | zstd | table | 16 | 128 | 15.0 | 17.8 | 18.2 | 2.7 | 1606 |
| text-24mib | none | numeric | 1 | 2 | 74.0 | 73.5 | 74.5 | -0.5 | 123 |
| text-24mib | none | numeric | 16 | 32 | 498.7 | 486.6 | 501.5 | -12.2 | 1374 |
| text-24mib | none | table | 1 | 2 | 74.0 | 72.9 | 74.5 | -1.1 | 120 |
| text-24mib | none | table | 16 | 32 | 508.1 | 487.9 | 510.8 | -20.2 | 1364 |
| text-24mib | zstd | numeric | 1 | 2 | 56.0 | 46.8 | 56.4 | -9.2 | 46 |
| text-24mib | zstd | numeric | 16 | 32 | 112.3 | 92.6 | 114.3 | -19.6 | 434 |
| text-24mib | zstd | table | 1 | 2 | 56.0 | 46.3 | 56.4 | -9.7 | 46 |
| text-24mib | zstd | table | 16 | 32 | 112.3 | 96.0 | 114.3 | -16.3 | 434 |

Largest OS resident peak growth: 487.9 MiB (text-24mib none table x16).

## Stream-count sweep: text-24mib, none, numeric, 2 messages per stream (MiB)

| Streams | Batches | Rust heap peak | OS resident peak | Per stream (resident) |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 2 | 74.0 | 73.3 | 73.3 |
| 2 | 4 | 147.2 | 139.9 | 70.0 |
| 4 | 8 | 190.9 | 180.5 | 45.1 |
| 8 | 16 | 301.0 | 285.8 | 35.7 |
| 16 | 32 | 508.6 | 488.0 | 30.5 |

## Message-size sweep: 16 streams, none, numeric, 2 messages per stream (MiB)

| Message | Rust heap peak | OS resident peak | Rust heap per stream |
| --- | ---: | ---: | ---: |
| text-8mib | 188.1 | 182.4 | 11.8 |
| text-16mib | 346.9 | 338.1 | 21.7 |
| text-24mib | 499.9 | 477.1 | 31.2 |

## Stream-cap sweep: 16 concurrent queries, text-24mib, none, numeric (MiB)

`EodPolicy::concurrent_streams` limits how many queries are dispatched at once.

| concurrent_streams | Rust heap peak | OS resident peak | Time (ms) |
| ---: | ---: | ---: | ---: |
| 16 | 508.2 | 502.9 | 1442 |
| 4 | 199.6 | 198.0 | 1639 |
| 2 | 148.0 | 147.4 | 1596 |
| 1 | 74.1 | 74.9 | 1727 |
