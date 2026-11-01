"""Compare consecutive ThetaData Python wheels statically; never import vendor code.

Research-only. Reuses the AST helpers in research_python.py and needs
google.protobuf (tools/research-requirements.txt) to parse serialized
descriptors. Each wheel's RECORD is verified, and its SHA-256 must match the
expected digest from PyPI's release metadata. Output is deterministic JSON.

    python tools/upstream_delta.py --output docs/research/python-1.0.13-1.0.17-delta.json \
        WHEEL_1.0.12=SHA256 WHEEL_1.0.13=SHA256 ...
"""
import argparse
import ast
import base64
import csv
from email.parser import Parser
import hashlib
import io
import json
from pathlib import Path
import zipfile

from research_python import bindings, encode, parameters, require, sha, symbols

CLIENT = "thetadata/client.py"


def load(path, expected):
    raw = Path(path).read_bytes()
    require(sha(raw) == expected, f"{path}: SHA-256 differs from the expected PyPI digest")
    with zipfile.ZipFile(io.BytesIO(raw)) as archive:
        files = {n: archive.read(n) for n in archive.namelist() if not n.endswith("/")}
    records = list(csv.reader(io.StringIO(files[next(n for n in files if n.endswith("/RECORD"))].decode())))
    require({r[0] for r in records} == set(files) and len(records) == len(files), f"{path}: RECORD coverage")
    for name, checksum, size in records:
        if checksum:
            algorithm, digest = checksum.split("=", 1)
            actual = base64.urlsafe_b64encode(hashlib.new(algorithm, files[name]).digest()).decode().rstrip("=")
            require(actual == digest and len(files[name]) == int(size), f"{path}: RECORD entry {name}")
    # dist-info directory names embed the version; compare by role instead.
    files = {(n.split("/", 1)[1] if ".dist-info/" in n else n): data for n, data in files.items()}
    metadata = Parser().parsestr(files["METADATA"].decode())
    return {"version": metadata["Version"], "sha256": sha(raw), "files": files, "metadata": metadata}


def protocol(files):
    messages, enums, rpcs, descriptor_files = {}, {}, {}, {}
    if not any(name.endswith("_pb2.py") for name in files):
        return {"files": descriptor_files, "messages": messages, "enums": enums, "rpcs": rpcs}
    # Research-only dependency, needed just to parse serialized descriptors.
    from google.protobuf import descriptor_pb2 as dp

    def kind(field):
        return field.type_name or dp.FieldDescriptorProto.Type.Name(field.type).removeprefix("TYPE_").lower()

    for name, data in sorted(files.items()):
        if not name.endswith("_pb2.py"):
            continue
        for node in ast.walk(ast.parse(data)):
            if isinstance(node, ast.Call) and getattr(node.func, "attr", "") == "AddSerializedFile":
                encoded = ast.literal_eval(node.args[0])
                descriptor = dp.FileDescriptorProto.FromString(encoded)
                descriptor_files[descriptor.name] = {"source": name, "sha256": sha(encoded)}

                def message(item, prefix):
                    full = f"{prefix}.{item.name}"
                    messages[full] = {f.name: {
                        "number": f.number, "type": kind(f),
                        "label": dp.FieldDescriptorProto.Label.Name(f.label).removeprefix("LABEL_").lower(),
                        "proto3_optional": f.proto3_optional} for f in item.field}
                    for nested in item.nested_type:
                        message(nested, full)
                    for enum in item.enum_type:
                        enums[f"{full}.{enum.name}"] = {v.name: v.number for v in enum.value}

                for item in descriptor.message_type:
                    message(item, descriptor.package)
                for enum in descriptor.enum_type:
                    enums[f"{descriptor.package}.{enum.name}"] = {v.name: v.number for v in enum.value}
                for service in descriptor.service:
                    for method in service.method:
                        rpcs[method.name] = {"request": method.input_type, "response": method.output_type,
                                             "client_streaming": method.client_streaming,
                                             "server_streaming": method.server_streaming}
    return {"files": descriptor_files, "messages": messages, "enums": enums, "rpcs": rpcs}


def python(files):
    result = {}
    for name, data in sorted(files.items()):
        if not name.endswith(".py") or name.endswith("_pb2.py"):
            continue
        source = data.decode("utf-8")
        tree = ast.parse(source)
        preceding = {}
        for parent in ast.walk(tree):
            body = getattr(parent, "body", None)
            if isinstance(body, list):
                for left, right in zip(body, body[1:]):
                    if isinstance(left, ast.Expr) and isinstance(left.value, ast.Constant) and isinstance(left.value.value, str):
                        preceding[id(right)] = left.value.value
        for qualified, node in symbols(tree):
            entry = {"source_sha256": sha(ast.get_source_segment(source, node).encode("utf-8")),
                     "description_sha256": sha((ast.get_docstring(node) or preceding.get(id(node), "")).encode())}
            if not isinstance(node, ast.ClassDef):
                entry["parameters"] = parameters(node)
                entry["bindings"] = [{k: b[k] for k in ("target", "expression", "when")} for b in bindings(node.body)]
                entry["rpc"] = sorted({n.func.attr for n in ast.walk(node) if isinstance(n, ast.Call)
                                       and isinstance(n.func, ast.Attribute) and ast.unparse(n.func.value) == "self.stub"})
            result[f"{name}:{qualified}"] = entry
        result[f"{name}:<module>"] = {
            "imports": sorted(ast.unparse(n) for n in ast.walk(tree) if isinstance(n, (ast.Import, ast.ImportFrom))),
            "environment_reads": sorted(ast.unparse(n) for n in ast.walk(tree)
                                        if isinstance(n, ast.Call) and ast.unparse(n.func) == "os.getenv")}
    return result


