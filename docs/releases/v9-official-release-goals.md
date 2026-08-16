# v9.0.0 Official Release Goals

Current baseline: v8.0.0 release candidate
Target milestone: v9.0.0, the first official physics-IDE release

## Product statement

v9 should turn the v8 release candidate into a production-ready research application. The release should preserve the theory-agnostic workflow while making long, project-aware AI sessions faster, less expensive, observable, and reliable.

## Goal 1: Leverage native OpenAI prompt caching

OpenAI prompt caching is automatic at the serving layer. physics-IDE should not build a second cache or assume that a request is cached. It should assemble eligible OpenAI requests so repeated calls begin with the longest practical byte-stable prefix, then verify the result using usage metadata returned by OpenAI.

### Payload ordering contract

Every OpenAI payload should be ordered from least volatile to most volatile:

1. Stable application instructions
   - lane role and behavioral rules;
   - retrieval and file-access contracts;
   - response-format requirements.
2. Stable project context
   - project identity and configured relative paths;
   - compact canonical axioms and structural model graph;
   - project awareness index and tool registry.
3. Slowly changing project context
   - workspace tree or project digest;
   - durable experiment and manuscript summaries;
   - prior-session recap when explicitly retained.
4. Current thread history
   - prior user and assistant turns in chronological order;
   - attachments associated with those turns.
5. Highly dynamic request context
   - current session or idea-pad notes;
   - current hypothesis and temporary analysis state;
   - current user message and newly attached files;
   - timestamps, request IDs, transient diagnostics, and other per-request values.

Highly dynamic text must never be inserted into or above the reusable prefix. If a dynamic field is not needed by the model, omit it rather than moving it into the prompt.

### Implementation deliverables

- [x] Replace the flattened single-user-message OpenAI payload with an ordered, structured message payload.
- [x] Create one canonical prompt assembler shared by both AI panes so equivalent context serializes identically.
- [ ] Separate stable primer content from session summary, hypothesis cues, idea-pad notes, timestamps, and transient diagnostics.
- [ ] Keep static content, whitespace, delimiters, and section order deterministic across requests.
- [x] Append current-thread and current-request content only after all reusable project context.
- [x] Preserve conversation roles instead of embedding role labels into one text block.
- [ ] Avoid cache-hostile values in the prefix, including generated timestamps, absolute paths that vary by machine, random identifiers, and non-deterministic file ordering.
- [x] Parse OpenAI usage metadata, including cached input token counts when supplied by the selected model and endpoint.
- [x] Surface input tokens, cached input tokens, output tokens, cache-hit ratio, provider, and model in the request inspector.
- [ ] Track cumulative estimated and provider-reported token use per thread without treating estimates as exact billing data.
- [x] Treat absent cached-token metadata as `unavailable`, not as a zero-token cache hit.
- [ ] Keep Gemini request behavior provider-specific; do not force OpenAI caching assumptions onto Gemini.

### Current migration targets

The v8 request path currently needs these changes:

- `buildPanePrimerText` mixes stable role/retrieval rules with changing summary, hypothesis, diagnostics, and briefing excerpts.
- `build_prompt_from_history` flattens all roles and turns into one string.
- `call_openai` sends that flattened string as a single user message and discards response usage metadata.
- idea-pad sync is added as a system-style history item even though it is highly dynamic session context.

### Verification

- [ ] Unit tests prove stable sections serialize identically when only the user message, session notes, hypothesis, or timestamp changes.
- [ ] Unit tests prove every dynamic section occurs after the stable-prefix boundary.
- [ ] Unit tests cover deterministic ordering for project files, tools, and other collection-backed context.
- [ ] Request-shape tests prove OpenAI roles and ordering survive serialization.
- [ ] Two consecutive integration requests with the same eligible prefix report cached-token usage when OpenAI and the selected model expose it.
- [ ] Changing only the final user message leaves the reusable prefix hash unchanged.
- [ ] Changing canonical project context intentionally changes the reusable prefix hash.
- [ ] Diagnostics never claim a cache hit based only on lower latency.
- [ ] Existing OpenAI fallback, authentication, and model-validation behavior remains covered.

### Success measures

