# AGENTS.md

The working context for this repository lives in [CLAUDE.md](CLAUDE.md), which
is written for any coding agent rather than one in particular. Read it first,
including its **Git conventions** section.

The two things most likely to trip an agent up, in short:

1. **A persisted session is not a validated session.** Status and logout are
   local, synchronous operations. Status reports `validity: "not_checked"`;
   logout deletes the selected local record without revoking the server token.
   Only HTTP authentication starts a Tokio runtime. Preserve these contracts.
2. **The roadmap is not an approved requirements baseline.** Authentication is
   the current baseline; the market-data crates are foundations. Preserve the
   original review verbatim and trace Python compatibility evidence into
   requirements and ADRs as work is selected. Do not infer undocumented vendor
   behavior or discard unresolved research.

Adapted from [siat-foreign-analysis AGENTS.md](https://github.com/joey-huckabee/siat-foreign-analysis/blob/main/AGENTS.md)
on 2026-10-04 (source blob `74c96e5e8edf3335094c8ba2981f85dc695575c0`).
