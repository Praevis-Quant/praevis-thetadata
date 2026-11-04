"""Summarize repeated `eod_benchmark compare` reports as candidate/baseline median ratios.

Each comparison file is one alternating round. Ratios > 1 mean the candidate is slower.
For an A/A run (same commit on both sides) the spread of these ratios is the
host/runner noise to compare any A/B result against. Descriptive only (PERF-L3-008).

    python tools/eod_compare_summary.py artifacts/eod-numeric-compare-r*.json [...]
"""
import argparse
import json
from pathlib import Path

METRICS = ("decode", "delivery", "first_batch")


def ratios(paths):
    """{(engine, case): {metric: [ratio per round]}} from comparison reports."""
    result = {}
    for path in paths:
        report = json.loads(Path(path).read_text(encoding="utf-8"))
        engine = report["candidate_engine"]
        if report["baseline_engine"] != engine:
            raise ValueError(f"{path}: engines differ ({report['baseline_engine']} vs {engine})")
        for case in report["cases"]:
            entry = result.setdefault((engine, case["case"]), {m: [] for m in METRICS})
            for metric in METRICS:
                base = case["baseline_summary"][metric]["p50_ns"]
                cand = case["candidate_summary"][metric]["p50_ns"]
                entry[metric].append(cand / base)
    return result


def kind(paths):
    """The comparison kind shared by all reports, with a header line: a
    dependency A/B (lockfiles differ by design) is labelled with both lock
    hashes; plain A/B reports (and reports predating the field) get none."""
    kinds, locks = set(), set()
    for path in paths:
        report = json.loads(Path(path).read_text(encoding="utf-8"))
        kinds.add(report.get("comparison_kind", "A/B"))
        if report.get("comparison_kind") == "dependency A/B":
            locks.add((report["baseline_lock_sha256"], report["candidate_lock_sha256"]))
    if len(kinds) != 1 or len(locks) > 1:
        raise ValueError("reports mix comparison kinds or lockfiles; summarize them separately")
    label = kinds.pop()
    if label != "dependency A/B":
        return label, ""
    baseline, candidate = locks.pop()
    return label, (f"**Dependency A/B**: only the lockfile differs "
                   f"(`{baseline[:12]}` -> `{candidate[:12]}`); same gate as any A/B.\n\n")


def markdown(data):
    lines = ["| Engine | Case | Rounds | Decode p50 ratio (min-max) | Delivery p50 ratio (min-max) | First batch p50 ratio (min-max) |",
             "| --- | --- | ---: | --- | --- | --- |"]
    for (engine, case), metrics in sorted(data.items()):
        cells = [f"{min(v):.3f}-{max(v):.3f}" for v in (metrics[m] for m in METRICS)]
        lines.append(f"| {engine} | {case} | {len(metrics['decode'])} | " + " | ".join(cells) + " |")
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("reports", nargs="+", type=Path)
    reports = parser.parse_args().reports
    _, header = kind(reports)
    print(header + markdown(ratios(reports)), end="")


if __name__ == "__main__":
    main()
