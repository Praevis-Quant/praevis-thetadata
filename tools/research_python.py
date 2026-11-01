"""Inventory Python artifacts without importing them; check the research closure.

Generation (--archive) needs google.protobuf. Offline --check uses only the
standard library and the committed evidence. It cannot re-authenticate an
absent upstream archive; --archive ... --check independently regenerates it.
"""
import argparse
import ast
import base64
from collections import Counter
import csv
from email.parser import Parser
import hashlib
import io
import json
from pathlib import Path
import re
import zipfile

ROOT = Path(__file__).resolve().parents[1]
RESEARCH = ROOT / "docs/research"
MANIFEST = ROOT / "crates/praevis-thetadata-proto/schema/manifest.json"
DESCRIPTOR = ROOT / "crates/praevis-thetadata-proto/schema/thetadata.bin"
# The original research set is frozen evidence; newer baselines are checked alongside it.
FROZEN_VERSION = "1.0.12"
BASELINE_VERSION = json.loads(MANIFEST.read_text(encoding="utf-8"))["source_version"]
REPORT = RESEARCH / "thetadata-1.0.12.md"
DESCRIPTOR_SOURCES = ["_proto/endpoints_pb2.py", "_proto/v3grpc/endpoints_pb2.py"]


def evidence(version):
    """Committed evidence paths for one upstream version."""
    return {"inventory": RESEARCH / f"python-{version}-inventory.json",
            "decisions": RESEARCH / f"python-{version}-dispositions.json",
            "catalog": RESEARCH / f"python-{version}-catalog.md",
            "legacy": RESEARCH / f"vendor-{version}.json"}


# Current-baseline paths, kept as module names for existing callers and tests.
INVENTORY, DECISIONS, CATALOG, LEGACY = (evidence(BASELINE_VERSION)[k] for k in ("inventory", "decisions", "catalog", "legacy"))
STATUSES = {"supported", "planned", "intentionally excluded", "unresolved"}
REVIEW_MARKER = b"## Original review and recommendations (verbatim)"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def read_json(path):
    return json.loads(path.read_text(encoding="utf-8"))


def encode(value):
    return (json.dumps(value, indent=2, ensure_ascii=False) + "\n").encode("utf-8")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def symbols(tree):
    """Qualified lexical definitions, including definitions under control flow."""
    result = []

    def visit(node, parents):
        defining = isinstance(node, (ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef))
        if defining:
            qualified = ".".join([*parents, node.name])
            result.append((qualified, node))
            parents = [*parents, node.name]
        for child in ast.iter_child_nodes(node):
            visit(child, parents)

    visit(tree, [])
    # Independent traversal catches a future visitor skipping a subtree.
    require(len(result) == sum(isinstance(n, (ast.ClassDef, ast.FunctionDef,
                                            ast.AsyncFunctionDef)) for n in ast.walk(tree)),
            "AST definition traversal is incomplete")
    return result


def bindings(statements, conditions=()):
    """Record request construction/presence, independently of logging metadata."""
    result = []
    for node in statements:
        if isinstance(node, ast.If):
            result += bindings(node.body, (*conditions, ast.unparse(node.test)))
            result += bindings(node.orelse, (*conditions, f"not ({ast.unparse(node.test)})"))
        elif isinstance(node, ast.Assign):
            for target in node.targets:
                name = ast.unparse(target)
                if name.startswith("query.") or name in {"contract_spec", "query", "query_info", "request"}:
                    result.append({"target": name, "expression": ast.unparse(node.value),
                                   "when": list(conditions), "line": node.lineno})
        elif isinstance(node, ast.Expr) and isinstance(node.value, ast.Call):
            name = ast.unparse(node.value.func)
            if name.startswith("query."):
                result.append({"target": name, "expression": ast.unparse(node.value),
                               "when": list(conditions), "line": node.lineno})
    return result


