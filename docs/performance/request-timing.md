# Where the time goes in a ThetaData request

A guide for users of the Rust and Python clients. It explains what to expect from
large option requests and why, so that a request that takes a minute does not
look like a hung client.

**Status of the evidence.** The numbers come from one run, `opt-sizes-001`
(2026-11-05, Windows, options PRO, SPY on 2024-06-14), through our Rust client,
one request at a time. They are single samples of a service whose speed varies a
lot: identical requests have differed by up to 14× in time
([P0 results](p0-results.md)). Read them as a shape, not as a benchmark.

## What we measured

| Workload | Rows | Wait for first data | Total | Share spent waiting | Size on the wire | Rows per message |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1-minute quotes, one expiration (W4a) | 176,732 | 3.5 s | 3.9 s | 90% | 2.0 MB | 391 |
| Tick trades, one expiration (W5a) | 123,146 | 1.7 s | 2.2 s | 78% | 1.4 MB | 832 |
| 1-second quotes, calls, 30 minutes (W4b) | 407,026 | **71.8 s** | 80.4 s | 89% | 2.5 MB | 1,801 |
| Tick quotes, calls, 15 minutes (W5b) | 938,663 | **59.4 s** | 62.3 s | 95% | 5.4 MB | 3,271 |
| 1-minute Greeks (W6a) | 176,732 | 3.4 s | 5.9 s | 57% | 5.2 MB | 391 |
| Trade Greeks (W6b) | 47,050 | 3.2 s | 6.1 s | 52% | 2.2 MB | 159 |

All 12 requests of the run were under the vendor's recommended 1 million rows.

## What this means

1. **Most of a request is waiting for the first data.** In every request above,
   more than half the time passed before the first message arrived, and up to
   95% for the largest quote requests. The rest of the result then arrived
   quickly. A long pause at the start of a large request is normal.
2. **The client cannot shorten that wait.** It happens before any data reaches
   the client, so a faster decoder does not change it. We measured the wait, not
   its cause, and we do not know why two of these requests waited about a minute.
3. **Responses are small on the wire.** Quote data was about 6 bytes per row
   compressed and Greeks about 29-47, so in this run the network was not the
   limit. Decoding and memory start to matter once data arrives.
4. **Message shape depends on the endpoint.** End-of-day data arrives as one row
   per message ([P0 results](p0-results.md)); interval and tick data arrived as
   hundreds or thousands of rows per message. Conclusions about one do not carry
   over to the other.

## Why some quote requests wait a minute, and what the client does

**The cause (measured).** Tick and 1-second quote requests across every
strike of an expiration wait about **60 s** before their first data,
whatever the time window. A 1-minute window waited 61-63 s, and 15 to 30
minutes waited 59-72 s. The same quotes for **one** contract over a whole
day (486k rows) arrived in 1.1 s. 1-minute quotes and tick trades across
every strike are not slow (`opt-sizes-002`).

**The fix: `job()` splits these requests per strike.** It lists the
expiration's strikes (one extra request, `option_list_strikes`), then sends
one request per strike, up to your tier's concurrency.

Measured on SPY calls expiring 2024-06-21, on 2024-06-14 from 10:00 to 10:15
(232 strikes, 938,663 rows):

| How | Time |
| --- | ---: |
| One request, every strike | 58.9-69.6 s (`opt-split-001`); 39.5-48.3 s (`opt-split-003`) |
| Per strike, one at a time | 95.9 s (`opt-split-001`) |
| Per strike, 8 at a time (PRO), hand-made | 9.1 s (`opt-split-001`) |
| **`job()` (PRO)** | **7.4 s** (`opt-split-003`) |

The service's speed varies from day to day, so compare rows from the same
run.

- **The rows are identical:** the same count, columns and values, checked
  exactly. 6 strikes had no quotes in the window and are skipped.
- **When it applies:** `option_history_quote` with every strike (`*`), a
  `tick` or `1s` interval (the vendor default is `1s`), one concrete
  expiration, and no `strike_range` or `max_dte`.
- **What stays whole:** other intervals, a single strike, `query()` in Rust,
  and `plan=False` in Python.
- **The cost:** one list request per job. One at a time, per-strike requests
  are slower than the single request (95.9 s), so `job()` splits only when
  the scheduler runs at least 2 requests at once. On PRO (8) the split was
  6.4-7.6× faster. STANDARD (4) is not measured.
- **Order:** rows come by date, then strike ascending, then the service's
  order within a strike. The single request's order is not documented, so
  sort if you depend on one.

## Running requests at once

If a request spends most of its time waiting, several requests running at once
should overlap those waits. That is why the client has a scheduler and why tier
concurrency (FREE 1, VALUE 2, STANDARD 4, PRO 8) should be the main speed lever
for large jobs. The vendor Python package runs one request at a time.

**Measured since** ([D2 results](options-d2-results.md), one host, one date,
3 rounds). For per-expiration option workloads, 8 requests at once finished
4.8-7.2× faster than one at a time, and 9.8-10.3× faster than the vendor
package used synchronously. Requests that ask for tick or 1-second quotes
across every strike behave differently: each waits about a minute regardless
of size, and `job()` splits them per strike (above).

## How to get the best results

- **Split large jobs and let the scheduler run them.** Use `client.job(...)` in
  Rust, or `plan` in the Python methods. It splits by date and stays within your
  tier's limits. Keep each request under about 1 million rows, as the
  [vendor recommends](https://thetadata.net/docs/Articles/Data-And-Requests/Request-Sizing.html).
- **Keep end-of-day requests to one year or less.** 366 days was accepted and 731
  days was rejected with `INVALID_ARGUMENT` ([P0 results](p0-results.md)).
- **Stream large results.** Use `batches()` or write to Parquet rather than
  holding everything in memory. Rows reach you as they arrive.
- **Use one login per account.** A second login ends the first session, which then
  fails with `SessionEnded`. Share a client instead: clone it in Rust, or pass
  `existing_authorized_client` in Python. This includes logins from the CLI and
  from the vendor's package.
- **Set timeouts with the wait in mind.** A request can be silent for over a
  minute before its first row. That wait counts against `request_timeout`
  (default 300 s for the whole request). `idle_timeout` (default 60 s) applies
  between messages once data is flowing: in the run above, a 71.8 s first wait
  succeeded with the default 60 s idle timeout. Raise `request_timeout` for
  requests you expect to take longer.