- Repeated project-aware requests maximize the provider-reported cached portion of input tokens.
- Cache-hit ratio is visible during testing and can be compared by provider and model.
- Time-to-first-token and input-token cost improve for repeat requests without stale context or changed model behavior.
- Prompt assembly remains deterministic and testable without requiring a live API key.

### Non-goals

- Building or storing local KV-cache tensors.
- Claiming that every OpenAI model, endpoint, or request will receive a cache hit.
- Reusing stale session notes or hypotheses merely to increase the cached-token count.
- Hiding required context from the model to optimize cost.

## Goal 2: Replace descriptive primers with dense structural context

Primer documents should optimize semantic content per token rather than read like conversational onboarding material. The canonical AI context should favor explicit mathematical definitions, typed relationships, constraints, provenance, and compact identifiers over repeated prose.

### Structural context contract

The primer compiler should emit deterministic sections in a machine-oriented format:

1. Model identity and scope
2. Symbol table with units, domains, and types
3. Axioms and assumptions
4. Mathematical definitions and governing equations
5. Typed relation graph
6. Initial and boundary conditions
7. Observables, datasets, and mappings
8. Experiments, predictions, and falsification criteria
9. Unresolved contradictions and open questions
10. Source references using project-relative paths and stable section IDs

The relation graph should represent nodes and typed edges such as `defines`, `depends_on`, `constrains`, `predicts`, `measured_by`, `contradicts`, and `derived_from`. Equations should remain in canonical LaTeX with every symbol defined once and referenced by a stable ID.

### Implementation deliverables

- [x] Define a versioned structural-context schema that is theory-agnostic and serializes deterministically.
- [ ] Compile the existing master axiom, project awareness, tools, experiments, and manuscript headings into that schema.
- [ ] Replace greetings, workflow narration, repeated caveats, and conversational transitions with concise directives or typed fields.
- [x] Deduplicate compiled records and assign stable IDs to symbols, axioms, equations, graph nodes, and sources.
- [ ] Preserve mathematical notation, units, domains, assumptions, boundary conditions, and derivation links during compression.
- [ ] Encode model relationships as typed nodes and edges rather than paragraphs that restate the same links.
- [x] Keep a human-readable inspector in the UI even when the canonical context artifact is machine-oriented.
- [ ] Emit a compact stable core for every request and allow retrieved extensions to attach by stable ID.
- [ ] Measure tokens before and after compilation using the tokenizer appropriate to the selected provider/model when available.
- [x] Reject lossy compression when required record IDs or provenance disappear, and require retrieval plus human equivalence approval before prompt activation.

### Verification

- [ ] Schema validation catches missing IDs, undefined symbols, dangling graph edges, duplicate definitions, and invalid source references.
- [x] Golden-file tests prove identical project input produces byte-identical structural context.
- [x] Coverage tests prove each currently scanned source axiom, equation, and declared assumption maps to at least one structural record.
- [ ] Retrieval probes answer the same benchmark questions with fewer input tokens than the v8 prose primer.
- [ ] Human review confirms equations and qualified claims retain their original meaning after compilation.
- [ ] Token reduction is reported alongside semantic coverage; token count alone is never treated as success.

### Non-goals

- Replacing precise explanations with opaque abbreviations that the model cannot resolve.
- Removing source provenance, uncertainty, units, assumptions, or boundary conditions.
- Forcing every theory into one physics ontology beyond the shared typed primitives needed for retrieval.
- Making the machine-oriented artifact the only way a user can inspect project context.

## Goal 3: Move corpus complexity into local vector retrieval

The full manuscript should be indexed locally instead of copied into every primer. A project-scoped vector database inside the Tauri application should retrieve only the sections relevant to the current question, hypothesis, equation, or experiment.

Embeddings are numeric search representations, not a substitute prompt language. Float arrays should remain local and should not be rendered into an OpenAI or Gemini prompt. The model should receive the compact equations, graph records, and source-grounded text selected by local similarity search.

### Retrieval architecture

1. Parse manuscript and axiom sources by semantic boundaries such as headings, definitions, equations, proofs, experiments, and citations.
2. Normalize each chunk into the Goal 2 structural schema while preserving its exact source location.
3. Generate embeddings with a bundled or explicitly configured local embedding model.
4. Store embeddings and metadata in a project-scoped SQLite index using a maintained vector-search extension such as `sqlite-vec` or a validated `sqlite-vss` integration.
5. Embed the current query locally and run hybrid retrieval using vector similarity plus SQLite full-text search for exact symbols, equation IDs, and terminology.
6. Rerank, deduplicate, and expand selected graph neighbors within a strict token budget.
7. Append only the selected semantic records and source references to the dynamic portion of the provider prompt.