def parameters(node):
    args = node.args
    positional = [*args.posonlyargs, *args.args]
    defaults = [None] * (len(positional) - len(args.defaults)) + list(args.defaults)
    result = []
    for arg, default in zip(positional, defaults):
        result.append({"name": arg.arg, "annotation": ast.unparse(arg.annotation) if arg.annotation else None,
                       "default": ast.unparse(default) if default is not None else None,
                       "kind": "positional-only" if arg in args.posonlyargs else "positional-or-keyword"})
    for arg, default in zip(args.kwonlyargs, args.kw_defaults):
        result.append({"name": arg.arg, "annotation": ast.unparse(arg.annotation) if arg.annotation else None,
                       "default": ast.unparse(default) if default is not None else None, "kind": "keyword-only"})
    for arg, kind in [(args.vararg, "varargs"), (args.kwarg, "kwargs")]:
        if arg:
            result.append({"name": arg.arg, "annotation": ast.unparse(arg.annotation) if arg.annotation else None,
                           "default": None, "kind": kind})
    return result


def detail_counts(entries, modules):
    return {
        "modules": len(modules),
        "parameters": sum(len(e.get("parameters", [])) for e in entries),
        "fields": sum(len(e.get("fields", [])) for e in entries),
        "enum_values": sum(len(e.get("values", [])) for e in entries if e["kind"] == "enum"),
        "oneofs": sum(len(e.get("oneofs", [])) for e in entries),
        "imports": sum(len(m["imports"]) for m in modules),
        "environment_reads": sum(len(m["environment_reads"]) for m in modules),
    }


def wheel_files(raw):
    """Read wheel members and verify RECORD; never extracts or imports them."""
    with zipfile.ZipFile(io.BytesIO(raw)) as archive:
        names = [n for n in archive.namelist() if not n.endswith("/")]
        require(len(names) == len(set(names)), "Duplicate archive members")
        files = {n: archive.read(n) for n in names}
    records = list(csv.reader(io.StringIO(files[next(n for n in files if n.endswith("/RECORD"))].decode())))
    require({r[0] for r in records} == set(files) and len(records) == len(files), "RECORD does not cover every file exactly once")
    for name, checksum, size in records:
        if checksum:
            algorithm, digest = checksum.split("=", 1)
            require(base64.urlsafe_b64encode(hashlib.new(algorithm, files[name]).digest()).decode().rstrip("=") == digest
                    and len(files[name]) == int(size), f"Invalid RECORD entry {name}")
    return files, sum(bool(r[1]) for r in records)


def serialized_descriptor(source):
    tree = ast.parse(source)
    return next(ast.literal_eval(n.args[0]) for n in ast.walk(tree) if isinstance(n, ast.Call)
                and isinstance(n.func, ast.Attribute) and n.func.attr == "AddSerializedFile")


def descriptor_set(files):
    """The checked-in descriptor-set encoding for a wheel: Struct plus both vendor descriptors."""
    from google.protobuf import descriptor_pb2, struct_pb2

    descriptor_set = descriptor_pb2.FileDescriptorSet()
    descriptor_set.file.add().ParseFromString(struct_pb2.DESCRIPTOR.serialized_pb)
    hashes = {}
    for relative in DESCRIPTOR_SOURCES:
        data = files[f"thetadata/{relative}"]
        descriptor_set.file.add().ParseFromString(serialized_descriptor(data.decode("utf-8")))
        hashes[relative] = sha(data)
    methods = [{"rpc": m.name, "request": m.input_type, "response": m.output_type, "server_streaming": m.server_streaming}
               for f in descriptor_set.file for service in f.service for m in service.method]
    return descriptor_set.SerializeToString(deterministic=True), hashes, methods


