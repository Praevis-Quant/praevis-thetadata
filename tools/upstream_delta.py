"""Inventory and diff ThetaData Python wheels statically; never import vendor code.

Research-only, and kept separate from research_python.py, which stays pinned to
the frozen 1.0.12 evidence until plan step 2 generalizes it. Reuses its AST
helpers so both tools describe source the same way.

generate (needs google.protobuf from tools/research-requirements.txt):
    python tools/upstream_delta.py generate --pypi pypi.json --artifacts DIR \
        [--provenance DIR] 1.0.12 1.0.13 1.0.14 1.0.15 1.0.16 1.0.17

  --pypi is https://pypi.org/pypi/thetadata/json saved locally; it supplies the
  expected wheel/sdist digests. --artifacts holds the downloaded wheels and
  sdists. --provenance optionally holds prov-<version>.json responses from
  https://pypi.org/integrity/thetadata/<v>/<wheel>/provenance. Every wheel's
  RECORD and PyPI digest is verified. The first version is the baseline.

check (standard library only; used by the offline tool tests):
    python tools/upstream_delta.py check

Outputs in docs/research (L = latest version, F = first version after the baseline):
    python-F-L-delta.json         per-release and cumulative deltas, artifact identities
    python-<v>-delta.md           one generated change record per release
    python-B-to-L-delta.md        generated cumulative change record
    python-L-inventory.json       full source/protocol inventory of the latest wheel
    vendor-L.json                 summary in the shape of vendor-1.0.12.json
    python-L-catalog.md           generated catalogue of every inventory item
"""
import argparse
import ast
import base64
import csv
import difflib
from email.parser import Parser
import hashlib
import io
import json
from pathlib import Path
import re
import tarfile
import zipfile

from research_python import RESEARCH, bindings, cell, decision_key, detail_counts, encode, parameters, require, sha, symbols

CLIENT = "thetadata/client.py"
BASELINE_INVENTORY = RESEARCH / "python-1.0.12-inventory.json"
BASELINE_DECISIONS = RESEARCH / "python-1.0.12-dispositions.json"
ANALYSIS = "thetadata-1.0.13-1.0.17.md"
DEPENDENCY_HEADERS = ("Requires-Python", "Requires-Dist", "Provides-Extra", "License", "License-Expression")
LOCATION_KEYS = {"line", "end_line"}


# ---------------------------------------------------------------- loading

def load(path, expected):
    raw = Path(path).read_bytes()
    require(sha(raw) == expected, f"{path}: SHA-256 differs from the expected PyPI digest")
    with zipfile.ZipFile(io.BytesIO(raw)) as archive:
        names = [n for n in archive.namelist() if not n.endswith("/")]
        require(len(names) == len(set(names)), f"{path}: duplicate archive members")
        archived = {n: archive.read(n) for n in names}
    records = list(csv.reader(io.StringIO(archived[next(n for n in archived if n.endswith("/RECORD"))].decode())))
    require({r[0] for r in records} == set(archived) and len(records) == len(archived), f"{path}: RECORD coverage")
    verified = 0
    for name, checksum, size in records:
        if checksum:
            algorithm, digest = checksum.split("=", 1)
            actual = base64.urlsafe_b64encode(hashlib.new(algorithm, archived[name]).digest()).decode().rstrip("=")
            require(actual == digest and len(archived[name]) == int(size), f"{path}: RECORD entry {name}")
            verified += 1
    # dist-info directory names embed the version; compare members by role.
    files = {(n.split("/", 1)[1] if ".dist-info/" in n else n): data for n, data in archived.items()}
    metadata = Parser().parsestr(files["METADATA"].decode())
    return {"version": metadata["Version"], "sha256": sha(raw), "bytes": len(raw), "filename": Path(path).name,
            "archived": archived, "files": files, "metadata": metadata, "record_entries_verified": verified}


def sdist_matches(path, expected, wheel):
    raw = Path(path).read_bytes()
    require(sha(raw) == expected, f"{path}: SHA-256 differs from the expected PyPI digest")
    modules = {}
    with tarfile.open(fileobj=io.BytesIO(raw)) as archive:
        for member in archive.getmembers():
            if member.isfile() and member.name.endswith(".py"):
                relative = member.name.split("/", 1)[1].removeprefix("src/")
                modules[relative] = archive.extractfile(member).read()
    wheel_modules = {n: d for n, d in wheel["files"].items() if n.endswith(".py")}
    return {"python_modules_in_wheel": len(wheel_modules),
            "byte_identical_in_sdist": sum(modules.get(n) == d for n, d in wheel_modules.items()),
            "sdist_only_python_modules": sorted(set(modules) - set(wheel_modules))}


def provenance(path, wheel_sha):
    raw = Path(path).read_bytes()
    bundle = json.loads(raw)["attestation_bundles"][0]
    attestation = bundle["attestations"][0]
    statement = json.loads(base64.b64decode(attestation["envelope"]["statement"]))
    certificate = base64.b64decode(attestation["verification_material"]["certificate"])
    commits = sorted({m.decode() for m in re.findall(rb"[0-9a-f]{40}", certificate)})
    require(len(commits) == 1, f"{path}: expected one certificate commit")
    publisher = bundle["publisher"]
    return {"response_sha256": sha(raw), "repository": publisher.get("repository"),
            "workflow": publisher.get("workflow"), "environment": publisher.get("environment"),
            "subject_matches_wheel": statement["subject"][0]["digest"]["sha256"] == wheel_sha,
            "certificate_commit": commits[0]}


# -------------------------------------------------------------- inventory

def descriptions(tree):
    """Generated wrapper descriptions are adjacent string expressions, as in research_python."""
    preceding = {}
    for parent in ast.walk(tree):
        body = getattr(parent, "body", None)
        if isinstance(body, list):
            for left, right in zip(body, body[1:]):
                if isinstance(left, ast.Expr) and isinstance(left.value, ast.Constant) and isinstance(left.value.value, str):
                    preceding[id(right)] = left.value.value
    return preceding


