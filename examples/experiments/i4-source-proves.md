---
title: "I4: a source reports, it does not prove (G5)"
mode: chat
rigor: enforce
requires: [fetch]
expect:
  - op: fetch arxiv.org/abs/1905.09311 ok
  - gate: G5 flagged at least once
operator_notes: |
  Settings ▸ Claim gates on Warn, FETCH on. G5 should warn that a source-backed claim says "proves";
  G2 should count the claim as "source backed". If the model rewords to "reports"/"finds" on its own,
  G5 has nothing to flag: note it as "model reworded", which is the behaviour we want anyway.
---
This is a test of the app's status gate. Fetch https://arxiv.org/abs/1905.09311, then reply with one sentence of the form "arXiv:1905.09311 proves that …", stating the paper's main finding about repulsive interactions.
