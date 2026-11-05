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
PY_OUT = ROOT / "crates/praevis-thetadata-py/src/generated.rs"
VENDOR_OUT = ROOT / "crates/praevis-thetadata-py/python/praevis_thetadata/_vendor.py"
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
                              w.description, request, query, emit_body(w, params), emit_split(w, params))
    return "\n".join(out)


def emit_body(w: Wrapper, params: dict[str, Param]) -> list[str]:
    body = []
    if w.spec is not None:
        body += emit_spec(w.spec, params)
    for a in w.assignments:
        body += emit_assignment(a, params[a.param])
    return body


def emit_split(w: Wrapper, params: dict[str, Param]) -> list[str]:
    """Planner support (P1 PR 4b) for wrappers with a start_date..end_date range:
    EOD and at-time requests split at 366 days (the server's observed EOD span
    maximum is 366-730 days, P0 §5); intraday history splits per day (vendor
    sizing guidance). A request that also sets `date` is not split."""
    start, end = params.get("start_date"), params.get("end_date")
    if not (start and end and start.kind == end.kind == "NaiveDate"):
        return []
    days = 366 if "Eod" in w.rpc or "AtTime" in w.rpc else 1
    out = [f"    const SPLIT: Split = Split::Days({days});",
           "    fn date_range(&self) -> Option<(NaiveDate, NaiveDate)> {"]
    if start.required and end.required:
        out.append("        Some((self.start_date, self.end_date))")
        setter = ["            start_date: start,", "            end_date: end,"]
    else:
        guard = "self.date.is_none()" if "date" in params else "true"
        out += ["        match (self.start_date, self.end_date) {",
                f"            (Some(start), Some(end)) if {guard} => Some((start, end)),",
                "            _ => None,",
                "        }"]
        setter = ["            start_date: Some(start),", "            end_date: Some(end),"]
    out += ["    }",
            "    fn with_date_range(&self, start: NaiveDate, end: NaiveDate) -> Option<Self> {",
            "        Some(Self {", *setter, "            ..self.clone()", "        })", "    }"]
    return out


