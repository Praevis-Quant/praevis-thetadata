"""Check tracked inputs, descriptor integrity, requirements, and document links."""
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parents[1]
REQUIREMENT = r"[A-Z][A-Z0-9]*(?:-[A-Z0-9]+)*-L[123]-\d{3}"
ORIGINS = {"upstream-contract", "rust-enhancement", "mixed"}


def audit_origins(registry, requirements, adr_paths, tracked_paths):
    """Check exhaustive, deliberate classification; not semantic noninterference."""
    errors = []
    if registry.get("schema_version") != 1:
        errors.append("Unsupported origin register schema version.")
    classified = set()
    groups = set()
    for group in registry.get("groups", []):
        name = group.get("id")
        if not isinstance(name, str) or not name.strip() or name in groups:
            errors.append(f"Invalid/duplicate origin group: {name}")
        groups.add(name)
        if group.get("origin") not in ORIGINS:
            errors.append(f"Invalid origin: {name}")
        for field in ("rationale", "protected_contract"):
            if not isinstance(group.get(field), str) or not group[field].strip():
                errors.append(f"Missing {field}: {name}")
        if not group.get("requirements") or not group.get("evidence"):
            errors.append(f"Missing requirements/evidence: {name}")
        for identifier in group.get("requirements", []):
            if identifier in classified:
                errors.append(f"Duplicate origin classification: {identifier}")
            classified.add(identifier)
        for path in group.get("evidence", []):
            if path not in tracked_paths:
                errors.append(f"Untracked/missing origin evidence: {path}")
    for identifier in sorted(set(requirements) - classified):
        errors.append(f"Unclassified requirement: {identifier}")
    for identifier in sorted(classified - set(requirements)):
        errors.append(f"Unknown classified requirement: {identifier}")
    classified_adrs = set()
    for record in registry.get("adrs", []):
        path = record.get("path")
        if path in classified_adrs:
            errors.append(f"Duplicate ADR origin: {path}")
        classified_adrs.add(path)
        if record.get("origin") not in ORIGINS:
            errors.append(f"Invalid ADR origin: {path}")
        if not isinstance(record.get("rationale"), str) or not record["rationale"].strip():
            errors.append(f"Missing ADR origin rationale: {path}")
    for path in sorted(set(adr_paths) - classified_adrs):
        errors.append(f"Unclassified ADR: {path}")
    for path in classified_adrs - set(adr_paths):
        errors.append(f"Unknown classified ADR: {path}")
    return errors


def audit_requirements(documents):
    """Audit canonical table IDs and immediate-level parent links in Markdown.

    Definitions use a bare ID in the first table cell inside a requirements
    directory. L2/L3 definitions use parent IDs in the second cell. Other
    tables should link to definitions instead of repeating their defining row.
    This checks structure, not the truth of implementation/evidence claims.
    """
    definitions = {}
    references = set()
    errors = []
    for path, content in documents.items():
        references.update(re.findall(rf"\b{REQUIREMENT}\b", content))
        for match in re.finditer(rf"^\|\s*({REQUIREMENT})\s*\|([^\n]*)", content, re.M):
            identifier, rest = match.groups()
            if "requirements" not in Path(path).parts:
                errors.append(f"Definition outside requirements directory: {identifier} in {path}")
            if identifier in definitions:
                errors.append(f"Duplicate requirement definition: {identifier}")
            parents = re.findall(REQUIREMENT, rest.split("|", 1)[0])
            definitions[identifier] = parents
    if references - definitions.keys():
        errors.append(f"Undefined requirements: {sorted(references - definitions.keys())}")
    namespaces = {identifier.rsplit("-L", 1)[0] for identifier in definitions}
    for namespace in namespaces | {"AUTH"}:
        for level in (1, 2, 3):
            if not any(identifier.startswith(f"{namespace}-L{level}-") for identifier in definitions):
                errors.append(f"Missing {namespace} L{level} requirements.")
    for identifier, parents in definitions.items():
        level = int(identifier.rsplit("-L", 1)[1][0])
        if level == 1:
            continue
        if not parents:
            errors.append(f"Missing parents: {identifier}")
        for parent in parents:
            if parent not in definitions:
                errors.append(f"Undefined parent: {identifier} -> {parent}")
            elif int(parent.rsplit("-L", 1)[1][0]) != level - 1:
                errors.append(f"Wrong parent level: {identifier} -> {parent}")
    return definitions, errors


def main():
    files = subprocess.check_output(["git", "ls-files", "-z"], cwd=ROOT).decode().split("\0")
    files = [path for path in files if path]
    if not files:
        raise SystemExit("No tracked/staged files to audit.")
    errors = []
    for path in files:
        parts = Path(path).parts
        name = parts[-1]
        if (parts[0] in {"target", "artifacts", "thetadata-1.0.12-py3-none-any"}
                or name == "thetadata-1.0.12-py3-none-any.zip" or name.endswith(".whl")
                or name == "creds.txt" or name.endswith(".credentials")
                or (name.startswith(".env") and name != ".env.example")):
            errors.append(f"Forbidden tracked input: {path}")
    manifest = json.loads((ROOT / "crates/thetadata-proto/schema/manifest.json").read_text())
    schema_hash = hashlib.sha256((ROOT / "crates/thetadata-proto/schema/thetadata.bin").read_bytes()).hexdigest()
    if schema_hash != manifest["descriptor_sha256"]:
        errors.append("Descriptor bytes do not match the provenance manifest.")
    documents = [ROOT / path for path in files if path.endswith(".md")]
    contents = {document.relative_to(ROOT).as_posix(): document.read_text(encoding="utf-8")
                for document in documents}
    defined, requirement_errors = audit_requirements(contents)
    errors.extend(requirement_errors)
    registry = json.loads((ROOT / "docs/requirements/origins.json").read_text(encoding="utf-8"))
    adrs = {path for path in files if re.fullmatch(r"docs/adr/\d{4}-.+\.md", path)}
    errors.extend(audit_origins(registry, defined, adrs, set(files)))
    for document in documents:
        content = contents[document.relative_to(ROOT).as_posix()]
        for link in re.findall(r"\]\(([^)]+)\)", content):
            if "://" in link or link.startswith(("#", "mailto:")):
                continue
            # The roadmap preserves a verbatim review with historical workstation
            # citations, including ignored measurement artifacts. These are not
            # portable repository links; keep auditing every relative link.
            if document == ROOT / "docs/ROADMAP.md" and re.match(r"/[A-Za-z]:/", link):
                continue
            target = unquote(link.split("#", 1)[0])
            if target and not (document.parent / target).exists():
                errors.append(f"Broken file link in {document.relative_to(ROOT)}: {target}")
    if errors:
        raise SystemExit("\n".join(errors))
    print(f"Checked {len(files)} tracked files, {len(defined)} requirements, {len(adrs)} ADR origins, "
          "exhaustive requirement origins, local document links, and descriptor integrity.")


if __name__ == "__main__":
    main()
