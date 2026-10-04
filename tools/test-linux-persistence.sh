#!/usr/bin/env bash
# Exercise a real, isolated Secret Service without touching the user's keyring.
set -euo pipefail

cd "$(dirname "$0")/.."
for program in cargo dbus-run-session gnome-keyring-daemon timeout; do
    command -v "$program" >/dev/null || {
        echo "Missing $program; install Rust, dbus, and gnome-keyring." >&2
        exit 1
    }
done

# Compile before isolating the keyring. Keep the user's Rust environment intact.
cargo test --locked -p thetadata-cli --test persistence --no-run
test_home=$(mktemp -d "${TMPDIR:-/tmp}/thetadata-keyring-test.XXXXXXXX")
trap 'rm -rf -- "$test_home"' EXIT
export XDG_DATA_HOME="$test_home/data"
export XDG_CONFIG_HOME="$test_home/config"
export XDG_RUNTIME_DIR="$test_home/run"
unset GNOME_KEYRING_CONTROL
mkdir -p "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_RUNTIME_DIR"
chmod 700 "$test_home" "$XDG_RUNTIME_DIR"

# This password only protects synthetic test data in a disposable keyring.
# The daemon exits with the private bus. Bound waits so CI cannot hang on prompts.
timeout 90s dbus-run-session -- bash -c '
    set -euo pipefail
    printf "%s" "thetadata-synthetic-test-keyring" |
        gnome-keyring-daemon --unlock --components=secrets >/dev/null
    cargo test --locked -p thetadata-cli --test persistence -- --ignored
'
