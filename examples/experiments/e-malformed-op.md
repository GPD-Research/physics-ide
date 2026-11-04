---
title: "E: malformed FILE block (G0)"
mode: chat
requires: [write]
expect:
  - file: notes.md absent
  - op: write notes.md not ok
  - gate: G0 failed at least once
operator_notes: |
  Second run: set Settings ▸ Send-backs per turn to 0 and load this again. Expected:
  UNVERIFIED straight away with no send-back (the audit shows one gate run). Set it back to 2.
---
Reply with a FILE block for notes.md but leave out the END FILE line.
