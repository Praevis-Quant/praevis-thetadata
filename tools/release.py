"""Release helpers for .github/workflows/trusted-publishing.yml (docs/releasing.md).

Standard library only. Subcommands:

- ``check``: the release gate (job ``gate``). On a tag push: an annotated
  tag ``v<workspace version>``, the tagged commit on ``main``, CI passed on
  that commit's push to ``main`` (every ``ci.yml`` run for the commit read
  and filtered here), the publishable workspace crates exactly the release
  set, a dated ``CHANGELOG.md`` entry and the release notes
  (``docs/releases/v<version>.md``). Dry runs report the same as warnings
  and continue. Outputs ``version``, ``publish`` and ``crates``.
- ``package-cli``: one CLI archive, ``praevis-thetadata-cli-<version>-<target>``
  (``.zip`` on Windows, ``.tar.gz`` elsewhere), with the licence files.
- ``sums``: ``SHA256SUMS`` over a directory, checked against the expected
  artifact count.
- ``collect-sboms``: the CycloneDX SBOMs cargo-cyclonedx wrote next to each
  manifest, renamed for the release: one per published crate (the CLI's
  covers the executables) and one for the wheels' extension module.
- ``notes``: the GitHub release body (or ``--title``) from the committed
  notes: ``docs/releases/v<version>.md`` (whole file; title its first
  heading), or ``docs/release-draft-<version>.md`` up to 0.1.5.
- ``github-release``: draft the GitHub release, check it, upload missing
  assets (never replacing one), check every asset, publish, check again.
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
- ``check-known-good``: the same download checks against a version already
  published (``KNOWN_GOOD``), so a dry run exercises them without a token.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import re
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


# Releases whose notes are a docs/release-draft-<version>.md with a "**Body:**"
# line and the tag as the title. From 0.1.6 the notes are the whole of
# docs/releases/v<version>.md, titled by its first heading (the release
# standard); verifiers keep reading the old layout for these versions.
LEGACY_NOTES = frozenset({"0.1.3", "0.1.4", "0.1.5"})


def notes_file(version: str, root: pathlib.Path = ROOT) -> pathlib.Path:
    if version in LEGACY_NOTES:
        return root / "docs" / f"release-draft-{version}.md"
    return root / "docs" / "releases" / f"v{version}.md"


def release_notes(version: str, root: pathlib.Path = ROOT) -> tuple[str, str]:
    """The GitHub release's (title, body) from the committed notes."""
    text = notes_file(version, root).read_text(encoding="utf-8")
    if version in LEGACY_NOTES:
        return f"v{version}", release_body(text)
    heading = next((line for line in text.splitlines() if line.startswith("# ")), None)
    if heading is None:
        raise ValueError(f"{notes_file(version, root).name} has no '# ' heading for the title")
    return heading[2:].strip(), text


def changelog_dated(version: str, changelog: str) -> bool:
    """Whether CHANGELOG.md has a dated '## [X.Y.Z] - YYYY-MM-DD' entry. Pure."""
    return re.search(rf"^## \[{re.escape(version)}\] - \d{{4}}-\d{{2}}-\d{{2}}$", changelog, re.M) is not None


def ci_succeeded(runs: list[dict]) -> bool:
    """Whether one of a commit's ci.yml runs is a successful push to main.
    Every run is read and filtered here, not by the API's status filter,
    which can miss a successful run (Rust Commons, cargo-libc#44). Pure."""
    return any(r.get("event") == "push" and r.get("head_branch") == "main" and r.get("conclusion") == "success"
               for r in runs)


def release_set_problems(packages: list[dict], version: str) -> list[str]:
    """The workspace's publishable crates must be exactly CRATES, all at the
    release version (cargo metadata packages). Pure."""
    publishable = {p["name"]: p["version"] for p in packages if p.get("publish") != []}
    found = []
    if set(publishable) != set(CRATES):
        found.append(f"publishable crates {sorted(publishable)} are not the release set {sorted(CRATES)}")
    found += [f"{name} is at {v}, not {version}" for name, v in sorted(publishable.items()) if v != version]
    return found


