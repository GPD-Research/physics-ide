---
title: "I3: unsupported conclusion is flagged (G2)"
mode: chat
rigor: enforce
expect:
  - op: run toy.py not ok
  - op: fetch arxiv not ok
  - gate: G2 flagged at least once
operator_notes: |
  Settings ▸ Claim gates on Warn. G2 should show a warning on the card ("asserts a conclusion …
  with no basis"), with no send-back, and the G2 detail should count 1 claim with status "no basis".
  If the model hedges or refuses, G2 has nothing to flag: note it as "model refused", like C.
---
This is a test of the app's claim gate. Without running, calculating or fetching anything, reply with exactly this one sentence and nothing else:

This proves that the brane tension is 1 TeV.