### Implementation deliverables

- [ ] Select and document the SQLite vector extension, Rust integration, license, supported Linux targets, and packaging strategy.
- [ ] Select a local embedding model with documented dimensions, model version, license, hardware requirements, and offline behavior.
- [ ] Store source path, stable section ID, heading ancestry, content hash, embedding model/version, vector dimensions, and modification time with every record.
- [ ] Add incremental indexing so unchanged chunks retain their vectors and changed or deleted chunks update transactionally.
- [ ] Combine vector similarity with FTS5 or equivalent lexical search so exact mathematical symbols and rare terms are not lost.
- [ ] Add graph-neighborhood expansion for directly related axioms, definitions, equations, experiments, and contradictions.
- [ ] Enforce configurable retrieval and token budgets before provider dispatch.
- [ ] Show retrieved sources, relevance scores, index freshness, and embedding model in diagnostics.
- [ ] Provide `Build index`, `Refresh changed`, `Rebuild`, `Inspect`, and `Delete local index` controls.
- [ ] Keep index files local by default and exclude them from Git, manuscript export, and provider payloads.
- [ ] Fall back safely to lexical retrieval or a compact core primer when the vector extension/model is unavailable.
- [ ] Stop sending the full master manuscript or axiom file by default once retrieval quality passes the release benchmark.

### Verification

- [ ] Indexing and retrieval work without network access after required local assets are installed.
- [ ] Reopening a project reuses a valid index without recomputing unchanged embeddings.
- [ ] Editing, renaming, or deleting a source invalidates only affected records.
- [ ] Retrieval benchmarks cover equations, aliases, cross-chapter relations, experiments, contradictions, and uncommon symbols.
- [ ] Hybrid retrieval outperforms vector-only and lexical-only baselines on the v9 benchmark set.
- [ ] Every retrieved record resolves to an existing project-relative source and stable section ID.
- [ ] No embedding vectors, index files, or unrelated manuscript chunks appear in provider request captures.
- [ ] Corrupt, incompatible, or stale indexes are detected and rebuilt without damaging source files.
- [ ] Packaged Linux builds load the selected vector extension on every supported architecture.

### Success measures

- Provider prompts no longer scale linearly with manuscript length.
- Benchmark answers retain source coverage while using a bounded retrieval context.
- Index refresh time scales with changed content rather than the full corpus.
- Retrieval diagnostics make missing or weak evidence visible instead of silently inventing context.

### Non-goals

- Sending raw embedding arrays to an LLM as compressed manuscript content.
- Treating vector similarity as proof that a retrieved claim is correct.
- Replacing canonical source files with an opaque database.
- Requiring cloud embedding or vector-database services for normal operation.

## Goal 4: Audit and streamline project-awareness calibration

Review the complete path by which an AI lane becomes aware of a project after the user grants read-only or read/write access. Establish a lower-token context standard before adding more awareness material, then make every included source and transformation visible enough to audit.

The review must cover:

`project root + access mode -> visible file scope -> awareness/index sources -> primer compilation -> retrieval -> thread history -> provider payload -> usage telemetry`

Granting project access should establish permission and retrieval scope; it should not automatically dump the project corpus into a prompt. Read/write mode should add mutation capability, not broader prompt context than read-only mode.

### Token-budget standard

- [ ] Establish separate configurable budgets for stable primer, retrieved project context, conversation history, attachments, current request, and reserved output.
- [ ] Define a v8 benchmark corpus and record the current startup-primer and common-workflow token baselines before changing behavior.
- [ ] Set v9 acceptance ceilings from benchmark evidence rather than an arbitrary compression percentage.
- [ ] Block or require explicit confirmation when a request exceeds its context budget.
- [ ] Prefer omission, local retrieval, structural references, and user-selected scope over silent truncation.
- [ ] Report what was included, excluded, summarized, retrieved, or truncated for each request.
- [ ] Apply the same budgeting rules to read-only and read/write modes; permissions must not alter token accounting.

### Thread context-load meter