def changed_keys(old, new):
    return {"added": sorted(set(new) - set(old)), "removed": sorted(set(old) - set(new)),
            "changed": sorted(k for k in set(old) & set(new) if old[k] != new[k])}


def delta(old, new):
    po, pn = protocol(old["files"]), protocol(new["files"])
    fields = {}
    for name in sorted(set(po["messages"]) & set(pn["messages"])):
        a, b = po["messages"][name], pn["messages"][name]
        if a != b:
            fields[name] = {"added": {f: b[f] for f in sorted(set(b) - set(a))},
                            "removed": sorted(set(a) - set(b)),
                            "changed": {f: {"old": a[f], "new": b[f]} for f in sorted(set(a) & set(b)) if a[f] != b[f]}}
    yo, yn = python(old["files"]), python(new["files"])
    symbols_delta = changed_keys(yo, yn)
    wrappers = {}
    for key in symbols_delta["changed"]:
        a, b = yo[key], yn[key]
        if not key.startswith(f"{CLIENT}:ThetaClient.") or "parameters" not in a:
            continue
        pa, pb = {p["name"]: p for p in a["parameters"]}, {p["name"]: p for p in b["parameters"]}
        wrappers[key.split(":", 1)[1]] = {
            "parameters_added": [pb[n] for n in pb if n not in pa],
            "parameters_removed": [n for n in pa if n not in pb],
            "parameters_changed": {n: {"old": pa[n], "new": pb[n]} for n in pa if n in pb and pa[n] != pb[n]},
            "bindings_added": [x for x in b["bindings"] if x not in a["bindings"]],
            "bindings_removed": [x for x in a["bindings"] if x not in b["bindings"]],
            "description_changed": a["description_sha256"] != b["description_sha256"],
            "rpc_changed": a["rpc"] != b["rpc"]}
    added_wrappers = {k.split(":", 1)[1]: {"parameters": yn[k]["parameters"], "rpc": yn[k]["rpc"]}
                      for k in symbols_delta["added"] if k.startswith(f"{CLIENT}:ThetaClient.") and "parameters" in yn[k]}
    metadata_changes = {k: {"old": old["metadata"].get_all(k), "new": new["metadata"].get_all(k)}
                        for k in sorted(set(old["metadata"].keys()) | set(new["metadata"].keys()))
                        if old["metadata"].get_all(k) != new["metadata"].get_all(k)}
    return {
        "from": old["version"], "to": new["version"], "wheel_sha256": new["sha256"],
        "files": {k: {"old_sha256": sha(old["files"][k]) if k in old["files"] else None,
                      "new_sha256": sha(new["files"][k]) if k in new["files"] else None}
                  for k in sorted(set(old["files"]) | set(new["files"]))
                  if old["files"].get(k) != new["files"].get(k)},
        "metadata_changes": metadata_changes,
        "metadata_payload_changed": old["metadata"].get_payload() != new["metadata"].get_payload(),
        "descriptor_files": {k: pn["files"][k] for k in sorted(pn["files"]) if po["files"].get(k) != pn["files"][k]},
        "messages": {**changed_keys(po["messages"], pn["messages"]), "changed": fields,
                     "added_definitions": {k: pn["messages"][k] for k in sorted(set(pn["messages"]) - set(po["messages"]))}},
        "enums": {"added": {k: pn["enums"][k] for k in sorted(set(pn["enums"]) - set(po["enums"]))},
                  "removed": sorted(set(po["enums"]) - set(pn["enums"])),
                  "changed": {k: {"old": po["enums"][k], "new": pn["enums"][k]}
                              for k in sorted(set(po["enums"]) & set(pn["enums"])) if po["enums"][k] != pn["enums"][k]}},
        "rpcs": {"added": {k: pn["rpcs"][k] for k in sorted(set(pn["rpcs"]) - set(po["rpcs"]))},
                 "removed": sorted(set(po["rpcs"]) - set(pn["rpcs"])),
                 "changed": sorted(k for k in set(po["rpcs"]) & set(pn["rpcs"]) if po["rpcs"][k] != pn["rpcs"][k])},
        "python_symbols": {"added": symbols_delta["added"], "removed": symbols_delta["removed"],
                           "changed": symbols_delta["changed"]},
        "wrappers_added": added_wrappers,
        "wrappers_changed": wrappers,
        "counts": {"messages": len(pn["messages"]), "enums": len(pn["enums"]), "rpcs": len(pn["rpcs"]),
                   "wrappers": sum(1 for k, v in yn.items() if k.startswith(f"{CLIENT}:ThetaClient.")
                                   and "parameters" in v and not k.rsplit(".", 1)[1].startswith("_"))},
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("wheels", nargs="+", help="WHEEL_PATH=EXPECTED_SHA256, oldest first")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    loaded = [load(*item.rsplit("=", 1)) for item in args.wheels]
    require(len(loaded) >= 2, "Provide at least two wheels")
    result = {"schema_version": 1, "baseline": {"version": loaded[0]["version"], "wheel_sha256": loaded[0]["sha256"]},
              "releases": [delta(a, b) for a, b in zip(loaded, loaded[1:])]}
    args.output.write_bytes(encode(result))
    print(f"Compared {len(loaded)} wheels: {', '.join(w['version'] for w in loaded)}")


if __name__ == "__main__":
    main()