def problems(event: str, ref: str, version: str, *, on_main: bool, ci_passed: bool | None,
             notes_exist: bool, dated: bool, annotated: bool | None, release_set: list[str]) -> list[str]:
    """What stops a release from this run. Pure, for the tests."""
    found = list(release_set)
    if not notes_exist:
        found.append(f"{notes_file(version).relative_to(ROOT)} is missing")
    if not dated:
        found.append(f"CHANGELOG.md has no dated '## [{version}] - YYYY-MM-DD' entry")
    if event == "push" and ref.startswith("refs/tags/"):
        tag = ref.removeprefix("refs/tags/")
        if tag != f"v{version}":
            found.append(f"tag {tag} does not match the workspace version {version} (expected v{version})")
        if annotated is not True:
            found.append(f"tag {tag} is not an annotated tag")
        if not on_main:
            found.append("the tagged commit is not on main")
        if ci_passed is not True:
            found.append("CI has not passed on this commit's push to main")
    return found


def on_main(sha: str) -> bool:
    subprocess.run(["git", "fetch", "--quiet", "origin", "main"], cwd=ROOT, check=True)
    return subprocess.run(["git", "merge-base", "--is-ancestor", sha, "origin/main"], cwd=ROOT).returncode == 0


def gh_json(path: str):
    return json.loads(subprocess.run(["gh", "api", path], cwd=ROOT, check=True, capture_output=True, text=True).stdout)


def ci_passed(repo: str, sha: str) -> bool | None:
    """Whether ci.yml succeeded on this commit's push to main (None: unknown)."""
    try:
        runs = gh_json(f"repos/{repo}/actions/workflows/ci.yml/runs?head_sha={sha}&per_page=100")["workflow_runs"]
    except (OSError, subprocess.CalledProcessError, KeyError, json.JSONDecodeError):
        return None
    return ci_succeeded(runs)


def tag_annotated(repo: str, tag: str) -> bool | None:
    """Whether the tag is an annotated tag object, from the API (None: unknown)."""
    try:
        return gh_json(f"repos/{repo}/git/ref/tags/{tag}")["object"]["type"] == "tag"
    except (OSError, subprocess.CalledProcessError, KeyError, json.JSONDecodeError):
        return None


def workspace_packages() -> list[dict]:
    out = subprocess.run(["cargo", "metadata", "--no-deps", "--locked", "--format-version", "1"],
                         cwd=ROOT, check=True, capture_output=True, text=True).stdout
    return json.loads(out)["packages"]


def check(args: argparse.Namespace) -> int:
    version = workspace_version()
    is_tag = args.event == "push" and args.ref.startswith("refs/tags/")
    main = on_main(args.sha) if args.event != "pull_request" else False
    ci = ci_passed(args.repo, args.sha) if is_tag else None
    annotated = tag_annotated(args.repo, args.ref.removeprefix("refs/tags/")) if is_tag else None
    changelog = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    found = problems(args.event, args.ref, version, on_main=main, ci_passed=ci,
                     notes_exist=notes_file(version).is_file(), dated=changelog_dated(version, changelog),
                     annotated=annotated, release_set=release_set_problems(workspace_packages(), version))
    print(f"version={version}")
    print(f"publish={'true' if is_tag and not found else 'false'}")
    print(f"crates={json.dumps(list(CRATES))}")
    if is_tag:
        for line in found:
            print(f"::error::{line}", file=sys.stderr)
        return 1 if found else 0
    # Dry runs (pull requests, manual runs) report what a release would need
    # and continue; they build and checksum but never sign or publish.
    for line in found:
        print(f"::warning::{line}", file=sys.stderr)
    print(f"::notice::dry run ({args.event}): builds and checksums only, signs and publishes nothing;"
          f" version {version}, commit on main: {main}", file=sys.stderr)
    return 0


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
    title, body = release_notes(args.version)
    if args.title:
        print(title)
    else:
        sys.stdout.write(body)
    return 0


