"""Generate typed endpoint requests from the pinned vendor 1.0.17 inventory.

P1 design section 3. Reads docs/research/python-1.0.17-inventory.json (the
committed research record; no vendor code is imported or run) and writes:

* crates/praevis-thetadata-client/src/endpoints/generated.rs: one typed request
  per RPC, reproducing the vendor wrapper's defaults, formatting and truthiness
  guards; RPCs without a wrapper are generated from their request message and
  marked unstable;
* docs/endpoints/coverage.md: the generated coverage table (the lighter
  process's replacement for per-clause matrices), merged with the manual
  statuses in docs/endpoints/status.json.

    python tools/generate_endpoints.py          # write
    python tools/generate_endpoints.py --check  # CI: fail if outputs are stale

Any binding expression or default the translator does not recognize is an
error, never a silent guess.
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
INVENTORY = ROOT / "docs/research/python-1.0.17-inventory.json"
STATUS = ROOT / "docs/endpoints/status.json"
RUST_OUT = ROOT / "crates/praevis-thetadata-client/src/endpoints/generated.rs"
DOC_OUT = ROOT / "docs/endpoints/coverage.md"
RUST_KEYWORDS = {"type", "match", "ref", "self", "fn", "move", "use", "loop", "where", "async", "await"}


class GenerationError(Exception):
    pass


def camel(name: str) -> str:
    return "".join(part[:1].upper() + part[1:] for part in name.split("_") if part)


def snake(name: str) -> str:
    """CamelCase to snake_case, matching tonic's method names (heck)."""
    out = re.sub(r"(?<=[a-z0-9])([A-Z])", r"_\1", name)
    out = re.sub(r"(?<=[A-Z])([A-Z][a-z])", r"_\1", out)
    return out.lower()


def ident(name: str) -> str:
    return f"r#{name}" if name in RUST_KEYWORDS else name


def rust_string(text: str) -> str:
    escaped = (text.replace("\\", "\\\\").replace('"', '\\"')
               .replace("\r", "").replace("\n", "\\n").replace("\t", "\\t"))
    return f'"{escaped}"'


# -- protocol --------------------------------------------------------------

@dataclass
class ProtoField:
    name: str
    type: str
    label: str
    optional: bool

    @property
    def scalar(self) -> str:
        return {"string": "String", "double": "f64", "int32": "i32", "bool": "bool",
                ".Endpoints.ContractSpec": "wire::ContractSpec"}[self.type]

    @property
    def rust_type(self) -> str:
        if self.label == "repeated":
            return f"Vec<{self.scalar}>"
        if self.optional or self.type == ".Endpoints.ContractSpec":
            return f"Option<{self.scalar}>"
        return self.scalar


def load(inventory: Path) -> tuple[list[dict], dict[str, dict], dict[str, dict]]:
    data = json.loads(inventory.read_text(encoding="utf-8"))
    entries = data["entries"]
    rpcs = [e for e in entries if e["kind"] == "rpc"]
    messages = {e["name"]: e for e in entries if e["kind"] == "message"}
    wrappers = {e["rpc"]: e for e in entries if e["kind"] == "wrapper"}
    return rpcs, messages, wrappers


def query_fields(rpc: dict, messages: dict[str, dict]) -> tuple[str, str, list[ProtoField]]:
    request = rpc["request"].lstrip(".")
    envelope = messages[request]
    params = [f for f in envelope["fields"] if f["name"] == "params"]
    if len(params) != 1:
        raise GenerationError(f"{rpc['name']}: request envelope without one params field")
    query = params[0]["type"].lstrip(".")
    fields = [ProtoField(f["name"], f["type"], f["label"], bool(f["proto3_optional"]))
              for f in messages[query]["fields"]]
    return request.split(".")[-1], query.split(".")[-1], fields


# -- wrapper translation ---------------------------------------------------

@dataclass
class Param:
    name: str
    kind: str                 # String, f64, i32, bool, NaiveDate, Expiration, TimeOfDay, Symbols
    required: bool
    default: str | None       # Rust expression for Some(...) content, or None
    annotation: str
    vendor_default: str | None


@dataclass
class Assignment:
    field: ProtoField
    param: str
    transform: str            # direct, date, time, expiration, extend
    when: list[str]