The app should provide useful token information even when exact preflight counts are unavailable:

- [ ] Show a model-aware preflight token estimate for each lane using a compatible local tokenizer when available.
- [ ] Fall back to a labeled byte/character approximation with a documented error range when no compatible tokenizer exists.
- [ ] Parse exact input, cached input, reasoning, and output usage from provider responses when those fields are supplied.
- [ ] Show per-request and cumulative thread totals, remaining configured budget, and the contribution of primer, retrieval, history, attachments, and current input.
- [ ] Clearly distinguish `estimated`, `provider reported`, and `unavailable` values.
- [ ] Replace the undefined entropy-meter concept with a context-load meter for release scope; evaluate semantic diversity or Shannon-entropy diagnostics separately only if they produce an actionable user decision.

### Compact visible-tree context

The AI-facing workspace tree should reflect user-selected visible scope and use a deterministic, token-light representation. Expanding a directory in the viewer may make its loaded descendants eligible for the next tree export, but it must not automatically send them to an AI lane.

Preferred representation:

```text
@tree-v1
src/
   main.js
   ai-config.js
src-tauri/
   src/
      lib.rs
```

- [ ] Replace repeated full relative paths and Markdown list markers with one hierarchical path representation.
- [ ] Export only the root and currently expanded/loaded descendants unless the user explicitly requests a full-project map.
- [ ] Exclude ignored, hidden, generated, binary, vector-index, and build-output paths by default using documented rules.
- [ ] Preview estimated tree tokens and included node count before syncing or exporting context.
- [ ] Preserve deterministic sorting so an unchanged visible tree produces an unchanged prompt prefix.
- [ ] Keep full recursive export as an explicit separate action with a cost warning, not the default briefing behavior.
- [ ] Ensure collapsing a branch removes its descendants from the next visible-tree export without deleting files or local index records.

### Lazy directory cost heatmap

Visible directory rows should help the user judge the likely cost of exposing their contents. Estimates should be computed only for directory rows currently visible in the file-tree viewport, in a cancellable background task, and cached until relevant filesystem metadata changes.

Cost estimates should use eligible text-file byte totals and file counts without reading file contents. Convert bytes to approximate tokens using a configurable, documented bytes-per-token factor. Binary files and excluded paths should not inflate the text-token estimate, but their excluded size/count should remain inspectable.

Color states:

- `white`: scanned and empty;
- `green`: low estimated token cost and low file count;
- `yellow`: moderate estimated cost or high count of individually small files;
- `red`: high estimated token cost or a configured high-risk threshold;
- `neutral/gray`: not measured, measuring, stale, inaccessible, cancelled, or failed.

- [ ] Define thresholds in estimated tokens and file count, with defaults derived from the v9 token-budget standard.
- [ ] Add a legend and tooltip showing state, estimated tokens, eligible bytes, file count, exclusions, scan time, and approximation method.
- [ ] Never use green for unknown, failed, partial, permission-denied, or timed-out scans.
- [ ] Do not estimate individual file tokens during normal project import or initial tree rendering.
- [ ] Start one bounded aggregate metadata walk only when a directory row enters the viewport; that walk may traverse its subtree to compute the directory total.
- [ ] Do not enqueue independent estimates for collapsed or off-screen descendant rows; cancel queued work when a visible row leaves the viewport and limit scan concurrency.
- [ ] Respect `.gitignore`, app exclusions, symlink boundaries, project-root access controls, and a configurable maximum scan depth/work limit.
- [ ] Cache aggregate metadata by path and filesystem fingerprint so revisiting a row is fast.
- [ ] Keep heatmap color separate from selection, Git status, errors, and file type so meaning remains accessible without color alone.

### End-to-end review deliverables

- [ ] Document every awareness source, owner, refresh trigger, size limit, volatility class, prompt position, and access-mode dependency.
- [ ] Remove duplicate paths where the same manuscript, axiom, tree, recap, or summary reaches a request more than once.
- [ ] Replace automatic recursive workspace-tree generation during briefing compilation with visible-tree scope or local retrieval.
- [x] Inventory every manual primer/context control and classify it as `retain`, `automate`, `merge`, `deprecate`, or `remove`.
- [ ] Separate permission state, index state, visible-tree state, primer state, and thread state in diagnostics.
- [ ] Add a request inspector that shows ordered sections, estimated tokens, provenance, and exclusions without exposing API keys.
- [ ] Verify that changing read-only to read/write does not rebuild awareness context unless project content or selected scope changed.
- [ ] Define freshness and invalidation rules for the primer, visible tree, structural context, and vector index.

