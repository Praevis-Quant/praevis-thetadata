# Live runbook

How the live checks run, with one command, on a schedule
([ADR-0022](adr/0022-emulator-and-dual-target-scenarios.md) part 4). It
replaces the Coordinator's ad-hoc scripts of 2026-11-10 to 2026-11-13
(`market_run.sh`, `close_run.sh`, `wsl_inner.sh`, `rc_live.sh`,
`rc_w5c.sh`, `live_recheck.sh`), whose steps it generalises.

**Live runs are manual or Coordinator-run, never CI.** The approval rules
in [AGENTS.md](../AGENTS.md) apply. The emulator target of the same
scenarios runs in CI on every pull request.

## When

**User decision (2026-11-13):**
- **weekly, during US market hours;**
- **before every release,** on the release candidate.

**The Coordinator owns the scheduling:** a Windows scheduled task that runs
`bash -lc "scripts/live/run.sh …"` from Git Bash, and a Cloudroom reminder
afterwards to review the report. The repository documents the procedure.
It does not schedule anything.

## What a run is approved to include

**Standing approval** (user, 2026-11-13; recorded in AGENTS.md) for the
weekly and pre-release runs of `scripts/live/run.sh`, and nothing else:
- **The scenarios:** the Rust and Python scenarios against the live
  target. They skip what the live rules forbid.
- **The option checks:** `live_p0 option-checks --same-response` and
  `bench/p1/option_checks.py`.
- **The stock, index, calendar and interest-rate checks:** `live_p0
  entitlements --set stock-index-rate` and
  `bench/p1/stock_index_rate_checks.py`.
- **Bounded flat-file probes:** at most one message of at most 1 MiB,
  as those runs make.
- **The stream:** the scenario, contract discovery, latency at 10, 100
  and 1,000 contracts, and the full trade stream.

**Excluded:**
- requests above the account's concurrency, and a second login;
- the gated flat files (the option quote and stock quote files);
- full flat-file downloads;
- the vendor speed comparison (W5c). It needs its own approval, as for
  the 0.1.0 release candidate.

## The command

```bash
scripts/live/run.sh --run-id weekly-20261120 --part all --terminal live
```

| Option | Meaning |
| --- | --- |
| `--run-id` | Names the report folder `artifacts/live/<run-id>/` and every run inside it |
| `--part anytime` | Historical requests only: the scenarios and the stock, index, calendar and rate checks |
| `--part market` | The option checks (their snapshots need a SPY expiration listed today), then the stream |
| `--part all` | Both, in that order (the default) |
| `--terminal none` | **The default.** The Terminal is never started, stopped or reconfigured. The stream steps need one already answering, and are skipped otherwise |
| `--terminal live` or `dev` | Restart the Terminal on that config: the live market, or the dev replay feed off-hours (allowed for the stream checks) |
| `--full-trades-seconds N` | The full trade stream's length (default 1,200; 0 skips it) |
| `--no-build` | Skip the release builds and the wheel |
| `--force-off-hours` | Run the market part even when markets are closed (see below) |
| `--platform` | `windows`, `wsl` or `linux`; detected when omitted |

**Market hours are checked, not assumed.** Before the market part, the
command asks the service's own `calendar_open_today`, through the library,
whether US markets are open now.
- **The rule:** they are open only when today's `open` and `close` times
  are both present and New York's time falls between them. A closed day
  has null times, or no data at all. The `type` text is not relied on.
- **If they are closed** (a weekend, a holiday, after the close), the
  market part is refused and the summary says so. Snapshots would be
  empty and latency misleading. **`--force-off-hours`** overrides this.
- **With `--terminal dev`, off-hours,** the dev-feed stream still runs
  (labelled `dev`), and only the option checks, whose snapshots need the
  market, are skipped.
- **If the calendar can't be read,** the market counts as closed.

**The order matters.** Our gRPC login ends the Terminal's REST session
(478, `w10-conflict-001`). So every login-based step runs first, and the
Terminal is (re)started for the stream afterwards. With `--terminal none`
on a machine whose Terminal serves other programs, the REST stays at 478
until the Terminal restarts.

**The report:** one private folder, `artifacts/live/<run-id>/`, ignored by
git. It holds:
- `run.log`, filtered of session, token and e-mail lines, as the ad-hoc
  scripts did;
- `summary.txt`: one exit code per step;
- the scenario reports and `drift.json`;
- the W10 latency JSONs.

