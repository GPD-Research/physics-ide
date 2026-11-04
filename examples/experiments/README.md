# Experiment files

An experiment is a `.md` or `.txt` file: an optional header between `---` lines, then the
instructions the AI works on. Load one with **Thread tools ▾ → Load experiment…** (or
Workflow ▸ Experiments). The app shows a preview; nothing is written or sent until you click
**Start**. At the end it checks the `expect` list against the operation ledger, the gate runs
and the disk, and writes `.scratch/experiments/<title>-<UTC time>.md` plus `.json`.

```
---
title: Brane tension sensitivity to k
mode: task                # task (default): runs as TASK with budgets and the DONE gate
                          # chat: sent as one chat message; finishes when the reply settles
rigor: enforce            # enforce | report-only | off — this run only, Settings untouched
max_rounds: 12            # task mode, this run only
token_budget: 150000
requires: [write, run]    # write | read-only | run | fetch — checked, never switched on
setup:                    # files the app writes on Start (ledgered as `setup`)
  params/toy.json: |
    {"k": 4.0}
expect:
  - file: results.md contains "omega"     # exists | absent | contains "text"
  - op: run sims/toy.py ok                # ok | refused | error | not ok
  - result: omega in 1.9..2.1             # latest RESULT from RUN/CALC/watch
  - gate: G1 never accepted-with-fail     # no non-sent-back reply had a G1 fail
  - gate: G0 failed at least once         # for tests that should be caught
  - verdict: no unverified                # no reply ended UNVERIFIED / report-only
operator_notes: |
  Shown to you in the preview; never sent to the AI.
---
# Goal
...
```

A file with no header is a plain task: its first `# heading` is the title.

**Rules.**
- Setup paths are relative, inside the project root, with no `..`, `.git/` or `project_memory.md`.
- Files that would be overwritten are flagged and need a second confirmation.
- The file is re-read on Start and refused if it changed since the preview.
- A check is **unchecked** when there is no evidence either way, for example no gate ran.
- `op:` targets match on a substring of the ledger target.

**Examples here** (scenarios A–H of the fixes.md acceptance plan, plus a task-mode run):

| File | Pane / settings |
|---|---|
| `a-false-write.md` | write toggle off |
| `b-honest-write.md` | pane may write, file access Read/write |
| `c-pathless-tests-passed.md` | any |
| `d-stale-write.md` | same thread as B; edit fixes.md before Start |
| `e-malformed-op.md` | may write; rerun with Send-backs 0 |
| `g-watch-proposal.md` | RUN on (python3); the rest of G by hand |
| `h1-fetch-arxiv.md`, `h2-unfetched-citation.md` | FETCH on (PR #48) |
| `t-toy-task.md` | may write, RUN on (python3) |
