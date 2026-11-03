# EOD client memory profile

Platform: linux x86_64. Method: Linux /proc/self/status: VmHWM (reset via clear_refs) and RssAnon.
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
| large-mixed | none | numeric | 1 | 8 | 4.7 | 8.0 | 7.8 | 3.3 | 42 |
| large-mixed | none | numeric | 16 | 128 | 38.8 | 44.0 | 43.9 | 5.1 | 402 |
| large-mixed | none | table | 1 | 8 | 7.0 | 10.7 | 10.4 | 3.6 | 65 |
| large-mixed | none | table | 16 | 128 | 43.2 | 50.0 | 50.4 | 6.8 | 651 |
| large-mixed | zstd | numeric | 1 | 8 | 3.2 | 4.3 | 3.9 | 1.1 | 53 |
| large-mixed | zstd | numeric | 16 | 128 | 9.7 | 11.7 | 11.2 | 2.0 | 455 |
| large-mixed | zstd | table | 1 | 8 | 5.2 | 6.5 | 5.8 | 1.3 | 63 |
| large-mixed | zstd | table | 16 | 128 | 13.8 | 17.1 | 16.4 | 3.3 | 537 |
| nulls | none | numeric | 1 | 8 | 4.2 | 5.6 | 5.3 | 1.5 | 27 |
| nulls | none | numeric | 16 | 128 | 36.9 | 37.4 | 37.7 | 0.5 | 235 |
| nulls | none | table | 1 | 8 | 6.5 | 8.2 | 7.8 | 1.7 | 38 |
| nulls | none | table | 16 | 128 | 41.4 | 49.1 | 49.2 | 7.8 | 338 |
| nulls | zstd | numeric | 1 | 8 | 2.4 | 3.4 | 2.7 | 1.0 | 24 |
| nulls | zstd | numeric | 16 | 128 | 4.9 | 6.2 | 5.7 | 1.2 | 183 |
| nulls | zstd | table | 1 | 8 | 4.5 | 5.5 | 4.8 | 1.0 | 33 |
| nulls | zstd | table | 16 | 128 | 9.3 | 10.3 | 9.8 | 1.0 | 250 |
| prices | none | numeric | 1 | 8 | 5.3 | 9.6 | 9.4 | 4.4 | 53 |
| prices | none | numeric | 16 | 128 | 39.8 | 51.6 | 51.9 | 11.7 | 544 |
| prices | none | table | 1 | 8 | 6.6 | 10.7 | 10.4 | 4.1 | 59 |
| prices | none | table | 16 | 128 | 42.2 | 52.8 | 53.1 | 10.6 | 629 |
| prices | zstd | numeric | 1 | 8 | 3.9 | 4.3 | 4.0 | 0.5 | 53 |
| prices | zstd | numeric | 16 | 128 | 8.0 | 8.8 | 8.3 | 0.8 | 474 |
| prices | zstd | table | 1 | 8 | 4.5 | 5.4 | 4.8 | 0.9 | 50 |
| prices | zstd | table | 16 | 128 | 9.3 | 10.4 | 9.8 | 1.1 | 372 |
| timestamps | none | numeric | 1 | 8 | 4.3 | 5.8 | 5.7 | 1.5 | 40 |
| timestamps | none | numeric | 16 | 128 | 38.0 | 40.4 | 40.6 | 2.4 | 368 |
| timestamps | none | table | 1 | 8 | 9.5 | 12.1 | 11.8 | 2.6 | 153 |
| timestamps | none | table | 16 | 128 | 48.3 | 53.3 | 53.5 | 5.0 | 1467 |
| timestamps | zstd | numeric | 1 | 8 | 2.4 | 3.5 | 2.9 | 1.1 | 36 |
| timestamps | zstd | numeric | 16 | 128 | 5.0 | 6.5 | 6.0 | 1.5 | 285 |
| timestamps | zstd | table | 1 | 8 | 7.4 | 8.9 | 8.4 | 1.5 | 153 |
| timestamps | zstd | table | 16 | 128 | 15.1 | 17.8 | 17.1 | 2.8 | 1336 |
| text-24mib | none | numeric | 1 | 2 | 50.0 | 75.6 | 75.2 | 25.5 | 188 |
| text-24mib | none | numeric | 16 | 32 | 128.6 | 227.6 | 227.8 | 99.0 | 1942 |
| text-24mib | none | table | 1 | 2 | 49.9 | 74.8 | 74.5 | 24.9 | 189 |
| text-24mib | none | table | 16 | 32 | 128.8 | 230.1 | 230.2 | 101.3 | 2351 |
| text-24mib | zstd | numeric | 1 | 2 | 56.0 | 48.4 | 48.1 | -7.6 | 71 |
| text-24mib | zstd | numeric | 16 | 32 | 112.3 | 98.7 | 98.5 | -13.6 | 421 |
| text-24mib | zstd | table | 1 | 2 | 56.0 | 48.6 | 47.8 | -7.4 | 65 |
| text-24mib | zstd | table | 16 | 32 | 112.3 | 98.9 | 98.5 | -13.4 | 425 |

Largest OS resident peak growth: 230.1 MiB (text-24mib none table x16).

## Stream-count sweep: text-24mib, none, numeric, 2 messages per stream (MiB)

| Streams | Batches | Rust heap peak | OS resident peak | Per stream (resident) |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 2 | 49.5 | 74.8 | 74.8 |
| 2 | 4 | 99.3 | 104.3 | 52.1 |
| 4 | 8 | 103.8 | 163.3 | 40.8 |
| 8 | 16 | 111.7 | 211.9 | 26.5 |
| 16 | 32 | 128.4 | 227.1 | 14.2 |

## Message-size sweep: 16 streams, none, numeric, 2 messages per stream (MiB)

| Message | Rust heap peak | OS resident peak | Rust heap per stream |
| --- | ---: | ---: | ---: |
| text-8mib | 63.7 | 116.4 | 4.0 |
| text-16mib | 96.7 | 182.2 | 6.0 |
| text-24mib | 128.5 | 253.1 | 8.0 |

## Stream-cap sweep: 16 concurrent queries, text-24mib, none, numeric (MiB)

`EodPolicy::concurrent_streams` limits how many queries are dispatched at once.

| concurrent_streams | Rust heap peak | OS resident peak | Time (ms) |
| ---: | ---: | ---: | ---: |
| 16 | 128.5 | 258.0 | 2130 |
| 4 | 103.7 | 202.4 | 2281 |
| 2 | 99.7 | 197.2 | 2084 |
| 1 | 50.1 | 101.5 | 2457 |
