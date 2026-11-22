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
- **Pull requests:** about five minutes on the targets the change affects
  (ClusterFuzzLite's code-change mode), with AddressSanitizer.
- **Weekly:** an hour across every target, with AddressSanitizer (Sundays;
  the first scheduled run was on 2026-11-22, and passed).

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