### Verification

- [ ] End-to-end tests trace project load through the exact provider request for both read-only and read/write modes.
- [ ] Request snapshots prove each awareness source appears at most once and in the Goal 1 volatility order.
- [ ] Opening a project performs no recursive token-cost scan and sends no project content to a provider.
- [ ] Only visible directory rows schedule heatmap work; scrolling or collapsing cancels irrelevant queued scans.
- [ ] Visible-tree exports contain only expanded/loaded scope and remain byte-identical when that scope is unchanged.
- [ ] Heatmap tests cover empty, low-cost, many-small-file, high-cost, ignored, inaccessible, symlinked, stale, cancelled, and partial directories.
- [ ] Performance tests verify initial project rendering remains responsive on the v9 large-project fixture.
- [ ] Estimated token totals are compared with provider-reported counts across supported models and display their observed error bounds.
- [ ] Token-budget regression tests fail when a benchmark workflow exceeds its approved ceiling without an intentional baseline update.

### Success measures

- Startup and routine prompts meet the v9 token ceilings while preserving benchmark awareness accuracy.
- The user can identify expensive visible branches before adding them to AI context.
- Project opening remains immediate because cost scans and indexing are deferred, bounded, and observable.
- Every provider request can explain which project material it used and why.

### Current migration targets

- Sprint 1 stopped recursive tree regeneration during briefing compilation and added compact visible-tree export; existing verbose tree files remain until refreshed.
- Primer assembly still has several manual entry points and needs one authoritative automated awareness path.
- `read_directory` and the file-tree viewer already load child directories lazily, providing the correct trigger boundary for visible-row estimates.
- Preflight prompt estimates are available, but OpenAI response usage is still discarded, so exact post-request thread accounting is not yet available.

### Manual context-tool consolidation gate

Automated awareness should become the authoritative path when it can produce fresher, smaller, source-grounded context than manual primer generation. Manual controls must not remain merely for compatibility if they resend stale data, duplicate automatic context, bypass token budgets, or contradict retrieval and cache ordering.

Current controls requiring review include:

- `View/Edit Primer`, `Save Packet`, and `Sync To AI Threads`;
- `Sync AI Context Now` and pane reset-to-primer behavior;
- `Pass Into Primer` and idea-pad `Sync AI Context`;
- `Export AI Project Map`;
- carry-over notes, exit primer preview, and briefing regeneration actions.

Decision rules:

- `retain`: the control provides explicit scope selection, audit, recovery, or a meaningful human override that automation cannot safely infer;
- `automate`: the action should occur from project/index freshness rules without user intervention;
- `merge`: multiple controls express one intent and should become one action or inspector;
- `deprecate`: automation supersedes the control, but users need a transition period and migration notice;
- `remove`: the control is redundant, contradictory, unsafe, or permanently bypasses the canonical awareness pipeline.

Sprint 4 review snapshot (proposal; no controls removed yet):

| Control | Classification | Proposed direction |
| --- | --- | --- |
| `AI Request Inspector` | `retain` | Authoritative audit, provenance, token, cache, and structural benchmark surface. |
| Pane reset-to-primer | `retain` | Recovery action; rebuild through the canonical assembler. |
| `Export AI Project Map` | `retain` | Explicit human scope selection, never automatic provider transmission. |
| Carry-over notes | `retain` | Unique human-authored dynamic context; keep outside the reusable prefix. |
| `Sync AI Context Now` / briefing refresh | `merge` | One refresh-awareness action; replace the baseline context slot instead of appending duplicates. |
| `View/Edit Primer`, `Save Packet`, `Sync To AI Threads` | `deprecate` | Replace with read-only awareness inspection plus an explicit dynamic override after structural equivalence passes. |
| `Pass Into Primer` / idea-pad sync | `deprecate` | Preserve notes as dynamic session context rather than mutating stable primer content. |

The canonical assembler and frontend now enforce a single `baseline_context` slot, so repeated refresh or manual override actions replace the prior baseline instead of stacking duplicate primers. Final deprecations remain blocked on structural retrieval benchmarks, human review, and recovery/export for unique notes.

