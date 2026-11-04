---
title: "G: watch proposal does not run before Arm"
mode: chat
requires: [run]
setup:
  params/toy.json: |
    {"k": 4.0}
  sims/toy.py: |
    import json
    k = json.load(open("params/toy.json"))["k"]
    print(f"RESULT: omega={k ** 0.5:.4f}±0.01 rad/s")
    print("RESULT: period=%.4f±0.02 s" % (6.2832 / k ** 0.5))
expect:
  - op: watch_propose toy.py ok
  - op: watch_run toy.py not ok
operator_notes: |
  Settings: AI may run scripts on, allowed programs include python3.
  This file covers the proposal only. Then, by hand: Arm the watch in Workflow ▸ Watches,
  change k to 4.001 (quiet), then 9.0 (moved card, auto-sent), break and fix the script,
  Rebaseline, restart (paused). Finish with Thread tools ▸ Export audit….
---
Propose a watch that reruns sims/toy.py whenever params/toy.json changes. Do not run it yourself.
