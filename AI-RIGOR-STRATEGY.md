# AI Rigor Strategy

## Purpose

Physics IDE should support open-ended scientific exploration without allowing fluent, agreeable, or confident prose to stand in for evidence. The AI may propose unconventional ideas and negative conclusions alike. The application should make claims traceable and constrain what may be reported as verified.

Success means reaching the most defensible conclusion available. “This does not pan out,” “the evidence is insufficient,” and “these possibilities remain unresolved” are successful outcomes when warranted. A promising hypothesis is not a failure because it remains unproven; it must simply remain labeled as a hypothesis.

## Design principle

**The model proposes; the application verifies observable state.**

The model can interpret, hypothesize, derive, and explain. The application is authoritative about its own file operations, tool executions, test outcomes, retrieved evidence, and other state it can directly inspect. The model's narration alone does not establish that an operation occurred or a claim was checked.

Enforcement should live in application logic and tool boundaries, not only in prompts. Prompts can guide behavior, but must not be the sole protection against unsupported claims.

## Epistemic principles

1. Evidence outranks fluency, confidence, and agreement.
2. Important claims have an explicit epistemic status and a traceable basis.
3. Claims about actions require application-observed results.
4. Uncertainty, contradiction, and failure are valid outcomes; they must not be converted into success-shaped prose.
5. Criticism must be evidence-sensitive. Reflexive praise and reflexive contrarianism are both failures.
6. Conclusions are revisable. New evidence may change an assessment, and the system should identify what could change it.
7. Exploration remains open. Speculative ideas may be developed, but must not be presented as established results without adequate support.
8. Human judgment remains central. The system supports discovery and scrutiny; it does not certify scientific truth.

## Claim statuses

Use a small, consistent vocabulary, adapting labels to the task:

- **Verified operation** — confirmed by an application-observed tool or filesystem result.
- **Source-backed** — supported by a cited, retrieved source passage; this means the source says it, not that the source is necessarily correct.
- **Derived conditionally** — follows from stated assumptions or premises, subject to checking the derivation.
- **Tentative** — a plausible interpretation or hypothesis that has not been established.
- **Contradicted** — conflicts with specified evidence or a checked result; explain the scope of the contradiction.
- **Underdetermined** — multiple possibilities remain and available evidence does not distinguish them.
- **Unknown / not checked** — there is not enough information or the relevant check was not performed.

Avoid numerical probabilities unless there is a defined method and evidence for estimating them. Otherwise use calibrated qualitative language and state the reasons and assumptions.

## Two-tier build-out

### Tier 1 — Operational truth and completion integrity

Make the application the source of truth for what it did and what state it observed. This tier applies across ordinary chat and Task mode wherever the application mediates operations.

1. **Canonical operation ledger.** Record app-mediated reads, writes, edits, RUN, CALC, and verification actions with stable operation IDs, arguments or target paths, status, and bounded results. UI reporting, model handoff, and completion checks must consume the same authoritative records.
2. **Explicit result handoff and fresh state.** Return each relevant operation result to the model before it can rely on that operation in a subsequent answer. Treat the startup tree as a snapshot, not live state. After a write, provide a fresh read, diff, or other direct check when the task requires verification.
3. **Outcome-based completion gates.** Treat model completion as a proposal. For requested outcomes, require application-verifiable evidence such as a successful write plus file existence/content check, or an actual test result. If evidence is missing or contradictory, reject verified completion with a specific reason while still allowing an honest incomplete/blocked report.
4. **Failure-safe and recoverable behavior.** Preserve failed, rejected, cancelled, and timed-out outcomes distinctly. Do not silently substitute success-shaped defaults. Make retry, clarification, and stopping paths available.
5. **Regression suite from operational failures.** Add deterministic tests for successful and failed writes, wrong paths, stale tree context, missing or contradictory operation results, unsupported test claims, and a model that ignores a failure report. Test enforcement in chat and Task completion paths where applicable.

**Tier 1 exit criterion:** the application cannot mark a requested operation as verified based solely on model prose; relevant operation records and current state agree, or the result is explicitly incomplete/unknown.

