# crates.io prerelease plan

Status: preparation plan, 2026-10-04; archives rehearsed 2026-11-01. No packages have been published by this
work. Publishing is enabled (`publish = ["crates-io"]`) in the PR the owner
approves for publication; no workflow publishes.

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