def release_problems(release: dict, tag: str, title: str, body: str, files: dict[str, tuple[int, str]],
                     *, published: bool) -> list[str]:
    """Differences between a GitHub release and what it must be: its tag,
    title, body, state, and exactly ``files`` (name -> (size, sha256)), each
    uploaded with that size and digest. GitHub stores the body with trailing
    whitespace trimmed, so the bodies are compared that way. Pure."""
    found = []
    if release.get("tag_name") != tag:
        found.append(f"tag {release.get('tag_name')!r}, expected {tag!r}")
    if release.get("name") != title:
        found.append(f"title {release.get('name')!r}, expected {title!r}")
    if (release.get("body") or "").rstrip() != body.rstrip():
        found.append("the body differs from the committed notes")
    if bool(release.get("draft")) == published:
        found.append("still a draft" if published else "already published")
    assets = {a["name"]: a for a in release.get("assets", [])}
    if set(assets) != set(files):
        found.append(f"assets differ: missing {sorted(set(files) - set(assets))}, extra {sorted(set(assets) - set(files))}")
    for name in sorted(set(assets) & set(files)):
        size, digest = files[name]
        asset = assets[name]
        if asset.get("state") != "uploaded" or asset.get("size") != size or asset.get("digest") != f"sha256:{digest}":
            found.append(f"{name}: uploaded {asset.get('size')} bytes, {asset.get('digest')}; expected {size}, sha256:{digest}")
    return found


def uploads_needed(release: dict, files: dict[str, tuple[int, str]]) -> tuple[list[str], list[int], list[str]]:
    """(files to upload, asset ids to delete first, conflicting assets) for a
    draft. A completed asset with the right size and digest is kept. One whose
    upload never completed (state not "uploaded": an interrupted run) is
    deleted and uploaded again; that happens only on a draft, which was never
    public. A completed asset that differs is a conflict the owner resolves
    (by deleting the draft). Pure."""
    assets = {a["name"]: a for a in release.get("assets", [])}
    upload, stale, conflicts = [], [], []
    for name, (size, digest) in sorted(files.items()):
        asset = assets.get(name)
        if asset is None:
            upload.append(name)
        elif asset.get("state") != "uploaded":
            stale.append(asset["id"])
            upload.append(name)
        elif asset.get("size") != size or asset.get("digest") != f"sha256:{digest}":
            conflicts.append(name)
    return upload, stale, conflicts


# The GitHub CLI; tests replace it with a fake (tools/fake_gh.py).
GH: tuple[str, ...] = ("gh",)


def gh(*args: str, input_path: pathlib.Path | None = None) -> dict | list:
    command = [*GH, "api", *args] + (["--input", str(input_path)] if input_path else [])
    out = subprocess.run(command, cwd=ROOT, check=True, capture_output=True, text=True).stdout
    return json.loads(out) if out.strip() else {}


def find_release(repo: str, tag: str) -> dict | None:
    """The release for the tag, drafts included. The tags endpoint cannot see a
    draft (a killed create leaves one), so the releases list is read."""
    page = 1
    while True:
        releases = gh(f"repos/{repo}/releases?per_page=100&page={page}")
        for release in releases:
            if release.get("tag_name") == tag:
                return release
        if len(releases) < 100:
            return None
        page += 1


