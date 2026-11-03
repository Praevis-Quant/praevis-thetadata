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
| large-mixed | none | numeric | 1 | 8 | 4.7 | 5.6 | 5.9 | 0.9 | 38 |
| large-mixed | none | numeric | 16 | 128 | 38.8 | 42.0 | 42.2 | 3.1 | 369 |
| large-mixed | none | table | 1 | 8 | 7.0 | 8.3 | 8.4 | 1.2 | 67 |
| large-mixed | none | table | 16 | 128 | 43.2 | 46.5 | 47.6 | 3.3 | 670 |
| large-mixed | zstd | numeric | 1 | 8 | 3.2 | 3.3 | 3.7 | 0.1 | 37 |
| large-mixed | zstd | numeric | 16 | 128 | 9.7 | 10.6 | 11.1 | 0.9 | 324 |
| large-mixed | zstd | table | 1 | 8 | 5.2 | 6.5 | 7.0 | 1.4 | 70 |
| large-mixed | zstd | table | 16 | 128 | 13.5 | 15.7 | 15.9 | 2.2 | 616 |
| nulls | none | numeric | 1 | 8 | 4.2 | 4.1 | 4.7 | -0.0 | 25 |
| nulls | none | numeric | 16 | 128 | 37.0 | 37.2 | 39.9 | 0.1 | 226 |
| nulls | none | table | 1 | 8 | 6.5 | 6.6 | 6.3 | 0.1 | 42 |
| nulls | none | table | 16 | 128 | 41.7 | 43.0 | 43.7 | 1.3 | 385 |
| nulls | zstd | numeric | 1 | 8 | 2.4 | 2.9 | 4.0 | 0.5 | 22 |
| nulls | zstd | numeric | 16 | 128 | 4.9 | 5.7 | 7.8 | 0.8 | 179 |
| nulls | zstd | table | 1 | 8 | 4.5 | 4.4 | 4.8 | -0.1 | 38 |
| nulls | zstd | table | 16 | 128 | 9.3 | 10.5 | 10.7 | 1.3 | 340 |
| prices | none | numeric | 1 | 8 | 5.2 | 5.8 | 5.6 | 0.6 | 46 |
| prices | none | numeric | 16 | 128 | 39.8 | 40.1 | 40.8 | 0.2 | 477 |
| prices | none | table | 1 | 8 | 6.6 | 7.3 | 7.7 | 0.7 | 62 |
| prices | none | table | 16 | 128 | 42.2 | 43.8 | 44.8 | 1.7 | 665 |
| prices | zstd | numeric | 1 | 8 | 3.9 | 3.8 | 5.5 | -0.1 | 38 |
| prices | zstd | numeric | 16 | 128 | 8.0 | 8.9 | 11.4 | 1.0 | 307 |
| prices | zstd | table | 1 | 8 | 4.5 | 5.5 | 6.5 | 1.0 | 54 |
| prices | zstd | table | 16 | 128 | 9.3 | 10.4 | 12.0 | 1.1 | 484 |
| timestamps | none | numeric | 1 | 8 | 4.3 | 4.9 | 5.1 | 0.5 | 33 |
| timestamps | none | numeric | 16 | 128 | 38.0 | 39.6 | 40.0 | 1.6 | 301 |
| timestamps | none | table | 1 | 8 | 9.5 | 11.4 | 11.4 | 1.9 | 148 |
| timestamps | none | table | 16 | 128 | 46.8 | 50.8 | 53.7 | 4.0 | 1452 |
| timestamps | zstd | numeric | 1 | 8 | 2.4 | 3.6 | 3.9 | 1.2 | 31 |
| timestamps | zstd | numeric | 16 | 128 | 4.9 | 5.7 | 6.9 | 0.8 | 247 |
| timestamps | zstd | table | 1 | 8 | 7.4 | 9.4 | 9.4 | 2.0 | 143 |
| timestamps | zstd | table | 16 | 128 | 15.0 | 18.0 | 18.6 | 2.9 | 1347 |
| text-24mib | none | numeric | 1 | 2 | 50.0 | 48.6 | 50.6 | -1.5 | 100 |
| text-24mib | none | numeric | 16 | 32 | 128.1 | 123.0 | 129.0 | -5.1 | 949 |
| text-24mib | none | table | 1 | 2 | 50.0 | 48.3 | 50.6 | -1.8 | 94 |
| text-24mib | none | table | 16 | 32 | 128.2 | 122.6 | 129.0 | -5.5 | 1028 |
| text-24mib | zstd | numeric | 1 | 2 | 56.0 | 48.5 | 56.8 | -7.5 | 39 |
| text-24mib | zstd | numeric | 16 | 32 | 112.3 | 96.4 | 114.0 | -15.9 | 355 |
| text-24mib | zstd | table | 1 | 2 | 56.0 | 47.1 | 55.9 | -8.9 | 38 |
| text-24mib | zstd | table | 16 | 32 | 112.3 | 95.4 | 114.8 | -16.9 | 375 |

Largest OS resident peak growth: 123.0 MiB (text-24mib none numeric x16).

## Stream-count sweep: text-24mib, none, numeric, 2 messages per stream (MiB)

| Streams | Batches | Rust heap peak | OS resident peak | Per stream (resident) |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 2 | 50.0 | 50.0 | 50.0 |
| 2 | 4 | 99.2 | 96.8 | 48.4 |
| 4 | 8 | 102.9 | 89.4 | 22.3 |
| 8 | 16 | 111.7 | 104.2 | 13.0 |
| 16 | 32 | 128.0 | 120.8 | 7.6 |

## Message-size sweep: 16 streams, none, numeric, 2 messages per stream (MiB)

| Message | Rust heap peak | OS resident peak | Rust heap per stream |
| --- | ---: | ---: | ---: |
| text-8mib | 63.0 | 59.3 | 3.9 |
| text-16mib | 96.1 | 93.6 | 6.0 |
| text-24mib | 127.9 | 123.0 | 8.0 |

## Stream-cap sweep: 16 concurrent queries, text-24mib, none, numeric (MiB)

`EodPolicy::concurrent_streams` limits how many queries are dispatched at once.

| concurrent_streams | Rust heap peak | OS resident peak | Time (ms) |
| ---: | ---: | ---: | ---: |
| 16 | 128.4 | 118.8 | 989 |
| 4 | 103.5 | 97.3 | 1060 |
| 2 | 100.1 | 97.8 | 1223 |
| 1 | 50.1 | 50.5 | 1341 |
