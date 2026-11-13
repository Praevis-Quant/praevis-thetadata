#!/usr/bin/env bash
# One live run of the approved checks (ADR-0022 part 4; docs/live-runbook.md).
# Manual or Coordinator-run only, never CI. It writes one private report
# folder, artifacts/live/<run-id>/ (ignored by git).
#
#   scripts/live/run.sh --run-id ID [--part anytime|market|all]
#       [--terminal none|live|dev] [--platform windows|wsl|linux]
#       [--full-trades-seconds N] [--no-build] [--force-off-hours]
#
# The key comes from THETADATA_API_KEY in the environment only: never a
# file, never the command line, never printed. Machine paths come from
# scripts/live/settings.env (see settings.example.env).
#
# --terminal none (the default) never starts, stops or reconfigures the
# Terminal: the market part then needs one already running. --terminal live
# or dev restarts it on that config (live market, or the dev replay feed
# off-hours). Only pass it when the operator has agreed to that.
#
# The market part first asks the service's calendar_open_today whether US
# markets are open now. If not (a weekend, a holiday, after the close), it
# is refused, because snapshots would be empty and latency misleading,
# unless --force-off-hours. With --terminal dev, the dev-feed stream still
# runs off-hours (labelled dev) and only the option checks are skipped.
set -u

usage() {
    sed -n '2,24p' "$0" | sed 's/^# \{0,1\}//'
    exit "${1:-0}"
}

RUN_ID="" PART=all TERMINAL=none PLATFORM="" FULL_SECONDS=1200 BUILD=1 FORCE_OFF_HOURS=0
while [ $# -gt 0 ]; do
    case "$1" in
        --run-id) RUN_ID="${2:-}"; shift 2 ;;
        --part) PART="${2:-}"; shift 2 ;;
        --terminal) TERMINAL="${2:-}"; shift 2 ;;
        --platform) PLATFORM="${2:-}"; shift 2 ;;
        --full-trades-seconds) FULL_SECONDS="${2:-}"; shift 2 ;;
        --no-build) BUILD=0; shift ;;
        --force-off-hours) FORCE_OFF_HOURS=1; shift ;;
        -h|--help) usage 0 ;;
        *) echo "unknown option: $1" >&2; usage 2 ;;
    esac
done
case "$PART" in anytime|market|all) ;; *) echo "--part: anytime, market or all" >&2; exit 2 ;; esac
case "$TERMINAL" in none|live|dev) ;; *) echo "--terminal: none, live or dev" >&2; exit 2 ;; esac
case "$FULL_SECONDS" in ''|*[!0-9]*) echo "--full-trades-seconds: a number (0 skips)" >&2; exit 2 ;; esac
[ -n "$RUN_ID" ] || { echo "--run-id is required" >&2; exit 2; }
case "$RUN_ID" in *[!A-Za-z0-9._-]*) echo "--run-id: letters, digits, . _ - only" >&2; exit 2 ;; esac
if [ -z "${THETADATA_API_KEY:-}" ]; then
    echo "THETADATA_API_KEY is not set (it is read from the environment only)" >&2
    exit 2
fi

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
SETTINGS="$ROOT/scripts/live/settings.env"
# shellcheck source=/dev/null
[ -f "$SETTINGS" ] && . "$SETTINGS"
THETA_REST="${THETA_REST:-127.0.0.1:25503}"
THETA_SETTLE="${THETA_SETTLE:-60}"

if [ -z "$PLATFORM" ]; then
    case "$(uname -s)" in
        MINGW*|MSYS*|CYGWIN*) PLATFORM=windows ;;
        Linux) if grep -qi microsoft /proc/version 2>/dev/null; then PLATFORM=wsl; else PLATFORM=linux; fi ;;
        *) echo "unknown platform; pass --platform" >&2; exit 2 ;;
    esac
fi
EXE=""; [ "$PLATFORM" = windows ] && EXE=".exe"
PY="${PRAEVIS_PY:-}"
if [ -z "$PY" ]; then
    if [ "$PLATFORM" = windows ]; then PY="$ROOT/.venv-p1/Scripts/python"; else PY="$ROOT/.venv-p1/bin/python"; fi
fi
REL="$ROOT/target/release"
OUT="$ROOT/artifacts/live/$RUN_ID"
mkdir -p "$OUT"
LOG="$OUT/run.log"
SUMMARY="$OUT/summary.txt"
: > "$LOG"
: > "$SUMMARY"
cd "$ROOT" || exit 1

log() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$LOG"; }
# Runs one step: its output goes to the log without session, token or
# e-mail lines, and its exit code to the summary.
step() {
    local name="$1"; shift
    log "== $name"
    PYTHONIOENCODING=utf-8 "$@" 2>&1 | grep -viE "session|token|@" >> "$LOG"
    local rc=${PIPESTATUS[0]}
    log "exit $rc"
    echo "$name: exit $rc" >> "$SUMMARY"
    return "$rc"
}