def build(files):
    """Static inventory entries/modules for one wheel's members; version-independent."""
    entries, modules, wrapper_ids = [], [], []
    descriptors = []
    for name, data in sorted(files.items()):
        entries.append({"id": f"file:{name}", "kind": "file", "name": name,
                        "bytes": len(data), "sha256": sha(data)})
        if not name.endswith(".py"):
            continue
        source = data.decode("utf-8")
        tree = ast.parse(source)
        module_symbols = symbols(tree)
        # Generated wrapper descriptions are adjacent class string expressions,
        # not function docstrings. Preserve both forms without executing them.
        preceding = {}
        for parent in ast.walk(tree):
            body = getattr(parent, "body", None)
            if not isinstance(body, list):
                continue
            for left, right in zip(body, body[1:]):
                if isinstance(left, ast.Expr) and isinstance(left.value, ast.Constant) and isinstance(left.value.value, str):
                    preceding[id(right)] = left.value.value
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
                except (ValueError, TypeError):
                    continue
                if not isinstance(value, bytes):
                    module["constants"].append({"names": [ast.unparse(t) for t in node.targets], "value": value, "line": node.lineno})
        for qualified, node in module_symbols:
            identifier = f"symbol:{name}:{qualified}"
            module["symbol_ids"].append(identifier)
            entry = {"id": identifier, "kind": "class" if isinstance(node, ast.ClassDef) else "function",
                     "name": qualified, "source": name, "line": node.lineno, "end_line": node.end_lineno,
                     "source_sha256": sha(ast.get_source_segment(source, node).encode("utf-8")),
                     "description": ast.get_docstring(node) or preceding.get(id(node), ""),
                     "decorators": [ast.unparse(d) for d in node.decorator_list]}
            if not isinstance(node, ast.ClassDef):
                entry.update(signature=f"{qualified}({ast.unparse(node.args)})",
                             returns=ast.unparse(node.returns) if node.returns else None,
                             parameters=parameters(node))
            else:
                entry["bases"] = [ast.unparse(n) for n in node.bases]
            if name == "thetadata/client.py" and qualified.startswith("ThetaClient.") and not node.name.startswith("_"):
                entry["kind"] = "wrapper"
                wrapper_ids.append(identifier)
                calls = [n for n in ast.walk(node) if isinstance(n, ast.Call)]
                rpc_calls = [n for n in calls if isinstance(n.func, ast.Attribute)
                             and ast.unparse(n.func.value) == "self.stub"]
                require(len(rpc_calls) == 1, f"Ambiguous RPC for {qualified}")
                require(isinstance(node.body[0], ast.Try) and isinstance(node.body[-1], ast.Try),
                        f"Unrecognized wrapper shape: {qualified}")
                handlers = node.body[-1].handlers
                require(len(handlers) == 1 and ast.unparse(handlers[0].type) == "grpc.RpcError"
                        and isinstance(handlers[0].body[0], ast.If)
                        and ast.unparse(handlers[0].body[0].test) == "e.code() == grpc.StatusCode.NOT_FOUND",
                        f"Common error description no longer applies: {qualified}")
                entry["rpc"] = rpc_calls[0].func.attr
                entry["bindings"] = bindings(node.body)
                entry["truthiness_guards"] = [ast.unparse(n.test) for n in node.body if isinstance(n, ast.If)]
                entry["metadata_construction"] = [ast.unparse(n) for n in node.body[0].body]
                entry["exceptions"] = [ast.unparse(n.exc) for n in ast.walk(node) if isinstance(n, ast.Raise)]
            entries.append(entry)
        for node in ast.walk(tree):
            if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute) and node.func.attr == "AddSerializedFile":
                descriptors.append((name, ast.literal_eval(node.args[0])))
        module["definition_count"] = len(module_symbols)
        modules.append(module)
    protocol_files = []
    if descriptors:
        # Research-only dependency, needed just to parse serialized descriptors.
        from google.protobuf import descriptor_pb2
    for source, encoded in descriptors:
        descriptor = descriptor_pb2.FileDescriptorProto.FromString(encoded)
        protocol_files.append({"name": descriptor.name, "source": source, "sha256": sha(encoded),
                               "package": descriptor.package, "syntax": descriptor.syntax,
                               "dependencies": list(descriptor.dependency)})

        def add_message(message, prefix):
            full = f"{prefix}.{message.name}"
            fields = []
            for field in message.field:
                fields.append({"name": field.name, "number": field.number,
                               "type": field.type_name or descriptor_pb2.FieldDescriptorProto.Type.Name(field.type).removeprefix("TYPE_").lower(),
                               "label": descriptor_pb2.FieldDescriptorProto.Label.Name(field.label).removeprefix("LABEL_").lower(),
                               "proto3_optional": field.proto3_optional,
                               "oneof": message.oneof_decl[field.oneof_index].name if field.HasField("oneof_index") else None})
            entries.append({"id": f"message:{full}", "kind": "message", "name": full, "source": source,
                            "fields": fields, "oneofs": [v.name for v in message.oneof_decl],
                            "map_entry": message.options.map_entry,
                            "descriptor_sha256": sha(message.SerializeToString(deterministic=True))})
            for nested in message.nested_type:
                add_message(nested, full)
            for enum in message.enum_type:
                add_enum(enum, full)

        def add_enum(enum, prefix):
            full = f"{prefix}.{enum.name}"
            entries.append({"id": f"enum:{full}", "kind": "enum", "name": full, "source": source,
                            "values": [{"name": v.name, "number": v.number} for v in enum.value]})

        for message in descriptor.message_type:
            add_message(message, descriptor.package)
        for enum in descriptor.enum_type:
            add_enum(enum, descriptor.package)
        for service in descriptor.service:
            for method in service.method:
                entries.append({"id": f"rpc:{descriptor.package}.{service.name}/{method.name}", "kind": "rpc",
                                "name": method.name, "source": source, "request": method.input_type,
                                "response": method.output_type, "client_streaming": method.client_streaming,
                                "server_streaming": method.server_streaming,
                                "path": f"/{descriptor.package}.{service.name}/{method.name}"})
    metadata = Parser().parsestr(files[next(n for n in files if n.endswith("/METADATA"))].decode())
    return {"entries": entries, "modules": modules, "protocol_files": protocol_files,
            "wrapper_ids": wrapper_ids, "metadata": metadata}