### Tier 2 — Evidence-backed scientific reasoning

Build on Tier 1 so investigations distinguish project statements, tool results, conditional derivations, and scientific conclusions.

1. **Claim-and-evidence record.** For investigation tasks, represent important claims with status, basis (source locations, operation/tool results, and assumptions), and what could change the assessment. Keep this proportional to the task; do not burden casual conversation with a full research dossier.
2. **Provenance-aware retrieval.** Provide relevant project material with stable paths and source locations. Validate that references resolve and quoted or paraphrased claims are supported by retrieved material. Make clear that a source-backed claim reports what the source says, not independent validation.
3. **Structured critical analysis.** For appropriate tasks, ask the model to identify assumptions, derive consequences, consider limiting cases or counterexamples, and distinguish support from attempted falsification. Do not require a ritual checklist where it is irrelevant; require the answer to expose checks that materially support its conclusion.
4. **Tool-backed checks.** Route arithmetic, units, symbolic work, code, simulations, and tests through suitable tools when available. Record inputs, tool identity/version where practical, outputs, and failures. A proposed calculation is not a completed calculation.
5. **Conclusion/status gate.** Check that the response's certainty does not exceed its basis: verified operations cite operation records; source claims cite sources; conditional results state premises; unresolved or speculative claims remain labeled. The gate checks traceability and status consistency, not ultimate truth.
6. **Human-directed iteration and evaluation.** Let the human introduce new mechanisms or hypotheses and ask the AI to formalize and challenge them. Preserve revisions and unresolved questions. Build a benchmark of known flaws, valid derivations, ambiguous cases, and false operational claims; evaluate whether the application catches them without suppressing justified discoveries or honest uncertainty.

**Tier 2 exit criterion:** a reviewer can trace important conclusions to evidence, assumptions, or tool results, distinguish established from tentative claims, and see what remains unresolved. This is not a guarantee that a scientific conclusion is true.

## Non-goals and safeguards

- Do not treat a longer prompt, a confident model, agreement between two models, or a polished derivation as proof.
- Do not require hidden chain-of-thought. Require concise, auditable rationales: premises, evidence, key derivation steps, checks performed, and uncertainty.
- Do not reward negativity for its own sake or block novel hypotheses merely because they are unconventional.
- Do not claim that project-wide context or a vector/retrieval representation retrains the model or guarantees comprehension.
- Do not silently change the existing Settings-based permission grants as part of epistemic enforcement. Operational execution isolation and authorized project access are separate concerns.
- Do not describe a gate as a truth oracle. It can verify observable facts and consistency requirements; scientific interpretation may still require expert and human judgment.

## Implementation approach

Implement incrementally. Complete Tier 1 and its regression tests before adding Tier 2 contracts. Reuse existing file-operation reporting and Task completion checks where possible, but consolidate them around application-observed state rather than duplicating prompt-only rules. Keep casual exploration responsive; apply stronger structured requirements when a user requests investigation, edits, calculations, or verification.

For each build item, define measurable acceptance tests before implementation. Use failures from real workflows as regression cases. A negative finding, a well-labeled tentative result, an honest unknown, and a supported discovery are all valid successful outcomes.

## Implementation checklist

### Tier 1 — Operational truth

- [ ] Define and implement the canonical operation ledger.
- [ ] Return structured operation results to the model and refresh state when required.
- [ ] Verify requested file outcomes against current filesystem state.
- [ ] Gate Task completion on application-observed evidence; preserve honest incomplete outcomes.
- [ ] Add regression tests for operational failures in chat and Task mode.

### Tier 2 — Evidence-backed reasoning

- [ ] Define a claim-and-evidence record with status, basis, assumptions, and revision conditions.
- [ ] Validate source provenance and references.
- [ ] Support falsifiable, assumption-aware critical analysis.
- [ ] Record tool-backed scientific checks and their inputs/results.
- [ ] Gate conclusion status against the available evidence without claiming to certify truth.
- [ ] Add human-directed investigation workflows and evaluate them against a benchmark of positive, negative, ambiguous, and adversarial cases.
