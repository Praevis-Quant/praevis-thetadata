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
| large-mixed | none | numeric | 1 | 8 | 5.4 | 8.1 | 8.5 | 2.7 | 35 |
| large-mixed | none | numeric | 16 | 128 | 49.7 | 63.7 | 63.9 | 14.1 | 367 |
| large-mixed | none | table | 1 | 8 | 7.7 | 10.8 | 11.1 | 3.0 | 51 |
| large-mixed | none | table | 16 | 128 | 54.3 | 59.7 | 60.0 | 5.4 | 529 |
| large-mixed | zstd | numeric | 1 | 8 | 3.2 | 4.2 | 4.0 | 1.0 | 38 |
| large-mixed | zstd | numeric | 16 | 128 | 9.7 | 11.8 | 11.4 | 2.1 | 353 |
| large-mixed | zstd | table | 1 | 8 | 5.2 | 6.5 | 5.9 | 1.3 | 49 |
| large-mixed | zstd | table | 16 | 128 | 13.5 | 15.8 | 15.2 | 2.4 | 470 |
| nulls | none | numeric | 1 | 8 | 4.5 | 5.3 | 5.3 | 0.8 | 22 |
| nulls | none | numeric | 16 | 128 | 42.1 | 43.8 | 44.2 | 1.8 | 184 |
| nulls | none | table | 1 | 8 | 6.8 | 8.1 | 8.1 | 1.2 | 27 |
| nulls | none | table | 16 | 128 | 46.1 | 47.9 | 48.4 | 1.9 | 231 |
| nulls | zstd | numeric | 1 | 8 | 2.4 | 3.3 | 2.7 | 1.0 | 18 |
| nulls | zstd | numeric | 16 | 128 | 4.9 | 6.3 | 5.7 | 1.4 | 130 |
| nulls | zstd | table | 1 | 8 | 4.5 | 5.4 | 4.8 | 0.8 | 25 |
| nulls | zstd | table | 16 | 128 | 9.2 | 14.7 | 14.6 | 5.5 | 204 |
| prices | none | numeric | 1 | 8 | 6.6 | 10.4 | 10.7 | 3.8 | 42 |
| prices | none | numeric | 16 | 128 | 60.9 | 67.2 | 67.7 | 6.3 | 454 |
| prices | none | table | 1 | 8 | 7.9 | 12.0 | 12.2 | 4.1 | 50 |
| prices | none | table | 16 | 128 | 63.1 | 81.7 | 81.9 | 18.6 | 515 |
| prices | zstd | numeric | 1 | 8 | 3.9 | 4.4 | 4.1 | 0.6 | 45 |
| prices | zstd | numeric | 16 | 128 | 8.0 | 8.8 | 8.5 | 0.9 | 380 |
| prices | zstd | table | 1 | 8 | 4.5 | 5.3 | 4.8 | 0.8 | 39 |
| prices | zstd | table | 16 | 128 | 9.3 | 10.4 | 9.9 | 1.1 | 297 |
| timestamps | none | numeric | 1 | 8 | 4.7 | 6.0 | 6.1 | 1.3 | 30 |
| timestamps | none | numeric | 16 | 128 | 44.3 | 49.4 | 49.9 | 5.1 | 287 |
| timestamps | none | table | 1 | 8 | 9.9 | 12.0 | 12.2 | 2.1 | 118 |
| timestamps | none | table | 16 | 128 | 53.8 | 63.3 | 63.5 | 9.4 | 1035 |
| timestamps | zstd | numeric | 1 | 8 | 2.4 | 3.4 | 2.9 | 1.1 | 27 |
| timestamps | zstd | numeric | 16 | 128 | 4.9 | 6.6 | 6.0 | 1.6 | 201 |
| timestamps | zstd | table | 1 | 8 | 7.4 | 8.9 | 8.4 | 1.5 | 107 |
| timestamps | zstd | table | 16 | 128 | 15.1 | 17.6 | 17.2 | 2.6 | 977 |
| text-24mib | none | numeric | 1 | 2 | 74.0 | 97.0 | 95.7 | 23.0 | 177 |
| text-24mib | none | numeric | 16 | 32 | 507.1 | 586.9 | 587.1 | 79.9 | 1687 |
| text-24mib | none | table | 1 | 2 | 73.5 | 97.4 | 95.7 | 23.9 | 164 |
| text-24mib | none | table | 16 | 32 | 484.3 | 595.4 | 595.9 | 111.1 | 1748 |
| text-24mib | zstd | numeric | 1 | 2 | 56.0 | 48.5 | 47.5 | -7.6 | 58 |
| text-24mib | zstd | numeric | 16 | 32 | 112.3 | 98.8 | 98.6 | -13.5 | 317 |
| text-24mib | zstd | table | 1 | 2 | 56.0 | 48.5 | 47.3 | -7.5 | 52 |
| text-24mib | zstd | table | 16 | 32 | 112.3 | 98.8 | 98.3 | -13.4 | 371 |

Largest OS resident peak growth: 595.4 MiB (text-24mib none table x16).

## Stream-count sweep: text-24mib, none, numeric, 2 messages per stream (MiB)

| Streams | Batches | Rust heap peak | OS resident peak | Per stream (resident) |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 2 | 74.1 | 98.1 | 98.1 |
| 2 | 4 | 147.8 | 194.5 | 97.3 |
| 4 | 8 | 186.9 | 250.4 | 62.6 |
| 8 | 16 | 276.3 | 353.7 | 44.2 |
| 16 | 32 | 509.0 | 587.9 | 36.7 |

## Message-size sweep: 16 streams, none, numeric, 2 messages per stream (MiB)

| Message | Rust heap peak | OS resident peak | Rust heap per stream |
| --- | ---: | ---: | ---: |
| text-8mib | 181.0 | 221.5 | 11.3 |
| text-16mib | 330.1 | 406.1 | 20.6 |
| text-24mib | 508.6 | 600.4 | 31.8 |

## Stream-cap sweep: 16 concurrent queries, text-24mib, none, numeric (MiB)

`EodPolicy::concurrent_streams` limits how many queries are dispatched at once.

| concurrent_streams | Rust heap peak | OS resident peak | Time (ms) |
| ---: | ---: | ---: | ---: |
| 16 | 502.2 | 586.7 | 1641 |
| 4 | 199.4 | 272.9 | 1748 |
| 2 | 147.0 | 268.5 | 1864 |
| 1 | 74.1 | 148.3 | 1974 |