def inventory(archive_path, previous=None):
    """Inventory one reviewed wheel against its committed vendor-<version>.json identity."""
    raw = archive_path.read_bytes()
    files, verified = wheel_files(raw)
    version = Parser().parsestr(files[next(n for n in files if n.endswith("/METADATA"))].decode())["Version"]
    legacy_path = evidence(version)["legacy"]
    require(legacy_path.exists(), f"No reviewed artifact identity for {version}: {legacy_path.name}")
    legacy = read_json(legacy_path)
    require(sha(raw) == legacy["artifact_sha256"], f"Archive differs from the reviewed {version} artifact")
    require(set(files) == set(legacy["files"]), "Archive file set differs from the original inventory")
    for name, data in files.items():
        require({"bytes": len(data), "sha256": sha(data)} == legacy["files"][name], f"Changed original file {name}")
    built = build(files)
    entries, modules = built["entries"], built["modules"]
    encoded, _, _ = descriptor_set(files)
    if version == BASELINE_VERSION:
        require(encoded == DESCRIPTOR.read_bytes(), "Recovered descriptor set differs from the checked-in descriptor")
    metadata = built["metadata"]
    # Git stores LF; older Windows checkouts may materialize this JSON as CRLF.
    legacy_bytes = legacy_path.read_bytes().replace(b"\r\n", b"\n")
    preserved = previous["preserved_research"] if previous else {
        "path": REPORT.relative_to(ROOT).as_posix(), "bytes": len(REPORT.read_bytes()), "sha256": sha(REPORT.read_bytes())}
    roadmap = (ROOT / "docs/ROADMAP.md").read_bytes()
    preserved_review = previous.get("preserved_review_sha256") if previous else None
    preserved_review = preserved_review or sha(roadmap[roadmap.index(REVIEW_MARKER):])
    return {"schema_version": 1, "source_version": metadata["Version"], "artifact_sha256": sha(raw),
            "record_entries_verified": verified,
            "legacy_inventory_sha256": sha(legacy_bytes), "preserved_research": preserved,
            "preserved_review_sha256": preserved_review,
            "descriptor_sha256": sha(encoded),
            "metadata": {key: metadata.get_all(key) for key in dict(metadata.items())},
            "metadata_python_examples": re.findall(r"```python\n(.*?)```", metadata.get_payload(), re.S),
            "modules": modules, "protocol_files": built["protocol_files"], "wrapper_ids": built["wrapper_ids"],
            "detail_counts": detail_counts(entries, modules),
            "counts": dict(sorted(Counter(e["kind"] for e in entries).items())), "entries": entries}


def decision_key(entry):
    if entry["id"] in {
        "symbol:thetadata/client.py:ThetaClient.stock_history_eod",
        "message:BetaEndpoints.StockHistoryEodRequestQuery",
        "message:BetaEndpoints.StockHistoryEodRequest",
        "rpc:BetaEndpoints.BetaThetaTerminal/GetStockHistoryEod",
    }:
        return "stock-eod"
    if entry["kind"] == "wrapper":
        return "flat-files" if "flat_file" in entry["name"] else "query-wrappers"
    if entry["kind"] == "rpc":
        if entry["name"].startswith("GetFuture"):
            return "futures"
        return "corporate-actions" if "CorporateAction" in entry["name"] else "wire-schema"
    if entry["kind"] == "message" and entry["name"].startswith("BetaEndpoints.Future"):
        return "futures"
    if entry["kind"] in {"message", "enum"}:
        return "wire-schema"
    if entry["kind"] == "file":
        return "artifact-packaging"
    if "_proto/" in entry["source"]:
        return "generated-python"
    if entry["name"] in {"AuthenticationError", "NoDataFoundError"}:
        return "errors"
    return {"ThetaClient": "construction", "ThetaClient.__init__": "construction",
            "ThetaClient._read_creds_from_file": "credential-file", "ThetaClient._convert_response_stream": "conversion",
            "_fmt_time": "date-time", "_fmt_if_date": "date-time"}[entry["name"]]


