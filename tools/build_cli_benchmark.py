"""Build and preserve a release CLI with source/toolchain provenance for comparisons."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def source_hash(root):
    paths = [root / "Cargo.toml", root / "Cargo.lock"]
    for folder in ("apps", "crates"):
        paths.extend(path for path in (root / folder).rglob("*")
                     if path.is_file() and path.suffix in {".rs", ".toml", ".bin"})
    digest = hashlib.sha256()
    for path in sorted(paths):
        digest.update(path.relative_to(root).as_posix().encode() + b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
    return digest.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--source", type=Path, default=ROOT, help="Source checkout to build (defaults to this workspace)")
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    root = args.source.resolve(strict=True)
    before = source_hash(root)
    # Baseline/candidate checkouts must not overwrite the same top-level binary
    # when Cargo considers another checkout's cached artifacts fresh. Also avoid
    # mixing Windows and WSL output when they share the source tree.
    target = root / "target" / "cli-benchmark" / platform.system().lower()
    command = ["cargo", "build", "--release", "--locked", "-p", "thetadata-cli",
               "--target-dir", str(target), "--message-format=json"]
    if args.offline:
        command.append("--offline")
    build = subprocess.run(command, cwd=root, capture_output=True, text=True)
    if build.returncode:
        raise SystemExit(build.stderr)
    executable = next(Path(item["executable"]) for line in build.stdout.splitlines()
                      if (item := json.loads(line)).get("reason") == "compiler-artifact"
                      and item.get("executable") and item["target"]["name"] == "theta")
    if source_hash(root) != before:
        raise RuntimeError("Source changed during build; rerun before recording provenance")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(executable, args.output)
    report = {"git_revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
              "worktree_dirty": bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=root, text=True).strip()),
              "source_sha256": before, "binary_sha256": hashlib.sha256(args.output.read_bytes()).hexdigest(),
              "rustc": subprocess.check_output(["rustc", "-Vv"], text=True), "build_command": command}
    args.output.with_suffix(".json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"Preserved {args.output} and {args.output.with_suffix('.json')}")


if __name__ == "__main__":
    main()
