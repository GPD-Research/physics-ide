---
title: "H1: fetched arXiv citation passes G1"
mode: chat
rigor: enforce
requires: [fetch]
expect:
  - op: fetch arxiv.org/abs/1905.09311 ok
  - gate: G1 never accepted-with-fail
operator_notes: |
  Needs FETCH (PR #48) on in Settings with the default allowlist. The full page text
  should also appear under .scratch/fetch/.
---
Fetch https://arxiv.org/abs/1905.09311 and summarise it in five sentences, citing it by its arXiv ID.