def anchor(identifier):
    # Hash the stable semantic ID, not its row position.
    return "item-" + sha(identifier.encode())[:16]


def cell(value):
    return str(value).replace("|", "&#124;").replace("\n", " ").replace("<", "&lt;")


def section_link(section):
    """A disposition section is an anchor in the 1.0.12 report, or FILE.md#anchor in research/."""
    return section if ".md#" in section else f"thetadata-1.0.12.md#{section}"


def render(data, decisions):
    version = data["source_version"]
    by_key = {d["id"]: d for d in decisions}
    lines = [f"# Python {version} exhaustive artifact catalogue", "",
             "Generated by `python tools/research_python.py --render`; do not edit by hand.", "",
             "Read [the research analysis](thetadata-1.0.12.md#deep-analysis-2026-10-04) first. "
             "This catalogue records source observations, not service guarantees. All dispositions "
             f"are scoped by [the decision ledger](python-{version}-dispositions.json). "
             "`planned` means retained for future scope selection, not approved implementation.", "",
             "## Coverage closure", "", "| Inventory kind | Count |", "| --- | ---: |"]
    lines += [f"| {kind} | {count} |" for kind, count in data["counts"].items()]
    lines += ["", "Every file, lexical class/function (including generated helpers), descriptor message "
              "and enum, and RPC has one stable semantic ID and one coverage row below. Message rows "
              "own their complete field/oneof inventories; function rows own parameters/defaults. "
              "Imports, environment reads, constants, and metadata are inventoried per module/artifact. "
              "Shared behavior is traced separately in the decision ledger. A covered row is not a "
              "claim that Rust implements or tests that feature.", "", "## Feature decisions and traceability", "",
              "ADR/requirement/code/test cells are explicit links or `unassigned`. Existing references "
              "cover the stated scope only. [Stock EOD requirements](../requirements/L1-stock-eod.md) "
              "are defined; their runtime implementation and acceptance verification remain planned.", "",
              "| Feature ID | Disposition | Research | ADR / requirements | Implementation / verification | Reason and limits |",
              "| --- | --- | --- | --- | --- | --- |"]
    for d in decisions:
        def links(key):
            return ", ".join(f"[{cell(p)}](../../{p})" for p in d[key]) or "unassigned"
        lines.append(f"| {d['id']} | {d['status']} | [analysis]({section_link(d['section'])}) | "
                     f"{links('adrs')}; {', '.join(d['requirements']) or 'unassigned'} | "
                     f"{links('implementation')}; {links('verification')} | {cell(d['rationale'])} |")
    lines += ["", "## Embedded Python examples", "",
              "Verbatim code blocks from wheel METADATA, retained as unexecuted artifact evidence. "
              "The INFO logging example can expose auth tokens; see the research's logging analysis.", ""]
    for number, example in enumerate(data["metadata_python_examples"], 1):
        lines += [f"### Metadata example {number}", "", "```python", example.rstrip(), "```", ""]
    lines += ["", "## Artifact metadata", "", "| Header | Value |", "| --- | --- |"]
    lines += [f"| {cell(k)} | {cell('; '.join(v))} |" for k, v in data["metadata"].items()]
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
    lines += ["", "## Every inventory item", "",
              "The feature key joins each row to all six traceability columns above. "
              "Items retain their disposition individually; descriptions below provide their source detail.", "",
              "| Item ID | Disposition | Feature key | Detail |", "| --- | --- | --- | --- |"]
    for e in data["entries"]:
        key = decision_key(e)
        lines.append(f"| `{cell(e['id'])}` | {by_key[key]['status']} | {key} | [detail](#{anchor(e['id'])}) |")
    lines += ["", "## Source and protocol details", ""]
    for e in data["entries"]:
        lines += [f'<a id="{anchor(e["id"])}"></a>', "", f"### `{e['name']}`", ""]
        if e["kind"] == "file":
            lines += [f"Artifact member; {e['bytes']} bytes; SHA-256 `{e['sha256']}`.", ""]
            continue
        lines += [f"Source: `{e['source']}`" + (f", lines {e['line']}-{e['end_line']}." if "line" in e else "; serialized descriptor."), ""]
        if e["kind"] in {"function", "wrapper"}:
            lines += ["```text", e["signature"], "```", ""]
            if e.get("returns"):
                lines += [f"Return annotation: `{cell(e['returns'])}` (annotation only; inspect actual behavior).", ""]
        if e["kind"] == "wrapper":
            lines += [f"RPC: `{e['rpc']}`. Common stream conversion/error handling is described in "
                      "[wrapper semantics](thetadata-1.0.12.md#wrapper-semantics).", "",
                      "| Request target | Expression | Assignment condition |", "| --- | --- | --- |"]
            for b in e["bindings"]:
                lines.append(f"| `{cell(b['target'])}` | `{cell(b['expression'])}` | {cell(' and '.join(b['when']) or 'unconditional')} |")
            lines += ["", "Metadata construction (failure falls back to `client=python`):", "", "```text",
                      *e["metadata_construction"], "```", "", "Raised/reraised by the gRPC handler:", "", "```text",
                      *e["exceptions"], "```"]
            lines += ["", "Artifact description (unverified vendor usage/entitlement claims):", ""]
            description = re.sub(r"\[([^\]]+)\]\(([^)]+)\)", r"\1 (\2)", e["description"])
            lines += [("> " + line).rstrip() for line in description.strip().splitlines()] + [""]
        elif e["kind"] == "message":
            lines += [f"Map entry: `{e['map_entry']}`; oneofs: `{', '.join(e['oneofs']) or 'none'}`.", "",
                      "| Field | Number | Type | Label | Explicit proto3 optional | Oneof |", "| --- | ---: | --- | --- | --- | --- |"]
            for f in e["fields"]:
                lines.append(f"| {f['name']} | {f['number']} | `{f['type']}` | {f['label']} | {f['proto3_optional']} | {f['oneof'] or 'none'} |")
            if not e["fields"]:
                lines += ["", "Empty query/message; it still participates in its request envelope."]
            lines.append("")
        elif e["kind"] == "enum":
            lines += ["| Name | Value |", "| --- | ---: |"]
            lines += [f"| {v['name']} | {v['number']} |" for v in e["values"]] + [""]
        elif e["kind"] == "rpc":
            wrappers = [w['name'] for w in data['entries'] if w['kind'] == 'wrapper' and w['rpc'] == e['name']]
            lines += [f"Path: `{e['path']}`. Request: `{e['request']}`; response: `{e['response']}`. "
                      f"Client streaming: `{e['client_streaming']}`; server streaming: `{e['server_streaming']}`. "
                      f"Python wrapper: `{', '.join(wrappers) or 'NONE'}`.", ""]
        elif e["kind"] == "class":
            lines += [f"Bases: `{', '.join(e['bases']) or 'none'}`. "
                      "See its feature decision and separately inventoried members.", ""]
    return ("\n".join(lines).rstrip() + "\n").encode("utf-8")


