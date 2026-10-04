"""Check tracked-file exclusions, descriptor integrity, and auth documentation links."""
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parents[1]


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
    defined = set()
    references = set()
    for document in documents:
        content = document.read_text(encoding="utf-8")
        defined.update(re.findall(r"^\|\s*(AUTH-L[123]-\d{3})\s*\|", content, re.M))
        references.update(re.findall(r"AUTH-L[123]-\d{3}", content))
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
    if references - defined:
        errors.append(f"Undefined requirements: {sorted(references - defined)}")
    for level in (1, 2, 3):
        if not any(identifier.startswith(f"AUTH-L{level}-") for identifier in defined):
            errors.append(f"Missing L{level} requirements.")
    if errors:
        raise SystemExit("\n".join(errors))
    print(f"Checked {len(files)} tracked files, {len(defined)} requirements, local document links, and descriptor integrity.")


if __name__ == "__main__":
    main()
