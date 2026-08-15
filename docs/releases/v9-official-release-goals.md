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

- [ ] Replace the flattened single-user-message OpenAI payload with an ordered, structured message payload.
- [ ] Create one canonical prompt assembler shared by both AI panes so equivalent context serializes identically.
- [ ] Separate stable primer content from session summary, hypothesis cues, idea-pad notes, timestamps, and transient diagnostics.
- [ ] Keep static content, whitespace, delimiters, and section order deterministic across requests.
- [ ] Append current-thread and current-request content only after all reusable project context.
- [ ] Preserve conversation roles instead of embedding role labels into one text block.
- [ ] Avoid cache-hostile values in the prefix, including generated timestamps, absolute paths that vary by machine, random identifiers, and non-deterministic file ordering.
- [ ] Parse OpenAI usage metadata, including cached input token counts when supplied by the selected model and endpoint.
- [ ] Surface input tokens, cached input tokens, output tokens, cache-hit ratio, provider, and model in AI diagnostics and probe reports.
- [ ] Treat absent cached-token metadata as `unavailable`, not as a zero-token cache hit.
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

- [ ] Define a versioned structural-context schema that is theory-agnostic and serializes deterministically.
- [ ] Compile the existing master axiom, project awareness, tools, experiments, and manuscript headings into that schema.
- [ ] Replace greetings, workflow narration, repeated caveats, and conversational transitions with concise directives or typed fields.
- [ ] Deduplicate definitions and assign stable IDs to symbols, axioms, equations, graph nodes, and sources.
- [ ] Preserve mathematical notation, units, domains, assumptions, boundary conditions, and derivation links during compression.
- [ ] Encode model relationships as typed nodes and edges rather than paragraphs that restate the same links.
- [ ] Keep a human-readable inspector in the UI even when the canonical context artifact is machine-oriented.
- [ ] Emit a compact stable core for every request and allow retrieved extensions to attach by stable ID.
- [ ] Measure tokens before and after compilation using the tokenizer appropriate to the selected provider/model when available.
- [ ] Reject lossy compression when required definitions, provenance, or falsification criteria disappear.

### Verification

- [ ] Schema validation catches missing IDs, undefined symbols, dangling graph edges, duplicate definitions, and invalid source references.
- [ ] Golden-file tests prove identical project input produces byte-identical structural context.
- [ ] Coverage tests prove each source axiom, equation, and declared assumption maps to at least one structural record.
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

## Official-release gates

These gates apply to every v9 feature track:

- [ ] No release-blocking regression in project import, briefing generation, dual AI lanes, manuscript tools, or file-access controls.
- [ ] Production Linux package installs, launches, upgrades, and uninstalls cleanly on the supported Ubuntu baseline.
- [ ] User-facing diagnostics explain provider failures without exposing API keys or sensitive prompt content.
- [ ] Release documentation identifies supported providers, tested models, known limitations, and upgrade steps from v8.

## Scope status

Goals 1-3 are defined and ready for implementation planning. Additional v9 product goals should be added as separate tracks without weakening the prompt-ordering contract, structural-context integrity, local-first retrieval boundary, or official-release gates above.
