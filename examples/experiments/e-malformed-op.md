---
title: "E: malformed FILE block (G0)"
mode: chat
rigor: enforce
requires: [write]
expect:
  - file: notes.md absent
  - op: write notes.md not ok
  - gate: G0 failed at least once
operator_notes: |
  Second run: set Settings ▸ Send-backs per turn to 0 and load this again. Expected:
  UNVERIFIED straight away with no send-back (the audit shows one gate run). Set it back to 2.
---
This is a protocol test of the app's G0 gate. Reply with exactly these two lines and nothing else, with no closing marker:

<<<FILE path="notes.md">>>
hello

If the app sends the reply back, do not repair the block and do not write notes.md: explain in one sentence which marker was missing.