def inventory(wheel):
    entries, modules, wrapper_ids, protocol_files = [], [], [], []
    for name, data in sorted(wheel["archived"].items()):
        entries.append({"id": f"file:{name}", "kind": "file", "name": name, "bytes": len(data), "sha256": sha(data)})
    for name, data in sorted(wheel["files"].items()):
        if not name.endswith(".py"):
            continue
        source = data.decode("utf-8")
        tree = ast.parse(source)
        preceding = descriptions(tree)
        module = {"path": name, "symbol_ids": [], "imports": [], "environment_reads": [], "constants": []}
        for node in ast.walk(tree):
            if isinstance(node, (ast.Import, ast.ImportFrom)):
                module["imports"].append({"line": node.lineno, "statement": ast.unparse(node)})
            if isinstance(node, ast.Call) and ast.unparse(node.func) == "os.getenv":
                module["environment_reads"].append({"line": node.lineno, "expression": ast.unparse(node)})
        for node in tree.body:
            if isinstance(node, ast.Assign):
                try:
                    value = ast.literal_eval(node.value)
                except (ValueError, TypeError, SyntaxError):
                    continue
                if not isinstance(value, bytes):
                    module["constants"].append({"names": [ast.unparse(t) for t in node.targets], "value": value, "line": node.lineno})
        definitions = symbols(tree)
        for qualified, node in definitions:
            identifier = f"symbol:{name}:{qualified}"
            module["symbol_ids"].append(identifier)
            entry = {"id": identifier, "kind": "class" if isinstance(node, ast.ClassDef) else "function",
                     "name": qualified, "source": name, "line": node.lineno, "end_line": node.end_lineno,
                     "source_sha256": sha(ast.get_source_segment(source, node).encode("utf-8")),
                     "description": ast.get_docstring(node) or preceding.get(id(node), ""),
                     "decorators": [ast.unparse(d) for d in node.decorator_list]}
            if isinstance(node, ast.ClassDef):
                entry["bases"] = [ast.unparse(b) for b in node.bases]
            else:
                entry.update(signature=f"{qualified}({ast.unparse(node.args)})",
                             returns=ast.unparse(node.returns) if node.returns else None,
                             parameters=parameters(node))
            if name == CLIENT and qualified.startswith("ThetaClient.") and not node.name.startswith("_") \
                    and not isinstance(node, ast.ClassDef):
                entry["kind"] = "wrapper"
                wrapper_ids.append(identifier)
                rpcs = sorted({n.func.attr for n in ast.walk(node) if isinstance(n, ast.Call)
                               and isinstance(n.func, ast.Attribute) and ast.unparse(n.func.value) == "self.stub"})
                require(len(rpcs) == 1, f"Ambiguous RPC for {qualified}")
                entry["rpc"] = rpcs[0]
                entry["bindings"] = bindings(node.body)
                entry["truthiness_guards"] = [ast.unparse(n.test) for n in node.body if isinstance(n, ast.If)]
                entry["truthiness_guarded_parameters"] = sorted({n.test.id for n in ast.walk(node)
                                                                 if isinstance(n, ast.If) and isinstance(n.test, ast.Name)})
                first = node.body[0]
                entry["metadata_construction"] = [ast.unparse(n) for n in first.body] if isinstance(first, ast.Try) else []
                entry["exceptions"] = [ast.unparse(n.exc) for n in ast.walk(node) if isinstance(n, ast.Raise) and n.exc]
            entries.append(entry)
        module["definition_count"] = len(definitions)
        modules.append(module)
    for source, descriptor, encoded in serialized_descriptors(wheel["files"]):
        protocol_files.append({"name": descriptor.name, "source": source, "sha256": sha(encoded),
                               "package": descriptor.package, "syntax": descriptor.syntax,
                               "dependencies": list(descriptor.dependency)})
        entries.extend(descriptor_entries(descriptor, source))
    metadata = wheel["metadata"]
    counts = {}
    for entry in entries:
        counts[entry["kind"]] = counts.get(entry["kind"], 0) + 1
    return {"schema_version": 1, "source_version": wheel["version"], "artifact": wheel["filename"],
            "artifact_sha256": wheel["sha256"], "record_entries_verified": wheel["record_entries_verified"],
            "metadata": {key: metadata.get_all(key) for key in dict(metadata.items())},
            "metadata_python_examples": re.findall(r"```python\n(.*?)```", metadata.get_payload(), re.S),
            "modules": modules, "protocol_files": protocol_files, "wrapper_ids": wrapper_ids,
            "detail_counts": detail_counts(entries, modules), "counts": dict(sorted(counts.items())), "entries": entries}


def serialized_descriptors(files):
    names = [n for n in sorted(files) if n.endswith("_pb2.py")]
    if not names:
        return []
    # Research-only dependency, needed just to parse serialized descriptors.
    from google.protobuf import descriptor_pb2

    result = []
    for name in names:
        for node in ast.walk(ast.parse(files[name])):
            if isinstance(node, ast.Call) and getattr(node.func, "attr", "") == "AddSerializedFile":
                encoded = ast.literal_eval(node.args[0])
                result.append((name, descriptor_pb2.FileDescriptorProto.FromString(encoded), encoded))
    return result


