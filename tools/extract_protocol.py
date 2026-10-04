"""Recover descriptors without importing or executing the vendor package.

Maintenance only: requires Python and google.protobuf. Normal Rust builds use
the checked-in descriptor set and need neither Python nor protoc.
"""
import ast
import hashlib
import json
from pathlib import Path

from google.protobuf import descriptor_pb2, struct_pb2

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "thetadata-1.0.12-py3-none-any" / "thetadata"
DEST = ROOT / "crates" / "thetadata-proto" / "schema"


def main():
    DEST.mkdir(parents=True, exist_ok=True)
    descriptor_set = descriptor_pb2.FileDescriptorSet()
    descriptor_set.file.add().ParseFromString(struct_pb2.DESCRIPTOR.serialized_pb)
    hashes = {}
    for relative in ["_proto/endpoints_pb2.py", "_proto/v3grpc/endpoints_pb2.py"]:
        path = SOURCE / relative
        tree = ast.parse(path.read_text(encoding="utf-8"))
        serialized = next(
            ast.literal_eval(node.args[0])
            for node in ast.walk(tree)
            if isinstance(node, ast.Call)
            and isinstance(node.func, ast.Attribute)
            and node.func.attr == "AddSerializedFile"
        )
        descriptor_set.file.add().ParseFromString(serialized)
        hashes[relative] = hashlib.sha256(path.read_bytes()).hexdigest()
    encoded = descriptor_set.SerializeToString(deterministic=True)
    (DEST / "thetadata.bin").write_bytes(encoded)
    methods = [
        {"rpc": method.name, "request": method.input_type,
         "response": method.output_type, "server_streaming": method.server_streaming}
        for file in descriptor_set.file for service in file.service for method in service.method
    ]
    manifest = {"source_version": "1.0.12", "source_sha256": hashes,
                "descriptor_sha256": hashlib.sha256(encoded).hexdigest(), "methods": methods}
    (DEST / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"Recovered {len(methods)} RPCs into {DEST}")


if __name__ == "__main__":
    main()
