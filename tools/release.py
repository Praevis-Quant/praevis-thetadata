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
- ``collect-sboms``: the CycloneDX SBOMs cargo-cyclonedx wrote next to each
  manifest, renamed for the release: one per published crate (the CLI's
  covers the executables) and one for the wheels' extension module.
- ``notes``: the GitHub release body from ``docs/release-draft-<version>.md``.
- ``package-crates``: ``cargo package`` of the seven published crates in one
  command, so each later crate resolves the earlier ones from the local
  package directory (nothing is fetched from crates.io for them), copied out
  as ``<crate>-<version>.crate`` for SHA256SUMS and the provenance.
- ``compare-crates``: package the seven crates again (without the
  verification build) and fail unless every ``.crate`` is byte-identical to
  the ones in a directory: the reproducibility check, and the gate before
  anything is published to crates.io.
- ``publish-crates``: one wave of ``cargo publish``, skipping a crate whose
  version the crates.io index already lists (a re-run after a failure). After
  each, the crate is downloaded back from crates.io and must equal the
  attested ``.crate`` byte for byte, as must the index's checksum.
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
import tempfile
import time
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
# Crates whose SBOM is released: the seven published crates, and the Python
# extension module (built into every wheel).
SBOM_CRATES = {
    "praevis-thetadata-proto": "crates/praevis-thetadata-proto",
    "praevis-thetadata-core": "crates/praevis-thetadata-core",
    "praevis-thetadata-auth": "crates/praevis-thetadata-auth",
    "praevis-thetadata-arrow": "crates/praevis-thetadata-arrow",
    "praevis-thetadata-client": "crates/praevis-thetadata-client",
    "praevis-thetadata-cli": "apps/praevis-thetadata-cli",
    "praevis-thetadata-stream": "crates/praevis-thetadata-stream",
    "praevis-thetadata-wheel": "crates/praevis-thetadata-py",
}
# crates.io waves, in dependency order (docs/releasing.md).
WAVES = {
    1: ("praevis-thetadata-proto", "praevis-thetadata-core", "praevis-thetadata-auth"),
    2: ("praevis-thetadata-arrow",),
    3: ("praevis-thetadata-client",),
    4: ("praevis-thetadata-cli", "praevis-thetadata-stream"),
}
# The published crates, in publish order.
CRATES = tuple(name for wave in sorted(WAVES) for name in WAVES[wave])


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


def sbom_name(name: str, version: str) -> str:
    return f"{name}-{version}.cdx.json"


def crate_file(name: str, version: str) -> str:
    return f"{name}-{version}.crate"


def expected_files(version: str) -> set[str]:
    return ({archive_name(version, target) for target in CLI_TARGETS}
            | {sbom_name(name, version) for name in SBOM_CRATES}
            | {crate_file(name, version) for name in CRATES})