def github_release(args: argparse.Namespace) -> int:
    """Draft, check, upload, check, publish, check. A re-run resumes a draft;
    a published release is only checked, never modified."""
    version, repo = args.version, args.repo
    tag = f"v{version}"
    title, body = release_notes(version)
    directory = pathlib.Path(args.dir)
    files = {p.name: (p.stat().st_size, sha256(p)) for p in sorted(directory.iterdir()) if p.is_file()}
    try:  # --verify-tag: the tag must already exist
        gh(f"repos/{repo}/git/ref/tags/{tag}")
    except subprocess.CalledProcessError:
        print(f"::error::tag {tag} does not exist; nothing was created", file=sys.stderr)
        return 1
    prerelease = "-" in version
    release = find_release(repo, tag)
    if release is not None and not release.get("draft"):
        found = release_problems(release, tag, title, body, files, published=True)
        for line in found:
            print(f"::error::published release {tag}: {line}", file=sys.stderr)
        if found:
            return 1
        print(f"{tag} is already published and matches; nothing changed")
        return 0
    if release is None:
        release = gh(f"repos/{repo}/releases", "-X", "POST", "-f", f"tag_name={tag}", "-f", f"name={title}",
                     "-f", f"body={body}", "-F", "draft=true", "-F", f"prerelease={str(prerelease).lower()}")
        print(f"created draft {release['id']} for {tag}")
    else:
        print(f"resuming draft {release['id']} for {tag}")
    found = [f for f in release_problems(release, tag, title, body, {}, published=False) if not f.startswith("assets differ")]
    if found:
        for line in found:
            print(f"::error::draft {tag}: {line}", file=sys.stderr)
        return 1
    upload, stale, conflicts = uploads_needed(release, files)
    if conflicts:
        print(f"::error::draft {tag} has assets that differ from the bundle: {conflicts}; delete the draft and re-run",
              file=sys.stderr)
        return 1
    # Only a draft reaches this point; a published release returned above.
    for asset_id in stale:
        gh(f"repos/{repo}/releases/assets/{asset_id}", "-X", "DELETE")
        print(f"deleted incomplete asset {asset_id}")
    for name in upload:
        gh(f"https://uploads.github.com/repos/{repo}/releases/{release['id']}/assets?name={name}",
           "-X", "POST", "-H", "Content-Type: application/octet-stream", input_path=directory / name)
        print(f"uploaded {name}")
    release = gh(f"repos/{repo}/releases/{release['id']}")
    found = release_problems(release, tag, title, body, files, published=False)
    if found:
        for line in found:
            print(f"::error::draft {tag} before publishing: {line}", file=sys.stderr)
        return 1
    gh(f"repos/{repo}/releases/{release['id']}", "-X", "PATCH", "-F", "draft=false",
       "-f", f"make_latest={'false' if prerelease else 'true'}")
    release = gh(f"repos/{repo}/releases/{release['id']}")
    found = release_problems(release, tag, title, body, files, published=True)
    for line in found:
        print(f"::error::published release {tag}: {line}", file=sys.stderr)
    if found:
        return 1
    print(f"published {tag}: {len(files)} assets checked by name, size and digest")
    return 0


def index_path(name: str) -> str:
    """The sparse-index path of a crate name of four or more characters."""
    return f"{name[:2]}/{name[2:4]}/{name}"


USER_AGENT = {"User-Agent": "praevis-thetadata-release"}


RETRY_DELAYS = (5, 15, 30, 60, 120)  # seconds, between attempts; about four minutes in all


def transient(error: Exception) -> bool:
    """A failure worth retrying: a server error, a timeout or a network error.
    A 4xx answer (for example 404, not served yet) is the caller's to judge."""
    if isinstance(error, urllib.error.HTTPError):
        return error.code >= 500
    return isinstance(error, (urllib.error.URLError, TimeoutError, ConnectionError))


