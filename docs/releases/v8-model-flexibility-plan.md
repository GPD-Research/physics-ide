# v8.0.0 Model Flexibility Plan

Current baseline: v7 stream (post cloud-first provider transition)
Target milestone: v8.0.0

## Why v8

v7 established stable cloud-provider behavior and strong context probe reliability.
v8 should focus on user-controlled model freedom while preserving reliability and testability.

## v7 status snapshot

### Completed in v7 stream

- [x] OpenAI/Gemini cloud-first provider path is active in UI and backend runtime.
- [x] Legacy Ollama runtime/backend path removed.
- [x] AI Testing moved into dedicated top-menu workflow.
- [x] Context probe upgraded from filename-only to semantic content evidence grounding.
- [x] Probe suite storage and reusable per-theory test suite execution added.
- [x] Batch/suite report generation and in-tool Open Report action added.
- [x] Context probe reliability reached 16/16 in user validation run.

### Remaining to formalize before v7 tag

- [ ] Write explicit v7 release checklist and tagging gates (similar to v6 checklist).
- [ ] Cross-platform packaging smoke checks (Linux/Windows/macOS) recorded for this feature set.
- [ ] Final release notes for v7 feature deltas.

## v8 scope: user-defined OpenAI/Gemini models

### Goal

Allow users to enter any model ID for OpenAI or Gemini (as enabled in their provider project settings), validate quickly, and benchmark in AI Testing without code edits.

### Core requirements

- [ ] Add advanced model input fields (freeform) per pane for OpenAI and Gemini.
- [ ] Keep presets, but allow custom override IDs.
- [ ] Add Validate Model action per pane/provider.
- [ ] Persist custom model IDs in user settings.
- [ ] Use custom IDs directly in runtime dispatch.
- [ ] Keep safe fallback behavior when custom model fails.

### Validation and safety

- [ ] Clear error messages for unsupported model/endpoint combinations.
- [ ] Keep current stable defaults available as one-click reset.
- [ ] Add a lightweight compatibility check before full probe/batch runs.
- [ ] Ensure probe suite metrics are keyed by actual custom model ID.

### AI Testing integration

- [ ] Add "Run probe against current custom models" action (no extra setup).
- [ ] Add suite metadata showing provider/model IDs used in each run.
- [ ] Include model IDs in summary report headers.

### Compatibility notes

- [ ] Document endpoint caveat: some legacy model families may not support current chat request shape.
- [ ] For unsupported families, provide direct next-step guidance (switch model or endpoint mode if/when supported).

## Suggested v8 implementation phases

### Phase 1: UI + persistence

- Add custom model fields and save/load behavior.
- No backend endpoint changes yet.

### Phase 2: runtime + validation

- Validate custom IDs and dispatch with robust fallback/errors.
- Ensure provider normalization remains safe.

### Phase 3: testing/reporting hardening

- Add model metadata into AI Testing outputs and suite runs.
- Add release-gate checklist for model flexibility.

## Exit criteria for v8

- [ ] User can run custom OpenAI model ID without code changes.
- [ ] User can run custom Gemini model ID without code changes.
- [ ] Validation errors are actionable and specific.
- [ ] AI Testing suites preserve and report exact model IDs.
- [ ] Stability remains at or above current probe quality baseline.
