"""Rehearse publication of the workspace version against an isolated local staging registry.

Nothing is uploaded and no registry token is read. Steps:

1. Package wave 1 (auth, core, proto) with `cargo package`, which verifies each
   archive by building it in isolation.
2. Add those archives to a local alternative registry: a git index plus
   file:// downloads. Dependencies on our packages stay in this registry, and
   third-party dependencies point to crates.io, exactly as a crates.io index
   entry would record them.
3. Package wave 2 (arrow), wave 3 (client, cli) and wave 4 (stream),
   resolving earlier waves' packages from the staging registry through
   `[patch.crates-io]`, and stage those archives too.
4. Inspect every archive: file lists, SHA-256, required license files and
   forbidden inputs.
5. Copy consumer crates (tools/release-consumers) to a temporary directory
   outside the workspace, build and run them against the staging registry, and
   install the CLI from it.

    python tools/staging_registry.py --work target/staging [--skip-consumers]

Writes report.json and per-step logs under --work. The registry name used in
Cargo configuration is `praevis-staging`; it exists only through environment
variables in the child processes started here.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
import tomllib

ROOT = Path(__file__).resolve().parents[1]
VERSION = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
# The publication order (docs/crates-io-release-plan.md). The arrow crate depends
# on core and proto, and the client depends on it, so it is staged in a wave
# between them. The stream crate has no internal dependencies, but its packaged
# lockfile resolves its dev-dependency on the client, so it comes last.
WAVES = [["praevis-thetadata-auth", "praevis-thetadata-core", "praevis-thetadata-proto"],
         ["praevis-thetadata-arrow"],
         ["praevis-thetadata-client", "praevis-thetadata-cli"],
         ["praevis-thetadata-stream"]]
OURS = {name for wave in WAVES for name in wave}
CRATES_IO_INDEX = "https://github.com/rust-lang/crates.io-index"
REGISTRY = "praevis-staging"
REQUIRED = {"LICENSE", "NOTICE", "README.md", "Cargo.toml"}
FORBIDDEN_PARTS = {"artifacts", "target", ".git"}
FORBIDDEN_NAMES = {"creds.txt", ".env", "id_rsa", "id_ed25519"}
# Keys, certificates and credential stores never belong in an archive.
FORBIDDEN_SUFFIXES = (".whl", ".credentials", ".zip", ".pem", ".key", ".p12", ".pfx", ".crt")
CONSUMERS = ROOT / "tools/release-consumers"
# ADR-0020: default auth (and therefore the client) must not pull native credential stores.
NO_NATIVE_STORE = {"auth-consumer", "client-consumer"}
NATIVE_STORE_PACKAGES = ("keyring", "dbus")


def run(command, log, cwd=ROOT, env=None):
    """Run a command, appending its output to a log file; raise on failure."""
    with open(log, "a", encoding="utf-8") as handle:
        handle.write(f"$ {' '.join(map(str, command))}\n")
        handle.flush()
        result = subprocess.run(command, cwd=cwd, env=env, stdout=handle, stderr=subprocess.STDOUT)
        handle.write(f"[exit {result.returncode}]\n\n")
    if result.returncode:
        sys.exit(f"Command failed ({result.returncode}); see {log}")


def index_path(name):
    name = name.lower()
    if len(name) <= 2:
        return Path(str(len(name))) / name
    if len(name) == 3:
        return Path("3") / name[0] / name
    return Path(name[:2]) / name[2:4] / name


def manifest_from_archive(archive):
    with tarfile.open(archive) as tar:
        prefix = Path(archive).name.removesuffix(".crate")
        names = sorted(m.name.removeprefix(prefix + "/") for m in tar.getmembers() if m.isfile())
        manifest = tomllib.loads(tar.extractfile(f"{prefix}/Cargo.toml").read().decode("utf-8"))
    return manifest, names


def index_entry(archive):
    """crates.io-compatible index entry from the archive's normalized Cargo.toml."""
    manifest, _ = manifest_from_archive(archive)
    package = manifest["package"]
    deps = []

    def add(table, kind, target=None):
        for rename, spec in (table or {}).items():
            spec = {"version": spec} if isinstance(spec, str) else spec
            real = spec.get("package", rename)
            deps.append({"name": rename, "req": spec.get("version", "*"), "features": spec.get("features", []),
                         "optional": spec.get("optional", False),
                         "default_features": spec.get("default-features", True), "target": target,
                         "kind": kind,
                         # Our packages are in this registry; everything else stays on crates.io.
                         **({} if real in OURS else {"registry": CRATES_IO_INDEX}),
                         **({"package": real} if real != rename else {})})

    for key, kind in (("dependencies", "normal"), ("build-dependencies", "build"), ("dev-dependencies", "dev")):
        add(manifest.get(key), kind)
    for target, tables in manifest.get("target", {}).items():
        for key, kind in (("dependencies", "normal"), ("build-dependencies", "build"), ("dev-dependencies", "dev")):
            add(tables.get(key), kind, target)
    # Index format v2: features using `dep:` or `?/` syntax belong in `features2`, as on
    # crates.io; older Cargo would otherwise misread them and leave optional deps disabled.
    features = manifest.get("features", {})
    new_syntax = {k: v for k, v in features.items() if any("dep:" in x or "?/" in x for x in v)}
    entry = {"name": package["name"], "vers": package["version"], "deps": deps,
             "cksum": hashlib.sha256(Path(archive).read_bytes()).hexdigest(),
             "features": {k: v for k, v in features.items() if k not in new_syntax}, "yanked": False,
             "links": package.get("links"), "rust_version": package.get("rust-version"), "v": 2}
    if new_syntax:
        entry["features2"] = new_syntax
    return entry


