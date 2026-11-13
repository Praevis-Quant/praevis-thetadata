# Reads scripts/live/settings.env as data, never as code (docs/live-runbook.md).
# Sourced by run.sh and wsl.sh, which define nothing else from the file.
#
# Each non-blank line that is not a # comment must be KEY=VALUE, with KEY
# one of the settings below. The value is taken literally up to the end
# of the line: spaces and backslashes need no quoting (C:\Program Files\...).
# One pair of surrounding "double" or 'single' quotes is removed. A value
# starting with ~/ or $HOME/ is expanded to the home folder; nothing else
# is expanded or executed. Anything else stops with the line number.

LIVE_SETTINGS="THETA_TERMINAL_DIR THETA_JAVA THETA_CONFIG_LIVE THETA_CONFIG_DEV THETA_REST THETA_SETTLE PRAEVIS_PY WSL_DISTRO WSL_REPO"

load_settings() {
    local file="$1" number=0 line key value
    [ -f "$file" ] || return 0
    while IFS= read -r line || [ -n "$line" ]; do
        number=$((number + 1))
        line="${line%$'\r'}"
        case "$line" in ''|'#'*) continue ;; esac
        if [[ ! "$line" =~ ^([A-Z_]+)=(.*)$ ]]; then
            echo "$file:$number: not KEY=VALUE" >&2
            return 1
        fi
        key="${BASH_REMATCH[1]}"
        value="${BASH_REMATCH[2]}"
        case " $LIVE_SETTINGS " in
            *" $key "*) ;;
            *) echo "$file:$number: unknown setting $key" >&2; return 1 ;;
        esac
        if [[ "$value" =~ ^\"(.*)\"$ || "$value" =~ ^\'(.*)\'$ ]]; then
            value="${BASH_REMATCH[1]}"
        fi
        case "$value" in
            '~/'*) value="$HOME/${value#'~/'}" ;;
            '$HOME/'*) value="$HOME/${value#'$HOME/'}" ;;
        esac
        printf -v "$key" '%s' "$value"
    done < "$file"
}
