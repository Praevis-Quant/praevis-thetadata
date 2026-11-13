"""Compare a live scenario report with the emulator's model (ADR-0022 part 3).

    python tools/emulator_drift.py artifacts/scenarios/<run-id>.json [more reports]

A live run of the scenarios (Rust and/or Python) records what it observed:
column kinds, gRPC codes, the account's tier and timings. This diffs that
against what the emulator would answer, from ``docs/endpoints/schemas.json``,
``docs/endpoints/status.json`` and ``crates/praevis-thetadata-emulator/model.json``.

**Every difference is a finding for a person to review. The model is never
updated automatically:** a change to it is a reviewed pull request citing
the run. Findings print as text and go to ``artifacts/drift/`` (ignored);
the exit code is 0 unless ``--strict``. Reports hold no values, and neither
does this output.
"""
from __future__ import annotations

import argparse
import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
SCHEMAS = ROOT / "docs" / "endpoints" / "schemas.json"
STATUS = ROOT / "docs" / "endpoints" / "status.json"
MODEL = ROOT / "crates" / "praevis-thetadata-emulator" / "model.json"
TIERS = {0: "FREE", 1: "VALUE", 2: "STANDARD", 3: "PRO"}


def expected_code(rpc: str, scenario: str, status: dict, column: str, shape: str | None = None,
                  model: dict | None = None) -> str | None:
    """The gRPC code the emulator gives ``rpc`` in ``scenario``, or None."""
    if scenario.startswith("not-found"):
        return "NotFound"
    if shape == "every-expiration-multi-day":
        refused = any(rpc in r["rpcs"] and r["expiration"] == "*" for r in (model or {}).get("refusals_by_shape", []))
        return "InvalidArgument" if refused else "OK"
    observed = status.get(rpc, {}).get(column)
    if observed is None:
        return None
    return "OK" if observed["class"] == "entitled" else observed["code"]


def drift(reports: list[dict], schemas: dict, status: dict, model: dict, profile: str,
          timing_factor: float) -> list[dict]:
    """The findings: each a dict with the scenario, what, observed, expected."""
    findings = []
    column = model["profiles"][profile]["status"]
    user = model["profiles"][profile]["user"]
    tier = TIERS[max(user["stockSubscription"], user["optionsSubscription"], user["indicesSubscription"])]
    first_event_ms = model["terminal"]["first_event_ms"]

    def finding(report, scenario, what, observed, expected):
        findings.append({"report": report.get("run_id"), "scenario": scenario, "what": what,
                         "observed": observed, "expected": expected})

    for report in reports:
        if report.get("target") != "live":
            finding(report, "-", "not a live report", report.get("target"), "live")
            continue
        for scenario in report.get("scenarios", []):
            name, observed = scenario["name"], scenario.get("observed") or {}
            if scenario["status"] == "fail":
                # Only the error's kind and code, never the detail text: it
                # can carry the service's message, which stays in the
                # private scenario report.
                finding(report, name, "scenario failed", scenario.get("error"), "pass")
            if "tier" in observed and observed["tier"] != tier:
                finding(report, name, "account tier", observed["tier"], tier)
            rpc = observed.get("rpc")
            if rpc and "columns" in observed:
                registered = schemas.get(rpc, {}).get("columns")
                seen = [list(c) for c in observed["columns"]]
                if registered is None:
                    finding(report, name, f"{rpc}: no registered schema", seen, None)
                elif seen != [list(c) for c in registered]:
                    finding(report, name, f"{rpc}: columns", seen, registered)
            if rpc and "code" in observed:
                expected = expected_code(rpc, name, status, column, observed.get("shape"), model)
                if expected is None:
                    finding(report, name, f"{rpc}: never probed on the {profile} profile", observed["code"], None)
                elif observed["code"] != expected:
                    finding(report, name, f"{rpc}: code", observed["code"], expected)
            if "first_trade_ms" in observed:
                ms = observed["first_trade_ms"]
                if not first_event_ms / timing_factor <= ms <= first_event_ms * timing_factor:
                    finding(report, name, "first stream event (ms)", ms,
                            f"{first_event_ms} within x{timing_factor:g}")
    return findings


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("reports", nargs="+", type=pathlib.Path)
    parser.add_argument("--profile", default="options-pro", help="the emulator profile the account matches")
    parser.add_argument("--timing-factor", type=float, default=5.0,
                        help="a timing within this factor of the model's is not a finding")
    parser.add_argument("--out", type=pathlib.Path, help="default artifacts/drift/<first report's run id>.json")
    parser.add_argument("--strict", action="store_true", help="exit 1 when there are findings")
    args = parser.parse_args(argv)
    reports = [json.loads(path.read_text()) for path in args.reports]
    findings = drift(reports, json.loads(SCHEMAS.read_text()), json.loads(STATUS.read_text()),
                     json.loads(MODEL.read_text()), args.profile, args.timing_factor)
    for f in findings:
        print(f"FINDING [{f['report']}] {f['scenario']}: {f['what']}: observed {f['observed']!r}, "
              f"model {f['expected']!r}")
    print(f"{len(findings)} finding(s) in {len(reports)} report(s); the model was not changed")
    out = args.out or ROOT / "artifacts" / "drift" / f"{reports[0].get('run_id', 'drift')}.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps({"findings": findings}, indent=2))
    return 1 if args.strict and findings else 0


if __name__ == "__main__":
    sys.exit(main())
