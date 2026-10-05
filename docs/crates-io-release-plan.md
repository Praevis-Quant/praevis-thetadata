# crates.io prerelease plan

Status: preparation plan, 2026-10-04. No packages have been published by this
work. Keep `publish = false` until the release preparation is reviewed.

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
| `praevis-thetadata-proto` | Checked-in descriptor and generated bindings for 82 RPCs | First wave; independent |
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