@dataclass
class Spec:
    params: dict[str, str] = field(default_factory=dict)       # spec field -> param
    transforms: dict[str, str] = field(default_factory=dict)   # spec field -> transform


QUERY_TARGET = re.compile(r"^query\.(\w+)(\.extend)?$")


def classify_expression(expression: str, params: set[str]) -> tuple[str, str]:
    """(param, transform) for one binding value expression."""
    if expression in params:
        return expression, "direct"
    match = re.fullmatch(r"(\w+)\.strftime\('%Y-%m-%d'\)", expression)
    if match and match.group(1) in params:
        return match.group(1), "date"
    match = re.fullmatch(r"_fmt_time\((\w+)\)", expression)
    if match and match.group(1) in params:
        return match.group(1), "time"
    match = re.fullmatch(r"_fmt_if_date\((\w+)\)", expression)
    if match and match.group(1) in params:
        return match.group(1), "expiration"
    match = re.fullmatch(r"query\.(\w+)\.extend\(\[(\w+)\] if isinstance\(\2, str\) else \2\)", expression)
    if match and match.group(2) in params:
        return match.group(2), "extend"
    raise GenerationError(f"unrecognized binding expression: {expression!r}")


def parse_spec(expression: str, params: set[str]) -> Spec:
    match = re.fullmatch(r"ContractSpec\((.*)\)", expression)
    if not match:
        raise GenerationError(f"unrecognized contract spec: {expression!r}")
    spec = Spec()
    for part in [p.strip() for p in match.group(1).split(", ")]:
        name, _, value = part.partition("=")
        param, transform = classify_expression(value, params)
        if name not in {"symbol", "expiration", "strike", "right"}:
            raise GenerationError(f"unknown contract spec field {name!r}")
        spec.params[name] = param
        spec.transforms[name] = transform
    return spec


KIND_BY_TRANSFORM = {"date": "NaiveDate", "time": "TimeOfDay", "expiration": "Expiration", "extend": "Symbols"}


def default_expression(kind: str, literal: str) -> str:
    """Rust expression for a vendor default literal, wrapped later in Some(...)."""
    if literal == "None":
        raise AssertionError
    string = re.fullmatch(r"'([^']*)'", literal)
    if string:
        value = string.group(1)
        if kind == "String":
            return f"{rust_string(value)}.to_owned()"
        if kind in ("Expiration", "TimeOfDay"):
            return f"{kind}::Text({rust_string(value)}.to_owned())"
    time = re.fullmatch(r"_fmt_time\('([^']*)'\)", literal)
    if time and kind == "TimeOfDay":
        # _fmt_time(str) returns the string unchanged.
        return f"TimeOfDay::Text({rust_string(time.group(1))}.to_owned())"
    if literal in ("True", "False") and kind == "bool":
        return literal.lower()
    if re.fullmatch(r"-?\d+", literal):
        if kind == "i32":
            return literal
        if kind == "f64":
            return f"{literal}.0"
    if re.fullmatch(r"-?\d+\.\d+", literal) and kind == "f64":
        return literal
    raise GenerationError(f"unrecognized default {literal!r} for a {kind} argument")


@dataclass
class Wrapper:
    rpc: str
    python: str
    struct: str
    params: list[Param]
    assignments: list[Assignment]
    spec: Spec | None
    description: str