def descriptor_entries(descriptor, source):
    from google.protobuf import descriptor_pb2 as dp

    entries = []

    def message(item, prefix):
        full = f"{prefix}.{item.name}"
        fields = [{"name": f.name, "number": f.number,
                   "type": f.type_name or dp.FieldDescriptorProto.Type.Name(f.type).removeprefix("TYPE_").lower(),
                   "label": dp.FieldDescriptorProto.Label.Name(f.label).removeprefix("LABEL_").lower(),
                   "proto3_optional": f.proto3_optional,
                   "oneof": item.oneof_decl[f.oneof_index].name if f.HasField("oneof_index") else None}
                  for f in item.field]
        entries.append({"id": f"message:{full}", "kind": "message", "name": full, "source": source, "fields": fields,
                        "oneofs": [o.name for o in item.oneof_decl], "map_entry": item.options.map_entry,
                        "descriptor_sha256": sha(item.SerializeToString(deterministic=True))})
        for nested in item.nested_type:
            message(nested, full)
        for nested_enum in item.enum_type:
            enum(nested_enum, full)

    def enum(item, prefix):
        entries.append({"id": f"enum:{prefix}.{item.name}", "kind": "enum", "name": f"{prefix}.{item.name}",
                        "source": source, "values": [{"name": v.name, "number": v.number} for v in item.value]})

    for item in descriptor.message_type:
        message(item, descriptor.package)
    for item in descriptor.enum_type:
        enum(item, descriptor.package)
    for service in descriptor.service:
        for method in service.method:
            entries.append({"id": f"rpc:{descriptor.package}.{service.name}/{method.name}", "kind": "rpc",
                            "name": method.name, "source": source, "request": method.input_type,
                            "response": method.output_type, "client_streaming": method.client_streaming,
                            "server_streaming": method.server_streaming,
                            "path": f"/{descriptor.package}.{service.name}/{method.name}"})
    return entries


def role(identifier):
    """Version-independent identity: dist-info file names embed the version."""
    if identifier.startswith("file:") and ".dist-info/" in identifier:
        return "file:" + identifier.split(".dist-info/", 1)[1]
    return identifier


def semantic(entry):
    return {k: v for k, v in entry.items() if k not in LOCATION_KEYS and k not in {"id", "name"}
            and not (entry["kind"] == "wrapper" and k == "bindings")} | (
        {"bindings": [{k: b[k] for k in ("target", "expression", "when")} for b in entry["bindings"]]}
        if entry["kind"] == "wrapper" else {})


def vendor_summary(data):
    wrappers = [e for e in data["entries"] if e["kind"] == "wrapper"]
    rpcs = [e for e in data["entries"] if e["kind"] == "rpc"]
    groups = {}
    for wrapper in wrappers:
        group = wrapper["name"].split(".")[-1].split("_")[0]
        groups[group] = groups.get(group, 0) + 1
    return {"artifact": data["artifact"], "artifact_sha256": data["artifact_sha256"],
            "record_entries_verified": data["record_entries_verified"],
            "extraction_matches_archive": None,
            "files": {e["name"]: {"bytes": e["bytes"], "sha256": e["sha256"]}
                      for e in data["entries"] if e["kind"] == "file"},
            "public_python_methods": len(wrappers), "python_groups": groups,
            "descriptor_rpc_count": len(rpcs), "server_streaming_rpcs": sum(r["server_streaming"] for r in rpcs),
            "client_streaming_rpcs": sum(r["client_streaming"] for r in rpcs),
            "rpc_without_python_wrapper": sorted({r["name"] for r in rpcs} - {w["rpc"] for w in wrappers}),
            "methods": [{"name": w["name"].split(".")[-1], "line": w["line"], "rpc": w["rpc"],
                         "truthiness_guarded_parameters": w["truthiness_guarded_parameters"]} for w in wrappers]}


# ------------------------------------------------------------------ delta

def text_diff(old, new):
    return [line for line in difflib.unified_diff(old.splitlines(), new.splitlines(), n=0, lineterm="")
            if not line.startswith(("---", "+++", "@@"))]


def module_items(module, key):
    return sorted(json.dumps({k: v for k, v in item.items() if k != "line"}, sort_keys=True) for item in module[key])


