# Fuzzing

The decoders that read bytes from outside the process are fuzzed with
[cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) (libFuzzer), in CI by
[ClusterFuzzLite](https://google.github.io/clusterfuzzlite/)
([workflow](../../.github/workflows/fuzz.yml)).

| Target | What it feeds | Code under test |
| --- | --- | --- |
| `response_data` | Arbitrary bytes as a gRPC response envelope (`ResponseData`) | protobuf decoding, zstd decompression, the columnar Arrow decoder (`praevis-thetadata-arrow`), twice in a row, then a flush |
| `zstd_payload` | Arbitrary bytes as a zstd-compressed table, with a fuzzed declared size | zstd frame handling (window limit, declared size, truncation) and whatever it decompresses to |
| `eod_envelope` | Arbitrary bytes as a stock EOD envelope | the client's own bounded envelope parser and its table and numeric EOD decoders |
| `terminal_event` | Arbitrary UTF-8 as a Theta Terminal WebSocket event | the stream crate's event parser |
| `legacy_table` | Arbitrary bytes as a `ResponseData` envelope (from `0.1.6`) | the client's legacy table decoder (`decode.rs`): protobuf, zstd within the batch limit, the table |
| `grpc_framing` | Arbitrary bytes as a gRPC response body, cut into transport chunks of 1 to 16 bytes as the first byte chooses (from `0.1.6`) | the client's length-prefix framing guard (`framing.rs`), with its invariants asserted: tonic sees only empty frames, every message fits the limit, is allocated exactly once and is taken before the next poll, and the body ends after an error |

**Every case must end in `Ok` within the limits or a clean error.** A panic,
an out-of-bounds access, a leak or a memory blow-up (the fuzzers run with
small decoder limits and a 2 GiB RSS cap) is a finding.

**When it runs:**
- **Pull requests:** a five-minute fuzzing budget across all six targets
  (ClusterFuzzLite's code-change mode), with AddressSanitizer. The job
  also builds the targets and looks up any saved corpora.
- **Weekly:** an hour across every target, with AddressSanitizer (Sundays;
  the first scheduled run was on 2026-11-22, and passed).

## Corpus download timeouts

The build action skips coverage lookup. The run action still looks up saved
corpora through GitHub's repository artifacts API before launching each
target. Confirmed missing corpora use the built-in synthetic seeds.

- On #227, the **build** action's coverage lookup paged to 4,702 and hit
  GitHub's rate limit. Its pager counted toward the first response's
  `total_count`, which can become stale as artifacts expire. #230 skipped
  the coverage lookup and kept all targets.
  [Run 44866417025](https://github.com/Praevis-Quant/praevis-thetadata/actions/runs/44866417025)
  [PR #230](https://github.com/Praevis-Quant/praevis-thetadata/pull/230)
- On #254, the **run** action got through five targets without a reported
  crash, then spent nearly 18 minutes looking up the `legacy_table`
  corpus. GitHub cancelled the job at 30 minutes with the Python stack
  blocked in an HTTPS socket read. It was still listing artifacts, before
  downloading an archive. The log does not identify the page or show the
  earlier pagination and rate-limit pattern. The retry passed.
  [Run 45641460417](https://github.com/Praevis-Quant/praevis-thetadata/actions/runs/45641460417)

### Mitigation

- **Keep the fuzzing budget.** `fuzz-seconds` sets total fuzz execution time.
  Corpus lookup runs before the per-target timer, so a shorter fuzzing
  budget would cut testing while leaving the socket read unbounded.
  `keep-unaffected-fuzz-targets` applies to the build action's coverage
  lookup. The run action exposes no HTTP timeout or download retry input.
  Its optional `storage-repo` can avoid the repository artifact pager,
  but requires provisioning maintained corpus storage and access for
  batch writes. It still needs bounded network operations.
  [Pinned run action inputs](https://github.com/google/clusterfuzzlite/blob/884713a6c30a92e5e8544c39945cd7cb630abcd1/actions/run_fuzzers/action.yml)
  [ClusterFuzzLite storage configuration](https://google.github.io/clusterfuzzlite/running-clusterfuzzlite/github-actions/#git-repo-for-storage)
  [Corpus retrieval and fuzz execution](https://github.com/google/oss-fuzz/blob/12774644f196861f196a97055314c2571259b3ec/infra/cifuzz/fuzz_target.py)
- **Bound and retry artifact I/O upstream.** The existing GET retry
  wrapper can catch an exception, but `requests.get` has no timeout in
  this path. Use GitHub's exact `name` filter and follow next-page links
  instead of counting toward the first page's `total_count`. Read-only
  checks on 2026-11-25 returned no `legacy_table` corpus and one
  unexpired `terminal_event` corpus with the matching name. Add connect
  and read timeouts and a total deadline (for example 60 seconds per
  corpus, including retries and backoff) covering both lookup and
  archive download. Retry transient network failures within that
  deadline. Keep the synthetic-seed path for confirmed absence and
  report an exhausted network budget as an infrastructure failure.
  [Artifact API](https://docs.github.com/en/rest/actions/artifacts#list-artifacts-for-a-repository)
  [ClusterFuzzLite GitHub API client](https://github.com/google/oss-fuzz/blob/12774644f196861f196a97055314c2571259b3ec/infra/cifuzz/filestore/github_actions/github_api.py)

Before adopting a fixed action, verify that its runtime image contains the
change. Test a stalled response, a transient read failure followed by
success, and a changing artifact count. A CI run should still exercise all
six targets with AddressSanitizer and the existing fuzz budget, use
available corpora, and fail on findings. Retrying the whole job after
diagnosing an infrastructure failure is recovery for that run, not a
bound on artifact I/O.

## Instrumentation and local runs

**What is instrumented:** all Rust code, with AddressSanitizer, coverage,
debug assertions and integer-overflow checks.
The C libraries underneath (zstd's decompressor and ring's primitives) are
compiled without instrumentation, because ClusterFuzzLite's sanitizer
`CFLAGS` would also reach build scripts that cannot link it; those libraries
are fuzzed by their own projects.

**Seeds are synthetic:** `fuzz/src/bin/seeds.rs` builds them at build time
from the emulator's synthetic fixture tables and hand-written synthetic
Terminal events. No market data, captured response or vendor payload is used
or committed, and a crash's reproducer stays an artifact of its run.

**Running it locally** (nightly Rust and `cargo install cargo-fuzz`):

```text
cd fuzz
cargo +nightly fuzz build -O --debug-assertions
target/x86_64-unknown-linux-gnu/release/seeds /tmp/seeds
cargo +nightly fuzz run -O response_data /tmp/seeds/response_data -- -max_total_time=60
```

**The client hook:** the EOD decoders are private to the client crate, so
the client exposes `praevis_thetadata_client::fuzzing` only under
`--cfg fuzzing`, which cargo-fuzz sets. A normal build and the published
crate do not contain it.

**First local run** (2026-11-15, aarch64 Linux, 60 seconds per target, no
sanitizer flags beyond cargo-fuzz's defaults): about 13.6 million cases in
total, no crashes; peak memory 180 to 550 MB.