def validate(data, decisions):
    version = data["source_version"]
    legacy_path = evidence(version)["legacy"]
    entries = data["entries"]
    ids = [e["id"] for e in entries]
    require(len(ids) == len(set(ids)), "Duplicate inventory ID")
    require(data["counts"] == dict(Counter(e["kind"] for e in entries)), "Inventory counts do not reconcile")
    require(data["detail_counts"] == detail_counts(entries, data["modules"]), "Detail counts do not reconcile")
    current = version == read_json(MANIFEST)["source_version"]
    require(current or version == FROZEN_VERSION, "Wrong compatibility baseline")
    if current:
        require(data["descriptor_sha256"] == sha(DESCRIPTOR.read_bytes()), "Descriptor hash changed")
    require(data["legacy_inventory_sha256"] == sha(legacy_path.read_bytes().replace(b"\r\n", b"\n")), "Original inventory was changed")
    preserved = data["preserved_research"]
    require(sha(REPORT.read_bytes()[:preserved["bytes"]]) == preserved["sha256"], "Original research was lost or rewritten")
    roadmap = (ROOT / "docs/ROADMAP.md").read_bytes()
    require(sha(roadmap[roadmap.index(REVIEW_MARKER):]) == data["preserved_review_sha256"], "Original roadmap review was rewritten")
    legacy = read_json(legacy_path)
    require(data["artifact_sha256"] == legacy["artifact_sha256"], "Wrong artifact hash")
    file_entries = {e["name"]: {"bytes": e["bytes"], "sha256": e["sha256"]} for e in entries if e["kind"] == "file"}
    require(file_entries == legacy["files"], "File coverage is incomplete")
    modules = data["modules"]
    require({m["path"] for m in modules} == {p for p in file_entries if p.endswith('.py')}, "Module coverage is incomplete")
    symbol_ids = [s for m in modules for s in m["symbol_ids"]]
    require(len(symbol_ids) == len(set(symbol_ids)), "Duplicate module symbol reference")
    require(set(symbol_ids) == {e["id"] for e in entries if e["kind"] in {"class", "function", "wrapper"}}, "Symbol coverage is incomplete")
    for module in modules:
        require(module["definition_count"] == len(module["symbol_ids"]), "Module definition count mismatch")
    for entry in entries:
        if "signature" in entry:
            # Parse signatures as syntax only; never evaluate default expressions.
            signature = entry["signature"].replace(entry["name"], entry["name"].split('.')[-1], 1)
            node = ast.parse(f"def {signature}: pass").body[0]
            require(parameters(node) == entry["parameters"], "Signature/parameter evidence differs")
    wrappers = [e for e in entries if e["kind"] == "wrapper"]
    require(set(data["wrapper_ids"]) == {w["id"] for w in wrappers}, "Wrapper index mismatch")
    require({w["name"].split('.')[-1]: w["rpc"] for w in wrappers} == {w["name"]: w["rpc"] for w in legacy["methods"]}, "Original wrapper mapping was lost")
    rpcs = [e for e in entries if e["kind"] == "rpc"]
    require(len(rpcs) == legacy["descriptor_rpc_count"], "RPC count mismatch")
    if current:
        require({r["name"] for r in rpcs} == {r["rpc"] for r in read_json(MANIFEST)["methods"]}, "RPC coverage mismatch")
    require(sorted({r["name"] for r in rpcs} - {w["rpc"] for w in wrappers}) == legacy["rpc_without_python_wrapper"], "Unmatched RPCs changed")
    types = {e["name"] for e in entries if e["kind"] == "message"}
    enums = {e["name"] for e in entries if e["kind"] == "enum"}
    for message in [e for e in entries if e["kind"] == "message"]:
        numbers = [f["number"] for f in message["fields"]]
        names = [f["name"] for f in message["fields"]]
        require(len(numbers) == len(set(numbers)) and len(names) == len(set(names)), "Duplicate protobuf field")
        for field in message["fields"]:
            if field["type"].startswith('.'):
                require(field["type"].lstrip('.') in types | enums | {"google.protobuf.NullValue"}, "Referenced protobuf type missing")
    for wrapper in wrappers:
        query = next(b['expression'].split('(')[0].split('.')[-1] for b in wrapper['bindings'] if b['target'] == 'query')
        require('BetaEndpoints.' + query in types, "Wrapper query message missing")
    for rpc in rpcs:
        require(rpc["request"].lstrip('.') in types and rpc["response"].lstrip('.') in types, "RPC message missing")
        require(rpc["server_streaming"] and not rpc["client_streaming"], "Unexpected stream shape")
    ledger = {d["id"]: d for d in decisions}
    require(len(ledger) == len(decisions), "Duplicate feature ID")
    require({decision_key(e) for e in entries} <= set(ledger), "Unclassified inventory item")
    report = REPORT.read_text(encoding="utf-8")
    requirements = "\n".join(p.read_text(encoding="utf-8") for p in (ROOT / 'docs/requirements').glob('*.md'))
    for decision in decisions:
        require(decision["status"] in STATUSES and decision["rationale"].strip(), "Missing disposition/rationale")
        target, anchor = section_link(decision["section"]).split("#", 1)
        text = report if target == REPORT.name else (RESEARCH / target).read_text(encoding="utf-8")
        require(f'<a id="{anchor}"></a>' in text, f"Missing research section: {decision['id']}")
        for key in ("adrs", "implementation", "verification"):
            for path in decision[key]:
                require((ROOT / path).is_file(), f"Missing {key} evidence: {path}")
        for identifier in decision["requirements"]:
            require(re.search(r"^\|\s*" + re.escape(identifier) + r"\s*\|", requirements, re.M), f"Undefined requirement {identifier}")
        if decision["status"] == "supported":
            require(decision["implementation"] and decision["verification"], "Supported feature lacks evidence")
    return len(entries)


