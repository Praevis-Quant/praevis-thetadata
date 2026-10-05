"""Fail when PyPI has a newer stable ThetaData release than the researched baseline."""
import json
from pathlib import Path
import sys
import time
import urllib.error
import urllib.request

from packaging.utils import canonicalize_name, parse_sdist_filename, parse_wheel_filename
from packaging.version import Version

ROOT = Path(__file__).resolve().parents[1]
URL = "https://pypi.org/simple/thetadata/"


def latest_stable(index):
    if not isinstance(index, dict) or canonicalize_name(index.get("name", "")) != "thetadata":
        raise ValueError("PyPI response does not describe thetadata")
    if not isinstance(index.get("files"), list):
        raise ValueError("PyPI response has no distribution list")
    versions = []
    for file in index["files"]:
        # An empty string is also a yanked reason under the Simple API.
        if file.get("yanked", False) is not False:
            continue
        filename = file["filename"]
        if filename.endswith(".whl"):
            name, version, _, _ = parse_wheel_filename(filename)
        else:
            name, version = parse_sdist_filename(filename)
        if canonicalize_name(name) != "thetadata":
            raise ValueError("PyPI distribution belongs to another project")
        if not version.is_prerelease and not version.is_devrelease:
            versions.append(version)
    if not versions:
        raise ValueError("PyPI returned no non-yanked stable distributions")
    return max(versions)


def fetch_index():
    request = urllib.request.Request(URL, headers={
        "Accept": "application/vnd.pypi.simple.v1+json",
        "User-Agent": "praevis-thetadata-upstream-monitor/1",
    })
    for attempt in range(3):
        try:
            with urllib.request.urlopen(request, timeout=20) as response:
                payload = response.read(16 * 1024 * 1024 + 1)
                if len(payload) > 16 * 1024 * 1024:
                    raise ValueError("PyPI metadata exceeds the response limit")
                return json.loads(payload)
        except (urllib.error.URLError, TimeoutError):
            if attempt == 2:
                raise
            time.sleep(attempt + 1)


def check(index, tracked):
    baseline = Version(tracked)
    if baseline.is_prerelease or baseline.is_devrelease:
        raise ValueError("The tracked compatibility baseline must be stable")
    latest = latest_stable(index)
    return latest, latest > baseline


def main():
    try:
        manifest = json.loads((ROOT / "crates/praevis-thetadata-proto/schema/manifest.json").read_text())
        tracked = manifest["source_version"]
        latest, newer = check(fetch_index(), tracked)
        print(f"Tracked Python compatibility: thetadata {tracked}; latest stable PyPI release: {latest}")
        if newer:
            print("A newer upstream release requires research and compatibility review. "
                  "Do not bump source_version without updating its source/protocol evidence.", file=sys.stderr)
            return 1
        return 0
    except (OSError, ValueError, KeyError, TypeError, AttributeError) as error:
        print(f"Upstream check could not verify PyPI: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