def translate_wrapper(rpc: dict, wrapper: dict, fields: list[ProtoField]) -> Wrapper:
    by_name = {f.name: f for f in fields}
    names = [p["name"] for p in wrapper["parameters"] if p["name"] != "self"]
    param_set = set(names)
    usage: dict[str, str] = {}
    assignments: list[Assignment] = []
    spec = None
    for binding in wrapper["bindings"]:
        target, expression = binding["target"], binding["expression"]
        if target in ("query_info", "request", "query"):
            continue  # constructed generically
        if target == "contract_spec":
            spec = parse_spec(expression, param_set)
            for name, param in spec.params.items():
                transform = spec.transforms[name]
                usage.setdefault(param, KIND_BY_TRANSFORM.get(transform, "String"))
            continue
        match = QUERY_TARGET.fullmatch(target)
        if not match:
            raise GenerationError(f"{rpc['name']}: unrecognized binding target {target!r}")
        proto = by_name.get(match.group(1))
        if proto is None:
            raise GenerationError(f"{rpc['name']}: no query field {match.group(1)!r}")
        param, transform = classify_expression(expression, param_set)
        if transform == "extend" and proto.label != "repeated":
            raise GenerationError(f"{rpc['name']}: extend on a non-repeated field")
        when = binding["when"]
        if when and when != [param]:
            raise GenerationError(f"{rpc['name']}: guard {when!r} on another argument than {param!r}")
        kind = KIND_BY_TRANSFORM.get(transform) or proto.scalar
        if usage.get(param, kind) != kind:
            raise GenerationError(f"{rpc['name']}: {param!r} used with two types")
        usage[param] = kind
        assignments.append(Assignment(proto, param, transform, when))
    if spec is None and any(f.type == ".Endpoints.ContractSpec" for f in fields):
        raise GenerationError(f"{rpc['name']}: contract spec field but no ContractSpec binding")
    params = []
    for raw in wrapper["parameters"]:
        if raw["name"] == "self":
            continue
        if raw["name"] not in usage:
            raise GenerationError(f"{rpc['name']}: argument {raw['name']!r} never reaches the wire")
        kind = usage[raw["name"]]
        literal = raw["default"]
        required = literal is None
        default = None
        if literal is not None and literal != "None":
            default = default_expression(kind, literal)
        params.append(Param(raw["name"], kind, required, default, raw["annotation"] or "", literal))
    python = wrapper["name"].split(".")[-1]
    return Wrapper(rpc["name"], python, camel(python), params, assignments, spec,
                   wrapper.get("description") or "")


# -- Rust emission ---------------------------------------------------------

ARG_TYPES = {"String": "impl Into<String>", "Symbols": "impl Into<Symbols>",
             "Expiration": "impl Into<Expiration>", "TimeOfDay": "impl Into<TimeOfDay>"}
INTO = {"String", "Symbols", "Expiration", "TimeOfDay"}
COPY = {"f64", "i32", "bool", "NaiveDate"}


def field_type(param: Param) -> str:
    kind = param.kind
    return kind if param.required else f"Option<{kind}>"


def value_expression(param: Param, transform: str, required: bool) -> str:
    """Wire value of an argument: `self.<name>` when required (owned type),
    otherwise `value` bound by `if let Some(value) = &self.<name>` (a reference)."""
    source = f"self.{ident(param.name)}" if required else "value"
    if transform == "date":
        return f"date_wire({source})" if required else "date_wire(*value)"
    if transform in ("time", "expiration"):
        return f"{source}.wire()"
    if param.kind in COPY:
        return source if required else "*value"
    return f"{source}.clone()"


def emit_assignment(a: Assignment, param: Param) -> list[str]:
    target = f"query.{ident(a.field.name)}"
    name = ident(param.name)
    if a.transform == "extend":
        source = f"self.{name}" if param.required else "value"
        statement = f"{target}.extend({source}.0.iter().cloned());"
    else:
        value = value_expression(param, a.transform, param.required)
        statement = f"{target} = {'Some(' + value + ')' if a.field.optional else value};"
    guard = bool(a.when)
    if param.required:
        if guard:
            return [f"if self.{name}.truthy() {{", f"    {statement}", "}"]
        return [statement]
    head = f"if let Some(value) = &self.{name} {{"
    if guard:
        return [head, "    if value.truthy() {", f"        {statement}", "    }", "}"]
    return [head, f"    {statement}", "}"]


def emit_spec(spec: Spec, params: dict[str, Param]) -> list[str]:
    lines = ["query.contract_spec = Some(wire::ContractSpec {"]
    for name in ("symbol", "expiration", "strike", "right"):
        param = params[spec.params[name]]
        transform = spec.transforms[name]
        optional_field = name in ("strike", "right")
        if param.required:
            value = value_expression(param, transform, True)
            lines.append(f"    {name}: {'Some(' + value + ')' if optional_field else value},")
        else:
            mapped = f"self.{ident(param.name)}.as_ref().map(|value| {value_expression(param, transform, False)})"
            lines.append(f"    {name}: {mapped if optional_field else mapped + '.unwrap_or_default()'},")
    lines.append("});")
    return lines


