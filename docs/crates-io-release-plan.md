# crates.io prerelease plan

Status: **executed.** `0.1.0-alpha.1` was published on 2026-11-02 UTC
(2026-11-01 US time), `0.1.0-alpha.2` on 2026-11-03 UTC and `0.1.0-alpha.3`
on 2026-11-08 and `0.1.0-beta.1` on 2026-11-11, each after owner approval. From
beta.1 the Python wheels are also on PyPI. The plan below is retained as the preparation record. Publishing
remains manual; no workflow publishes.

## Publication record: 0.1.0-beta.1

**Published 2026-11-11 at about 17:10 UTC,** approved by the user after the
live checks (`err-notfound-001`, and the WSL Linux run).
- **Source:** the owner ran `cargo publish` locally from clean `main` at
  `c0c3dbe2cf13a7cfa8b70d94daa973e024991cf6` (#156).
- **CI:** CI and Auth performance were green on that commit.
- **The token** was never in argv or logs.

**Crates:**
- **Order:** proto, core, auth, arrow, client, cli, stream. Each was
  "Published", and cargo waited for the index before the next.
- **Checksums:** all seven registry checksums match the local `.crate`
  files.
- **Not yanked:** none.
- **Publisher:** `joey-huckabee`.

| Crate | `.crate` SHA-256 (registry = local = downloaded) |
| --- | --- |
| [`praevis-thetadata-proto`](https://crates.io/crates/praevis-thetadata-proto/0.1.0-beta.1) | `8cecb0c0a67474e01efdf49951808ec7241c5ecaa1f9837d9d9c0fc26b65c3bc` |
| [`praevis-thetadata-core`](https://crates.io/crates/praevis-thetadata-core/0.1.0-beta.1) | `d2f4b195fe2c906a40ee33ff502476c352fb453ca957227d7d43f863a810ce42` |
| [`praevis-thetadata-auth`](https://crates.io/crates/praevis-thetadata-auth/0.1.0-beta.1) | `23bbd4a00dfada5aec9ce371c9a92915ac64bfa886cfad1b1502a7e649ed5346` |
| [`praevis-thetadata-arrow`](https://crates.io/crates/praevis-thetadata-arrow/0.1.0-beta.1) | `96aa964b01a88fc4def20fa42adfe8c337dab705a1face34f5ecbaf599096787` |
| [`praevis-thetadata-client`](https://crates.io/crates/praevis-thetadata-client/0.1.0-beta.1) | `cee668961265da5c0fd11c98e764b177bb0d2425e956f37bec88d948f3c094e9` |
| [`praevis-thetadata-cli`](https://crates.io/crates/praevis-thetadata-cli/0.1.0-beta.1) | `5b239ef5222c78b14b5821bed316a46859062ddd9229fbcbee0aeda1201914ae` |
| [`praevis-thetadata-stream`](https://crates.io/crates/praevis-thetadata-stream/0.1.0-beta.1) | `c62a89a7dda6f8e29fdf396248d9cb03cd79d8adc467c8da03dbf668956a0b16` |

`praevis-thetadata-py` is not published to crates.io.

**PyPI** (first upload of the project):
- **Release:** [`praevis-thetadata` 0.1.0b1](https://pypi.org/project/praevis-thetadata/0.1.0b1/).
- **Upload:** `twine check` passed. `twine upload`, with the token from the
  environment only, uploaded the two wheels from `da57cab`'s CI run
  43021749387. No sdist.
- **Why those wheels:** between `da57cab` and `c0c3dbe`, only a CI step
  name and a `pyproject.toml` comment differ, so the wheels match the
  tagged source.
- **PyPI's SHA-256 equals the CI artifacts:**

| Wheel | SHA-256 |
| --- | --- |
| `praevis_thetadata-0.1.0b1-cp310-abi3-manylinux_2_28_x86_64.whl` | `7f690072cd34ed497644fbe97e060e6c7129e77485ccf8814e6412123e303913` |
| `praevis_thetadata-0.1.0b1-cp310-abi3-win_amd64.whl` | `ca079ee65e4772f975ee1d51d8d473f1c0ac1c7463d9eb56fb584be268a01ca3` |

**Tag and release:**
- **Tag:** annotated `v0.1.0-beta.1`, tag object `18ac0433a9b1`, pointing
  to `c0c3dbe`.
- **Release:** the
  [GitHub pre-release](https://github.com/Praevis-Quant/praevis-thetadata/releases/tag/v0.1.0-beta.1),
  its body taken from the [release draft](release-draft-0.1.0-beta.1.md),
  with the same two wheels attached.

**Post-publication checks (owner):**
- **The CLI, Windows:** `cargo install praevis-thetadata-cli --version
  0.1.0-beta.1 --locked` printed `praevis-thetadata 0.1.0-beta.1`.
- **PyPI:** `pip install --pre praevis-thetadata` from PyPI into clean
  venvs imported `0.1.0-beta.1` with 81 vendor-named methods, on:
  - Windows (Python 3.12);
  - WSL Ubuntu 24.04 (Python 3.12.3, glibc 2.39).
- **docs.rs** built proto, core, auth, arrow, client and stream. The CLI
  has no documentation build, which is expected: it is binary-only, as
  before.

**Post-publication checks, Linux (the agent's sandbox, aarch64, 2026-11-11):**
- **The crates:** all seven `.crate` files downloaded from crates.io match
  the SHA-256 values above and the crates.io index's checksums, and none is
  yanked.
- **Crates.io-only consumers:** copies of `tools/release-consumers`, with
  the staging registry removed, built and ran against crates.io. Each
  printed its check:
  - `proto`: 100 RPCs;
  - `core`;
  - `auth`, and `auth-store` (the native store);
  - `arrow`: an exact decimal and a typed all-null column;
  - `client`: the frozen surface compiled; 100 endpoints, the planner and
    the tier scheduler offline;
  - `stream`: a parsed quote.

  The default `auth` and `client` consumers resolve no keyring or D-Bus
  packages.
- **The CLI:** `cargo install praevis-thetadata-cli --version 0.1.0-beta.1
  --locked` from crates.io built and printed `praevis-thetadata
  0.1.0-beta.1`.

## Beta.1 publication checklist and order (prepared 2026-11-11)

**Executed 2026-11-11** ([record above](#publication-record-010-beta1)). Kept as
the preparation record. **Approved by the user, conditional on the remaining live checks.** The user
runs `cargo publish` locally, with the token never in argv or logs.
Scope: [beta.1 scope record](releases.md#beta1-scope-record-user-2026-11-10);
the API-freeze checklist must be complete.

**Same crate set as alpha.3:**
- **Seven crates are published.** `praevis-thetadata-stream` is published
  but documented as experimental and outside the freeze.
  `praevis-thetadata-proto` is published and documented as unstable.
- **`praevis-thetadata-py` is not published to crates.io** (`publish = false`).
  **Its abi3 wheels go to PyPI** as `praevis-thetadata` `0.1.0b1` (user
  decision 2026-11-11; the step after the crates waves), and to the GitHub
  release. No sdist this time.
- **Nothing else** is unpublished, or newly published.

**Before publishing:**
1. **The live checks pass:**
   - `err-notfound-001` (`live_p0 not-found`, the error review's open
     question): **done 2026-11-11.** The two cases are identical, so
     `NoData` stands as documented;
   - the WSL Linux run.

   The checklist's error item is then ticked or recorded.
2. **This bump PR is merged last,** after the API-freeze PRs, with Verify,
   the Python package job and the packaging rehearsal green on Windows and
   Linux. The rehearsal packages all seven crates, stages them in the order
   below, and builds the external consumers outside the workspace (the
   client consumer compiles the frozen surface). It also installs the CLI.
   The Python job builds the abi3 wheels, installs them into a clean
   environment and runs the consumer examples.
3. **Archive review:** the per-package file lists and hashes from the
   rehearsal. Check the LICENSE and NOTICE copies (plus `LICENSE.upstream`
   in proto), and that there are no `artifacts/`, captures, `.env` files,
   keys or report files.
4. **The maintainer's final local review** on the merged commit, then the
   user's go-ahead for that exact commit.

**Publish, in dependency order,** each step waiting until the index serves
the previous crates:

| Wave | Crates | Depends on |
| ---: | --- | --- |
| 1 | `praevis-thetadata-proto`, `praevis-thetadata-core`, `praevis-thetadata-auth` | — |
| 2 | `praevis-thetadata-arrow` | core, proto |
| 3 | `praevis-thetadata-client` | auth, arrow, core, proto |
| 4 | `praevis-thetadata-cli`, `praevis-thetadata-stream` | auth; none (the client is only a stream dev-dependency) |

**Then PyPI** (the user, by hand, after wave 4):
- **What:** upload the two CI-tested wheels from `da57cab`'s CI run
  (artifacts `wheel-windows-latest` and `wheel-ubuntu-latest`) with
  `twine upload`.
  - `praevis_thetadata-0.1.0b1-cp310-abi3-win_amd64.whl`;
  - `praevis_thetadata-0.1.0b1-cp310-abi3-manylinux_2_28_x86_64.whl`.
- **No sdist,** and nothing in CI uploads.
- **The token** comes from the environment (`TWINE_USERNAME=__token__`,
  `TWINE_PASSWORD`). It is never in argv, logs or shell history.
- **Check the files before uploading** (`twine check`). A PyPI version can
  be yanked but never replaced: a file name, once uploaded, can never be
  uploaded again with different contents.

**After publishing:**
1. **Tag:** an annotated tag `v0.1.0-beta.1` on the published commit.
2. **GitHub release:** from the [draft](release-draft-0.1.0-beta.1.md),
   marked as a prerelease. Attach the same two wheels as the alternative
   to PyPI.
3. **Checks on Windows and Linux:**
   - `cargo install praevis-thetadata-cli --version 0.1.0-beta.1 --locked`
     prints the version;
   - the crates.io-only consumers build;
   - the downloaded `.crate` hashes match the registry;
   - docs.rs builds;
   - `pip install --pre praevis-thetadata` from PyPI into a clean venv
     resolves `0.1.0b1` and imports, with `__version__` `0.1.0-beta.1`. The
     PyPI files' SHA-256 matches the CI artifacts;
   - `pip install` of each attached GitHub release wheel does the same.
4. **Records:** a "Publication record: 0.1.0-beta.1" section here; the
   CHANGELOG section dated; the beta.1 row of `releases.md` marked
   released.

## Install and build outside the workspace (beta.1 record, 2026-11-11)

What CI proves on every change, on Windows and Linux. Nothing is published,
and no token is read.

**Rust (the packaging rehearsal, `tools/staging_registry.py`):**
- **Each published crate is packaged and built in isolation** (`cargo package`).
  The crates are auth, core, proto, arrow, client, cli and stream.
- **They are staged** in a local registry in publication order.
- **Every archive is inspected:** file lists, SHA-256, license files and
  forbidden inputs.
- **Seven consumer crates** (`tools/release-consumers`) are copied outside
  the workspace, then built, run and tested against the staged archives only:
  - proto, core, auth and auth with the native store;
  - arrow, client and stream.
- **The client consumer covers the frozen client surface** (added for
  beta.1):
  - compiled: generated endpoints through `query`/`query_with`,
    `job`/`job_with`, `collect`/`unify`, `RequestOptions`,
    `SchedulerPolicy`/`Tier` and `to_parquet`;
  - run offline: 100 endpoints, the job planner and the tier limits.
- **`cargo install praevis-thetadata-cli`** works from the staged registry.
- **The default auth and client consumers** resolve no native credential-store
  packages.
- **The minimum supported Rust version:** `cargo +1.88 check --workspace
  --all-targets` runs on Linux (added for beta.1). Before this, 1.88 was
  checked only by hand.

**Python (the "Python package" job):**
- **The abi3 wheel is built:** `manylinux_2_28` on Linux, and win_amd64.
- **It is installed into a clean virtual environment** with no other
  packages. It imports with the workspace version and 100 endpoints, and
  `describe()` builds a wire query and its job plan offline (added for
  beta.1).
- **The installed wheel runs the offline test suite and the D.2 gates.**

**After each publication, by hand** (recorded per release below): crates.io-only
consumers, `.crate` hashes against the index, docs.rs, and `pip install` of the
released wheels from the release URL on Windows and on Linux x86_64.

**Not covered:**
- macOS, and ARM wheels: not supported, and not built;
- a PyPI install, before publication: from beta.1 the wheels go to PyPI by
  hand, and the post-publication checks install from it;
- Python versions: the wheel declares Python ≥ 3.10, but CI tests 3.12 only.
  **Closed 2026-11-12:** a matrix job installs the same wheel into a clean
  environment and runs the package tests on 3.10, 3.11, 3.12 and 3.13, on
  both platforms.

## Publication record: 0.1.0-alpha.3

**Published 2026-11-08,** approved by the user the same day (decision A on
item 8: published with it open). The owner ran `cargo publish` locally on
Windows from clean `main` at
`7402533404ad7fb91be751d9320910f802ada055` (#124). The token was never in
argv or logs.

**Order:** proto, core, auth, arrow, client, cli, stream. Each upload exited
0, and cargo waited for the index before the next. All seven registry
checksums match the local `.crate` files. None is yanked. The publisher is
`joey-huckabee`.

| Crate | `.crate` SHA-256 (registry = local = downloaded) |
| --- | --- |
| [`praevis-thetadata-proto`](https://crates.io/crates/praevis-thetadata-proto/0.1.0-alpha.3) | `2aa08851c418e7798d16b4e76cee5518fdf9440dfd27a34bca6dfe46a2b29295` |
| [`praevis-thetadata-core`](https://crates.io/crates/praevis-thetadata-core/0.1.0-alpha.3) | `d87d79e9cddaf302b1636c13a6796d09b8b5217dceadf69137f97c83ad0f5033` |
| [`praevis-thetadata-auth`](https://crates.io/crates/praevis-thetadata-auth/0.1.0-alpha.3) | `dca696c193f1f4d089346883f39746bb5065e207cddf3c41c475db78a6cbced0` |
| [`praevis-thetadata-arrow`](https://crates.io/crates/praevis-thetadata-arrow/0.1.0-alpha.3) (**new**) | `56504d88baeb343ca2858c71df3c82ed1e12b80467d41fcf5d95eceb8b5fde64` |
| [`praevis-thetadata-client`](https://crates.io/crates/praevis-thetadata-client/0.1.0-alpha.3) | `9d84a8e68dcc8746ec18da56a3ea7a011f10c7b95fd5a8d1f453e97826b04597` |
| [`praevis-thetadata-cli`](https://crates.io/crates/praevis-thetadata-cli/0.1.0-alpha.3) | `7d0c2f2fbc5d7546b34c7f98535f731c6451408407c279ead5f4d561e09cdb86` |
| [`praevis-thetadata-stream`](https://crates.io/crates/praevis-thetadata-stream/0.1.0-alpha.3) (**new**) | `81c730c5fe178bd6997f0aa1055e68373a167055ba2bc9a4fd036ad0710561c6` |

`praevis-thetadata-py` is not published.

**Tag and release:**
- **Tag:** annotated `v0.1.0-alpha.3`, tag object `8f457308db47`, pointing
  to `7402533`.
- **Release:** the
  [GitHub pre-release](https://github.com/Praevis-Quant/praevis-thetadata/releases/tag/v0.1.0-alpha.3),
  its body taken from the [release draft](release-draft-0.1.0-alpha.3.md).
- **Not on PyPI.**
- **Assets:** the wheels from CI run 42450063300 on `7402533`, a run that
  passed and smoke-imported each wheel in a clean environment:

| Asset | SHA-256 |
| --- | --- |
| `praevis_thetadata-0.1.0a3-cp310-abi3-manylinux_2_28_x86_64.whl` | `a0ee5f5a827052b80c1144dcae7ffb31c1cc18d26a6ffc840656383e3d199c06` |
| `praevis_thetadata-0.1.0a3-cp310-abi3-win_amd64.whl` | `e3251ab30c2389adecf126d6c8ce634b2c5a1315b8ff9d0ca555df8c0ce6bd0e` |

**Post-publication checks, Windows (owner):**
- `cargo install praevis-thetadata-cli --version 0.1.0-alpha.3 --locked`
  printed `praevis-thetadata 0.1.0-alpha.3`.
- `pip install` of the win_amd64 wheel from the release URL into a clean
  venv imported it: `__version__` `0.1.0-alpha.3`, 81 vendor-named methods.

**Post-publication checks, Linux (the agent's sandbox, aarch64, 2026-11-08):**
- **The CLI:** `cargo install praevis-thetadata-cli --version 0.1.0-alpha.3
  --locked` from crates.io built and printed `praevis-thetadata
  0.1.0-alpha.3`.
- **The crates:** all seven `.crate` files downloaded from crates.io match
  the SHA-256 values above and the crates.io index, and none is yanked.
- **Crates.io-only consumers:** copies of `tools/release-consumers` with
  the staging registry removed built and ran:
  - `proto` (100 RPCs), `core`, `auth`, `auth-store` (native store) and
    `client`;
  - the new **`arrow`** (an exact decimal and a typed all-null column) and
    **`stream`** (a parsed quote).

  The default `auth` and `client` pull no keyring or D-Bus packages.
- **The Linux wheel, inspected** (the sandbox is aarch64, so it was
  installed on x86_64 by the owner; see below):
  - the release asset matches its SHA-256 and is byte-identical to the CI
    artifact of run 42450063300, which installed and imported this exact
    file in a clean environment;
  - its tag is `cp310-abi3-manylinux_2_28_x86_64` (version `0.1.0a3`,
    Python ≥ 3.10);
  - its native library needs at most `GLIBC_2.28` and links only libc,
    libm, libpthread, libdl and libgcc_s;
  - it holds no files beyond the package, metadata, licenses and SBOM.

**Post-publication check, Linux x86_64 (owner, WSL Ubuntu, glibc 2.39,
Python 3.12.3):** `pip install` of
`praevis_thetadata-0.1.0a3-cp310-abi3-manylinux_2_28_x86_64.whl` straight
from the release URL into a clean venv imported it: `__version__`
`0.1.0-alpha.3`, 81 vendor-named methods.

**docs.rs:** proto, core, auth, arrow, client and stream built. The CLI has
no documentation build, which is expected: it is a binary-only crate (no
library), as in alpha.2.

## Alpha.3 publication checklist and order (prepared 2026-11-08)

**Executed 2026-11-08** (record above). Kept as the preparation record.
The user runs `cargo publish` locally, with the token never in argv or
logs. Scope: [alpha.3 scope record](releases.md#alpha3-scope-record-2026-11-08).

**Before publishing:**
1. The version-bump PR is merged, with Verify and the packaging rehearsal
   green on Windows and Linux. The rehearsal packages all seven crates,
   stages them in the order below, builds the external consumers and builds
   the abi3 wheels with a smoke import (step c).
2. **Archive review:** the per-package file lists and hashes from the
   rehearsal. Check the LICENSE and NOTICE copies (plus `LICENSE.upstream`
   in proto), and that there are no `artifacts/`, captures, `.env` files,
   keys or report files.
3. The maintainer's final local review on the merged bump commit; then the
   user's explicit approval.

**Publish, in dependency order,** each step waiting until the index serves
the previous crates:

| Wave | Crates | Depends on |
| ---: | --- | --- |
| 1 | `praevis-thetadata-proto`, `praevis-thetadata-core`, `praevis-thetadata-auth` | — |
| 2 | `praevis-thetadata-arrow` (**new**) | core, proto |
| 3 | `praevis-thetadata-client` | auth, arrow, core, proto |
| 4 | `praevis-thetadata-cli`, `praevis-thetadata-stream` (**new**) | auth; none (the client is only a stream dev-dependency) |

`praevis-thetadata-py` is **not** published (`publish = false`).

**After publishing:**
1. Annotated tag `v0.1.0-alpha.3` on the published commit.
2. A GitHub release from the prepared draft (step d), with the Windows and
   Linux abi3 wheels from the rehearsal attached as assets. **Not on PyPI.**
3. **Checks on Windows and Linux:**
   - `cargo install praevis-thetadata-cli --version 0.1.0-alpha.3 --locked`
     prints the version;
   - the crates.io-only consumers build;
   - the downloaded `.crate` hashes match the registry;
   - docs.rs builds, including the two new crates;
   - `pip install` of the attached wheel imports.
4. Record all of this in a "Publication record: 0.1.0-alpha.3" section
   here, and date the CHANGELOG section.

## Publication record: 0.1.0-alpha.2

The Coordinator published from a clean `main` at `47ef74e7cd11` (#37) on Windows,
following the approved [release procedure](releases.md#alpha2-release-procedure-pending-approval).
The order was auth, core, proto, client, then CLI. Every upload exited 0, and
Cargo waited for index availability each time. The annotated tag
`v0.1.0-alpha.2` (tag object `ae1d4b7`) points to that commit. The crates.io
version API lists `joey-huckabee` as publisher, and none of the versions is
yanked. Each archive's `.cargo_vcs_info.json` names `47ef74e7cd11` and a clean
tree.

| Package | crates.io SHA-256 | Bytes | Published (UTC) | docs.rs |
| --- | --- | ---: | --- | --- |
| [`praevis-thetadata-auth`](https://crates.io/crates/praevis-thetadata-auth/0.1.0-alpha.2) | `e0bba436bc667072c3f3423f20a00c91f28e26bb540e5943c333520a36bd96d4` | 29381 | 2026-11-03 05:23:36 | Built; [docs](https://docs.rs/praevis-thetadata-auth/0.1.0-alpha.2) include `SessionStore` (built with `native-store`) |
| [`praevis-thetadata-core`](https://crates.io/crates/praevis-thetadata-core/0.1.0-alpha.2) | `21deb38185b3cd49654c1294bc60c76f48af800241ceeed50ba1085d47fb1734` | 14250 | 2026-11-03 05:23:41 | Built; [docs](https://docs.rs/praevis-thetadata-core/0.1.0-alpha.2) |
| [`praevis-thetadata-proto`](https://crates.io/crates/praevis-thetadata-proto/0.1.0-alpha.2) | `d69fb46fda1bf0c150553c1661bf3d3b5bac0e6c00b22557cdf299d00520db9c` | 23332 | 2026-11-03 05:24:09 | Built; [docs](https://docs.rs/praevis-thetadata-proto/0.1.0-alpha.2) |
| [`praevis-thetadata-client`](https://crates.io/crates/praevis-thetadata-client/0.1.0-alpha.2) | `18c3c7d105e4e448e9cea7f9cb2dd15cd6b9a8415ec907d352894fc8ccacced1` | 61448 | 2026-11-03 05:25:03 | Built; [docs](https://docs.rs/praevis-thetadata-client/0.1.0-alpha.2) |
| [`praevis-thetadata-cli`](https://crates.io/crates/praevis-thetadata-cli/0.1.0-alpha.2) | `914d63e2defc3ec904b08d608582a42722a242d47954ba474c687d7defd1451a` | 31443 | 2026-11-03 05:25:54 | Not applicable: binary-only package ("is not a library"); docs.rs build itself succeeded |

**Before publishing:**
- The packaging rehearsal for the bump PR passed on Windows and Linux.
- Offline replay on `47ef74e` (procedure step 3, no credentials) exited 0 for
  `prod-aapl-alpha1-002`, `prod-aapl-range-a2-001`, `prod-msft-a2-001` and
  `prod-nvda-split-a2-001`, with `matches: true`.
- Each semantic SHA-256 was identical across numeric, Table and raw replay, and
  equal to its [recorded value](research/live-observations.md#alpha2-prod-service-semantics-runs-2026-11-02).
  The compiled source SHA-256 prefix was `0cb4ebc71638`.
- So the post-#29 decoder reproduces the PROD values the earlier build captured.

**Windows (Coordinator, 2026-11-03):**
- `cargo install praevis-thetadata-cli --version 0.1.0-alpha.2 --locked` into a
  temporary root printed `0.1.0-alpha.2`.
- `auth status` read the existing default record: `persisted: true`, Windows
  Credential Manager, `not_checked`.
- The temporary root was removed.

**Linux (crates.io only, no workspace or path dependencies, 2026-11-03):**
- The five `.crate` files downloaded from `static.crates.io` match the registry
  SHA-256 values and sizes above.
- `cargo install` into a temporary root succeeded. `--version` prints
  `0.1.0-alpha.2`, and `auth --help` lists `--save`.
- The published binary's lifecycle, run in a disposable keyring against a
  synthetic local identity server:
  - `auth` gives `persisted: false`, and `status` exits 3;
  - `auth --save` gives `persisted: true` in Linux Secret Service;
  - `status` shows `persisted: true`, target
    `praevis.thetadata.auth.v1:STAGE:published-check`, `not_checked`;
  - `logout` gives `removed: true`, and `status` then exits 3.
- The `tools/release-consumers` set, with crates.io as the only source,
  resolved `0.1.0-alpha.2` and built and ran:
  - `auth-consumer` (default features): no keyring/D-Bus packages;
  - `auth-store-consumer` (`native-store`): `keyring`, `dbus`,
    `dbus-secret-service` and `libdbus-sys`, through the feature only; the
    identity check passes;
  - `core-consumer`: README values;
  - `proto-consumer`: 100 RPCs;
  - `client-consumer`: no keyring/D-Bus packages; typed request validation
    passes.

The published CLI description now says persistence is opt-in, which closes
the alpha.1 follow-up below.

**docs.rs:** the public queue was backlogged ("too long"), so the builds ran
about 2.5 hours after publication. They were confirmed at 07:47 UTC on
2026-11-03: auth, core, proto and client are documented, and auth includes
`SessionStore`. The CLI is binary-only, as for alpha.1.

## Publication record: 0.1.0-alpha.1

The maintainer published from `main` at `0d3ea724a88b` (#26), in wave order:
auth, core, proto, then client, then CLI. Every upload exited 0, and Cargo
waited for index availability each time. The annotated tag `v0.1.0-alpha.1`
(tag object `4c008cebb81c`) points to that commit. The crates.io version API
lists `joey-huckabee` as publisher, and none of the versions is yanked.

| Package | crates.io SHA-256 | Bytes | Published (UTC) | docs.rs |
| --- | --- | ---: | --- | --- |
| [`praevis-thetadata-auth`](https://crates.io/crates/praevis-thetadata-auth/0.1.0-alpha.1) | `254443a3fc08fef752d2f0e124ad3dc855ae9bccdf0d73d0efe11b9d93da8874` | 29381 | 2026-11-02 02:37:44 | Built; [docs](https://docs.rs/praevis-thetadata-auth/0.1.0-alpha.1) include `SessionStore` (built with `native-store`) |
| [`praevis-thetadata-core`](https://crates.io/crates/praevis-thetadata-core/0.1.0-alpha.1) | `38b53065e424baf52f431ddf9054f73ac6d8127730127d97b7aed7e55af14fb4` | 12992 | 2026-11-02 02:38:07 | Built; [docs](https://docs.rs/praevis-thetadata-core/0.1.0-alpha.1) |
| [`praevis-thetadata-proto`](https://crates.io/crates/praevis-thetadata-proto/0.1.0-alpha.1) | `1d3ab38f805df728146e633ed3ca1a9d858419009a5f6bc318635112caa54254` | 23333 | 2026-11-02 02:38:50 | Built; [docs](https://docs.rs/praevis-thetadata-proto/0.1.0-alpha.1) |
| [`praevis-thetadata-client`](https://crates.io/crates/praevis-thetadata-client/0.1.0-alpha.1) | `142dfae9f8da15a0545f9e5716e182861a1c6b389afb115b6afec8c0b2e19d8b` | 53187 | 2026-11-02 02:39:33 | Built; [docs](https://docs.rs/praevis-thetadata-client/0.1.0-alpha.1) |
| [`praevis-thetadata-cli`](https://crates.io/crates/praevis-thetadata-cli/0.1.0-alpha.1) | `c171d3123da4de430172d5364e06763ed6bfbc64235cd01ed7625217be91a9ca` | 31433 | 2026-11-02 02:39:42 | Not applicable: binary-only package ("is not a library"); docs.rs build itself succeeded |

The archives were built by `cargo publish` from the git checkout, so they
include `.cargo_vcs_info.json` and differ in hash from the earlier rehearsal
copies (see below). The hashes above are the registry's.

**Windows (maintainer, 2026-11-01):**
- `cargo install praevis-thetadata-cli --version 0.1.0-alpha.1 --locked`
  into a temporary root succeeded, and `--version` printed `0.1.0-alpha.1`.
- `auth status --json` read the user's existing default PROD record, saved on
  2026-10-04 by a pre-release build: exit 0, `persisted: true`,
  `validity: "not_checked"`. Saved v1 records therefore carry over to the
  published build.

**Linux (crates.io only, no workspace or path dependencies, 2026-11-02):**
- The same `cargo install` into a temporary root succeeded; `--version`
  prints `0.1.0-alpha.1`, and `auth --help` lists `--save`.
- The published binary's lifecycle, run in a disposable keyring against a
  synthetic local identity server:
  - `auth` gives `persisted: false`;
  - `status` exits 3;
  - `auth --save` gives `persisted: true` in Linux Secret Service;
  - `status` shows `persisted: true`, target
    `praevis.thetadata.auth.v1:STAGE:published-check`, `not_checked`;
  - `logout` gives `removed: true`, and `status` then exits 3.
- Fresh consumer crates, the `tools/release-consumers` set with crates.io as
  the only source, built, ran and tested against the published packages:
  - `auth-consumer` (default features): 0 keyring/D-Bus packages;
  - `auth-store-consumer` (`native-store`): 3 keyring/D-Bus packages, through
    the feature only; the identity check passes;
  - `core-consumer`: README values;
  - `proto-consumer`: 100 RPCs;
  - `client-consumer`: 0 keyring/D-Bus packages; typed request validation
    passes.
- The downloaded `.crate` files match the registry SHA-256 values and sizes
  above.

**Owners (public crates.io API, 2026-11-02):** each of the five crates has a
single owner, user `joey-huckabee`. **Recommendation:** add a backup owner
(a second trusted account or a GitHub team, `cargo owner --add <login|github:org:team> <crate>`)
for continuity. The owner chooses who.

**Follow-up for alpha.2 (done in #28, ships with the next release):** the
published alpha.1 CLI description still says "with native Windows/Linux session
storage". Since ADR-0020, persistence is opt-in (`auth --save`). The manifest
now says so; published metadata changes only with a new version.


The [versioned release milestones](releases.md) define scope and exit gates;
this document supplies the alpha.1 packaging/publication work. The manual live
capture/replay example is checkout-only and feature-gated: explicitly exclude
it from archives or make its fixture/provenance inputs self-contained. Never
include `artifacts/live` or credential files in a package.

Publish useful, clearly experimental `0.1.0-alpha.1` packages containing the
existing implementation and honest support boundaries. Empty name reservations
are unnecessary for this active project. The [crates.io policy update](https://blog.rust-lang.org/2023/09/22/crates-io-usage-policy-rfc/)
adopted restrictions on prolonged name squatting without functionality, purpose
or significant repository development. See the [policy text](https://github.com/rust-lang/rfcs/blob/master/text/3463-crates-io-policy-update.md).
Names are first come, first served; availability checks do not reserve them.

## Readiness observations

**Selected names:** use the [praevis-thetadata family](project-naming.md) below.
Repeat registry availability checks immediately before publication.
The root manifest is a virtual workspace; `praevis-thetadata` is not a sixth package.
`apps/praevis-thetadata-cli` is a separate binary package whose current executable is
`praevis-thetadata`. Its own README and explicit manifest metadata distinguish that install
surface from the libraries and repository overview.

Earlier lookups on 2026-10-04 returned HTTP 404 for the former `thetadata-auth`,
`thetadata-core`, `thetadata-proto`, `thetadata-client` and `thetadata-cli` names.
Those results do not establish availability of the renamed packages below.
Do not create speculative umbrella or future PyO3 packages just to claim names.

| Package | Useful initial contents | Publication order / dependency |
| --- | --- | --- |
| `praevis-thetadata-auth` | Explicit HTTP authentication, sessions, native persistence | First wave; independent |
| `praevis-thetadata-core` | Exact values, tables and validated numeric batches | First wave; independent |
| `praevis-thetadata-proto` | Checked-in descriptor and generated bindings for 100 RPCs (Python 1.0.17) | First wave; independent |
| `praevis-thetadata-client` | Raw RPC foundations and experimental typed EOD path | After auth/core/proto resolve from the registry |
| `praevis-thetadata-cli` | Working `praevis-thetadata auth`, local status and logout | After auth resolves from the registry |

Cargo's default-registry token entry exists in the local `credentials.toml`;
no environment token override was present. Its value was never printed or
copied into the repository. A read-only authenticated `/api/v1/me` request
returned HTTP 403. **Credential presence is confirmed; validity, identity,
expiry and publish permissions are not confirmed.** A scoped token can be
inappropriate for that endpoint, so the response alone does not prove an
invalid token. Review its account, expiry and crate-creation/publish scope in
the owner's crates.io token settings before publication. Do not ask for the
token in chat. A successful package dry run will not establish publish rights.

## Preparation work

### Local package inspection (2026-10-04)

`cargo package --list --allow-dirty --offline -p <package>` succeeded for all five
packages. This lists candidates; it does not build/verify distributable archives.

| Package | Package README | License/NOTICE in candidate listing | Remaining blocker |
| --- | --- | --- | --- |
| Auth | Missing | Missing | Public metadata, attribution files and standalone consumer verification |
| Core | Missing | Missing | Public metadata, attribution files and standalone consumer verification |
| Proto | Missing | `LICENSE.upstream` only | Project license/NOTICE, public metadata and standalone build verification |
| Client | Missing | Missing | Metadata/attribution, sibling descriptor and internal benchmark/manual-runner inputs |
| CLI | Present; explicitly selected in manifest | Missing | Attribution files, registry dependency versions and standalone install verification |

An actual offline CLI packaging attempt failed because `praevis-thetadata-auth` has no
registry version requirement. Adding a version alone would not make an unpublished
dependency resolvable. Set the release versions for the selected name family and prepare the first-wave
archives before verifying dependent archives against an isolated staging registry
or published, approved dependencies. `publish = false` remains in place. No
registry credentials were read and no package was uploaded during this inspection.

### Metadata, licenses and versions (alpha.1 PR A, 2026-11-01)

All five packages are `0.1.0-alpha.1`, versioned together and independently
of the Python compatibility baseline (`source_version` 1.0.17). Internal
dependencies carry exact registry requirements (`=0.1.0-alpha.1`) next to
their workspace paths. `rust-version = "1.88"` is verified with
`cargo +1.88 check --workspace --all-targets --locked` and
`cargo +1.88 test --workspace --locked` (62 passed); the locked tonic 0.14 and
ICU 2.x dependencies require 1.88. Each package has a description, a README,
docs.rs documentation, keywords and categories, an explicit supported-platform
statement, and an independence/experimental/no-full-parity note. Library
READMEs are also the crate-level docs, so their examples compile as doctests.
Byte-identical `LICENSE` and `NOTICE` copies sit in every package directory,
and `tools/check_repository.py` fails if a copy drifts from the root file;
proto also carries `LICENSE.upstream`. `publish = false` remains.

| Package | README | LICENSE / NOTICE in `cargo package --list` | Remaining before staging rehearsal |
| --- | --- | --- | --- |
| Auth | Added | Yes / yes | Extracted-archive consumer build |
| Core | Added | Yes / yes | Extracted-archive consumer build |
| Proto | Added | Yes / yes, plus `LICENSE.upstream` | Extracted-archive consumer build |
| Client | Added | Yes / yes | Resolved in part B: descriptor taken from the proto build dependency; examples excluded |
| CLI | Updated | Yes / yes | Resolved in part B: verified and installed from the staging registry |

### Archive staging rehearsal (alpha.1 part B, 2026-11-01)

`python tools/staging_registry.py` rehearses publication without uploading
anything or reading a token:

1. It packages wave 1 (auth, core, proto) with `cargo package --locked`, which
   builds each archive from its extracted contents.
2. It stages those archives in a fresh local alternative registry (a git index
   plus `file://` downloads).
3. It packages wave 2 (client, CLI), resolving wave 1 **from that registry**
   through `[patch.crates-io]`, and stages it too.
4. It copies the consumer crates in
   [`tools/release-consumers`](../tools/release-consumers/README.md) to a
   temporary directory outside the workspace, then builds, runs and tests them
   against the staged archives only.
5. It installs the CLI from the registry with `cargo install` and runs it.

Packaging runs in a copy of the tracked files, so this checkout's lockfile is
never edited.

Every archive is checked for `LICENSE`, `NOTICE`, `README.md` and `Cargo.toml`
(plus proto's `LICENSE.upstream`), and for the absence of credentials, `.env`
files, `artifacts/`, wheels and ZIPs. The CI job "Packaging rehearsal" runs the
same script on **Ubuntu and Windows** and uploads `report.json` and the logs.

Changes needed to make the archives self-contained:

- **Client build script:** it now takes the descriptor from
  `praevis_thetadata_proto::FILE_DESCRIPTOR_SET` through a build dependency,
  instead of reading `../praevis-thetadata-proto/schema/thetadata.bin`.
- **Examples:** `examples/` is excluded from the client archive. The EOD
  benchmark and the feature-gated `live_eod` runner include sibling sources,
  the workspace lockfile and live-capture paths, so they stay checkout-only;
  Cargo drops their targets from the published manifest. Tests remain, because
  they need only packaged files.
- **Staging index:** the rehearsal surfaced and fixed a bug in the generator.
  Features using `dep:` syntax must sit in the index's `features2`, or
  `native-store` cannot activate the optional `keyring` dependency.

Linux rehearsal at the publish-enable commit. These hashes replace those recorded in part B: the `publish` setting and the README install sections changed every archive. Archives are
byte-deterministic for identical contents: no VCS info is embedded, and core
and proto hashed identically across runs.

| Wave | Archive | Bytes | SHA-256 | Reviewed file list |
| --- | --- | ---: | --- | --- |
| 1 | `praevis-thetadata-auth-0.1.0-alpha.1.crate` | 29269 | `246f70bae29d1f0481e3fe49ec12a97f58dcf710295f2df1d94d984b08ffe7ae` | `Cargo.lock`, `Cargo.toml`, `Cargo.toml.orig`, `LICENSE`, `NOTICE`, `README.md`, `src/lib.rs`, `src/store.rs`, `src/tests.rs` |
| 1 | `praevis-thetadata-core-0.1.0-alpha.1.crate` | 12885 | `1702de2f3283c82571bffd915e5029f1e642526ae9328527ee89bf2897d9407b` | `Cargo.lock`, `Cargo.toml`, `Cargo.toml.orig`, `LICENSE`, `NOTICE`, `README.md`, `src/batch.rs`, `src/lib.rs` |
| 1 | `praevis-thetadata-proto-0.1.0-alpha.1.crate` | 23217 | `16f7189a2b8065b1fb51221e6e52b7d27bee2ef7a8f23d4f0d11677967219ea2` | `Cargo.lock`, `Cargo.toml`, `Cargo.toml.orig`, `LICENSE`, `LICENSE.upstream`, `NOTICE`, `README.md`, `build.rs`, `schema/manifest.json`, `schema/thetadata.bin`, `src/lib.rs` |
| 2 | `praevis-thetadata-client-0.1.0-alpha.1.crate` | 53101 | `9e3826b8e7288b8350dfdd180c3a498a6ba62b36e2969b2e2d95200832931662` | `Cargo.lock`, `Cargo.toml`, `Cargo.toml.orig`, `LICENSE`, `NOTICE`, `README.md`, `build.rs`, `src/bounded.rs`, `src/decode.rs`, `src/envelope.rs`, `src/eod.rs`, `src/framing.rs`, `src/lib.rs`, `tests/eod_contracts.rs`, `tests/eod_fixture.rs`, `tests/eod_observed_shape.rs`, `tests/eod_typed.rs`, `tests/support/mod.rs` |
| 2 | `praevis-thetadata-cli-0.1.0-alpha.1.crate` | 31367 | `69aa95aa9f04bd9b6ada22083598e241c011f3f3a0569336ac629a7feaf433bd` | `Cargo.lock`, `Cargo.toml`, `Cargo.toml.orig`, `LICENSE`, `NOTICE`, `README.md`, `src/main.rs`, `tests/no_save.rs`, `tests/persistence.rs`, `tests/startup.rs` |

| Consumer (outside the workspace) | Result |
| --- | --- |
| `auth-consumer` (default features) | Builds, runs and tests; README in-memory example compiles; **no** keyring/D-Bus packages resolved |
| `auth-store-consumer` (`native-store`) | README store example compiles; identity `praevis.thetadata.auth.v1:STAGE:release-consumer`; resolves dbus, dbus-secret-service, keyring only through the feature |
| `core-consumer` | README example asserts pass (`123.45`, RFC 3339 epoch) |
| `proto-consumer` | Packaged descriptor has 100 RPCs; `ZonedDateTime` round-trips |
| `client-consumer` | README EOD example compiles; typed request validation runs; **no** keyring/D-Bus packages resolved |
| `cargo install praevis-thetadata-cli` from the staging registry | Installs; `--version` reports `0.1.0-alpha.1`; `auth --help` lists `--save` |

With `publish = false`, Cargo refused `cargo publish --dry-run`, so the
rehearsal used `cargo package` and the staging registry. Now that publishing is
enabled, the owner can also run `cargo publish --dry-run --locked -p <package>`
for wave 1 immediately before publishing. Dependent packages still need their
dependencies published first.

### Remaining before publication (owner decisions)

1. **Token and owners:** review the crates.io account, the token's scope
   (crate creation and publish for these five names only) and its expiry, in
   crates.io settings, through Cargo's credential provider. Never put the
   token on a command line or in a file in the repository. Choose a backup
   owner or team.
2. **Name availability:** recheck all five `praevis-thetadata-*` names
   immediately before publishing. A check does not reserve a name.
3. **Publication approval:** approve these exact archives. Then set
   `publish = true` (or a crates.io-only list) in a reviewed change, publish
   auth, core and proto, wait for registry visibility, and publish client,
   then CLI. Record package URLs, hashes and a release tag. No workflow
   publishes automatically.
4. **Optional live check:** the manually recorded PROD auth and small EOD
   result for the alpha.1 exit gate in [releases.md](releases.md) is owned by
   the account holder.

### Work to close

1. Choose and review the exact version and scope of each initial package.
   Keep the Rust release version separate from Python compatibility version
   `source_version` in the protocol manifest. Describe this as an independent
   Rust project; do not imply vendor endorsement or full Python parity.
2. Add descriptions, repository/documentation links, package README files,
   appropriate categories/keywords and an explicit supported-platform statement.
   Preserve Apache-2.0, NOTICE and upstream attribution in the actual archives.
   Verify native keyring prerequisites for auth/CLI and normal builds without
   Python or protoc.
3. Give every workspace path dependency a registry version requirement. Change
   publication settings deliberately per package. The client build script reads
   the sibling proto descriptor today: remove that outside-package dependency,
   for example by consuming the proto crate's descriptor through a build
   dependency. Validate generated helpers from an extracted archive.
4. Make archive contents self-contained. The internal EOD benchmark references
   sibling source files and the workspace lockfile; explicitly exclude internal
   benchmark targets/assets from releases or reorganize their packaging. Do not
   remove source files needed by included tests/examples. Review
   `cargo package --list` and inspect each archive for secrets, local research
   archives, artifacts and missing license/build inputs.
5. Run Windows/Ubuntu checks and consumer builds from extracted archives, outside
   this workspace. First-wave crates can be checked with
   `cargo publish --dry-run --locked -p <package>`. Dependent-crate dry runs need
   published dependencies or a documented local-registry staging rehearsal;
   do not mistake workspace path resolution for registry readiness.
6. Prepare a release PR with exact package lists, versions, archive hashes,
   metadata, verification results and dependency order. Confirm the intended
   crates.io owner account and organizational backup owner/team. Review the
   existing token locally and replace it only if required, using Cargo's
   credential provider rather than a command-line token or committed file.
7. Obtain approval for those concrete public artifacts, then publish first-wave
   crates, wait for registry visibility, and publish dependent crates. Verify
   clean consumer installs and documentation. Record owners, release/tag and
   package URLs; retain the original package files and hashes. No scheduled or
   PR workflow should publish these first releases automatically.

Publication is a public release, not a reversible branch experiment. Cargo's
[publishing guide](https://doc.rust-lang.org/cargo/reference/publishing.html)
explains archive verification and registry version immutability. A yank is not
a replacement for reviewing contents before uploading. Use the
[registry authentication guide](https://doc.rust-lang.org/cargo/reference/registry-authentication.html)
for credential-provider configuration.