def stage(archives, registry, log):
    for archive in archives:
        entry = index_entry(archive)
        shutil.copy2(archive, registry / "crates" / Path(archive).name)
        target = registry / "index" / index_path(entry["name"])
        target.parent.mkdir(parents=True, exist_ok=True)
        with open(target, "a", encoding="utf-8") as handle:
            handle.write(json.dumps(entry, separators=(",", ":")) + "\n")
    git = ["git", "-C", str(registry / "index")]
    run([*git, "add", "-A"], log)
    run([*git, "-c", "user.name=staging", "-c", "user.email=staging@localhost", "commit", "-q", "-m",
         "stage " + ", ".join(Path(a).name for a in archives)], log)


def inspect(archive):
    manifest, names = manifest_from_archive(archive)
    problems = sorted(REQUIRED - set(names))
    if manifest["package"]["name"] == "praevis-thetadata-proto" and "LICENSE.upstream" not in names:
        problems.append("LICENSE.upstream")
    forbidden = [n for n in names if FORBIDDEN_PARTS & set(Path(n).parts) or Path(n).name in FORBIDDEN_NAMES
                 or Path(n).name.startswith(".env") or n.endswith(FORBIDDEN_SUFFIXES)]
    if problems or forbidden:
        sys.exit(f"{Path(archive).name}: missing {problems}, forbidden {forbidden}")
    return {"archive": Path(archive).name, "sha256": hashlib.sha256(Path(archive).read_bytes()).hexdigest(),
            "bytes": Path(archive).stat().st_size, "files": names}


def source_copy(destination):
    """Copy tracked (and modified) workspace files, so packaging never edits this checkout."""
    files = subprocess.run(["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"], cwd=ROOT,
                           check=True, capture_output=True).stdout.decode().split("\0")
    for name in filter(None, files):
        if (ROOT / name).is_file():
            (destination / name).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / name, destination / name)
    return destination


def lock_packages(lock):
    return [p for p in tomllib.loads(lock.read_text(encoding="utf-8")).get("package", [])]


def environment(registry, cargo_home=None):
    env = dict(os.environ)
    env[f"CARGO_REGISTRIES_{REGISTRY.upper().replace('-', '_')}_INDEX"] = (registry / "index").as_uri()
    if cargo_home:
        env["CARGO_HOME"] = str(cargo_home)
    return env