**Failures are recorded by kind and gRPC code only.** `summary.txt` holds
exit codes, and `drift.json` holds kinds and codes. The service's message
text can appear in a check's own private report, never in these two.

The checks' own private reports stay where they always were
(`artifacts/p0`, `artifacts/p1`). Nothing in it is committed. A person
reviews it, and records codes, classifications and timings in the docs as
before.

**Then the drift check:** `tools/emulator_drift.py` compares the scenario
reports with the emulator's model. Every finding is for a person to
review. **The model is never updated automatically:** a change is a pull
request citing the run.

## Settings

Copy [`scripts/live/settings.example.env`](../scripts/live/settings.example.env)
to `scripts/live/settings.env` (ignored by git) and fill in this machine's
paths.
- **It is read as data, never executed** (`scripts/live/settings.sh`):
  - one `KEY=VALUE` per line, with the value taken literally, so spaces
    and backslashes need no quotes;
  - one pair of surrounding quotes is removed;
  - a leading `~/` or `$HOME/` means the home folder;
  - an unknown key or any other line stops the run with its line number.
- **No key goes there.** The key comes from `THETADATA_API_KEY` in the
  environment only: never a file, never the command line, never printed.
- **No personal path is committed.**

| Setting | Windows (Git Bash) | WSL or Linux |
| --- | --- | --- |
| `THETA_TERMINAL_DIR` | the Terminal's folder, as a Windows path (for example `C:\Users\<you>\.thetadata`) | for example `$HOME/thetadata` |
| `THETA_JAVA` | `javaw.exe` of Temurin 21 | the JDK 21 `bin` folder. It goes first on `PATH`, because the Terminal's bootstrapper launches `lib/*.jar` with `java` by name |
| `THETA_CONFIG_LIVE`, `THETA_CONFIG_DEV` | saved `config.toml` copies, live (`fpss_nj_hosts`) and dev replay (`fpss_dev_hosts`), copied over `config.toml` by `--terminal live` or `dev`. The previous file is kept in the report folder | the same |
| `THETA_REST` | `127.0.0.1:25503`: readiness is a 200 from its expirations list | the same |
| `THETA_SETTLE` | seconds to wait after the 200 before streaming (default 60; the ad-hoc runs used 30-60) | the same |
| `PRAEVIS_PY` | `.venv-p1/Scripts/python`, with the bench requirements and `maturin` | `.venv-p1/bin/python` |

## Windows

- **The command runs from Git Bash,** as the scheduled task does.
- **The Terminal** is the user's `javaw` running `ThetaTerminalv3.jar` from
  `THETA_TERMINAL_DIR`.
  - **Found** by its command line, and **stopped** with `Stop-Process`.
  - **Started** with `Start-Process` from its folder, only with
    `--terminal live|dev`.
- **After the run,** the Terminal is left running, as before.

## WSL

**WSL steps only.** WSL Ubuntu 24.04, with systemd on and interop
disabled, so the Windows Terminal can't be reached from inside WSL. One
account streams through one Terminal.
- **Inside WSL:** a Terminal copy with the live config, Temurin 21
  unpacked without sudo (for example `~/opt/jdk21`), the repository
  (for example `~/praevis-thetadata`), cargo, and its own
  `scripts/live/settings.env`.
- **From Windows (Git Bash):** `scripts/live/wsl.sh --run-id wsl-20261120
  --part market`.
  - It stops the Windows Terminal.
  - It runs `run.sh --platform wsl --terminal live` inside WSL, passing
    the key with `WSLENV=THETADATA_API_KEY` only.
  - It restarts the Windows Terminal.
- **Inside WSL, `run.sh`** starts the WSL Terminal in its own session.
  At the end it stops it by that process group: only that Terminal, not
  any process found by name.
- **Where the reports go:** the WSL report lands in the WSL repository's
  `artifacts/live/`. The Windows side keeps a short log.

## Native Linux

**The same command,** `--platform linux`, with the Linux settings. This
is the W10 run still open
([ROADMAP](ROADMAP.md#2-native-linux-w10-latency)).

## Off-hours

- **`--part anytime`** runs any time.
- **The stream** may use the dev replay feed (`--terminal dev`). Its
  latency results are labelled `--feed dev`.
- **The option checks' snapshots need market hours.** Off-hours, the
  command skips them by itself (see *Market hours are checked*).

## What it does not do

- **It never runs in CI.**
- **It never uploads anything.**
- **It never sends** a request above the account's limit, a second
  login, or a gated flat file.
- **It never pattern-kills processes.**
- **It never changes the emulator's model.**