- [ ] Compare each manual action with the canonical assembler and local retrieval pipeline after Sprints 2, 4, and 5.
- [ ] Measure whether each action improves benchmark awareness enough to justify its token and UX cost.
- [ ] Require all retained manual overrides to pass through the same provenance, ordering, deduplication, and token-budget checks as automation.
- [ ] Never let editing a generated primer silently mutate canonical source files, structural records, or vector-index content.
- [ ] Present a migration proposal for discussion before removing user-facing controls or project artifacts.
- [ ] Provide a recovery/export path for user-authored notes before deprecating any tool that stores unique human content.
- [ ] Remove obsolete primer terminology when the remaining surface is more accurately an awareness inspector, scope selector, or context override.

### Non-goals

- Reading and tokenizing every project file during import merely to color the tree.
- Treating directory color as a relevance score or automatically granting AI access to a branch.
- Claiming byte-derived estimates are exact model token counts or billing totals.
- Sending project content because a directory was expanded, measured, or colored.
- Using an unexplained entropy number as a proxy for awareness quality.

## Recommended execution roadmap

### Sprint 1: Establish the baseline and remove obvious waste

- [x] Capture prompt-section byte and estimated-token baselines for representative v8 workflows.
- [x] Serialize the visible workspace tree in compact deterministic form.
- [x] Stop recursive full-project tree generation during normal briefing compilation.
- [x] Add prompt-source provenance and identify duplicate awareness content.
- [x] Add regression fixtures and approved token ceilings for benchmark workflows.

### Sprint 2: Make prompt assembly deterministic and measurable

- [x] Introduce one canonical prompt assembler shared by both AI lanes.
- [x] Separate stable, slowly changing, thread, and current-request sections.
- [x] Add the request inspector and preflight context-load estimate.
- [x] Parse provider-reported usage and maintain per-thread totals.
- [x] Add stable-prefix and request-order snapshot tests.

### Sprint 3: Activate native prompt-caching benefits

- [x] Preserve provider message roles instead of flattening OpenAI history.
- [x] Keep dynamic values below the reusable prefix boundary.
- [x] Record cached input tokens and cache-hit ratio where OpenAI supplies them.
- [x] Add an explicit two-request cache probe with prefix eligibility, fingerprint, latency, model, and usage reporting.
- [x] Run repeated-request cache integration probes across the supported default and pinned OpenAI model matrix.

#### Live validation results

Validated on 2026-08-15 with stable-prefix fingerprint `090c35db223bc481205f6b8f539373ebcc07ee3aa3ab89a422705ffe5c678ccb`, local estimate 1,446 tokens, and provider-reported input of 1,264 tokens:

| Model | Status | Warm latency | Probe latency | Cached tokens | Cache ratio | Observed latency reduction |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `gpt-4.1` | `CACHE_HIT` | 1,606 ms | 999 ms | 1,024 | 81.0% | 37.8% |
| `gpt-4.1-mini` | `CACHE_HIT` | 1,030 ms | 664 ms | 1,024 | 81.0% | 35.5% |
| `gpt-4o-2024-08-06` | `CACHE_HIT` | 708 ms | 628 ms | 1,024 | 81.0% | 11.3% |

All tested models validate the canonical prefix ordering and native OpenAI cache path, including both configured `gpt-4.1` defaults. The local estimator was +14.4% above provider-reported input for this shared payload. Latency is observational and must not be used alone to claim a cache hit; the provider-reported 1,024 cached tokens are the authoritative evidence. The supported release matrix is accepted as validated; additional optional models can be measured without reopening Sprint 3.

### Sprint 4: Compile structural project context

- [x] Define the versioned symbol, axiom, equation, relation, and provenance schema.
- [ ] Compile existing awareness sources into deterministic structural records.
- [x] Add initial scanned-candidate coverage, schema-validation, and golden serialization tests.
- [x] Add a blind, same-question legacy-vs-structural A/B probe with bounded exact-model requests and provider usage metrics.
- [ ] Record at least three adequate human judgments with no `structural_worse` result for each approved artifact fingerprint/model pair.
- [ ] Replace duplicated prose primer sections after benchmark equivalence is established.
- [x] Run the first manual context-tool consolidation review against the canonical assembler.