TODAY=$(date -u +%Y%m%d)
# The flat files: a recent weekday within the vendor's 7 calendar days.
FLAT_DATE=$(date -u -d "yesterday" +%Y-%m-%d)
case "$(date -u -d "$FLAT_DATE" +%u)" in
    6) FLAT_DATE=$(date -u -d "$FLAT_DATE - 1 day" +%Y-%m-%d) ;;
    7) FLAT_DATE=$(date -u -d "$FLAT_DATE - 2 days" +%Y-%m-%d) ;;
esac
log "run $RUN_ID: part $PART, terminal $TERMINAL, platform $PLATFORM, at $(git log --oneline -1)"
echo "run $RUN_ID at $(git rev-parse HEAD), $(date -u +%Y-%m-%dT%H:%MZ), part $PART, terminal $TERMINAL, platform $PLATFORM" >> "$SUMMARY"

# ---- the Terminal (only with --terminal live|dev) --------------------------

STARTED_PGID=""
terminal_rest() {
    curl -s -m 5 -o /dev/null -w "%{http_code}" "http://$THETA_REST/v3/option/list/expirations?symbol=SPY"
}
terminal_stop() {
    case "$PLATFORM" in
        windows)
            # Windows-only: the user's Terminal, found by its command line.
            powershell -NoProfile -Command "
\$t = Get-CimInstance Win32_Process -Filter \"Name='javaw.exe'\" | Where-Object { \$_.CommandLine -like '*ThetaTerminalv3.jar*' }
'stopping terminal pids: ' + (\$t.ProcessId -join ',')
\$t | ForEach-Object { Stop-Process -Id \$_.ProcessId -Force }" 2>&1 | tee -a "$LOG"
            sleep 5 ;;
        *)
            # WSL or Linux: only a Terminal this run started, by its process group.
            if [ -n "$STARTED_PGID" ]; then
                kill -- "-$STARTED_PGID" 2>/dev/null
                sleep 3
                log "stopped the Terminal this run started (process group $STARTED_PGID)"
                STARTED_PGID=""
            fi ;;
    esac
}
terminal_start() {
    [ -n "${THETA_TERMINAL_DIR:-}" ] || { log "THETA_TERMINAL_DIR is not set"; return 1; }
    local config="" ; [ "$TERMINAL" = live ] && config="${THETA_CONFIG_LIVE:-}"
    [ "$TERMINAL" = dev ] && config="${THETA_CONFIG_DEV:-}"
    local dir="$THETA_TERMINAL_DIR"
    [ "$PLATFORM" = windows ] && dir=$(cygpath -u "$THETA_TERMINAL_DIR")
    if [ -n "$config" ]; then
        local source="$config"; [ "$PLATFORM" = windows ] && source=$(cygpath -u "$config")
        cp "$dir/config.toml" "$OUT/config.toml.before"
        cp "$source" "$dir/config.toml"
    fi
    log "terminal config: $(grep -h fpss_region "$dir/config.toml" 2>/dev/null)"
    case "$PLATFORM" in
        windows)
            [ -n "${THETA_JAVA:-}" ] || { log "THETA_JAVA is not set"; return 1; }
            powershell -NoProfile -Command "Start-Process -FilePath '$THETA_JAVA' -ArgumentList '-jar','ThetaTerminalv3.jar' -WorkingDirectory '$THETA_TERMINAL_DIR'" 2>&1 | tee -a "$LOG" ;;
        *)
            # The bootstrapper launches lib/*.jar with `java` by name.
            [ -n "${THETA_JAVA:-}" ] && export PATH="$THETA_JAVA:$PATH"
            (cd "$dir" && setsid nohup java -jar ThetaTerminalv3.jar > "$OUT/terminal.out" 2>&1 < /dev/null &
             echo $! > "$OUT/terminal.pid")
            STARTED_PGID=$(cat "$OUT/terminal.pid") ;;
    esac
    local code=""
    for _ in $(seq 1 60); do
        code=$(terminal_rest)
        [ "$code" = 200 ] && break
        sleep 5
    done
    log "terminal REST http $code"
    [ "$code" = 200 ] || return 1
    log "settling ${THETA_SETTLE} s"
    sleep "$THETA_SETTLE"
}

# ---- build ------------------------------------------------------------------

if [ "$BUILD" = 1 ]; then
    step build-rust bash -c "cargo build --release --locked -q -p praevis-thetadata-scenarios \
        && cargo build --release --locked -q -p praevis-thetadata-client --features live-tests --example live_p0 \
        && cargo build --release --locked -q -p praevis-thetadata-stream --features live-tests --examples"
    step build-python bash -c "rm -rf '$OUT/wheel' && '$PY' -m maturin build --release --locked -q \
        -m crates/praevis-thetadata-py/Cargo.toml -o '$OUT/wheel' \
        && '$PY' -m pip install -q --no-index --find-links '$OUT/wheel' --force-reinstall --no-deps praevis-thetadata"
fi
SCEN="$REL/praevis-thetadata-scenarios$EXE"
P0="$REL/examples/live_p0$EXE"