def compatible(frozen, current):
    """Upstream additions are allowed; removed, renumbered or retyped items are not."""
    old = {e["id"]: e for e in frozen["entries"] if e["kind"] in {"wrapper", "rpc", "message", "enum"}}
    new = {e["id"]: e for e in current["entries"]}
    for identifier, entry in old.items():
        require(identifier in new, f"Upstream removed {identifier}")
        later = new[identifier]
        if entry["kind"] == "wrapper":
            require(later["rpc"] == entry["rpc"], f"Wrapper RPC changed: {identifier}")
            names = [p["name"] for p in later["parameters"]]
            require(names[:len(entry["parameters"])] == [p["name"] for p in entry["parameters"]],
                    f"Wrapper parameters removed or reordered: {identifier}")
        elif entry["kind"] == "rpc":
            keys = ("request", "response", "client_streaming", "server_streaming", "path")
            require({k: later[k] for k in keys} == {k: entry[k] for k in keys}, f"RPC shape changed: {identifier}")
        elif entry["kind"] == "message":
            fields = {f["name"]: f for f in later["fields"]}
            for field in entry["fields"]:
                require(fields.get(field["name"]) == field, f"Field removed or changed: {identifier}.{field['name']}")
        else:
            values = {v["name"]: v["number"] for v in later["values"]}
            require(all(values.get(v["name"]) == v["number"] for v in entry["values"]), f"Enum value changed: {identifier}")
    return len(old)


