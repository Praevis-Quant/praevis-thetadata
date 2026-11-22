"""Verify a published release from the outside, as a user would (read-only).

Run by .github/workflows/verify-publication.yml after every release, on
native runners for each platform, and by hand for any published version.
Standard library only; it needs ``gh`` and network access, and no token
beyond read access. It exits non-zero on the first failed check and writes
``report.json`` with what it verified.

Every leg:
- the GitHub release for ``v<version>``: published, not a draft, exactly
  the expected 35 assets, each uploaded with GitHub's recorded SHA-256
  equal to the downloaded file's;
- ``SHA256SUMS``: well-formed, naming exactly the 31 artifacts, each
  matching;
- build provenance for every artifact, online and offline (the attached
  ``.intoto.jsonl`` bundle and Sigstore's trusted root), signed by
  ``build-release.yml`` for ``refs/tags/v<version>``;
- the SBOM attestations of every executable archive and wheel;
- the CLI executable for ``--target``, unpacked and run natively without
  any token in its environment: ``--version`` prints the version.

With ``--registries`` (one leg):
- the release body equals ``docs/release-draft-<version>.md``'s;
- crates.io: each crate's API checksum, index checksum and downloaded
  ``.crate`` equal the attested release asset, and none is yanked;
- PyPI: exactly the eight attested wheels (no sdist), each with the
  attested SHA-256, none yanked;
- docs.rs: the six library crates built (pending is reported as pending,
  never as a pass; a failed build fails);
- clean installs that run: ``cargo install praevis-thetadata-cli
  --version =<version> --locked`` and ``pip install
  praevis-thetadata==<version>`` (retried while the index lags).
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tarfile
import time
import urllib.error
import urllib.request
import zipfile

import release

REPO = "Praevis-Quant/praevis-thetadata"
SIGNER = f"{REPO}/.github/workflows/build-release.yml"
PROVENANCE = "https://slsa.dev/provenance/v1"
SBOM = "https://cyclonedx.org/bom"
BUNDLES = ("{v}.intoto.jsonl", "cli-{v}.sbom.sigstore.jsonl", "wheel-{v}.sbom.sigstore.jsonl")
LIBRARIES = tuple(name for name in release.CRATES if name != "praevis-thetadata-cli")
USER_AGENT = {"User-Agent": "praevis-thetadata-release-verification (https://github.com/Praevis-Quant/praevis-thetadata)"}
TOKENS = ("GH_TOKEN", "GITHUB_TOKEN", "ACTIONS_RUNTIME_TOKEN", "ACTIONS_ID_TOKEN_REQUEST_TOKEN",
          "ACTIONS_ID_TOKEN_REQUEST_URL", "CARGO_REGISTRY_TOKEN")


class Failed(Exception):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise Failed(message)


def sha256(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(*args, env=None, cwd=None) -> str:
    return subprocess.run([str(a) for a in args], check=True, capture_output=True, text=True,
                          encoding="utf-8", env=env, cwd=cwd).stdout


def public_env() -> dict:
    """The environment without workflow or registry credentials."""
    return {k: v for k, v in os.environ.items() if k not in TOKENS}


def get(url: str, attempts: int = 5) -> bytes:
    for attempt in range(attempts):
        try:
            with urllib.request.urlopen(urllib.request.Request(url, headers=USER_AGENT), timeout=60) as response:
                return response.read()
        except urllib.error.HTTPError as error:
            if error.code < 500 or attempt == attempts - 1:
                raise
        except (urllib.error.URLError, TimeoutError, ConnectionError):
            if attempt == attempts - 1:
                raise
        time.sleep(5 * (attempt + 1))
    raise AssertionError("unreachable")


def bundle_names(version: str) -> set[str]:
    return {"praevis-thetadata-" + b.format(v=version) for b in BUNDLES}


def check_sums(version: str, sums_text: str) -> dict[str, str]:
    """The SHA256SUMS entries, after checking the manifest's shape. Pure."""
    lines = sums_text.splitlines()
    require(all(re.fullmatch(r"[0-9a-f]{64}  [A-Za-z0-9_.+-]+", line) for line in lines), "SHA256SUMS is malformed")
    entries = {line.split("  ", 1)[1]: line[:64] for line in lines}
    require(len(entries) == len(lines), "SHA256SUMS names a file twice")
    expected = release.expected_files(version)
    wheels = [n for n in entries if n.endswith(".whl")]
    require(expected <= set(entries), f"SHA256SUMS lacks {sorted(expected - set(entries))}")
    require(len(wheels) == release.WHEELS and all(w.startswith(f"praevis_thetadata-{version}-") for w in wheels),
            f"SHA256SUMS has {len(wheels)} wheels")
    require(len(entries) == len(expected) + release.WHEELS, f"SHA256SUMS has {len(entries)} entries")
    return entries


