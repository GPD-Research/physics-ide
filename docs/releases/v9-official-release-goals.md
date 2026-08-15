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
   - master axiom or canonical theory context;
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

## Official-release gates

These gates apply to every v9 feature track:

- [ ] No release-blocking regression in project import, briefing generation, dual AI lanes, manuscript tools, or file-access controls.
- [ ] Production Linux package installs, launches, upgrades, and uninstalls cleanly on the supported Ubuntu baseline.
- [ ] User-facing diagnostics explain provider failures without exposing API keys or sensitive prompt content.
- [ ] Release documentation identifies supported providers, tested models, known limitations, and upgrade steps from v8.

## Scope status

Goal 1 is defined and ready for implementation planning. Additional v9 product goals should be added as separate tracks without weakening the prompt-ordering contract or the official-release gates above.