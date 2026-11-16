# Contributing

Thank you for helping. praevis-thetadata is an independent, unofficial
client for ThetaData's market-data service, maintained by one person, so
small, focused changes are easiest to review.

## Reporting

- **Bugs and feature requests:**
  [GitHub issues](https://github.com/Praevis-Quant/praevis-thetadata/issues).
  Say what you ran, what you expected and what happened, with the versions
  and platform. Never include an API key, a session token or market data.
- **Security vulnerabilities:** privately, as [SECURITY.md](SECURITY.md)
  describes. Not in a public issue.

## Changing the code

1. **Open an issue first** for anything beyond a small fix, so the approach
   can be agreed before you spend time on it.
2. **Fork, branch and open a pull request** against `main`. Every change
   goes through a pull request and the required CI checks; `main` takes
   squash merges only.
3. **Requirements for a pull request to be merged:**
   - **Tests:** new functionality comes with tests, and a bug fix with a
     test that fails without it. Tests must not contact ThetaData: use the
     emulator in `crates/praevis-thetadata-emulator` and synthetic data only.
   - **Checks pass locally:** `cargo fmt --all --check`,
     `cargo clippy --workspace --all-targets --locked -- -D warnings`,
     `cargo test --workspace --locked`, and `python tools/check_repository.py`
     (the full list is in [AGENTS.md](AGENTS.md#common-commands)).
   - **No new `unsafe` code:** crates forbid it (two audited calls into
     zstd excepted), and the repository check enforces that.
   - **No new dependencies** without discussing them first; `cargo deny
     check` must pass (advisories, licences, sources).
   - **Commit messages explain why**, not only what: the problem, and the
     alternatives rejected.
   - **Documentation** is updated with the behaviour it describes, and the
     CHANGELOG under `Unreleased`.
4. **Licence:** contributions are accepted under the project's
   [Apache-2.0 licence](LICENSE).

## Releases

Releases are published only by the release workflow, after the
maintainer's approval ([docs/releasing.md](docs/releasing.md)).
