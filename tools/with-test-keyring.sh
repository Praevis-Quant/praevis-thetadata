#!/usr/bin/env bash
# Run a command using a disposable, persistent Secret Service collection.
set -euo pipefail
for program in dbus-run-session gnome-keyring-daemon timeout; do
    command -v "$program" >/dev/null || {
        echo "Missing $program; install dbus and gnome-keyring." >&2
        exit 1
    }
done
test_keyring_dir=$(mktemp -d "${TMPDIR:-/tmp}/thetadata-keyring-test.XXXXXXXX")
trap 'rm -rf -- "$test_keyring_dir"' EXIT
export XDG_DATA_HOME="$test_keyring_dir/data"
export XDG_CONFIG_HOME="$test_keyring_dir/config"
export XDG_RUNTIME_DIR="$test_keyring_dir/run"
unset GNOME_KEYRING_CONTROL
mkdir -p "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_RUNTIME_DIR"
chmod 700 "$test_keyring_dir" "$XDG_RUNTIME_DIR"
timeout 180s dbus-run-session -- bash -c '
    set -euo pipefail
    printf "%s" "thetadata-synthetic-test-keyring" |
        gnome-keyring-daemon --unlock --components=secrets >/dev/null
    "$@"
' bash "$@"
