---
title: "T: toy omega task (task mode end to end)"
mode: task
rigor: enforce
max_rounds: 8
token_budget: 120000
requires: [write, run]
setup:
  params/toy.json: |
    {"k": 4.0}
  sims/toy.py: |
    import json
    k = json.load(open("params/toy.json"))["k"]
    print(f"RESULT: omega={k ** 0.5:.4f}±0.01 rad/s")
    print("RESULT: period=%.4f±0.02 s" % (6.2832 / k ** 0.5))
expect:
  - op: run toy.py ok
  - result: omega in 1.99..2.01
  - result: period in 3.12..3.16
  - file: results.md contains "omega"
  - gate: G1 never accepted-with-fail
  - verdict: no unverified
operator_notes: |
  Settings: AI may run scripts on, allowed programs include python3.
---
# Goal
Run sims/toy.py with python3 and write results.md containing the RESULT lines it printed,
copied exactly, plus one sentence on how omega would scale if k were doubled.

# Constraints
- Do not edit sims/toy.py or params/toy.json.
- Write results.md only after you have seen the RUN output.
