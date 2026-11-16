#!/bin/bash -eu
# Builds the fuzz targets in fuzz/ with the sanitizer ClusterFuzzLite asks
# for, and their synthetic seed corpora (never market data).
# The sanitizer CFLAGS would also reach C code compiled for build scripts
# (ring, under the client's build-time proto dependency), which are linked
# without the sanitizer runtime and fail. Rust code stays instrumented through
# RUSTFLAGS; the C dependencies (zstd, ring) are built uninstrumented.
unset CFLAGS CXXFLAGS
cd "$SRC/praevis-thetadata/fuzz"
cargo fuzz build -O
release=target/x86_64-unknown-linux-gnu/release
for target in response_data zstd_payload eod_envelope terminal_event; do
  cp "$release/$target" "$OUT/"
done
"$release/seeds" /tmp/seeds
for target in response_data zstd_payload eod_envelope terminal_event; do
  (cd "/tmp/seeds/$target" && zip -q -r "$OUT/${target}_seed_corpus.zip" .)
done
