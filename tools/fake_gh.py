"""A fake `gh` for tests/test_release.py: `gh api` against a JSON state file
(FAKE_GH_STATE) that behaves like GitHub's releases API, drafts included.
Every call is recorded as [method, endpoint]. Standard library only."""
import hashlib
import json
import os
import pathlib
import re
import sys


def main(argv: list[str]) -> int:
    state_path = pathlib.Path(os.environ["FAKE_GH_STATE"])
    state = json.loads(state_path.read_text())
    assert argv[0] == "api", argv
    args = argv[1:]
    endpoint, method, fields, input_path = None, "GET", {}, None
    i = 0
    while i < len(args):
        arg = args[i]
        if arg == "-X":
            method = args[i + 1]; i += 2
        elif arg in ("-f", "-F"):
            key, value = args[i + 1].split("=", 1)
            if arg == "-F" and value in ("true", "false"):
                value = value == "true"
            fields[key] = value; i += 2
        elif arg == "-H":
            i += 2
        elif arg == "--input":
            input_path = pathlib.Path(args[i + 1]); i += 2
        else:
            endpoint = arg; i += 1
    if method == "GET" and fields:
        method = "POST"
    path = re.sub(r"^https://[^/]+/", "", endpoint)
    state["calls"].append([method, path])
    releases = state["releases"]

    def done(result=None) -> int:
        state_path.write_text(json.dumps(state))
        if result is not None:
            print(json.dumps(result))
        return 0

    def fail(code: int) -> int:
        state_path.write_text(json.dumps(state))
        print(f"gh: HTTP {code}", file=sys.stderr)
        return 1

    if m := re.fullmatch(r"repos/[^/]+/[^/]+/git/ref/tags/(.+)", path):
        if m[1] not in state["tags"]:
            return fail(404)
        return done({"object": {"type": "tag"}})
    if re.fullmatch(r"repos/[^/]+/[^/]+/releases\?per_page=100&page=\d+", path):
        return done(releases)
    if method == "POST" and re.fullmatch(r"repos/[^/]+/[^/]+/releases", path):
        state["next_id"] += 1
        release = {"id": state["next_id"], "tag_name": fields["tag_name"], "name": fields["name"],
                   "body": fields["body"], "draft": fields["draft"], "prerelease": fields["prerelease"],
                   "assets": []}
        releases.append(release)
        return done(release)
    if m := re.fullmatch(r"repos/[^/]+/[^/]+/releases/assets/(\d+)", path):
        assert method == "DELETE"
        for release in releases:
            assert not (release["draft"] is False and any(a["id"] == int(m[1]) for a in release["assets"])), \
                "an asset of a published release was deleted"
            release["assets"] = [a for a in release["assets"] if a["id"] != int(m[1])]
        return done()
    if m := re.fullmatch(r"repos/[^/]+/[^/]+/releases/(\d+)/assets\?name=(.+)", path):
        release = next(r for r in releases if r["id"] == int(m[1]))
        assert release["draft"], "an asset was uploaded to a published release"
        data = input_path.read_bytes()
        state["next_id"] += 1
        release["assets"].append({"id": state["next_id"], "name": m[2], "size": len(data), "state": "uploaded",
                                  "digest": "sha256:" + hashlib.sha256(data).hexdigest()})
        return done(release["assets"][-1])
    if m := re.fullmatch(r"repos/[^/]+/[^/]+/releases/(\d+)", path):
        release = next(r for r in releases if r["id"] == int(m[1]))
        if method == "PATCH":
            assert release["draft"], "a published release was modified"
            release["draft"] = fields["draft"]
        return done(release)
    return fail(404)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
