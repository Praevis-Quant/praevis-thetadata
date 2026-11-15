"""Release helpers for .github/workflows/trusted-publishing.yml (docs/releasing.md).

Standard library only. Subcommands:

- ``check``: the gate before anything is built for a release. On a tag push
  the tag must be ``v<workspace version>``, the tagged commit must be on
  ``main``, and CI must have passed on that commit's push to ``main``. Other
  events (the dry run, pull requests) report the same facts without failing
  on them. The release-notes draft must exist in every mode.
- ``package-cli``: one CLI archive, ``praevis-thetadata-cli-<version>-<target>``
  (``.zip`` on Windows, ``.tar.gz`` elsewhere), with the licence files.
- ``sums``: ``SHA256SUMS`` over a directory, checked against the expected
  artifact count.
- ``notes``: the GitHub release body from ``docs/release-draft-<version>.md``.
- ``publish-crates``: one wave of ``cargo publish``, skipping a crate whose
  version the crates.io index already lists (a re-run after a failure).
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import subprocess
import sys
import tarfile
import tomllib
import urllib.error
import urllib.request
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
CLI = ROOT / "apps" / "praevis-thetadata-cli"
BINARY = "praevis-thetadata"
CLI_TARGETS = (
    "x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc",
    "x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu",
    "x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl",
    "x86_64-apple-darwin", "aarch64-apple-darwin",
)
WHEELS = 8
# crates.io waves, in dependency order (docs/releasing.md).
WAVES = {
    1: ("praevis-thetadata-proto", "praevis-thetadata-core", "praevis-thetadata-auth"),
    2: ("praevis-thetadata-arrow",),
    3: ("praevis-thetadata-client",),
    4: ("praevis-thetadata-cli", "praevis-thetadata-stream"),
}


def workspace_version(root: pathlib.Path = ROOT) -> str:
    with open(root / "Cargo.toml", "rb") as handle:
        return tomllib.load(handle)["workspace"]["package"]["version"]


def draft(version: str, root: pathlib.Path = ROOT) -> pathlib.Path:
    return root / "docs" / f"release-draft-{version}.md"


def problems(event: str, ref: str, version: str, on_main: bool, ci_passed: bool | None,
             has_draft: bool) -> list[str]:
    """What stops a release. Pure, for the tests."""
    found = []
    if not has_draft:
        found.append(f"docs/release-draft-{version}.md is missing")
    if event == "push" and ref.startswith("refs/tags/"):
        tag = ref.removeprefix("refs/tags/")
        if tag != f"v{version}":
            found.append(f"tag {tag} does not match the workspace version {version} (expected v{version})")
        if not on_main:
            found.append("the tagged commit is not on main")
        if ci_passed is not True:
            found.append("CI has not passed on this commit's push to main")
    return found


def on_main(sha: str) -> bool:
    subprocess.run(["git", "fetch", "--quiet", "origin", "main"], cwd=ROOT, check=True)
    return subprocess.run(["git", "merge-base", "--is-ancestor", sha, "origin/main"], cwd=ROOT).returncode == 0


def ci_passed(repo: str, sha: str) -> bool | None:
    """Whether ci.yml succeeded on this commit's push to main (None: unknown)."""
    try:
        out = subprocess.run(
            ["gh", "api", f"repos/{repo}/actions/workflows/ci.yml/runs?head_sha={sha}&event=push&branch=main",
             "--jq", "[.workflow_runs[].conclusion]"],
            cwd=ROOT, check=True, capture_output=True, text=True).stdout
    except (OSError, subprocess.CalledProcessError):
        return None
    return "success" in json.loads(out)


def check(args: argparse.Namespace) -> int:
    version = workspace_version()
    is_tag = args.event == "push" and args.ref.startswith("refs/tags/")
    main = on_main(args.sha) if args.event != "pull_request" else False
    ci = ci_passed(args.repo, args.sha) if is_tag else None
    found = problems(args.event, args.ref, version, main, ci, draft(version).is_file())
    print(f"version={version}")
    print(f"publish={'true' if is_tag and not found else 'false'}")
    for line in found:
        print(f"::error::{line}", file=sys.stderr)
    if not is_tag:
        print(f"::notice::dry run ({args.event}): builds and attests only, publishes nothing;"
              f" version {version}, commit on main: {main}", file=sys.stderr)
    return 1 if found else 0


def archive_name(version: str, target: str) -> str:
    extension = "zip" if "windows" in target else "tar.gz"
    return f"praevis-thetadata-cli-{version}-{target}.{extension}"


