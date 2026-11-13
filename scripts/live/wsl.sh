#!/usr/bin/env bash
# Windows side (Git Bash) of a WSL live run (docs/live-runbook.md, WSL).
# The Windows Terminal can't be reached from WSL, and one account runs one
# Terminal stream: this stops the Windows Terminal, runs scripts/live/run.sh
# inside WSL with its own Terminal (--terminal live), then restarts the
# Windows Terminal. Only run it when the operator has agreed to that.
#
#   scripts/live/wsl.sh --run-id ID [run.sh options for the WSL side]
#
# The key passes to WSL through WSLENV only, never on a command line.
set -u
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
# The settings file is read as data, never executed (settings.sh).
# shellcheck source=settings.sh
. "$ROOT/scripts/live/settings.sh"
load_settings "${PRAEVIS_LIVE_SETTINGS:-$ROOT/scripts/live/settings.env}" || exit 2
case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) ;; *) echo "run this from Git Bash on Windows" >&2; exit 2 ;; esac
[ -n "${THETADATA_API_KEY:-}" ] || { echo "THETADATA_API_KEY is not set" >&2; exit 2; }
for v in THETA_JAVA THETA_TERMINAL_DIR; do
    [ -n "${!v:-}" ] || { echo "$v is not set (scripts/live/settings.env)" >&2; exit 2; }
done
DISTRO="${WSL_DISTRO:-Ubuntu}"
REPO="${WSL_REPO:-~/praevis-thetadata}"
LOG="$ROOT/artifacts/live/wsl-outer-$(date -u +%Y%m%dT%H%M%SZ).log"
mkdir -p "$(dirname "$LOG")"
log() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$LOG"; }

log "stopping the Windows Terminal"
powershell -NoProfile -Command "
\$t = Get-CimInstance Win32_Process -Filter \"Name='javaw.exe'\" | Where-Object { \$_.CommandLine -like '*ThetaTerminalv3.jar*' }
'stopping terminal pids: ' + (\$t.ProcessId -join ',')
\$t | ForEach-Object { Stop-Process -Id \$_.ProcessId -Force }" 2>&1 | tee -a "$LOG"
sleep 5

# Arguments are passed through as they are; run.sh validates them.
log "running in WSL ($DISTRO): run.sh --platform wsl --terminal live $*"
WSLENV=THETADATA_API_KEY MSYS_NO_PATHCONV=1 wsl.exe -d "$DISTRO" -- bash -lc \
    "cd $REPO && scripts/live/run.sh --platform wsl --terminal live $(printf '%q ' "$@")" 2>&1 | tee -a "$LOG"
rc=${PIPESTATUS[0]}
log "WSL run exit $rc"

log "restarting the Windows Terminal"
powershell -NoProfile -Command "Start-Process -FilePath '$THETA_JAVA' -ArgumentList '-jar','ThetaTerminalv3.jar' -WorkingDirectory '$THETA_TERMINAL_DIR'" 2>&1 | tee -a "$LOG"
log "== END (the WSL report is in the WSL repository's artifacts/live/)"
exit "$rc"