def emit_wrapper(w: Wrapper, request: str, query: str) -> str:
    params = {p.name: p for p in w.params}
    required = [p for p in w.params if p.required]
    optional = [p for p in w.params if not p.required]
    out = [f"/// Vendor wrapper `{w.python}`, RPC `{w.rpc}`.",
           "///",
           "/// Required arguments are constructor parameters, in vendor order; optional",
           "/// ones start at the vendor defaults and are set with the builder methods.",
           "#[derive(Clone, Debug, PartialEq)]",
           f"pub struct {w.struct} {{"]
    for p in w.params:
        note = f"vendor default `{p.vendor_default}`" if p.vendor_default is not None else "required"
        out.append(f"    /// `{p.name}: {p.annotation}` ({note}).")
        out.append(f"    pub {ident(p.name)}: {field_type(p)},")
    out.append("}")
    args = ", ".join(f"{ident(p.name)}: {ARG_TYPES.get(p.kind, p.kind)}" for p in required)
    out.append(f"impl {w.struct} {{")
    out.append(f"    pub fn new({args}) -> Self {{")
    out.append("        Self {")
    for p in w.params:
        if p.required:
            value = f"{ident(p.name)}.into()" if p.kind in INTO else ident(p.name)
            out.append(f"            {ident(p.name)}: {value},")
        elif p.default is None:
            out.append(f"            {ident(p.name)}: None,")
        else:
            out.append(f"            {ident(p.name)}: Some({p.default}),")
    out.append("        }")
    out.append("    }")
    for p in optional:
        arg = ARG_TYPES.get(p.kind, p.kind)
        value = "value.into()" if p.kind in INTO else "value"
        out.append(f"    pub fn {ident(p.name)}(mut self, value: {arg}) -> Self {{")
        out.append(f"        self.{ident(p.name)} = Some({value});")
        out.append("        self")
        out.append("    }")
    out.append("}")
    out += emit_endpoint_impl(w.struct, w.rpc, f'Some("{w.python}")', "Stability::VendorWrapper",
                              w.description, request, query, emit_body(w, params))
    return "\n".join(out)


def emit_body(w: Wrapper, params: dict[str, Param]) -> list[str]:
    body = []
    if w.spec is not None:
        body += emit_spec(w.spec, params)
    for a in w.assignments:
        body += emit_assignment(a, params[a.param])
    return body


def emit_endpoint_impl(struct, rpc, python, stability, description, request, query, body) -> list[str]:
    method = snake(rpc)
    out = [f"impl Endpoint for {struct} {{",
           f'    const RPC: &\'static str = "{rpc}";',
           f"    const PYTHON_NAME: Option<&'static str> = {python};",
           f"    const STABILITY: Stability = {stability};",
           f"    const VENDOR_DESCRIPTION: &'static str = {rust_string(description.strip())};",
           f"    type Query = api::{query};",
           "    fn to_query(&self) -> Self::Query {",
           f"        let {'mut ' if body else ''}query = api::{query}::default();"]
    out += [f"        {line}" for line in body]
    out += ["        query",
            "    }",
            "    fn send(client: &ThetaClient, query: Self::Query) -> BoxFuture<'_, Result<RawStream, tonic::Status>> {",
            "        Box::pin(async move {",
            f"            let request = client.raw_request(api::{request} {{",
            "                query_info: Some(query_info(client)),",
            "                params: Some(query),",
            "            });",
            f"            Ok(client.raw_client().{method}(request).await?.into_inner())",
            "        })",
            "    }",
            "}"]
    return out


def emit_unwrapped(rpc: dict, request: str, query: str, fields: list[ProtoField]) -> str:
    struct = rpc["name"][3:] if rpc["name"].startswith("Get") else rpc["name"]
    out = [f"/// RPC `{rpc['name']}`: in the descriptor only, with no vendor wrapper or",
           "/// documentation. Fields mirror the request message one to one. **Unstable.**",
           "#[derive(Clone, Debug, Default, PartialEq)]",
           f"pub struct {struct} {{"]
    for f in fields:
        out.append(f"    pub {ident(f.name)}: {f.rust_type},")
    out.append("}")
    body = [f"query.{ident(f.name)} = self.{ident(f.name)}.clone();" for f in fields]
    out += emit_endpoint_impl(struct, rpc["name"], "None", "Stability::Unstable", "", request, query, body)
    return "\n".join(out)