def check_version(version, data=None):
    paths = evidence(version)
    data = data or read_json(paths["inventory"])
    decisions = read_json(paths["decisions"])
    count = validate(data, decisions)
    require(paths["catalog"].read_bytes() == render(data, decisions), "Catalogue/coverage rows are stale; run --render")
    return data, decisions, count


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, help="Regenerate from the exact original wheel-compatible ZIP")
    parser.add_argument("--check", action="store_true", help="Read-only closure and generated Markdown check")
    parser.add_argument("--render", action="store_true", help="Render committed inventory and decision ledger")
    args = parser.parse_args()
    require(args.archive or args.check or args.render, "Choose --archive, --check, or --render")
    if args.archive:
        files, _ = wheel_files(args.archive.read_bytes())
        version = Parser().parsestr(files[next(n for n in files if n.endswith("/METADATA"))].decode())["Version"]
        target = evidence(version)["inventory"]
        previous = read_json(target) if target.exists() else None
        data = inventory(args.archive, previous)
        if args.check:
            require(data == previous, "Source regeneration differs from the committed inventory")
        else:
            target.write_bytes(encode(data))
        if version != BASELINE_VERSION:
            check_version(version, data)
            print(f"Regenerated and checked {version} evidence.")
            return
    else:
        data = read_json(INVENTORY) if INVENTORY.exists() else None
        require(data is not None, "Generate the inventory from --archive first")
    decisions = read_json(DECISIONS)
    count = validate(data, decisions)
    rendered = render(data, decisions)
    if args.check:
        require(CATALOG.read_bytes() == rendered, "Catalogue/coverage rows are stale; run --render")
    else:
        CATALOG.write_bytes(rendered)
    frozen_note = ""
    if BASELINE_VERSION != FROZEN_VERSION:
        frozen, _, frozen_count = check_version(FROZEN_VERSION)
        compared = compatible(frozen, data)
        frozen_note = f" Frozen {FROZEN_VERSION}: {frozen_count} items checked; {compared} carried forward without removal."
    print(f"Checked {count} inventory items, {len(decisions)} feature dispositions, "
          f"{len(data['modules'])} modules, {data['counts']['wrapper']} wrappers / {data['counts']['rpc']} RPCs, "
          "preserved research, and generated coverage." + frozen_note)


if __name__ == "__main__":
    main()