def release_assets(version: str) -> dict:
    data = json.loads(run("gh", "api", f"repos/{REPO}/releases/tags/v{version}"))
    require(not data["draft"], "the release is a draft")
    require(data["prerelease"] == ("-" in version), "the release's pre-release flag does not match the version")
    return data


def verify_attestation(path: pathlib.Path, version: str, predicate: str, out: pathlib.Path,
                       bundle: pathlib.Path | None = None, root: pathlib.Path | None = None) -> None:
    args = ["gh", "attestation", "verify", path, "--repo", REPO, "--signer-workflow", SIGNER,
            "--source-ref", f"refs/tags/v{version}", "--predicate-type", predicate, "--format", "json"]
    if bundle:
        args += ["--bundle", bundle, "--custom-trusted-root", root]
    result = json.loads(run(*args))
    require(bool(result), f"{path.name}: empty attestation verification")
    out.write_text(json.dumps(result, indent=1), encoding="utf-8")


def unpack_cli(archive: pathlib.Path, version: str, target: str, into: pathlib.Path) -> pathlib.Path:
    exe = release.BINARY + (".exe" if "windows" in target else "")
    member = f"praevis-thetadata-cli-{version}-{target}/{exe}"
    into.mkdir(parents=True, exist_ok=True)
    binary = into / exe
    if archive.name.endswith(".zip"):
        with zipfile.ZipFile(archive) as z:
            binary.write_bytes(z.read(member))
    else:
        with tarfile.open(archive) as t:
            data = t.extractfile(t.getmember(member))
            require(data is not None, f"{member} is not a file")
            binary.write_bytes(data.read())
    binary.chmod(0o755)
    return binary


def check_release(version: str, target: str, out: pathlib.Path, report: dict) -> pathlib.Path:
    data = release_assets(version)
    assets = {a["name"]: a for a in data["assets"]}
    downloads = out / "downloads"
    run("gh", "release", "download", f"v{version}", "--repo", REPO, "--dir", downloads)
    entries = check_sums(version, (downloads / "SHA256SUMS").read_text(encoding="utf-8"))
    expected_assets = set(entries) | {"SHA256SUMS"} | bundle_names(version)
    require(set(assets) == expected_assets,
            f"release assets differ: missing {sorted(expected_assets - set(assets))}, extra {sorted(set(assets) - expected_assets)}")
    for name, asset in assets.items():
        digest = sha256(downloads / name)
        require(asset["state"] == "uploaded", f"{name}: not uploaded")
        require(asset.get("digest") == f"sha256:{digest}", f"{name}: GitHub's digest differs from the download")
        if name in entries:
            require(entries[name] == digest, f"{name}: SHA256SUMS mismatch")
    report["release"] = {"url": data["html_url"], "assets": len(assets), "summed": len(entries),
                         "published_at": data["published_at"], "body_sha256": hashlib.sha256(data["body"].encode()).hexdigest()}
    print(f"release v{version}: {len(assets)} assets, {len(entries)} SHA256SUMS entries match")

    proofs = out / "attestations"
    proofs.mkdir()
    root = out / "trusted_root.jsonl"
    root.write_text(run("gh", "attestation", "trusted-root"), encoding="utf-8")
    bundle = downloads / f"praevis-thetadata-{version}.intoto.jsonl"
    for name in sorted(entries):
        verify_attestation(downloads / name, version, PROVENANCE, proofs / f"provenance-{name}.json")
        verify_attestation(downloads / name, version, PROVENANCE, proofs / f"provenance-offline-{name}.json", bundle, root)
    print(f"provenance: {len(entries)}/{len(entries)} online and offline (--source-ref refs/tags/v{version})")
    bound = [n for n in sorted(entries) if n.startswith("praevis-thetadata-cli-") and n.endswith((".zip", ".tar.gz"))
             or n.endswith(".whl")]
    require(len(bound) == len(release.CLI_TARGETS) + release.WHEELS, f"{len(bound)} executables and wheels")
    for name in bound:
        verify_attestation(downloads / name, version, SBOM, proofs / f"sbom-{name}.json")
    print(f"SBOM attestations: {len(bound)}/{len(bound)}")
    report["attestations"] = {"provenance_online": len(entries), "provenance_offline": len(entries), "sbom": len(bound)}

    binary = unpack_cli(downloads / release.archive_name(version, target), version, target, out / "cli")
    printed = run(binary, "--version", env=public_env()).strip()
    require(printed == f"praevis-thetadata {version}", f"the {target} executable prints {printed!r}")
    report["cli"] = {"target": target, "archive": release.archive_name(version, target),
                     "sha256": sha256(binary), "version_output": printed}
    print(f"CLI {target}: {printed}")
    return downloads


