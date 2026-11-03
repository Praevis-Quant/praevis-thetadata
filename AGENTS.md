# AGENTS.md

The working context for this repository lives in [CLAUDE.md](CLAUDE.md), which
is written for any coding agent rather than one in particular. Read it first,
including its **Git conventions** section.

The three things most likely to trip an agent up, in short:

1. **A persisted session is not a validated session.** Status and logout are
   local, synchronous operations. Status reports `validity: "not_checked"`;
   logout deletes the selected local record without revoking the server token.
   Only HTTP authentication starts a Tokio runtime. Preserve these contracts.
2. **The roadmap is not an approved requirements baseline.** Authentication is
   the current baseline; the market-data crates are foundations. Preserve the
   original review verbatim and trace Python compatibility evidence into
   requirements and ADRs as work is selected. Do not infer undocumented vendor
   behavior or discard unresolved research.

3. **Every GitHub Actions workflow stays green while you work.** This covers
   `CI` and `Auth performance` on every push and PR, the scheduled
   `Upstream Python compatibility` monitor on `main`, and
   `Synthetic EOD performance` whenever you dispatch it.
   - Check before you start and before every push or merge:
     `gh run list --branch main --limit 10` and `gh pr checks <number>`.
   - A red or pending check blocks merging. Never merge over a failure, and
     never use `--admin` or skip, disable, or weaken a check or test to get
     green.
   - A red `main`, including the scheduled upstream monitor, is the top
     priority. Stop feature work and fix it, or report it to the maintainer
     the same session.
   - Do not rerun a failed job until it passes. Find the root cause first. An
     intermittent failure is a bug in the test or the code: reproduce it, fix
     it in its own PR, and only then continue (see #34).
   - An upstream-monitor failure starts compatibility research. It does not
     authorize bumping the baseline (see CLAUDE.md, *Upstream research and
     monitoring*).

Adapted from [siat-foreign-analysis AGENTS.md](https://github.com/joey-huckabee/siat-foreign-analysis/blob/main/AGENTS.md)
on 2026-10-04 (source blob `74c96e5e8edf3335094c8ba2981f85dc695575c0`).