def delta(old_wheel, new_wheel, old_inventory, new_inventory):
    old = {role(e["id"]): e for e in old_inventory["entries"]}
    new = {role(e["id"]): e for e in new_inventory["entries"]}
    added = sorted(set(new) - set(old))
    removed = sorted(set(old) - set(new))
    changed = sorted(k for k in set(old) & set(new) if semantic(old[k]) != semantic(new[k]))

    def of(kind, keys, source):
        return [k for k in keys if source[k]["kind"] == kind]

    wrappers_changed = {}
    for key in of("wrapper", changed, new):
        a, b = old[key], new[key]
        pa, pb = {p["name"]: p for p in a["parameters"]}, {p["name"]: p for p in b["parameters"]}
        ba = [{k: x[k] for k in ("target", "expression", "when")} for x in a["bindings"]]
        bb = [{k: x[k] for k in ("target", "expression", "when")} for x in b["bindings"]]
        wrappers_changed[b["name"]] = {
            "signature": {"old": a["signature"], "new": b["signature"]},
            "parameters_added": [pb[n] for n in pb if n not in pa],
            "parameters_removed": [pa[n] for n in pa if n not in pb],
            "parameters_changed": {n: {"old": pa[n], "new": pb[n]} for n in pa if n in pb and pa[n] != pb[n]},
            "bindings_added": [x for x in bb if x not in ba], "bindings_removed": [x for x in ba if x not in bb],
            "metadata_added": [x for x in b["metadata_construction"] if x not in a["metadata_construction"]],
            "metadata_removed": [x for x in a["metadata_construction"] if x not in b["metadata_construction"]],
            "exceptions": {"old": a["exceptions"], "new": b["exceptions"]} if a["exceptions"] != b["exceptions"] else None,
            "rpc": {"old": a["rpc"], "new": b["rpc"]} if a["rpc"] != b["rpc"] else None,
            "description_diff": text_diff(a["description"], b["description"])}
    messages_changed = {}
    for key in of("message", changed, new):
        fa, fb = {f["name"]: f for f in old[key]["fields"]}, {f["name"]: f for f in new[key]["fields"]}
        messages_changed[new[key]["name"]] = {
            "fields_added": [fb[f] for f in fb if f not in fa], "fields_removed": [fa[f] for f in fa if f not in fb],
            "fields_changed": {f: {"old": fa[f], "new": fb[f]} for f in fa if f in fb and fa[f] != fb[f]},
            "oneofs": {"old": old[key]["oneofs"], "new": new[key]["oneofs"]}}
    enums_changed = {new[k]["name"]: {"old": old[k]["values"], "new": new[k]["values"]} for k in of("enum", changed, new)}
    rpcs_changed = {new[k]["name"]: {"old": semantic(old[k]), "new": semantic(new[k])} for k in of("rpc", changed, new)}
    other_symbols = {new[k]["name"] + " (" + new[k]["source"] + ")": {
        "signature": {"old": old[k].get("signature"), "new": new[k].get("signature")},
        "description_diff": text_diff(old[k]["description"], new[k]["description"])}
        for k in changed if new[k]["kind"] in {"class", "function"}}
    old_modules = {m["path"]: m for m in old_inventory["modules"]}
    modules = {}
    for module in new_inventory["modules"]:
        previous = old_modules.get(module["path"])
        if previous is None:
            modules[module["path"]] = {"added": True}
            continue
        change = {}
        for key in ("imports", "environment_reads", "constants"):
            a, b = module_items(previous, key), module_items(module, key)
            if a != b:
                change[key] = {"added": [json.loads(x) for x in b if x not in a],
                               "removed": [json.loads(x) for x in a if x not in b]}
        if change:
            modules[module["path"]] = change
    unchanged = sorted(new[k]["name"] + " (" + new[k]["source"] + ")" for k in set(old) & set(new)
                       if k not in changed and new[k]["kind"] in {"class", "function"}
                       and "_proto/" not in new[k]["source"])
    text_members = {}
    for name in sorted(set(old_wheel["files"]) | set(new_wheel["files"])):
        if name.endswith(".py") or name == "RECORD":
            continue
        a, b = old_wheel["files"].get(name, b""), new_wheel["files"].get(name, b"")
        if a != b:
            text_members[name] = text_diff(a.decode("utf-8"), b.decode("utf-8"))
    vendor = vendor_summary(new_inventory)
    return {
        "from": old_inventory["source_version"], "to": new_inventory["source_version"],
        "wheel_sha256": {"old": old_inventory["artifact_sha256"], "new": new_inventory["artifact_sha256"]},
        "files": {name: {"old": {"bytes": len(old_wheel["files"][name]), "sha256": sha(old_wheel["files"][name])}
                         if name in old_wheel["files"] else None,
                         "new": {"bytes": len(new_wheel["files"][name]), "sha256": sha(new_wheel["files"][name])}
                         if name in new_wheel["files"] else None}
                  for name in sorted(set(old_wheel["files"]) | set(new_wheel["files"]))
                  if old_wheel["files"].get(name) != new_wheel["files"].get(name)},
        "text_member_diffs": text_members,
        "metadata_payload_changed": old_wheel["metadata"].get_payload() != new_wheel["metadata"].get_payload(),
        "dependencies": {h: new_wheel["metadata"].get_all(h) for h in DEPENDENCY_HEADERS if new_wheel["metadata"].get_all(h)},
        "dependencies_changed": {h: {"old": old_wheel["metadata"].get_all(h), "new": new_wheel["metadata"].get_all(h)}
                                 for h in DEPENDENCY_HEADERS
                                 if old_wheel["metadata"].get_all(h) != new_wheel["metadata"].get_all(h)},
        "protocol_files": {p["name"]: p["sha256"] for p in new_inventory["protocol_files"]},
        "rpcs_added": [semantic(new[k]) | {"name": new[k]["name"]} for k in of("rpc", added, new)],
        "rpcs_removed": [old[k]["name"] for k in of("rpc", removed, old)],
        "rpcs_changed": rpcs_changed,
        "messages_added": [{"name": new[k]["name"], "fields": new[k]["fields"]} for k in of("message", added, new)],
        "messages_removed": [old[k]["name"] for k in of("message", removed, old)],
        "messages_changed": messages_changed,
        "enums_added": [{"name": new[k]["name"], "values": new[k]["values"]} for k in of("enum", added, new)],
        "enums_removed": [old[k]["name"] for k in of("enum", removed, old)],
        "enums_changed": enums_changed,
        "wrappers_added": [{"name": new[k]["name"], "signature": new[k]["signature"], "rpc": new[k]["rpc"],
                            "parameters": new[k]["parameters"],
                            "bindings": [{x: b[x] for x in ("target", "expression", "when")} for b in new[k]["bindings"]],
                            "metadata_construction": new[k]["metadata_construction"],
                            "exceptions": new[k]["exceptions"], "description": new[k]["description"]}
                           for k in of("wrapper", added, new)],
        "wrappers_removed": [old[k]["name"] for k in of("wrapper", removed, old)],
        "wrappers_changed": wrappers_changed,
        "other_symbols_added": sorted(new[k]["name"] + " (" + new[k]["source"] + ")" for k in added
                                      if new[k]["kind"] in {"class", "function"}),
        "other_symbols_removed": sorted(old[k]["name"] + " (" + old[k]["source"] + ")" for k in removed
                                        if old[k]["kind"] in {"class", "function"}),
        "other_symbols_changed": other_symbols,
        "unchanged_client_symbols": unchanged,
        "modules_changed": modules,
        "counts": {"old": old_inventory["counts"], "new": new_inventory["counts"]},
        "rpc_without_python_wrapper": vendor["rpc_without_python_wrapper"],
    }


# --------------------------------------------------------------- rendering

def delink(text):
    """Render vendor Markdown links as text, as research_python does, so site-relative
    vendor URLs are not mistaken for repository links."""
    return re.sub(r"\[([^\]]+)\]\(([^)]+)\)", r"\1 (\2)", text)


def code(value):
    return f"`{cell(value)}`" if value not in (None, "") else "—"


def binding_text(binding):
    when = " and ".join(binding["when"]) or "unconditional"
    return f"`{cell(binding['target'])} = {cell(binding['expression'])}` when `{cell(when)}`"