# Our gRPC login ends the Terminal's REST session (478), so every
# login-based step runs before the Terminal is (re)started for streaming.
if [ "$TERMINAL" != none ]; then
    terminal_stop
fi

# ---- any time: historical requests -----------------------------------------

if [ "$PART" != market ]; then
    step scenarios-rust "$SCEN" --target live --confirm-live --part anytime --run-id "$RUN_ID-scenarios" \
        --report "$OUT/scenarios-rust.json"
    step scenarios-python "$PY" crates/praevis-thetadata-scenarios/python/scenarios.py --target live --confirm-live \
        --run-id "$RUN_ID-scenarios-py" --report "$OUT/scenarios-python.json"
    step stock-index-rate-rust "$P0" entitlements --confirm-live --api-key-env --run-id "$RUN_ID-sir" \
        --set stock-index-rate --flat-file-date "$FLAT_DATE" --max-messages 20
    step stock-index-rate-python "$PY" bench/p1/stock_index_rate_checks.py --run-id "$RUN_ID-sir-py"
fi

# ---- market hours: snapshots, then the stream ------------------------------

MARKET=0 OPTION_CHECKS=0
if [ "$PART" != anytime ]; then
    "$SCEN" --target live --confirm-live --market-open-check 2>&1 | tee -a "$LOG"
    case "${PIPESTATUS[0]}" in
        0) MARKET=1 OPTION_CHECKS=1 ;;
        3) log "markets are closed now (calendar_open_today)" ;;
        *) log "cannot tell whether markets are open; treated as closed" ;;
    esac
    if [ "$MARKET" = 0 ] && [ "$FORCE_OFF_HOURS" = 1 ]; then
        log "--force-off-hours: running the market part anyway"
        MARKET=1 OPTION_CHECKS=1
    elif [ "$MARKET" = 0 ] && [ "$TERMINAL" = dev ]; then
        log "off-hours on the dev feed: the stream runs, the option checks (snapshots) are skipped"
        MARKET=1
        echo "option checks: skipped (off-hours, dev feed)" >> "$SUMMARY"
    elif [ "$MARKET" = 0 ]; then
        echo "market part: refused (markets closed; --force-off-hours overrides)" >> "$SUMMARY"
    fi
fi
if [ "$MARKET" = 1 ] && [ "$OPTION_CHECKS" = 1 ]; then
    step option-checks-rust "$P0" option-checks --confirm-live --api-key-env --run-id "$RUN_ID-opt" --same-response \
        --snapshot-expiration "$TODAY" --flat-file-date "$FLAT_DATE"
    step option-checks-python "$PY" bench/p1/option_checks.py --run-id "$RUN_ID-opt-py" --snapshot-expiration "$TODAY"
fi
if [ "$MARKET" = 1 ]; then
    if [ "$TERMINAL" != none ]; then
        terminal_start || log "the Terminal did not become ready"
    fi
    if [ "$(terminal_rest)" != 200 ]; then
        log "no Terminal answering on $THETA_REST: stream steps skipped"
        echo "stream: skipped (no Terminal)" >> "$SUMMARY"
    else
        FEED=live; [ "$TERMINAL" = dev ] && FEED=dev
        step scenarios-stream "$SCEN" --target live --confirm-live --part market --run-id "$RUN_ID-stream" \
            --report "$OUT/scenarios-stream.json"
        step discover "$REL/examples/discover_contracts$EXE" --confirm-live --seconds 60 --top 10,100,1000 --out-dir "$OUT"
        for n in 10 100 1000; do
            if [ -s "$OUT/contracts-top-$n.txt" ]; then
                step "latency-$n" "$REL/examples/latency$EXE" --confirm-live --run-id "$RUN_ID-w10-$n" \
                    --contracts "$OUT/contracts-top-$n.txt" --seconds 120 --feed "$FEED" --consumer sleep
            fi
        done
        if [ "$FULL_SECONDS" -gt 0 ]; then
            step full-trades "$REL/examples/latency$EXE" --confirm-live --run-id "$RUN_ID-w10-full" --full-trades \
                --seconds "$FULL_SECONDS" --feed "$FEED"
        fi
        cp artifacts/w10/"$RUN_ID"-w10-*.json "$OUT/" 2>/dev/null
        rm -f "$OUT"/contracts-top-*.txt
    fi
    # A Terminal this run started on WSL or Linux is stopped again; on
    # Windows the user's Terminal is left running, as before the run.
    [ "$PLATFORM" != windows ] && terminal_stop
fi

# ---- drift: what the live run saw against the emulator's model -------------

reports=()
for r in scenarios-rust scenarios-python scenarios-stream; do
    [ -f "$OUT/$r.json" ] && reports+=("$OUT/$r.json")
done
if [ ${#reports[@]} -gt 0 ]; then
    step drift "$PY" tools/emulator_drift.py "${reports[@]}" --out "$OUT/drift.json"
fi
log "== END; summary in $SUMMARY"
cat "$SUMMARY"
