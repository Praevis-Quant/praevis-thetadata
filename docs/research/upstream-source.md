# Python 1.0.12 source repository and provenance

Checked: 2026-10-04. This investigation identifies the publishing repository and available source artifacts. The [deep wheel analysis and coverage matrix](python-1.0.12-catalog.md) cover the original distribution's features/functions; service contracts remain separately unverified.

## Repository identity

The 1.0.12 package metadata lists [AXIOMXLLC/python_library](https://github.com/AXIOMXLLC/python_library). Public GitHub web/API requests returned **404**.

The stronger release-specific evidence is [PyPI's publishing provenance for 1.0.12](https://pypi.org/project/thetadata/1.0.12/#files):

| Evidence | Value |
| --- | --- |
| Publishing repository | [AXIOMXLLC/thetadata-python](https://github.com/AXIOMXLLC/thetadata-python) |
| Publishing commit | `a7912e4dfa72a985b03d4d75516568b230aabb92` |
| Workflow | `.github/workflows/publish.yml` |
| Publisher environment | `pypi` |
| Publishing run | [36776004508, attempt 1](https://github.com/AXIOMXLLC/thetadata-python/actions/runs/36776004508/attempts/1) |
| Wheel SHA-256 | `d9a5f4d19d0a6e7abc9861e8aee6bba3838e3551105babf76c4e361ff233d565` |

The [machine-readable wheel attestation](https://pypi.org/integrity/thetadata/1.0.12/thetadata-1.0.12-py3-none-any.whl/provenance) independently exposes the publisher repository/workflow/environment and the artifact subject/hash. The retrieved response SHA-256 was `afea4e72e57cde45af129bc44d97626df46d0a815cd2764281feed0789ab28ec`; its raw JSON is retained locally under ignored `artifacts/cli-startup/upstream-provenance.json`. The commit/run above are recorded from PyPI's provenance display. This review consumed PyPI's verified publishing record; it did not independently reverify the Sigstore certificate/signature chain.

The attested wheel digest **exactly matches the local file named `thetadata-1.0.12-py3-none-any.zip`**. Its extension was changed, but its bytes match the published wheel. This adds publisher provenance to the existing internal RECORD/hash verification.

The attested repository also returned **404** publicly, both on the GitHub website and through `api.github.com/repos/AXIOMXLLC/thetadata-python`. Thus the publishing repository is identified, but no publicly accessible Git tree for 1.0.12 was located. A 404 does not establish whether a repository is private, deleted, renamed, or restricted. Do not assert a source-tree/tag match or invent a replacement URL; obtain vendor confirmation or access before relying on Git history.

## Available source fallback

PyPI also publishes [the 1.0.12 source distribution](https://files.pythonhosted.org/packages/de/9f/a8e1abcd8c771518a9d617b347193e065e4303d36d9d117c5ee920ef8fcf/thetadata-1.0.12.tar.gz), SHA-256 `5c038a36f9b44a4b17dab63a352c5754f31a7a296d21230f96df9cecab8b7f87`.

I downloaded this artifact into ignored research storage, verified its digest against PyPI's release metadata, and inspected it in memory without installing, importing, or extracting executable code. All **nine Python package files** matched the bundled wheel byte-for-byte: the three top-level modules plus the six initializer/generated-protocol modules under `_proto/` and `_proto/v3grpc/`. Additional source-distribution packaging files are outside the completed wheel inventory and remain a separately identified research input; this does not prove the archive contains every file from the inaccessible repository.

The public archived [baileydanseglio/thetadata-python](https://github.com/baileydanseglio/thetadata-python) is an older, deprecated Java/Terminal-dependent client, with an MIT license and a 0.9-series release history. It is not a verified source replacement for this Apache-2.0, direct-gRPC 1.0.12 distribution. Third-party wrappers likewise do not establish upstream identity.

## Recovering the removed local artifacts

The root-level `thetadata-1.0.12-py3-none-any.zip` and extracted
`thetadata-1.0.12-py3-none-any/` were removed on 2026-10-04 after a final
archive-to-inventory check and a hash/count check of all 14 extracted files.
They were ignored, untracked research inputs. Tracked descriptors, licenses,
inventories, research, and Rust sources remain. Ordinary builds and the
offline research check require neither copy.

The exact wheel can be recovered from [PyPI's versioned file](https://files.pythonhosted.org/packages/15/62/b7f18d6aa9f42572eb075d0d711a8fb4dd7e0578ea378f3d0a9c059f109b/thetadata-1.0.12-py3-none-any.whl).
The URL and SHA-256 were checked against the
[1.0.12 release metadata](https://pypi.org/pypi/thetadata/1.0.12/json) before
cleanup. Use the committed inventory's digest as the expected value, not a
newly downloaded checksum alone. Never install/import the vendor package for
this verification.

Run from the repository root in PowerShell only when source regeneration is
needed:

```powershell
$researchDirectory = Join-Path (Get-Location) 'artifacts/research'
New-Item -ItemType Directory -Path $researchDirectory -Force | Out-Null
$vendorWheel = Join-Path $researchDirectory 'thetadata-1.0.12-py3-none-any.whl'
$wheelUrl = 'https://files.pythonhosted.org/packages/15/62/b7f18d6aa9f42572eb075d0d711a8fb4dd7e0578ea378f3d0a9c059f109b/thetadata-1.0.12-py3-none-any.whl'
if (-not (Test-Path -LiteralPath $vendorWheel)) {
    Invoke-WebRequest -Uri $wheelUrl -OutFile $vendorWheel
}
$sourceInventory = Get-Content 'docs/research/python-1.0.12-inventory.json' -Raw | ConvertFrom-Json
if ((Get-FileHash -LiteralPath $vendorWheel -Algorithm SHA256).Hash.ToLowerInvariant() -ne $sourceInventory.artifact_sha256) {
    throw 'Recovered wheel does not match the reviewed artifact.'
}
python -m pip install -r tools/research-requirements.txt
python tools/research_python.py --archive $vendorWheel --check
```

The checker reads ZIP-format wheel bytes directly; it needs no extraction and
accepts either filename extension. On other platforms, download that same URL
to ignored research storage, compare the same SHA-256, and run the same Python
checker. Existing extraction-based maintenance scripts require separately
restoring their documented inputs if those older workflows are used. No
normal build automatically downloads or restores them.

Recovery depends on continued artifact availability; a hash alone is not an
archive. See [ADR-0011](../adr/0011-market-data-verification-and-artifact-retention.md)
for the proposed retention and Rust-native verification policy.

## Remaining research actions

- Preserve the hash-matched wheel/source-distribution identities. Wheel coverage does not include the source distribution's additional packaging files; examine those separately if packaging/build provenance becomes part of the port's requirements.
- Preserve the stale metadata link and the attested publishing identity as separate evidence fields.
- Ask ThetaData to confirm the accessible source repository/tag for the attested commit; do not silently substitute a legacy repository.
- Link each future feature requirement to artifact/symbol evidence even while Git history is unavailable.

See [the existing feature research](thetadata-1.0.12.md), [artifact inventory](vendor-1.0.12.json), and [forward-looking roadmap](../ROADMAP.md).