def patch_args(staged):
    """Resolve already-staged dependencies from the staging registry instead of crates.io."""
    args = []
    for name in sorted(staged):
        args += ["--config", f'patch.crates-io.{name}.version="={VERSION}"',
                 "--config", f'patch.crates-io.{name}.registry="{REGISTRY}"']
    return args


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--work", type=Path, default=ROOT / "target/staging")
    parser.add_argument("--skip-consumers", action="store_true")
    args = parser.parse_args()
    work = args.work.resolve()
    shutil.rmtree(work, ignore_errors=True)
    # Cargo caches extracted sources by registry URL and version, so a fixed path could
    # silently reuse archives from an earlier rehearsal. A fresh path per run prevents that.
    registry = work / f"registry-{os.getpid()}-{int(time.time())}"
    (registry / "crates").mkdir(parents=True)
    (registry / "index").mkdir()
    logs = work / "logs"
    logs.mkdir()
    (registry / "index/config.json").write_text(json.dumps(
        {"dl": (registry / "crates").as_uri() + "/{crate}-{version}.crate"}) + "\n", encoding="utf-8")
    run(["git", "init", "-q", str(registry / "index")], logs / "registry.log")
    run(["git", "-C", str(registry / "index"), "add", "-A"], logs / "registry.log")
    run(["git", "-C", str(registry / "index"), "-c", "user.name=staging", "-c", "user.email=staging@localhost",
         "commit", "-q", "-m", "registry config"], logs / "registry.log")
    env = environment(registry)
    source = source_copy(work / "source")
    original_lock = lock_packages(source / "Cargo.lock")
    package_dir = work / "package-target"
    report = {"version": VERSION, "registry": REGISTRY, "waves": [], "consumers": []}
    for number, wave in enumerate(WAVES, 1):
        log = logs / f"wave{number}.log"
        selection = [arg for name in wave for arg in ("-p", name)]
        extra = patch_args([n for earlier in WAVES[:number - 1] for n in earlier])
        # cargo package builds each archive from its extracted contents (verification).
        # Wave 1 is fully locked. Later waves patch staged packages into crates.io, which only
        # adds unused-patch records to the copy's lockfile; anything else fails below.
        locked = ["--locked"] if not extra else []
        run(["cargo", "package", *locked, "--allow-dirty", *extra, *selection,
             "--target-dir", str(package_dir)], log, cwd=source, env=env)
        if lock_packages(source / "Cargo.lock") != original_lock:
            sys.exit(f"Wave {number} changed locked package versions; see {log}")
        archives = [package_dir / "package" / f"{name}-{VERSION}.crate" for name in wave]
        inspected = [inspect(a) for a in archives]
        stage(archives, registry, logs / "registry.log")
        report["waves"].append({"wave": number, "packages": inspected})
    if not args.skip_consumers:
        with tempfile.TemporaryDirectory(prefix="praevis-consumers-") as temporary:
            temporary = Path(temporary)
            consumer_env = environment(registry)
            consumer_env["CARGO_TARGET_DIR"] = str(work / "consumer-target")
            for consumer in sorted(p for p in CONSUMERS.iterdir() if (p / "Cargo.toml").exists()):
                target = temporary / consumer.name
                shutil.copytree(consumer, target)
                log = logs / f"consumer-{consumer.name}.log"
                run(["cargo", "run", "--quiet"], log, cwd=target, env=consumer_env)
                run(["cargo", "test", "--quiet"], log, cwd=target, env=consumer_env)
                tree = subprocess.run(["cargo", "tree", "-e", "normal", "--target", "all", "--prefix", "none"],
                                      cwd=target, env=consumer_env, check=True, capture_output=True, text=True).stdout
                (logs / f"consumer-{consumer.name}.tree.txt").write_text(tree, encoding="utf-8")
                native = sorted({line.split()[0] for line in tree.splitlines()
                                 if line.split() and line.split()[0].startswith(NATIVE_STORE_PACKAGES)})
                if consumer.name in NO_NATIVE_STORE and native:
                    sys.exit(f"{consumer.name} resolved native credential-store packages: {native}")
                report["consumers"].append({"name": consumer.name, "outside_workspace": str(target),
                                            "log": log.name, "native_store_packages": native})
            install_root = temporary / "install"
            log = logs / "consumer-cli-install.log"
            # The installed package's own manifest names crates.io for its dependencies, as a
            # published crate would; patch them to the staged archives for this rehearsal.
            run(["cargo", "install", "--quiet", "--registry", REGISTRY, "--version", f"={VERSION}",
                 *patch_args(sorted(OURS - {"praevis-thetadata-cli"})),
                 "--root", str(install_root), "praevis-thetadata-cli"], log, cwd=temporary, env=consumer_env)
            binary = install_root / "bin" / ("praevis-thetadata.exe" if os.name == "nt" else "praevis-thetadata")
            run([str(binary), "--version"], log, cwd=temporary)
            run([str(binary), "auth", "--help"], log, cwd=temporary)
            report["consumers"].append({"name": "cargo install praevis-thetadata-cli", "log": log.name})
    (work / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    for wave in report["waves"]:
        for package in wave["packages"]:
            print(f"wave {wave['wave']}: {package['archive']} {package['sha256']} ({len(package['files'])} files)")
    print(f"Consumers: {', '.join(c['name'] for c in report['consumers']) or 'skipped'}. Report: {work / 'report.json'}")


if __name__ == "__main__":
    main()
