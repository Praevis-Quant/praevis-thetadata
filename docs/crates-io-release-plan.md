# crates.io prerelease plan

Status: preparation plan, 2026-10-04. No packages have been published by this
work. Keep `publish = false` until the release preparation is reviewed.

Publish useful, clearly experimental `0.1.0-alpha.1` packages containing the
existing implementation and honest support boundaries. Empty name reservations
are unnecessary for this active project. The [crates.io policy update](https://blog.rust-lang.org/2023/09/22/crates-io-usage-policy-rfc/)
adopted restrictions on prolonged name squatting without functionality, purpose
or significant repository development. See the [policy text](https://github.com/rust-lang/rfcs/blob/master/text/3463-crates-io-policy-update.md).
Names are first come, first served; availability checks do not reserve them.

## Readiness observations

The five public API lookups below returned HTTP 404 on 2026-10-04. Check again
immediately before publication. These are the existing workspace package names;
do not create speculative umbrella or future PyO3 packages just to claim names.

| Package | Useful initial contents | Publication order / dependency |
| --- | --- | --- |
| `thetadata-auth` | Explicit HTTP authentication, sessions, native persistence | First wave; independent |
| `thetadata-core` | Exact values, tables and validated numeric batches | First wave; independent |
| `thetadata-proto` | Checked-in descriptor and generated bindings for 82 RPCs | First wave; independent |
| `thetadata-client` | Raw RPC foundations and experimental typed EOD path | After auth/core/proto resolve from the registry |
| `thetadata-cli` | Working `theta auth`, local status and logout | After auth resolves from the registry |

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