def fetch(url: str, delays: tuple[int, ...] = RETRY_DELAYS) -> bytes:
    """GET with bounded retries and backoff on transient failures, so a slow
    or flaky registry does not fail a release after a crate is published."""
    for attempt in range(len(delays) + 1):
        try:
            with urllib.request.urlopen(urllib.request.Request(url, headers=USER_AGENT), timeout=60) as response:
                return response.read()
        except Exception as error:  # noqa: BLE001 (re-raised unless transient)
            if not transient(error) or attempt == len(delays):
                raise
            print(f"{url}: {error}; retrying in {delays[attempt]} s", file=sys.stderr)
            time.sleep(delays[attempt])
    raise AssertionError("unreachable")


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
    one; exit on any difference."""
    check_published(name, version, sha256(dist / crate_file(name, version)), attempts)


def check_published(name: str, version: str, attested: str, attempts: int = 10) -> None:
    """crates.io serves ``attested`` for this crate, and its index lists that
    checksum; exit otherwise. Network failures are retried in ``fetch``; a 403
    or 404 (not served yet) and a missing index entry, every 30 s."""
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
    index = None
    for attempt in range(attempts):
        index = index_checksum(name, version)
        if index is not None or attempt == attempts - 1:
            break
        time.sleep(30)
    problem = download_mismatch(name, version, attested, downloaded, index)
    if problem:
        sys.exit(f"::error::{problem}")
    print(f"{crate_file(name, version)}: crates.io serves the attested .crate ({attested})")


# A published version whose crates.io files are known (its publication
# record, docs/crates-io-release-plan.md): the dry run checks the download leg
# of publishing against it, with no token.
KNOWN_GOOD = ("0.1.4", {
    "praevis-thetadata-proto": "61bed2af84535480907d84c24015b93ec21df1418280af77b9e19b6c44b15f29",
    "praevis-thetadata-core": "c1c872a2cfa35e79d71e95d5ae263e2bdaecb9e17fc79576ad0a9a17e8b68af6",
    "praevis-thetadata-auth": "379d23cff023334233118187cdb3448f2300a235c64fb113b064bed590608dd5",
    "praevis-thetadata-arrow": "633dc3a2e879d5c7b94126e2f5b58b1470718478d1ea06a274dccffe64c5a590",
    "praevis-thetadata-client": "6ba16e88fccbc48e5b1bc8739b7b2c26afc839c90f7e4ce1dfb8fc49bbe127db",
    "praevis-thetadata-cli": "93bf0da90e61643ae7e404db09b07a025a01ad31e6783d084b36f073766dd18c",
    "praevis-thetadata-stream": "09707307a145e6e8bc9b242a7b4e7d4015457594fea484e0a4b579cb5dbce1b0",
})


def check_known_good(args: argparse.Namespace) -> int:
    version, hashes = KNOWN_GOOD
    for name in CRATES:
        check_published(name, version, hashes[name])
    return 0


def publish_crates(args: argparse.Namespace) -> int:
    if args.version != workspace_version():
        sys.exit(f"version {args.version} is not the workspace version {workspace_version()}")
    dist = pathlib.Path(args.dist)
    for name in WAVES[args.wave]:
        if published(name, args.version):
            print(f"{name} {args.version}: already on crates.io, skipped")
        else:
            # --no-verify: this job holds the crates.io token and an OIDC
            # token, so it must not run dependency build scripts and proc
            # macros. The build job's cargo package already ran the
            # verification build on these bytes, and compare-crates proved
            # them identical before the first upload.
            subprocess.run(["cargo", "publish", "--locked", "--no-verify", "-p", name], cwd=ROOT, check=True)
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
    n.add_argument("--title", action="store_true", help="print the release title instead of the body")
    c = sub.add_parser("publish-crates")
    c.add_argument("--version", required=True)
    c.add_argument("--wave", type=int, choices=sorted(WAVES), required=True)
    c.add_argument("--dist", required=True, help="the checked release bundle, with the attested .crate files")
    k = sub.add_parser("package-crates")
    k.add_argument("--version", required=True)
    k.add_argument("--out", required=True)
    g = sub.add_parser("github-release")
    g.add_argument("--version", required=True)
    g.add_argument("--dir", required=True, help="every file to attach, already checked against SHA256SUMS")
    g.add_argument("--repo", default=os.environ.get("GITHUB_REPOSITORY", ""))
    m = sub.add_parser("compare-crates")
    m.add_argument("--version", required=True)
    m.add_argument("--dist", required=True)
    sub.add_parser("check-known-good", help="the download checks against a published version (dry runs)")
    args = parser.parse_args(argv)
    return {"check": check, "package-cli": package_cli, "sums": sums, "notes": notes,
            "publish-crates": publish_crates, "collect-sboms": collect_sboms,
            "package-crates": package_crates, "compare-crates": compare_crates,
            "check-known-good": check_known_good, "github-release": github_release}[args.command](args)


if __name__ == "__main__":
    sys.exit(main())