def check_body(version: str, report: dict) -> None:
    live = json.loads(run("gh", "api", f"repos/{REPO}/releases/tags/v{version}"))["body"]
    draft = release.release_body(release.draft(version).read_text(encoding="utf-8"))
    require(live.rstrip() == draft.rstrip(), "the release body differs from the committed draft")
    report["release_body_equals_draft"] = True
    print("release body equals the committed draft")


def check_crates_io(version: str, downloads: pathlib.Path, report: dict) -> None:
    results = {}
    for name in release.CRATES:
        attested = sha256(downloads / release.crate_file(name, version))
        meta = json.loads(get(f"https://crates.io/api/v1/crates/{name}/{version}"))["version"]
        served = hashlib.sha256(get(f"https://static.crates.io/crates/{name}/{release.crate_file(name, version)}")).hexdigest()
        index = release.index_checksum(name, version)
        require(not meta["yanked"], f"{name} {version} is yanked")
        require(meta["checksum"] == served == index == attested,
                f"{name}: api {meta['checksum']}, download {served}, index {index}, attested {attested}")
        results[name] = attested
    report["crates_io"] = results
    print(f"crates.io: {len(results)}/{len(release.CRATES)} equal the attested .crate (API, index, download)")


def check_pypi(version: str, downloads: pathlib.Path, report: dict) -> None:
    data = json.loads(get(f"https://pypi.org/pypi/praevis-thetadata/{version}/json"))
    files = {f["filename"]: f for f in data["urls"]}
    wheels = {p.name: sha256(p) for p in downloads.glob("*.whl")}
    require(set(files) == set(wheels), f"PyPI files differ: {sorted(set(files) ^ set(wheels))}")
    for name, f in files.items():
        require(f["packagetype"] == "bdist_wheel" and not f["yanked"], f"{name}: not a live wheel")
        require(f["digests"]["sha256"] == wheels[name], f"{name}: PyPI SHA-256 differs from the attested wheel")
    report["pypi"] = wheels
    print(f"PyPI: {len(files)}/{release.WHEELS} wheels equal the attested ones")


def check_docs_rs(version: str, report: dict, wait: int) -> None:
    deadline = time.monotonic() + wait
    status = {}
    while True:
        for name in LIBRARIES:
            if status.get(name) in ("built", "failed"):
                continue
            try:
                doc = json.loads(get(f"https://docs.rs/crate/{name}/{version}/status.json")).get("doc_status")
            except urllib.error.HTTPError:
                doc = None
            status[name] = {True: "built", False: "failed"}.get(doc, "pending")
        if all(s != "pending" for s in status.values()) or time.monotonic() >= deadline:
            break
        time.sleep(30)
    report["docs_rs"] = status
    for name, s in status.items():
        print(f"docs.rs {name} {version}: {s.upper()}")
    require(not any(s == "failed" for s in status.values()), "a docs.rs build failed")
    if any(s == "pending" for s in status.values()):
        print("::warning::docs.rs builds still pending; reported as PENDING, not a pass")


