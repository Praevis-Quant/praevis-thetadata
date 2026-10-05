# Project and package naming decision

Status: selected by the project owner, 2026-10-04. Supersedes the earlier
Praevis Feed/Wire/Tick/Stream suggestions; those names were not adopted.

Public project name: **Praevis ThetaData**. Repository: **praevis-thetadata**.

> Independently maintained Rust client libraries for ThetaData services. This project is not affiliated with, sponsored by, or endorsed by Theta Data Inc. “ThetaData” identifies the service with which this software interoperates.

| Cargo package | Source directory | Rust import / executable |
| --- | --- | --- |
| `praevis-thetadata-auth` | `crates/praevis-thetadata-auth` | `praevis_thetadata_auth` |
| `praevis-thetadata-core` | `crates/praevis-thetadata-core` | `praevis_thetadata_core` |
| `praevis-thetadata-proto` | `crates/praevis-thetadata-proto` | `praevis_thetadata_proto` |
| `praevis-thetadata-client` | `crates/praevis-thetadata-client` | `praevis_thetadata_client` |
| `praevis-thetadata-cli` | `apps/praevis-thetadata-cli` | executable `praevis-thetadata` |

The root is a virtual workspace, not a sixth publishable package. The un-suffixed
name identifies the repository and executable. Package publication is still
disabled; the rename neither publishes nor reserves crates.io names. Recheck
exact registry names before publication under the [release plan](crates-io-release-plan.md).

## Compatibility and evidence

- Replace old `thetadata-*` Cargo dependencies/imports with the corresponding
  `praevis-thetadata-*` package / `praevis_thetadata_*` import. Public Rust type
  names, including `ThetaClient`, retain their meaning.
- Use `praevis-thetadata auth`, `auth status`, and `auth logout` in place of
  `theta`. There is no newly installed `theta` alias. Existing old binaries in a
  developer's target directory or PATH are not removed by a source rename.
- Preserve all `THETADATA_*` credential/configuration variables, the native-store
  service `praevis.thetadata.auth.v1`, profile/environment isolation, record format,
  wire names and the Python `thetadata` compatibility baseline. No session migration
  or new authentication is required because of the rename.
- Preserve protocol bytes, upstream licenses, artifact hashes and the original
  research/review. Historical benchmark commands and reports may use earlier
  package names; run those commands against their recorded source revisions.
  The CLI benchmark builder discovers both old and new checkout identities.
- The GitHub repository becomes `Praevis-Quant/praevis-thetadata`. Current links
  and the local origin use that URL. The account holder renames the local root
  directory after this session; current source and tooling do not depend on its name.

Two retained detached benchmark worktrees live under ignored `artifacts/` and
remain historical source evidence. If moving the root breaks their registered
absolute paths, run the following from the moved root:

```text
git worktree repair artifacts/cli-startup/baseline-source artifacts/eod/acceptance-control
git worktree list
```

Do not delete those baselines or rewrite their historical sources for branding.

## Naming context

The owner selected this descriptive, organization-prefixed name. This decision
does not assert trademark clearance or vendor endorsement. See the
[USPTO distinction](https://www.uspto.gov/trademarks/basics/trademark-patent-copyright)
and [Apache-2.0 section 6](https://www.apache.org/licenses/LICENSE-2.0#trademarks)
for the distinction between source licensing and trademark rights. Third-party
names such as ThetaDataDx do not establish permission for this project.