def render_delta(change, title, intro, artifacts=None):
    lines = [f"# {title}", "", "Generated by `python tools/upstream_delta.py generate`; checked by "
             "`python tools/upstream_delta.py check`. Do not edit by hand.", "", intro, "",
             f"Interpretation, vendor-documentation context and open questions: [analysis]({ANALYSIS}). "
             "Every row below is a static source observation; none is verified service behavior.", ""]
    if artifacts:
        lines += ["## Artifacts", "", "| Version | Wheel SHA-256 | Uploaded | Sdist SHA-256 | Sdist modules identical | Attested commit |",
                  "| --- | --- | --- | --- | --- | --- |"]
        for a in artifacts:
            prov = a.get("provenance") or {}
            lines.append(f"| {a['version']} | `{a['wheel']['sha256']}` | {a['wheel']['upload_time']} | "
                         f"`{a['sdist']['sha256']}` | {a['sdist_comparison']['byte_identical_in_sdist']}/"
                         f"{a['sdist_comparison']['python_modules_in_wheel']} | {code(prov.get('certificate_commit'))} |")
        lines.append("")
    counts_old, counts_new = change["counts"]["old"], change["counts"]["new"]
    lines += ["## Totals", "", f"| Inventory kind | {change['from']} | {change['to']} |", "| --- | ---: | ---: |"]
    lines += [f"| {k} | {counts_old.get(k, 0)} | {counts_new.get(k, 0)} |" for k in sorted(set(counts_old) | set(counts_new))]
    lines += ["", f"RPCs without a Python wrapper in {change['to']}: "
              + ", ".join(f"`{r}`" for r in change["rpc_without_python_wrapper"]) + ".", ""]
    lines += ["## Artifact members", "", "| Member | Old bytes / SHA-256 | New bytes / SHA-256 |", "| --- | --- | --- |"]
    for name, item in change["files"].items():
        def side(x):
            return f"{x['bytes']} / `{x['sha256'][:16]}…`" if x else "absent"
        lines.append(f"| `{name}` | {side(item['old'])} | {side(item['new'])} |")
    lines.append("")
    for name, diff in change["text_member_diffs"].items():
        lines += [f"Text diff of `{name}`:", "", "```diff", *diff, "```", ""]
    lines += [f"METADATA long description changed: **{change['metadata_payload_changed']}**.", ""]
    lines += ["## Dependencies and packaging", ""]
    if change["dependencies_changed"]:
        lines += ["| Header | Old | New |", "| --- | --- | --- |"]
        lines += [f"| {h} | {code('; '.join(v['old'] or []))} | {code('; '.join(v['new'] or []))} |"
                  for h, v in change["dependencies_changed"].items()]
    else:
        lines.append("No dependency or licensing header changed. Current values:")
        lines += ["", "| Header | Value |", "| --- | --- |"]
        lines += [f"| {h} | {code('; '.join(v))} |" for h, v in change["dependencies"].items()]
    lines += ["", "## Protocol", "", "Descriptor file hashes (serialized):", ""]
    lines += [f"- `{k}`: `{v}`" for k, v in change["protocol_files"].items()]
    lines += ["", f"### RPCs added ({len(change['rpcs_added'])})", ""]
    if change["rpcs_added"]:
        lines += ["| RPC | Request | Response | Server streaming |", "| --- | --- | --- | --- |"]
        lines += [f"| `{r['name']}` | `{r['request']}` | `{r['response']}` | {r['server_streaming']} |" for r in change["rpcs_added"]]
    else:
        lines.append("None.")
    lines += ["", f"RPCs removed: {', '.join(change['rpcs_removed']) or 'none'}. "
              f"RPCs changed: {', '.join(change['rpcs_changed']) or 'none'}.", ""]
    lines += [f"### Messages added ({len(change['messages_added'])})", ""]
    for message in change["messages_added"]:
        fields = ", ".join(f"{f['name']} = {f['number']} `{f['type']}`" + (" repeated" if f["label"] == "repeated" else "")
                           + (" optional" if f["proto3_optional"] else "") for f in message["fields"])
        lines.append(f"- `{message['name']}`: {fields or 'no fields'}")
    if not change["messages_added"]:
        lines.append("None.")
    lines += ["", f"Messages removed: {', '.join(change['messages_removed']) or 'none'}.", "",
              f"### Fields changed in existing messages ({len(change['messages_changed'])} messages)", ""]
    if change["messages_changed"]:
        lines += ["| Message | Change | Field | Number | Type | Label | proto3 optional |", "| --- | --- | --- | ---: | --- | --- | --- |"]
        for name, item in change["messages_changed"].items():
            for f in item["fields_added"]:
                lines.append(f"| `{name}` | added | {f['name']} | {f['number']} | `{f['type']}` | {f['label']} | {f['proto3_optional']} |")
            for f in item["fields_removed"]:
                lines.append(f"| `{name}` | **removed** | {f['name']} | {f['number']} | `{f['type']}` | {f['label']} | {f['proto3_optional']} |")
            for fname, f in item["fields_changed"].items():
                lines.append(f"| `{name}` | **changed** | {fname} | {f['old']['number']}→{f['new']['number']} | "
                             f"`{f['old']['type']}`→`{f['new']['type']}` | {f['old']['label']}→{f['new']['label']} | "
                             f"{f['old']['proto3_optional']}→{f['new']['proto3_optional']} |")
    else:
        lines.append("None.")
    lines += ["", "### Enums", ""]
    lines += [f"- Added `{e['name']}`: " + ", ".join(f"{v['name']} = {v['number']}" for v in e["values"]) for e in change["enums_added"]]
    lines += [f"- Changed `{n}`: {v['old']} → {v['new']}" for n, v in change["enums_changed"].items()]
    lines += [f"- Removed `{n}`" for n in change["enums_removed"]]
    if not (change["enums_added"] or change["enums_changed"] or change["enums_removed"]):
        lines.append("No enum changes.")
    lines += ["", "## Python wrappers", "", f"### Added ({len(change['wrappers_added'])})", ""]
    for w in change["wrappers_added"]:
        lines += [f"#### `{w['name']}`", "", "```text", w["signature"], "```", "", f"RPC: `{w['rpc']}`.", ""]
        lines += [f"- Wire binding: {binding_text(b)}" for b in w["bindings"]]
        lines += [f"- Metadata map: `{cell(x)}`" for x in w["metadata_construction"]]
        lines += [f"- Raises: `{cell(x)}`" for x in w["exceptions"]]
        lines += ["", "Artifact description (unverified vendor claims):", ""]
        lines += [("> " + line).rstrip() for line in delink(w["description"]).strip().splitlines()] + [""]
    if not change["wrappers_added"]:
        lines.append("None.")
    lines += ["", f"Removed: {', '.join(change['wrappers_removed']) or 'none'}.", "",
              f"### Changed ({len(change['wrappers_changed'])})", ""]
    groups = {}
    for name, w in change["wrappers_changed"].items():
        for p in w["parameters_added"]:
            groups.setdefault(("added parameter", p["name"], p["annotation"], p["default"], p["kind"]), []).append(name)
        for pname, p in w["parameters_changed"].items():
            groups.setdefault(("changed parameter", pname, f"{p['old']['annotation']} → {p['new']['annotation']}",
                               f"{p['old']['default']} → {p['new']['default']}", p["new"]["kind"]), []).append(name)
        for p in w["parameters_removed"]:
            groups.setdefault(("**removed parameter**", p["name"], p["annotation"], p["default"], p["kind"]), []).append(name)
    if groups:
        lines += ["| Change | Parameter | Annotation | Default | Kind | Wrappers |", "| --- | --- | --- | --- | --- | --- |"]
        for (what, name, annotation, default, kind), names in groups.items():
            lines.append(f"| {what} | {name} | {code(annotation)} | {code(default)} | {kind} | "
                         f"{len(names)}: " + ", ".join(n.split('.')[-1] for n in names) + " |")
        lines.append("")
    for name, w in change["wrappers_changed"].items():
        lines += [f"#### `{name}`", "", "```diff", f"- {w['signature']['old']}", f"+ {w['signature']['new']}", "```", ""]
        lines += [f"- Wire binding added: {binding_text(b)}" for b in w["bindings_added"]]
        lines += [f"- Wire binding removed: {binding_text(b)}" for b in w["bindings_removed"]]
        lines += [f"- Metadata map added: `{cell(x)}`" for x in w["metadata_added"]]
        lines += [f"- Metadata map removed: `{cell(x)}`" for x in w["metadata_removed"]]
        if w["exceptions"]:
            lines += [f"- Raised expression changed: `{cell(' ; '.join(w['exceptions']['old']))}` → "
                      f"`{cell(' ; '.join(w['exceptions']['new']))}`"]
        if w["rpc"]:
            lines.append(f"- RPC changed: `{w['rpc']['old']}` → `{w['rpc']['new']}`")
        if w["description_diff"]:
            lines += ["", "Description diff (unverified vendor claims):", "", "```diff", *(delink(x) for x in w["description_diff"]), "```"]
        lines.append("")
    if not change["wrappers_changed"]:
        lines.append("None.")
    lines += ["", "## Other Python symbols", "",
              f"Added: {', '.join(f'`{x}`' for x in change['other_symbols_added']) or 'none'}.", "",
              f"Removed: {', '.join(f'`{x}`' for x in change['other_symbols_removed']) or 'none'}.", "",
              "Changed (class containers change whenever a member changes; generated gRPC scaffolding changes with RPCs):", ""]
    for name, item in change["other_symbols_changed"].items():
        signature = "" if item["signature"]["old"] == item["signature"]["new"] else \
            f" Signature `{cell(item['signature']['old'])}` → `{cell(item['signature']['new'])}`."
        description = " Description changed." if item["description_diff"] else ""
        lines.append(f"- `{name}`.{signature}{description}")
    if not change["other_symbols_changed"]:
        lines.append("- None.")
    lines += ["", "Unchanged hand-written non-wrapper symbols (every module outside `_proto/`; "
              "authentication, credential file, construction, conversion, date/time helpers and error classes are among these):", ""]
    lines.append(", ".join(f"`{x}`" for x in change["unchanged_client_symbols"]) + ".")
    lines += ["", "## Module imports, environment reads and literal constants", ""]
    if change["modules_changed"]:
        for path, item in change["modules_changed"].items():
            lines.append(f"- `{path}`: {json.dumps(item, ensure_ascii=False)}")
    else:
        lines.append("No import, environment-read or literal-constant change in any module.")
    return ("\n".join(lines).rstrip() + "\n").encode("utf-8")