def generate(inventory: Path = INVENTORY, status_path: Path = STATUS) -> tuple[str, str]:
    rpcs, messages, wrappers = load(inventory)
    items, rows = [], []
    for rpc in rpcs:
        request, query, fields = query_fields(rpc, messages)
        if rpc["name"] in wrappers:
            w = translate_wrapper(rpc, wrappers[rpc["name"]], fields)
            items.append(emit_wrapper(w, request, query))
            rows.append((rpc["name"], w.python, w.struct, "vendor wrapper"))
        else:
            struct = rpc["name"][3:] if rpc["name"].startswith("Get") else rpc["name"]
            items.append(emit_unwrapped(rpc, request, query, fields))
            rows.append((rpc["name"], None, struct, "unstable"))
    registry = ["/// Every generated endpoint, in descriptor order.",
                "pub const ALL: &[EndpointInfo] = &["]
    for rpc, python, struct, _ in rows:
        registry.append(f"    EndpointInfo {{ rpc: <{struct} as Endpoint>::RPC, python_name: <{struct} as Endpoint>::PYTHON_NAME, stability: <{struct} as Endpoint>::STABILITY }},")
    registry.append("];")
    header = [
        "// @generated by tools/generate_endpoints.py from docs/research/python-1.0.17-inventory.json.",
        "// Do not edit by hand; run `python tools/generate_endpoints.py`.",
        "use super::*;",
        "use chrono::NaiveDate;",
        "",
    ]
    rust = "\n".join(header + ["\n\n".join(items), "", *registry]) + "\n"
    statuses = json.loads(status_path.read_text(encoding="utf-8")) if status_path.exists() else {}
    unknown = set(statuses) - {row[0] for row in rows}
    if unknown:
        raise GenerationError(f"status.json names unknown RPCs: {sorted(unknown)}")
    doc = ["# Endpoint coverage",
           "",
           "Generated by `tools/generate_endpoints.py` from the pinned 1.0.17 inventory and",
           "[`status.json`](status.json); do not edit by hand. This table replaces",
           "per-clause requirement matrices for generated endpoints",
           "([lighter process](../architecture/performance-plan.md#f-process-simplification)).",
           "",
           f"{len(rows)} RPCs: {sum(1 for r in rows if r[1])} with a vendor wrapper, "
           f"{sum(1 for r in rows if not r[1])} descriptor-only (unstable).",
           "",
           "| RPC | Vendor wrapper | Rust request | Stability | Decoder test | Live check (FREE) |",
           "| --- | --- | --- | --- | --- | --- |"]
    for rpc, python, struct, stability in rows:
        status = statuses.get(rpc, {})
        doc.append(f"| `{rpc}` | {'`' + python + '`' if python else '—'} | `endpoints::{struct}` | {stability} "
                   f"| {status.get('decoder', 'pending (P1 PR 3)')} | {status.get('live', 'pending (entitlement map)')} |")
    return rust, "\n".join(doc) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true", help="fail if generated files are stale")
    args = parser.parse_args()
    try:
        rust, doc = generate()
    except GenerationError as error:
        print(f"generation failed: {error}", file=sys.stderr)
        return 2
    outputs = {RUST_OUT: rust, DOC_OUT: doc}
    if args.check:
        stale = [str(p.relative_to(ROOT)) for p, text in outputs.items()
                 if not p.exists() or p.read_text(encoding="utf-8") != text]
        if stale:
            print("stale generated files (run python tools/generate_endpoints.py): " + ", ".join(stale),
                  file=sys.stderr)
            return 1
        print(f"Generated endpoint files are up to date ({rust.count('impl Endpoint for')} endpoints).")
        return 0
    for path, text in outputs.items():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
    print(f"Wrote {RUST_OUT.relative_to(ROOT)} and {DOC_OUT.relative_to(ROOT)}.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
