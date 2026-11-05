# Options baselines D2 (2026-11-05)

The first head-to-head comparison of the options phase
([plan](../architecture/options-phase.md)). Whole workloads were timed into
Polars by three runners, synchronously and 8 at a time:
- the vendor package 1.0.17 (`options_vendor.py`);
- our Python package (`options_ours.py`);
- our Rust `query()` (`live_p0 workload`).

## Conditions and caveats

- **Host and service:** the maintainer's Ryzen 7 2700, Windows 11, options
  PRO, PROD, main 480f558. All runs were by the Coordinator.
- **Every run is clean:** exit 0, nothing stopped, row counts identical
  across the runners, and the secret scan was clean.
- **The runs were sequential, not interleaved.** Each runner ran all its
  rounds before the next started:
  1. vendor, 00:39-00:54;
  2. our Python package, 00:54-01:00;
  3. Rust at 8, 01:00-01:01;
  4. Rust at 1, 01:01-01:06 (local time).

  Service speed that drifts over 25 minutes would favour or penalise one
  runner as a whole. Within a runner, rounds alternate the thread order.
- **3 rounds; values are medians.** One host, one trade date (2024-06-14),
  one root (SPY).
- **The two Python environments have different Polars versions:** 1.44.2
  (the vendor's pinned set) and 2.1.0 (ours). Frame building is part of
  both clients' times.
- **The workloads are one request per expiration.** That is the shape the
  service answers in seconds. They are **not** the slow all-strike tick and
  1 s quote shape, which waits about 60 s per request
  ([shape probes](../architecture/options-phase.md#shape-probes-opt-sizes-002-2026-11-05));
  that shape is measured separately (`split-probe`).

| Workload | Requests | Rows |
| --- | ---: | ---: |
| W5c: tick trades, 12 expirations | 12 | 265,034 |
| W4c: 1-minute quotes, 12 expirations | 12 | 1,208,190 |
| W6c: 1-minute Greeks, 8 expirations | 8 | 785,910 |

## Time to complete (median of 3, seconds)

| Workload | Vendor, sync | Vendor, 8 threads | Ours Python, 1 | Ours Python, 8 | Rust, 1 | Rust, 8 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| W5c | 27.5 | 9.2 | 20.2 | **2.8** | 19.8 | **2.5** |
| W4c | 71.0 | 38.2 | 36.6 | **7.6** | 37.4 | **6.9** |
| W6c | 85.3 | 65.5 | 42.1 | **8.3** | 43.7 | **8.1** |

**Spread across the 3 rounds (seconds, lowest to highest):**

| Workload | Vendor, sync | Vendor, 8 | Ours Python, 1 | Ours Python, 8 | Rust, 1 | Rust, 8 |
| --- | --- | --- | --- | --- | --- | --- |
| W5c | 27.4-29.2 | 9.0-9.9 | 15.8-23.3 | 2.7-3.2 | 18.0-20.6 | 2.4-3.0 |
| W4c | 68.1-72.0 | 37.6-38.7 | 36.1-37.9 | 5.8-8.9 | 35.8-40.3 | 5.5-7.1 |
| W6c | 85.1-85.5 | 65.3-65.7 | 39.1-53.8 | 8.2-9.0 | 40.3-46.5 | 8.0-9.2 |

The vendor's times were steady. Ours varied up to ±30% between rounds, and
since the data is the same each time, that variation is the service's. The
target ratios hold even in the worst case: W5c ours at 8 is at least 8.6×
the vendor's fastest synchronous round, and 2.8× its fastest threaded round.

## Against the targets

| Target | Python, 8 | Rust, 8 | Status |
| --- | ---: | ---: | --- |
| W5 ≥ 7× the synchronous vendor package at 8-way | 9.8× | 11.0× | **Met** (W5c) |
| W5 ≥ 1.0× the vendor threaded at the same concurrency | 3.3× | 3.7× | **Met** (W5c) |
| W4 ≥ 1.0× threaded | 5.0× | 5.5× | **Met** (W4c) |
| W4 ≥ C× synchronous minus scheduling losses (C = 8) | 9.3× | 10.3× | **Met** (W4c) |
| W6 ≥ 1.0× threaded | 7.9× | 8.1× | **Met** (W6c) |
| W6 ≥ C× synchronous minus scheduling losses (C = 8) | 10.3× | 10.5× | **Met** (W6c) |
| Memory bounded by configuration under W5 | — | — | **Not assessed** (below) |

The targets are met for these per-expiration workloads only. The slow
all-strike quote shape is not covered.

## What the numbers show

**1. Python adds little over Rust.** Our Python package is within 12% of
Rust at both concurrencies, and slightly faster at 1 on W4c and W6c. Decoding runs in Rust with the GIL released,
so the Python layer costs little here.

**2. One request at a time, ours is 1.4-2.0× faster than the vendor's.**

| Workload | Vendor sync | Ours sync | Ratio |
| --- | ---: | ---: | ---: |
| W5c | 27.5 s | 20.2 s | 1.4× |
| W4c | 71.0 s | 36.6 s | 1.9× |
| W6c | 85.3 s | 42.1 s | 2.0× |

Both clients send the same requests to the same service, so the difference,
7.3 s, 34.4 s and 43.2 s, is time spent in the vendor's client.

**3. The vendor's threads barely overlap its client time.** The vendor's
8-thread time is close to that client time in every workload:

| Workload | Vendor client time (sync minus ours sync) | Vendor, 8 threads |
| --- | ---: | ---: |
| W5c | 7.3 s | 9.2 s |
| W4c | 34.4 s | 38.2 s |
| W6c | 43.2 s | 65.5 s |

Per request, the vendor's W6c requests took 7-19 s alone, but 16-65 s under
8 threads, most of them over 50 s: the requests waited on one another. The
server work overlapped (ours finishes the same requests in 8 s at 8-way); the
vendor's client-side work did not.
- **Consistent with:** work that holds Python's GIL, such as per-row protobuf
  handling and building the frame in Python.
- **Not proven:** these runs have no CPU profile. One gRPC channel's
  receive path could also serialise.
- **To prove it:** a profile of the threaded vendor run.

**4. Ours scales 4.8-7.2× from 1 to 8.** W5c went from 20.2 s to 2.8 s, W4c from
36.6 s to 7.6 s, and W6c from 42.1 s to 8.3 s.
- **W5c** (7.2×) is close to ideal.
- **W4c and W6c** have 12 and 8 requests of uneven size. The largest request
  (177k rows) sets the floor, and W4c needs two waves of 8.
- The per-request split and the largest request, not the client, bound these
  workloads at 8-way. In Rust, W4c at 8 ends when its largest request
  (176,732 rows) ends. That request's first data arrived at 3.1-5.4 s, while
  the other 11 requests finished inside its time.

**5. Greeks keep arriving long after their first data (open lead).** In Rust,
a W6c request receives its rows for 2.0-6.5 s after the first batch, about
40-60% of the request. The 1-minute quote requests, with the same row
counts, take 0.2-1.5 s after their first batch. Greeks are about 3-4× larger
on the wire.
- **Probably not our decoding:** the time after the first batch barely grows
  from 1 to 8 requests at once (about 2.0-2.5 s against 2.5-3.3 s for about
  100k rows), on 4 worker threads. If decoding were the limit, 8 requests
  would roughly double it.
- **Probably not the flow-control window:** each stream has a 2 MiB HTTP/2
  window, and at about 2 MB/s per stream it would bind only with a round
  trip of about a second.
- **That leaves the server's sending rate.** This is not proven. To settle
  it: capture one W6c response and replay it offline to time decoding
  alone.

## Memory (peak RSS, whole run)

| Runner | Peak | After imports and login | Delta |
| --- | ---: | ---: | ---: |
| Vendor | 906.8 MiB | 101.9 MiB | 805 MiB |
| Ours, Python | 479.3 MiB | 65.3 MiB | 414 MiB |

Ours used about half the vendor's memory over the run (0.51× delta, 0.53×
total). Each peak is the whole run, with up to 8 results built at once in the
threaded runs.

**The memory target ("bounded by configuration under W5") is not
assessed.** These runners build each result as a Polars frame, so memory
grows with the result. A bounded run would stream (`batches()` or
`to_parquet`) with a configured limit, and is a separate measurement.

## Next

- **An interleaved repeat** (cheap: about 25 minutes): each runner with
  `--rounds 1`, three times, alternating vendor, ours Python, Rust 8 and
  Rust 1. It removes the drift caveat.
- **The all-strike quote shape:** `split-probe` (`opt-split-001`), then the
  per-contract planner split if it holds.
- **A memory-bounded W5 run** through `batches()` or `to_parquet`.