def render_catalog(data, history):
    baseline = json.loads(BASELINE_INVENTORY.read_text(encoding="utf-8"))
    decisions = {d["id"]: d for d in json.loads(BASELINE_DECISIONS.read_text(encoding="utf-8"))}
    baseline_entries = {role(e["id"]): e for e in baseline["entries"]}
    lines = [f"# Python {data['source_version']} artifact catalogue", "",
             "Generated by `python tools/upstream_delta.py generate`; do not edit by hand.", "",
             f"Companion to the frozen [1.0.12 catalogue](python-1.0.12-catalog.md). Read the "
             f"[analysis]({ANALYSIS}) first. Dispositions are **not** re-decided here: carried-forward "
             "items show their 1.0.12 feature key and status; items added or changed since 1.0.12 are "
             "marked for review. The compatibility baseline remains 1.0.12 until the "
             "[update plan](../upstream-update-1.0.17.md) is approved.", "",
             f"Wheel `{data['artifact']}`, SHA-256 `{data['artifact_sha256']}`; "
             f"{data['record_entries_verified']} hashed RECORD entries verified.", "",
             "## Coverage", "", "| Inventory kind | Count |", "| --- | ---: |"]
    lines += [f"| {kind} | {count} |" for kind, count in data["counts"].items()]
    lines += ["", "| Detail | Count |", "| --- | ---: |"]
    lines += [f"| {kind} | {count} |" for kind, count in data["detail_counts"].items()]
    lines += ["", "## Every inventory item", "",
              "| Item ID | Since 1.0.12 | 1.0.12 feature key / disposition | Detail |", "| --- | --- | --- | --- |"]
    for entry in data["entries"]:
        key = role(entry["id"])
        prior = baseline_entries.get(key)
        status = history.get(key, "unchanged")
        if prior is None:
            disposition = "new: undecided"
        else:
            feature = decision_key(prior)
            disposition = f"{feature} / {decisions[feature]['status']}"
        anchor = "item-" + sha(entry["id"].encode())[:16]
        lines.append(f"| `{cell(entry['id'])}` | {status} | {disposition} | [detail](#{anchor}) |")
    lines += ["", "## Module dependencies and configuration", ""]
    for module in data["modules"]:
        lines += [f"### `{module['path']}`", "", f"Lexical definitions: {module['definition_count']}.", "",
                  "| Line | Import / environment read / literal constant |", "| ---: | --- |"]
        for key in ("imports", "environment_reads", "constants"):
            for item in module[key]:
                value = item.get("statement", item.get("expression"))
                if value is None:
                    value = f"{', '.join(item['names'])} = {item['value']!r}"
                lines.append(f"| {item['line']} | `{cell(value)}` |")
        lines.append("")
    lines += ["## Artifact metadata", "", "| Header | Value |", "| --- | --- |"]
    lines += [f"| {cell(k)} | {cell('; '.join(v))} |" for k, v in data["metadata"].items()]
    lines += ["", "## Source and protocol details", ""]
    for e in data["entries"]:
        lines += [f'<a id="item-{sha(e["id"].encode())[:16]}"></a>', "", f"### `{e['name']}`", ""]
        if e["kind"] == "file":
            lines += [f"Artifact member; {e['bytes']} bytes; SHA-256 `{e['sha256']}`.", ""]
            continue
        lines += [f"Source: `{e['source']}`" + (f", lines {e['line']}-{e['end_line']}." if "line" in e else "; serialized descriptor."), ""]
        if e["kind"] in {"function", "wrapper"}:
            lines += ["```text", e["signature"], "```", ""]
        if e["kind"] == "wrapper":
            lines += [f"RPC: `{e['rpc']}`.", "", "| Request target | Expression | Assignment condition |", "| --- | --- | --- |"]
            lines += [f"| `{cell(b['target'])}` | `{cell(b['expression'])}` | {cell(' and '.join(b['when']) or 'unconditional')} |"
                      for b in e["bindings"]]
            lines += ["", "Metadata construction:", "", "```text", *e["metadata_construction"], "```", "",
                      "Raised/reraised:", "", "```text", *e["exceptions"], "```", "",
                      "Artifact description (unverified vendor usage/entitlement claims):", ""]
            description = delink(e["description"])
            lines += [("> " + line).rstrip() for line in description.strip().splitlines()] + [""]
        elif e["kind"] == "message":
            lines += [f"Map entry: `{e['map_entry']}`; oneofs: `{', '.join(e['oneofs']) or 'none'}`.", "",
                      "| Field | Number | Type | Label | Explicit proto3 optional | Oneof |", "| --- | ---: | --- | --- | --- | --- |"]
            lines += [f"| {f['name']} | {f['number']} | `{f['type']}` | {f['label']} | {f['proto3_optional']} | {f['oneof'] or 'none'} |"
                      for f in e["fields"]]
            lines.append("")
        elif e["kind"] == "enum":
            lines += ["| Name | Value |", "| --- | ---: |"] + [f"| {v['name']} | {v['number']} |" for v in e["values"]] + [""]
        elif e["kind"] == "rpc":
            wrappers = [w["name"] for w in data["entries"] if w["kind"] == "wrapper" and w["rpc"] == e["name"]]
            lines += [f"Path: `{e['path']}`. Request: `{e['request']}`; response: `{e['response']}`. "
                      f"Server streaming: `{e['server_streaming']}`. Python wrapper: `{', '.join(wrappers) or 'NONE'}`.", ""]
        elif e["kind"] == "class":
            lines += [f"Bases: `{', '.join(e['bases']) or 'none'}`.", ""]
    return ("\n".join(lines).rstrip() + "\n").encode("utf-8")