def package_cli(args: argparse.Namespace) -> int:
    if args.target not in CLI_TARGETS:
        sys.exit(f"unknown target {args.target}")
    exe = BINARY + (".exe" if "windows" in args.target else "")
    binary = pathlib.Path(args.binary)
    out = pathlib.Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    name = archive_name(args.version, args.target)
    top = name.removesuffix(".zip").removesuffix(".tar.gz")
    files = [(binary, exe), (CLI / "LICENSE", "LICENSE"), (CLI / "NOTICE", "NOTICE"), (CLI / "README.md", "README.md")]
    path = out / name
    if name.endswith(".zip"):
        with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as archive:
            for source, member in files:
                archive.write(source, f"{top}/{member}")
    else:
        with tarfile.open(path, "w:gz") as archive:
            for source, member in files:
                info = archive.gettarinfo(str(source), f"{top}/{member}")
                info.uid = info.gid = 0
                info.uname = info.gname = ""
                info.mode = 0o755 if member == exe else 0o644
                with open(source, "rb") as handle:
                    archive.addfile(info, handle)
    print(path)
    return 0


def expected_files(version: str) -> set[str]:
    return {archive_name(version, target) for target in CLI_TARGETS}


def sums(args: argparse.Namespace) -> int:
    directory = pathlib.Path(args.dir)
    files = sorted(p for p in directory.iterdir() if p.is_file() and p.name != "SHA256SUMS")
    names = {p.name for p in files}
    missing = expected_files(args.version) - names
    wheels = [n for n in names if n.endswith(".whl")]
    if missing or len(wheels) != WHEELS or len(files) != len(CLI_TARGETS) + WHEELS:
        sys.exit(f"unexpected artifacts: missing {sorted(missing)}, {len(wheels)} wheels, {len(files)} files")
    lines = [f"{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n" for p in files]
    (directory / "SHA256SUMS").write_text("".join(lines), encoding="utf-8")
    print(f"SHA256SUMS: {len(lines)} artifacts")
    return 0


def release_body(text: str) -> str:
    """The text after the ``**Body:**`` line of a release-notes draft."""
    marker = "**Body:**"
    if marker not in text:
        raise ValueError("the draft has no **Body:** line")
    return text.split(marker, 1)[1].strip() + "\n"


def notes(args: argparse.Namespace) -> int:
    sys.stdout.write(release_body(draft(args.version).read_text(encoding="utf-8")))
    return 0


def index_path(name: str) -> str:
    """The sparse-index path of a crate name of four or more characters."""
    return f"{name[:2]}/{name[2:4]}/{name}"


def published(name: str, version: str) -> bool:
    url = f"https://index.crates.io/{index_path(name)}"
    request = urllib.request.Request(url, headers={"User-Agent": "praevis-thetadata-release"})
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            lines = response.read().decode().splitlines()
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return False
        raise
    return any(json.loads(line)["vers"] == version for line in lines if line.strip())


def publish_crates(args: argparse.Namespace) -> int:
    if args.version != workspace_version():
        sys.exit(f"version {args.version} is not the workspace version {workspace_version()}")
    for name in WAVES[args.wave]:
        if published(name, args.version):
            print(f"{name} {args.version}: already on crates.io, skipped")
            continue
        subprocess.run(["cargo", "publish", "--locked", "-p", name], cwd=ROOT, check=True)
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="command", required=True)
    c = sub.add_parser("check")
    c.add_argument("--event", default=os.environ.get("GITHUB_EVENT_NAME", ""))
    c.add_argument("--ref", default=os.environ.get("GITHUB_REF", ""))
    c.add_argument("--sha", default=os.environ.get("GITHUB_SHA", ""))
    c.add_argument("--repo", default=os.environ.get("GITHUB_REPOSITORY", ""))
    p = sub.add_parser("package-cli")
    p.add_argument("--version", required=True)
    p.add_argument("--target", required=True)
    p.add_argument("--binary", required=True)
    p.add_argument("--out", required=True)
    s = sub.add_parser("sums")
    s.add_argument("--version", required=True)
    s.add_argument("--dir", required=True)
    n = sub.add_parser("notes")
    n.add_argument("--version", required=True)
    c = sub.add_parser("publish-crates")
    c.add_argument("--version", required=True)
    c.add_argument("--wave", type=int, choices=sorted(WAVES), required=True)
    args = parser.parse_args(argv)
    return {"check": check, "package-cli": package_cli, "sums": sums, "notes": notes,
            "publish-crates": publish_crates}[args.command](args)


if __name__ == "__main__":
    sys.exit(main())
