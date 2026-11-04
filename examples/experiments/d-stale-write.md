---
title: "D: stale write is a warning, not a failure"
mode: chat
rigor: enforce
expect:
  - file: fixes.md exists
  - gate: G1 never accepted-with-fail
  - verdict: no unverified
operator_notes: |
  Load into the SAME thread as B (do not reset it). Before clicking Start, edit fixes.md
  in another editor and save it. Expected: the gate card shows a stale-write warning,
  G1 still passes and nothing is sent back. The warning is on the card and in the audit's
  gate table; it is not an expectation because it is not a pass/fail outcome.
---
Summarise what you wrote to fixes.md.