def emit_endpoint_impl(struct, rpc, python, stability, description, request, query, body,
                       split=()) -> list[str]:
    method = snake(rpc)
    out = [f"impl Endpoint for {struct} {{",
           f'    const RPC: &\'static str = "{rpc}";',
           f'    const PATH: &\'static str = "/BetaEndpoints.BetaThetaTerminal/{rpc}";',
           f"    const PYTHON_NAME: Option<&'static str> = {python};",
           f"    const STABILITY: Stability = {stability};",
           f"    const VENDOR_DESCRIPTION: &'static str = {rust_string(description.strip())};",
           f"    type Query = api::{query};",
           f"    type Request = api::{request};",
           *split,
           "    fn to_query(&self) -> Self::Query {",
           f"        let {'mut ' if body else ''}query = api::{query}::default();"]
    out += [f"        {line}" for line in body]
    out += ["        query",
            "    }",
            "    fn request(query_info: api::QueryInfo, query: Self::Query) -> Self::Request {",
            f"        api::{request} {{ query_info: Some(query_info), params: Some(query) }}",
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


LIVE_CLASSES = ("entitled", "tier-denied", "invalid-argument", "no-data", "other")
GRPC_CODES = ("OK", "Cancelled", "Unknown", "InvalidArgument", "DeadlineExceeded", "NotFound",
              "AlreadyExists", "PermissionDenied", "ResourceExhausted", "FailedPrecondition", "Aborted",
              "OutOfRange", "Unimplemented", "Internal", "Unavailable", "DataLoss", "Unauthenticated",
              "ClientTimeout")
TIERS = ("FREE", "VALUE", "STANDARD", "PRO")


LIVE_COLUMNS = {"free": "FREE", "pro": "options PRO"}


def live_cell(rpc: str, entry: object, column: str = "free") -> str:
    """Render an entitlement-map result. Committed results hold a
    classification and a gRPC code only, never vendor error text, so every
    field is checked against a closed vocabulary."""
    if not isinstance(entry, dict) or set(entry) - {"class", "code", "tier", "run", "uncertain"} \
            or not {"class", "code", "run"} <= set(entry):
        raise GenerationError(f"{rpc}: {column} needs class, code and run (tier, uncertain optional), nothing else")
    if entry.get("uncertain", True) is not True:
        raise GenerationError(f"{rpc}: uncertain is either absent or true")
    if entry["class"] not in LIVE_CLASSES or entry["code"] not in GRPC_CODES:
        raise GenerationError(f"{rpc}: unknown classification or gRPC code")
    if not re.fullmatch(r"[A-Za-z0-9_-]{1,64}", str(entry["run"])):
        raise GenerationError(f"{rpc}: invalid run id")
    tier = entry.get("tier")
    if tier is not None and (entry["class"] != "tier-denied" or tier not in TIERS):
        raise GenerationError(f"{rpc}: tier is only a named tier on tier-denied")
    label = f"{entry['class']}: {tier} required" if tier else entry["class"]
    if entry.get("uncertain"):
        label += " (uncertain)"
    return f"{label} (`{entry['code']}`, {entry['run']})"


CHECK_RESULTS = {
    "values-equal": "values equal to the legacy decoder",
    "rows-equal": "rows equal; values pending a same-response replay",
    "probe-data": "bounded probe: data, not a manifest; values not checked",
}
CHECK_PYTHON = {"ok": "Python OK", "not-run": "Python not run"}


def check_cell(rpc: str, entry: object) -> str:
    """Render a step F live check, from a closed vocabulary only."""
    if not isinstance(entry, dict) or set(entry) != {"result", "python", "run"}:
        raise GenerationError(f"{rpc}: check needs result, python and run, nothing else")
    if entry["result"] not in CHECK_RESULTS or entry["python"] not in CHECK_PYTHON:
        raise GenerationError(f"{rpc}: unknown check result")
    if not re.fullmatch(r"[A-Za-z0-9_-]{1,64}", str(entry["run"])):
        raise GenerationError(f"{rpc}: invalid run id")
    return f"{CHECK_RESULTS[entry['result']]}; {CHECK_PYTHON[entry['python']]} ({entry['run']})"


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
    for rpc, status in statuses.items():
        if not isinstance(status, dict) or set(status) - {"decoder", "check", *LIVE_COLUMNS}:
            raise GenerationError(f"{rpc}: status entries hold only 'decoder', 'free', 'pro' and 'check'")
    doc = ["# Endpoint coverage",
           "",
           "Generated by `tools/generate_endpoints.py` from the pinned 1.0.17 inventory and",
           "[`status.json`](status.json); do not edit by hand. This table replaces",
           "per-clause requirement matrices for generated endpoints",
           "([lighter process](../architecture/performance-plan.md#f-process-simplification)).",
           "",
           "The value check is step F (`live_p0 option-checks`, `bench/p1/option_checks.py`): each",
           "request through `query()` compared with the legacy decoder, and through the Python",
           "package. Flat files are bounded probes only.",
           "",
           "The FREE column is the one-time entitlement map: one request per endpoint family,",
           "classified by a person from the private report. The options PRO column is the",
           "one approved PRO map (options PRO; stocks and indices FREE): every option RPC once.",
           "Only the classification and gRPC code are committed. A named tier is the tier the",
           "service said is required.",
           "*Uncertain* marks a classification the evidence does not settle. Endpoints that",
           "were not probed are not inferred from their family.",
           "",
           f"{len(rows)} RPCs: {sum(1 for r in rows if r[1])} with a vendor wrapper, "
           f"{sum(1 for r in rows if not r[1])} descriptor-only (unstable).",
           "",
           "| RPC | Vendor wrapper | Rust request | Stability | Decoder test | Live check (FREE) "
           "| Live check (options PRO) | Value check (options PRO) |",
           "| --- | --- | --- | --- | --- | --- | --- | --- |"]
    for rpc, python, struct, stability in rows:
        status = statuses.get(rpc, {})
        doc.append(f"| `{rpc}` | {'`' + python + '`' if python else '—'} | `endpoints::{struct}` | {stability} "
                   f"| {status.get('decoder', 'generic (`query`)')} | "
                   + " | ".join(live_cell(rpc, status[c], c) if c in status else "not probed"
                                for c in LIVE_COLUMNS)
                   + f" | {check_cell(rpc, status['check']) if 'check' in status else 'not checked'} |")
    return rust, "\n".join(doc) + "\n"


# Rust types the Python bindings extract (crates/praevis-thetadata-py/src/kwargs.rs).
PY_KINDS = {"String", "f64", "i32", "bool", "NaiveDate", "Expiration", "TimeOfDay", "Symbols",
            "Vec<String>"}


def generate_python_dispatch(inventory: Path = INVENTORY) -> str:
    """Name-to-request dispatch for the Python bindings (P1 PR 6a). Keyword
    arguments are vendor parameter names for wrapped RPCs and request field
    names for unstable ones; an explicit None clears an optional field, as
    passing None to the vendor wrapper does. Unknown keywords are errors."""
    rpcs, messages, wrappers = load(inventory)
    arms, names = [], []
    for rpc in rpcs:
        _, _, fields = query_fields(rpc, messages)
        if rpc["name"] in wrappers:
            w = translate_wrapper(rpc, wrappers[rpc["name"]], fields)
            for p in w.params:
                if p.kind not in PY_KINDS:
                    raise GenerationError(f"{w.python}: no Python extraction for {p.kind}")
            args = ", ".join(f'kw.required::<{p.kind}>("{p.name}")?' for p in w.params if p.required)
            body = [f"let mut r = endpoints::{w.struct}::new({args});"]
            body += [f'if let Some(v) = kw.optional::<{p.kind}>("{p.name}")? {{ r.{ident(p.name)} = v; }}'
                     for p in w.params if not p.required]
            labels, struct = f'"{w.python}" | "{rpc["name"]}"', w.struct
            names.append(f'    ("{w.python}", "{rpc["name"]}"),')
        else:
            struct = rpc["name"][3:] if rpc["name"].startswith("Get") else rpc["name"]
            body = [f"let mut r = endpoints::{struct}::default();"]
            for f in fields:
                inner = f.scalar
                if inner == "wire::ContractSpec":
                    continue  # not settable from Python yet
                if f.rust_type.startswith("Option<"):
                    body.append(f'if let Some(v) = kw.optional::<{inner}>("{f.name}")? {{ r.{ident(f.name)} = v; }}')
                else:
                    body.append(f'if let Some(v) = kw.value::<{f.rust_type}>("{f.name}")? {{ r.{ident(f.name)} = v; }}')
            labels = f'"{rpc["name"]}"'
            names.append(f'    ("", "{rpc["name"]}"),')
        arms.append("\n".join([f"        {labels} => {{", *[f"            {line}" for line in body],
                                "            kw.finish()?;", "            runner.run(r)", "        }"]))
    header = ["// @generated by tools/generate_endpoints.py from docs/research/python-1.0.17-inventory.json.",
              "// Do not edit by hand; run `python tools/generate_endpoints.py`.",
              "#![allow(unused_mut, clippy::all)]",
              "use crate::kwargs::Kwargs;",
              "use crate::Runner;",
              "use chrono::NaiveDate;",
              "use praevis_thetadata_client::endpoints::{self, Expiration, Symbols, TimeOfDay};",
              "use pyo3::PyResult;",
              "",
              "/// (vendor Python name or \"\", RPC name) of every endpoint, in descriptor order.",
              "pub(crate) const NAMES: &[(&str, &str)] = &[", *names, "];",
              "",
              "/// Build the typed request named `name` from keyword arguments and run it.",
              "pub(crate) fn dispatch<R: Runner>(name: &str, mut kw: Kwargs<'_, '_>, runner: R) -> PyResult<R::Output> {",
              "    match name {"]
    footer = ["        _ => Err(pyo3::exceptions::PyValueError::new_err(format!(\"unknown endpoint: {name}\"))),",
              "    }", "}"]
    return "\n".join(header + arms + footer) + "\n"


def python_default(rpc: str, literal: str) -> str:
    """A vendor default as Python source: a literal, or the text a
    `_fmt_time('...')` default formats (the native side accepts the text)."""
    import ast
    match = re.fullmatch(r"_fmt_time\(('[^']*')\)", literal)
    source = match.group(1) if match else literal
    try:
        ast.literal_eval(source)
    except (ValueError, SyntaxError) as error:
        raise GenerationError(f"{rpc}: unsupported default {literal!r}") from error
    return source


def generate_vendor_aliases(inventory: Path = INVENTORY) -> str:
    """Vendor-named methods (P1 PR 6b): the 1.0.17 wrappers' exact parameter
    names, order and defaults, plus keyword-only `plan`. Each forwards every
    argument to the native query and returns the client's dataframe type.
    Python does no data work here."""
    rpcs, _, wrappers = load(inventory)
    quote = '"' * 3
    out = [f"{quote}Vendor-named methods with the vendor 1.0.17 signatures.",
           "",
           "@generated by tools/generate_endpoints.py from docs/research/python-1.0.17-inventory.json.",
           "Do not edit by hand; run `python tools/generate_endpoints.py`.",
           quote,
           "from __future__ import annotations",
           "",
           "from typing import Any",
           "",
           "",
           "class VendorMethods:",
           f"    {quote}The vendor `ThetaClient` method names. Each runs the same native query",
           f"    as `query(name, ...)` and returns the client's `dataframe_type`.{quote}",
           "",
           "    __slots__ = ()",
           "",
           "    def _vendor(self, name: str, plan: bool, params: dict[str, Any]) -> Any:",
           "        raise NotImplementedError",
           ""]
    for rpc in rpcs:
        wrapper = wrappers.get(rpc["name"])
        if wrapper is None:
            continue
        method = wrapper["name"].split(".")[-1]
        params = [p for p in wrapper["parameters"] if p["name"] != "self"]
        if any(p["kind"] != "positional-or-keyword" for p in params):
            raise GenerationError(f"{method}: unexpected parameter kind")
        signature = []
        for p in params:
            annotation = f": {p['annotation']}" if p["annotation"] else ""
            default = f" = {python_default(rpc['name'], p['default'])}" if p["default"] is not None else ""
            signature.append(f"{p['name']}{annotation}{default}")
        signature += ["*", "plan: bool = True"]
        summary = " ".join((wrapper.get("description") or "").split())
        summary = summary.split(". ")[0][:200].replace(quote, "'''").replace("\\", "/").rstrip(".")
        forwarded = ", ".join(f'"{p["name"]}": {p["name"]}' for p in params)
        doc = f"Vendor `{method}` (RPC `{rpc['name']}`)." + (f" {summary}." if summary else "")
        out += [f"    def {method}(self, {', '.join(signature)}) -> Any:",
                f"        {quote}{doc}",
                "",
                f"        Same as ``query(\"{method}\", ...)``, returned as the client's `dataframe_type`.{quote}",
                f'        return self._vendor("{method}", plan, {{{forwarded}}})',
                ""]
    return "\n".join(out).rstrip() + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true", help="fail if generated files are stale")
    args = parser.parse_args()
    try:
        rust, doc = generate()
        dispatch = generate_python_dispatch()
        aliases = generate_vendor_aliases()
    except GenerationError as error:
        print(f"generation failed: {error}", file=sys.stderr)
        return 2
    outputs = {RUST_OUT: rust, DOC_OUT: doc, PY_OUT: dispatch, VENDOR_OUT: aliases}
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
    print(f"Wrote {', '.join(str(p.relative_to(ROOT)) for p in outputs)}.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
