"""Recover descriptors from a reviewed wheel without importing or executing vendor code.

Maintenance only: requires google.protobuf (tools/research-requirements.txt).
Normal Rust builds use the checked-in descriptor set and need neither Python nor protoc.

    python tools/extract_protocol.py --archive thetadata-<version>-py3-none-any.whl [--check]

The wheel is read in memory, its RECORD is verified, and its SHA-256 must equal
the reviewed identity in docs/research/vendor-<version>.json. The version comes
from the wheel METADATA, never from a hard-coded value. --check compares
instead of writing.
"""
import argparse
from email.parser import Parser
import json
from pathlib import Path

from research_python import DESCRIPTOR, MANIFEST, descriptor_set, evidence, read_json, require, sha, wheel_files


def recover(archive):
    raw = Path(archive).read_bytes()
    files, _ = wheel_files(raw)
    version = Parser().parsestr(files[next(n for n in files if n.endswith("/METADATA"))].decode())["Version"]
    identity = evidence(version)["legacy"]
    require(identity.exists(), f"{version} has no reviewed artifact identity ({identity.name}); research it first")
    require(sha(raw) == read_json(identity)["artifact_sha256"], f"Archive differs from the reviewed {version} artifact")
    encoded, hashes, methods = descriptor_set(files)
    manifest = {"source_version": version, "source_sha256": hashes,
                "descriptor_sha256": sha(encoded), "methods": methods}
    return encoded, (json.dumps(manifest, indent=2) + "\n").encode("utf-8"), len(methods)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--check", action="store_true", help="Compare with the checked-in schema instead of writing")
    args = parser.parse_args()
    encoded, manifest, count = recover(args.archive)
    if args.check:
        require(DESCRIPTOR.read_bytes() == encoded, "Checked-in descriptor differs from the archive")
        require(MANIFEST.read_bytes().replace(b"\r\n", b"\n") == manifest, "Checked-in manifest differs from the archive")
        print(f"Checked-in descriptor and manifest match the archive ({count} RPCs).")
    else:
        DESCRIPTOR.write_bytes(encoded)
        MANIFEST.write_bytes(manifest)
        print(f"Recovered {count} RPCs into {DESCRIPTOR.parent}")


if __name__ == "__main__":
    main()