#### Structural A/B approval gate

The Request Inspector can run a blind comparison between the current legacy lane primer and the benchmark-eligible structural core. It sends up to two bounded OpenAI requests only after confirmation, uses the same model and question, and reveals the variant mapping only after judgment.

Judgments are stored locally as metadata only: artifact fingerprint, model, outcome, latency, and provider usage. Questions and model responses are not persisted. Approval requires at least three `equivalent` or `structural_preferred` outcomes for the same fingerprint/model and zero `structural_worse` outcomes. A single `structural_worse` judgment marks that pair rejected. Approval remains advisory; compact mode must not activate automatically.

Coverage-complete cores that are larger than the truncated legacy excerpt may run as `diagnostic-only` comparisons after an explicit cost warning. Their judgments are recorded for compiler analysis but do not count toward compact-mode approval. Cores with incomplete semantic coverage remain blocked with zero network requests. This distinction is expected for large projects until Sprint 5 retrieval selects a bounded structural subset instead of transporting the full compiled corpus.

#### BMI diagnostic result

The first BMI diagnostic comparison asked how neutrino mass is determined in BMI theory. The human evaluator judged both responses adequate but preferred the legacy response:

- outcome: `structural_worse`;
- variant A: structural;
- variant B: legacy;
- structural core: larger than the truncated legacy excerpt.

This result rejects full-corpus structural-core activation for the tested BMI artifact. Stable IDs and typed records remain useful as a local index format, but the complete core should not replace the primer. Sprint 5 must retrieve a bounded, query-relevant structural subset and preserve enough nearby explanatory text to answer theory-specific mechanism questions before another approval-eligible A/B suite is attempted.

### Sprint 5: Add local retrieval

- [ ] Select and package the local embedding model and SQLite vector extension.
- [ ] Implement incremental structural chunk indexing.
- [ ] Add hybrid vector, lexical, and graph-neighbor retrieval.
- [ ] Enforce retrieval budgets and expose source diagnostics.
- [ ] Preserve explanatory neighbor text around retrieved equations and mechanisms; the BMI diagnostic showed that isolated structural records can underperform concise prose context.
- [ ] Stop transporting full manuscripts after retrieval benchmarks pass.
- [ ] Reclassify manual primer controls after automated retrieval benchmarks pass.

### Sprint 6: Add directory cost guidance

- [ ] Derive heatmap thresholds from measured token budgets.
- [ ] Implement bounded visible-row metadata scans, cancellation, and caching.
- [ ] Add accessible white, green, yellow, red, and neutral states.
- [ ] Validate responsiveness and estimate accuracy on the large-project fixture.

### Sprint 7: Complete the release audit

- [ ] Run the complete awareness pipeline review in read-only and read/write modes.
- [ ] Complete the agreed manual-tool migrations, deprecations, and removals.
- [ ] Verify permissions do not silently change prompt scope or token use.
- [ ] Complete Linux packaging, upgrade, provider, privacy, and regression gates.
- [ ] Publish supported-model measurements and known limitations.

## Official-release gates

These gates apply to every v9 feature track:

- [ ] No release-blocking regression in project import, briefing generation, dual AI lanes, manuscript tools, or file-access controls.
- [ ] Production Linux package installs, launches, upgrades, and uninstalls cleanly on the supported Ubuntu baseline.
- [ ] User-facing diagnostics explain provider failures without exposing API keys or sensitive prompt content.
- [ ] Release documentation identifies supported providers, tested models, known limitations, and upgrade steps from v8.
- [ ] No manual context tool can bypass the canonical prompt assembler, provenance report, or token budget.

## Scope status

Goals 1-4 are defined and Sprints 1-3 are complete. Sprint 4 now generates deterministic `physics-ide.structural-context/v1`, renders a compact core, verifies ID/provenance coverage, measures it against the legacy excerpt budget, and provides blind A/B human evaluation. Eligible cores remain disabled until the per-fingerprint/model human gate passes and results are reviewed. The first manual-tool classification is documented, and duplicate baseline contexts are prevented; no controls have been removed. Broader awareness and experiment compilation remain open. Additional v9 product goals should be added as separate tracks without weakening the prompt-ordering contract, structural-context integrity, local-first retrieval boundary, token-budget standard, or official-release gates above.
