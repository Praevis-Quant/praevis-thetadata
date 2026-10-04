# Upstream Python compatibility monitor

The [scheduled workflow](../.github/workflows/upstream-version.yml) runs daily at **13:17 UTC** and supports manual dispatch. It checks PyPI without installing or executing ThetaData. Scheduled GitHub workflows run from the default branch, so this schedule becomes active after the workflow is merged there. GitHub may delay scheduled runs; it is not an exact-time service.

The tracked Python version comes from `source_version` in [the protocol provenance manifest](../crates/thetadata-proto/schema/manifest.json), currently **1.0.12**. This is intentionally distinct from the Rust workspace's `0.1.0` version. Do not bump the baseline merely to make a failing check green: first research the release, decide scope, update requirements/ADRs and compatibility evidence, and deliberately regenerate or reconcile the source provenance.

[The checker](../tools/check_upstream.py) requests the [PyPI JSON Simple Index API](https://docs.pypi.org/api/index-api/) and compares distribution versions using [PEP 440 ordering from packaging](https://packaging.pypa.io/en/stable/version.html). The policy is the latest non-yanked stable release, including post-releases; prereleases and development releases are excluded. It considers wheel and source distributions regardless of this runner's Python/platform compatibility. No package download is needed.

- Exit **0**: latest stable version is equal to or older than the tracked version.
- Exit **1**: a newer stable version exists; the scheduled job fails and calls for compatibility research.
- Exit **2**: network/metadata/baseline verification failed; the job fails rather than incorrectly reporting that the baseline is current.

Network requests have a 20-second timeout, up to three attempts for connection/timeouts, and a response-size cap; the workflow is bounded to five minutes. Offline policy tests cover equality, older/newer versions, numeric ordering, post-releases, prereleases, yanks (including empty reasons), invalid responses, and retry failure. Normal PR CI runs these synthetic tests without requiring PyPI availability for the comparison itself. Tool dependencies are pinned in [tools/requirements.txt](../tools/requirements.txt).

```powershell
python -m pip install -r tools/requirements.txt
python -m unittest discover -s tools -p 'test_*.py'
python tools/check_upstream.py
```

Live validation on 2026-10-04 reported tracked **1.0.12**, latest stable **1.0.12**, exit **0**. The workflow does not automatically update files, create issues, or send separate messages. GitHub's ordinary workflow notifications govern failure alerts.

Reference: [GitHub scheduled event behavior](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#schedule).