def sha256(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def cargo_package(target_dir: pathlib.Path, verify: bool) -> pathlib.Path:
    """``cargo package`` of the published crates; returns the package directory."""
    command = ["cargo", "package", "--locked", *(["--no-verify"] if not verify else [])]
    for name in CRATES:
        command += ["-p", name]
    subprocess.run(command, cwd=ROOT, check=True, env={**os.environ, "CARGO_TARGET_DIR": str(target_dir)})
    return target_dir / "package"


def package_crates(args: argparse.Namespace) -> int:
    if args.version != workspace_version():
        sys.exit(f"version {args.version} is not the workspace version {workspace_version()}")
    package = cargo_package(ROOT / "target", verify=True)
    out = pathlib.Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    for name in CRATES:
        source = package / crate_file(name, args.version)
        (out / source.name).write_bytes(source.read_bytes())
        print(f"{source.name}  {sha256(source)}")
    return 0


def crate_mismatches(version: str, ours: pathlib.Path, theirs: pathlib.Path) -> list[str]:
    """The published crates whose ``.crate`` differs between two directories."""
    found = []
    for name in CRATES:
        a, b = ours / crate_file(name, version), theirs / crate_file(name, version)
        if not a.is_file() or not b.is_file():
            found.append(f"{crate_file(name, version)}: missing")
        elif sha256(a) != sha256(b):
            found.append(f"{a.name}: {sha256(a)} here, {sha256(b)} attested")
    return found


def compare_crates(args: argparse.Namespace) -> int:
    with tempfile.TemporaryDirectory() as tmp:
        package = cargo_package(pathlib.Path(tmp), verify=False)
        found = crate_mismatches(args.version, package, pathlib.Path(args.dist))
    for line in found:
        print(f"::error::{line}", file=sys.stderr)
    if found:
        return 1
    print(f"{len(CRATES)} crates byte-identical to {args.dist}")
    return 0


def collect_sboms(args: argparse.Namespace) -> int:
    out = pathlib.Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    for name, directory in SBOM_CRATES.items():
        manifest = ROOT / directory / "Cargo.toml"
        with open(manifest, "rb") as handle:
            package = tomllib.load(handle)["package"]["name"]
        source = ROOT / directory / f"{package}.cdx.json"
        document = json.loads(source.read_text(encoding="utf-8"))
        component = document["metadata"]["component"]
        if component["name"] != package or component.get("version") != args.version:
            sys.exit(f"{source}: describes {component['name']} {component.get('version')}")
        (out / sbom_name(name, args.version)).write_text(json.dumps(document, indent=1), encoding="utf-8")
    print(f"{len(SBOM_CRATES)} SBOMs")
    return 0


def sums(args: argparse.Namespace) -> int:
    directory = pathlib.Path(args.dir)
    files = sorted(p for p in directory.iterdir() if p.is_file() and p.name != "SHA256SUMS")
    names = {p.name for p in files}
    missing = expected_files(args.version) - names
    wheels = [n for n in names if n.endswith(".whl")]
    if missing or len(wheels) != WHEELS or len(files) != len(CLI_TARGETS) + WHEELS + len(SBOM_CRATES) + len(CRATES):
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


USER_AGENT = {"User-Agent": "praevis-thetadata-release"}


def fetch(url: str) -> bytes:
    with urllib.request.urlopen(urllib.request.Request(url, headers=USER_AGENT), timeout=60) as response:
        return response.read()


def index_checksum(name: str, version: str) -> str | None:
    """The crates.io index's checksum of a version (None: not listed)."""
    try:
        lines = fetch(f"https://index.crates.io/{index_path(name)}").decode().splitlines()
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return None
        raise
    for line in lines:
        if line.strip() and json.loads(line)["vers"] == version:
            return json.loads(line)["cksum"]
    return None


def published(name: str, version: str) -> bool:
    return index_checksum(name, version) is not None


def download_mismatch(name: str, version: str, attested: str, downloaded: bytes | None,
                      index: str | None) -> str | None:
    """Why the crate crates.io serves is not the attested one (None: it is). Pure."""
    if downloaded is None:
        return f"{crate_file(name, version)}: not downloadable from crates.io"
    if hashlib.sha256(downloaded).hexdigest() != attested:
        return (f"{crate_file(name, version)}: crates.io serves {hashlib.sha256(downloaded).hexdigest()},"
                f" the attested .crate is {attested}")
    if index != attested:
        return f"{crate_file(name, version)}: the crates.io index lists {index}, the attested .crate is {attested}"
    return None


def verify_published(name: str, version: str, dist: pathlib.Path, attempts: int = 10) -> None:
    """Download the crate back from crates.io and compare it with the attested
    one; exit on any difference. Retries only while it is not served yet."""
    attested = sha256(dist / crate_file(name, version))
    url = f"https://static.crates.io/crates/{name}/{crate_file(name, version)}"
    downloaded = None
    for attempt in range(attempts):
        try:
            downloaded = fetch(url)
            break
        except urllib.error.HTTPError as error:
            if error.code not in (403, 404) or attempt == attempts - 1:
                break
            time.sleep(30)
    problem = download_mismatch(name, version, attested, downloaded, index_checksum(name, version))
    if problem:
        sys.exit(f"::error::{problem}")
    print(f"{crate_file(name, version)}: crates.io serves the attested .crate ({attested})")


def publish_crates(args: argparse.Namespace) -> int:
    if args.version != workspace_version():
        sys.exit(f"version {args.version} is not the workspace version {workspace_version()}")
    dist = pathlib.Path(args.dist)
    for name in WAVES[args.wave]:
        if published(name, args.version):
            print(f"{name} {args.version}: already on crates.io, skipped")
        else:
            subprocess.run(["cargo", "publish", "--locked", "-p", name], cwd=ROOT, check=True)
        verify_published(name, args.version, dist)
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
    b = sub.add_parser("collect-sboms")
    b.add_argument("--version", required=True)
    b.add_argument("--out", required=True)
    n = sub.add_parser("notes")
    n.add_argument("--version", required=True)
    c = sub.add_parser("publish-crates")
    c.add_argument("--version", required=True)
    c.add_argument("--wave", type=int, choices=sorted(WAVES), required=True)
    c.add_argument("--dist", required=True, help="the checked release bundle, with the attested .crate files")
    k = sub.add_parser("package-crates")
    k.add_argument("--version", required=True)
    k.add_argument("--out", required=True)
    m = sub.add_parser("compare-crates")
    m.add_argument("--version", required=True)
    m.add_argument("--dist", required=True)
    args = parser.parse_args(argv)
    return {"check": check, "package-cli": package_cli, "sums": sums, "notes": notes,
            "publish-crates": publish_crates, "collect-sboms": collect_sboms,
            "package-crates": package_crates, "compare-crates": compare_crates}[args.command](args)


if __name__ == "__main__":
    sys.exit(main())
