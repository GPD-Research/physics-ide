---
title: "I2: tool-backed numbers pass G4"
mode: task
rigor: enforce
max_rounds: 6
token_budget: 80000
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
  - gate: G2 never flagged
  - gate: G3 never flagged
  - gate: G4 never flagged
  - gate: G5 never flagged
  - verdict: no unverified
operator_notes: |
  Settings ▸ Claim gates on Warn. AI may run scripts on, with python3 allowed. G2–G5 should pass
  every draft, with G4's "number(s) checked" count above 0. Any G2–G5 warning here is a false
  positive: send the audit.
---
# Goal
Run sims/toy.py with python3, then report omega and period with their uncertainties and units, exactly as printed, citing the run's op#.

# Constraints
- Do not edit sims/toy.py or params/toy.json, and do not write any files.
- Report the numbers only after you have seen the RUN output.
