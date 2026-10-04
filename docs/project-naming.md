# Project and package naming decision brief

Status: suggestions, not a selected rename. Reviewed 2026-10-04.

Prefer an independent project brand with a descriptive subtitle such as
"Independent Rust client for ThetaData." Keep package names consistent across
libraries and the CLI, while preserving vendor names in protocol provenance,
compatibility documentation and attribution.

In the United States, names identifying the source of goods/services principally
raise trademark questions; copyright covers original expression. See the
[USPTO distinction](https://www.uspto.gov/trademarks/basics/trademark-patent-copyright).
[Apache-2.0 section 6](https://www.apache.org/licenses/LICENSE-2.0#trademarks)
does not generally grant the licensor's trademark rights. A suffix or an
unaffiliated disclaimer alone does not establish permission or legal clearance.
ThetaDataDx's name is not evidence of permission for our project. This brief
does not determine whether an existing or proposed name infringes a mark.

| Candidate | Example crate prefix | Why consider it | Tradeoff |
| --- | --- | --- | --- |
| **Praevis Feed** (preferred) | `praevis-feed-*` | Connects to the organization; covers historical and live market data | Needs a ThetaData compatibility subtitle for discovery |
| Praevis Wire | `praevis-wire-*` | Emphasizes efficient transport and precise delivery | Sounds lower-level than a complete SDK |
| Praevis Tick | `praevis-tick-*` | Short and recognizably market-data related | Tick focus understates EOD/calendar/rate scope |
| Praevis Stream | `praevis-stream-*` | Fits incremental delivery and future event subscriptions | May suggest real-time support before it is implemented |

These are unselected ideas, not availability or trademark clearance results.
Before selection/publication check exact crate names, hyphen/underscore
collisions, repository/domain/package-ecosystem use and relevant trademark/common
law usage; seek qualified advice if a conflict remains. Record the dated checks.

For the preferred option, use repository `praevis-feed`, packages
`praevis-feed-auth`, `praevis-feed-core`, `praevis-feed-proto`,
`praevis-feed-client`, and `praevis-feed-cli`, with executable `praevis-feed`.
There is no need for an empty umbrella package. `apps/` versus `crates/` is a
source-layout convention; either location can contain a separately published
Cargo package. See [Cargo workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html).

## Rename scope after selection

1. Update manifests, dependency/import names, binary/help text, installation
   examples, repository/documentation URLs, workflows and package READMEs together.
   Keep each package's README specific to that package, especially the CLI.
2. Preserve upstream artifact hashes, original research, the roadmap's verbatim
   review and attribution. Historical names remain valid historical evidence.
3. Keep credential environment names and the persisted session-store identity
   stable unless a separate migration decision explicitly changes them; branding
   must not strand sessions or silently change authentication contracts.
4. Re-run Windows/Linux tests, external consumer/archive checks and name checks.
   Do not enable publication or reserve new names as part of this discussion.

This decision is a first-publication gate in the
[packaging plan](crates-io-release-plan.md). Performance regression investigation
and credential-free EOD tests can continue before a name is selected.