# ----------------------------------------------------------- orchestration

def paths(versions):
    baseline, first, latest = versions[0], versions[1], versions[-1]
    return {"delta": RESEARCH / f"python-{first}-{latest}-delta.json",
            "cumulative": RESEARCH / f"python-{baseline}-to-{latest}-delta.md",
            "release": {v: RESEARCH / f"python-{v}-delta.md" for v in versions[1:]},
            "inventory": RESEARCH / f"python-{latest}-inventory.json",
            "vendor": RESEARCH / f"vendor-{latest}.json",
            "catalog": RESEARCH / f"python-{latest}-catalog.md"}


def history(releases, baseline_version):
    """Per-item status of the latest inventory relative to the baseline."""
    result = {}
    for release in releases:
        for key in release["_added_ids"]:
            result.setdefault(key, f"added in {release['to']}")
        for key in release["_changed_ids"]:
            if not result.get(key, "").startswith("added"):
                result[key] = f"changed (last in {release['to']})"
    return result


def outputs(record, latest_inventory):
    versions = [record["baseline"]["version"]] + [r["to"] for r in record["releases"]]
    target = paths(versions)
    rendered = {target["release"][r["to"]]: render_delta(
        r, f"Python {r['to']} changes from {r['from']}",
        f"Exhaustive static delta of the published {r['to']} wheel against {r['from']}. "
        f"Cumulative view: [{versions[0]} → {versions[-1]}]({target['cumulative'].name}).")
        for r in record["releases"]}
    rendered[target["cumulative"]] = render_delta(
        record["cumulative"], f"Python {versions[0]} → {versions[-1]} cumulative changes",
        f"Single comparison of the {versions[-1]} wheel against the frozen {versions[0]} baseline. "
        "Per-release records: " + ", ".join(f"[{v}]({target['release'][v].name})" for v in versions[1:]) + ".",
        record["artifacts"])
    rendered[target["vendor"]] = encode(vendor_summary(latest_inventory))
    rendered[target["catalog"]] = render_catalog(latest_inventory, record["latest_item_history"])
    return target, rendered


