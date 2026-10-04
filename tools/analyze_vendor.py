"""Audit a separately supplied wheel-compatible ZIP; never import vendor code.

Requires google.protobuf; --openapi additionally requires PyYAML. Writes only
hashes and structural metadata, not vendor source or downloaded documentation.
"""
import argparse
import ast
import base64
from collections import Counter
import csv
import hashlib
import io
import json
from pathlib import Path
import zipfile

from google.protobuf import descriptor_pb2


def digest(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, default=Path("thetadata-1.0.12-py3-none-any.zip"))
    parser.add_argument("--extracted", type=Path, default=Path("thetadata-1.0.12-py3-none-any"))
    parser.add_argument("--openapi", type=Path)
    parser.add_argument("--output", type=Path, default=Path("docs/research/vendor-1.0.12.json"))
    args = parser.parse_args()
    with zipfile.ZipFile(args.archive) as archive:
        files = {name: archive.read(name) for name in archive.namelist() if not name.endswith("/")}
    record = next(name for name in files if name.endswith(".dist-info/RECORD"))
    checked = 0
    for name, checksum, size in csv.reader(io.StringIO(files[record].decode())):
        if checksum:
            algorithm, expected = checksum.split("=", 1)
            actual = base64.urlsafe_b64encode(hashlib.new(algorithm, files[name]).digest()).decode().rstrip("=")
            if expected != actual or len(files[name]) != int(size):
                raise SystemExit(f"RECORD verification failed: {name}")
            checked += 1
    for name, data in files.items():
        if (args.extracted / name).read_bytes() != data:
            raise SystemExit(f"Extraction differs from archive: {name}")
    source = files["thetadata/client.py"].decode()
    functions = [node for node in ast.walk(ast.parse(source)) if isinstance(node, ast.FunctionDef) and not node.name.startswith("_")]
    descriptors = []
    for name in ["thetadata/_proto/endpoints_pb2.py", "thetadata/_proto/v3grpc/endpoints_pb2.py"]:
        tree = ast.parse(files[name].decode())
        encoded = next(ast.literal_eval(n.args[0]) for n in ast.walk(tree) if isinstance(n, ast.Call) and isinstance(n.func, ast.Attribute) and n.func.attr == "AddSerializedFile")
        descriptors.append(descriptor_pb2.FileDescriptorProto.FromString(encoded))
    methods = [method for file in descriptors for service in file.service for method in service.method]
    wrappers = []
    for function in functions:
        rpc = next(n.func.attr for n in ast.walk(function) if isinstance(n, ast.Call) and isinstance(n.func, ast.Attribute) and isinstance(n.func.value, ast.Attribute) and n.func.value.attr == "stub")
        guards = sorted({n.test.id for n in ast.walk(function) if isinstance(n, ast.If) and isinstance(n.test, ast.Name)})
        wrappers.append({"name": function.name, "line": function.lineno, "rpc": rpc, "truthiness_guarded_parameters": guards})
    result = {
        "artifact": args.archive.name, "artifact_sha256": digest(args.archive.read_bytes()),
        "record_entries_verified": checked, "extraction_matches_archive": True,
        "files": {name: {"bytes": len(data), "sha256": digest(data)} for name, data in sorted(files.items())},
        "public_python_methods": len(functions), "python_groups": dict(Counter(n.name.split("_")[0] for n in functions)),
        "descriptor_rpc_count": len(methods), "server_streaming_rpcs": sum(m.server_streaming for m in methods),
        "client_streaming_rpcs": sum(m.client_streaming for m in methods),
        "rpc_without_python_wrapper": sorted({m.name for m in methods} - {w["rpc"] for w in wrappers}),
        "methods": wrappers,
    }
    if args.openapi:
        import yaml
        spec = yaml.safe_load(args.openapi.read_bytes())
        operations = {v["operationId"] for path in spec["paths"].values() for v in path.values() if isinstance(v, dict) and "operationId" in v}
        names = {n.name for n in functions}
        result["openapi"] = {"source": "https://thetadata.net/docs/openapiv3.yaml", "sha256": digest(args.openapi.read_bytes()),
            "openapi_version": spec["openapi"], "api_version": spec["info"]["version"], "servers": spec["servers"],
            "path_count": len(spec["paths"]), "operation_count": len(operations), "global_security": spec.get("security"),
            "security_schemes": spec.get("components", {}).get("securitySchemes"),
            "auth_named_paths": [p for p in spec["paths"] if any(s in p.lower() for s in ("auth", "token", "session", "login", "logout"))],
            "only_python": sorted(names - operations), "only_openapi": sorted(operations - names)}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(f"Verified {checked} RECORD entries; inventoried {len(functions)} Python methods and {len(methods)} RPCs.")


if __name__ == "__main__":
    main()