def check_installs(version: str, out: pathlib.Path, report: dict, pip_wait: int) -> None:
    env = public_env()
    cargo_home, root = out / "cargo-home", out / "cargo-install"
    # A fresh CARGO_HOME: nothing cached from this machine's builds. The
    # toolchain's proxies stay on PATH.
    env_cargo = {**env, "CARGO_HOME": str(cargo_home)}
    cargo_home.mkdir()
    run("cargo", "install", "praevis-thetadata-cli", "--version", f"={version}", "--locked", "--root", root,
        env=env_cargo)
    exe = root / "bin" / (release.BINARY + (".exe" if os.name == "nt" else ""))
    printed = run(exe, "--version", env=env).strip()
    require(printed == f"praevis-thetadata {version}", f"cargo install: {printed!r}")
    print(f"cargo install --locked: {printed}")

    venv = out / "venv"
    run(sys.executable, "-m", "venv", venv, env=env)
    python = venv / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
    deadline = time.monotonic() + pip_wait
    while True:
        result = subprocess.run([str(python), "-m", "pip", "install", "--no-cache-dir", "--only-binary", ":all:",
                                 f"praevis-thetadata=={version}"], capture_output=True, text=True, env=env)
        if result.returncode == 0:
            break
        require(time.monotonic() < deadline, f"pip install failed: {result.stderr[-500:]}")
        time.sleep(30)
    imported = run(python, "-I", "-c",
                   "import praevis_thetadata as pt; q, n = pt.describe('stock_history_eod', symbol='AAPL',"
                   " start_date='2024-01-02', end_date='2024-01-05'); assert n == 1 and q; print(pt.__version__)",
                   env=env).strip()
    require(imported == version, f"pip install: {imported!r}")
    print(f"pip install: praevis_thetadata {imported}, describe() works offline")
    report["installs"] = {"cargo": printed, "pip": imported}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--version", required=True, help='a published version, or "latest"')
    parser.add_argument("--target", required=True, choices=release.CLI_TARGETS)
    parser.add_argument("--out", required=True)
    parser.add_argument("--registries", action="store_true", help="also the release body, crates.io, PyPI, docs.rs and installs")
    parser.add_argument("--docs-rs-wait", type=int, default=1200, help="seconds to wait for pending docs.rs builds")
    parser.add_argument("--pip-wait", type=int, default=600, help="seconds to retry pip install while the index lags")
    args = parser.parse_args(argv)
    if args.version == "latest":
        args.version = json.loads(run("gh", "api", f"repos/{REPO}/releases/latest"))["tag_name"].removeprefix("v")
        print(f"latest release: {args.version}")
    out = pathlib.Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=False)
    report = {"version": args.version, "tag": f"v{args.version}", "target": args.target, "registries": args.registries,
              "read_only": True}
    try:
        downloads = check_release(args.version, args.target, out, report)
        if args.registries:
            check_body(args.version, report)
            check_crates_io(args.version, downloads, report)
            check_pypi(args.version, downloads, report)
            check_docs_rs(args.version, report, args.docs_rs_wait)
            check_installs(args.version, out, report, args.pip_wait)
        report["result"] = "verified"
    except (Failed, subprocess.CalledProcessError, urllib.error.URLError, KeyError) as error:
        detail = error.stderr if isinstance(error, subprocess.CalledProcessError) else ""
        report["result"] = f"FAILED: {error} {detail}".strip()
        print(f"::error::{report['result']}", file=sys.stderr)
    finally:
        shutil.rmtree(out / "downloads", ignore_errors=True)
        for scratch in ("cargo-home", "cargo-install", "venv", "cli"):
            shutil.rmtree(out / scratch, ignore_errors=True)
        (out / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    return 0 if report["result"] == "verified" else 1


if __name__ == "__main__":
    sys.exit(main())