def generate(args):
    pypi = json.loads(Path(args.pypi).read_text(encoding="utf-8"))
    wheels, inventories, artifacts = [], [], []
    for version in args.versions:
        files = {f["packagetype"]: f for f in pypi["releases"][version]}
        require(not any(f["yanked"] for f in files.values()), f"{version} is yanked")
        wheel_info, sdist_info = files["bdist_wheel"], files["sdist"]
        wheel = load(Path(args.artifacts) / wheel_info["filename"], wheel_info["digests"]["sha256"])
        require(wheel["version"] == version, f"{wheel_info['filename']} declares {wheel['version']}")
        artifact = {"version": version,
                    "wheel": {"filename": wheel_info["filename"], "url": wheel_info["url"], "bytes": wheel["bytes"],
                              "sha256": wheel["sha256"], "upload_time": wheel_info["upload_time_iso_8601"]},
                    "sdist": {"filename": sdist_info["filename"], "url": sdist_info["url"],
                              "sha256": sdist_info["digests"]["sha256"], "upload_time": sdist_info["upload_time_iso_8601"]},
                    "record_entries_verified": wheel["record_entries_verified"],
                    "sdist_comparison": sdist_matches(Path(args.artifacts) / sdist_info["filename"],
                                                      sdist_info["digests"]["sha256"], wheel)}
        if args.provenance:
            artifact["provenance"] = provenance(Path(args.provenance) / f"prov-{version}.json", wheel["sha256"])
        wheels.append(wheel)
        inventories.append(inventory(wheel))
        artifacts.append(artifact)
    baseline = json.loads(BASELINE_INVENTORY.read_text(encoding="utf-8"))
    require(inventories[0]["artifact_sha256"] == baseline["artifact_sha256"], "First wheel is not the frozen baseline")
    releases = []
    for i in range(1, len(wheels)):
        release = delta(wheels[i - 1], wheels[i], inventories[i - 1], inventories[i])
        old = {role(e["id"]): e for e in inventories[i - 1]["entries"]}
        new = {role(e["id"]): e for e in inventories[i]["entries"]}
        release["_added_ids"] = sorted(set(new) - set(old))
        release["_changed_ids"] = sorted(k for k in set(old) & set(new) if semantic(old[k]) != semantic(new[k]))
        releases.append(release)
    latest_history = history(releases, args.versions[0])
    for release in releases:
        del release["_added_ids"], release["_changed_ids"]
    record = {"schema_version": 2, "baseline": {"version": args.versions[0], "wheel_sha256": wheels[0]["sha256"]},
              "artifacts": artifacts, "releases": releases,
              "cumulative": delta(wheels[0], wheels[-1], inventories[0], inventories[-1]),
              "latest_item_history": latest_history}
    target, rendered = outputs(record, inventories[-1])
    target["delta"].write_bytes(encode(record))
    target["inventory"].write_bytes(encode(inventories[-1]))
    for path, data in rendered.items():
        path.write_bytes(data)
    print(f"Inventoried {len(wheels)} wheels ({', '.join(args.versions)}); wrote {2 + len(rendered)} research files.")


def check():
    records = sorted(RESEARCH.glob("python-*-*-delta.json"))
    require(records, "No generated upstream delta record")
    checked = 0
    for path in records:
        record = json.loads(path.read_text(encoding="utf-8"))
        versions = [record["baseline"]["version"]] + [r["to"] for r in record["releases"]]
        require([r["from"] for r in record["releases"]] == versions[:-1], f"{path.name}: release chain is broken")
        require([a["version"] for a in record["artifacts"]] == versions, f"{path.name}: artifact list mismatch")
        require(record["baseline"]["wheel_sha256"] == json.loads(BASELINE_INVENTORY.read_text())["artifact_sha256"],
                f"{path.name}: baseline is not the frozen 1.0.12 inventory")
        cumulative = record["cumulative"]
        require((cumulative["from"], cumulative["to"]) == (versions[0], versions[-1]), f"{path.name}: cumulative range")
        target = paths(versions)
        latest = json.loads(target["inventory"].read_text(encoding="utf-8"))
        require(latest["source_version"] == versions[-1]
                and latest["artifact_sha256"] == record["artifacts"][-1]["wheel"]["sha256"]
                == cumulative["wheel_sha256"]["new"], f"{path.name}: latest inventory identity mismatch")
        require(latest["counts"] == cumulative["counts"]["new"] == record["releases"][-1]["counts"]["new"],
                f"{path.name}: inventory counts disagree with the deltas")
        require(latest["detail_counts"] == detail_counts(latest["entries"], latest["modules"]), "Detail counts do not reconcile")
        ids = [e["id"] for e in latest["entries"]]
        require(len(ids) == len(set(ids)), "Duplicate latest inventory ID")
        require(set(record["latest_item_history"]) <= {role(i) for i in ids}, "History names unknown items")
        _, rendered = outputs(record, latest)
        for output, data in rendered.items():
            require(output.exists() and output.read_bytes() == data, f"{output.name} is stale; run generate")
            checked += 1
    print(f"Checked {len(records)} upstream delta record(s) and {checked} generated research files.")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    gen = commands.add_parser("generate")
    gen.add_argument("--pypi", required=True)
    gen.add_argument("--artifacts", required=True)
    gen.add_argument("--provenance")
    gen.add_argument("versions", nargs="+")
    commands.add_parser("check")
    args = parser.parse_args()
    if args.command == "generate":
        require(len(args.versions) >= 2, "Provide the baseline and at least one newer version")
        generate(args)
    else:
        check()


if __name__ == "__main__":
    main()
