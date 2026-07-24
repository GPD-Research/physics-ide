# v6.0.0 Desktop Stabilization Checklist

Current iteration baseline: v5.2.1
Target milestone: v6.0.0

This checklist defines release gates for desktop readiness. Every item must be complete before tagging v6.0.0.

## Version cadence (v5.2.x -> v6.0.0)

- v5.2.1: Baseline stabilization start.
- v5.2.2: 100% UI control wiring map complete and verified.
- v5.2.3: Threaded AI reliability pass across configured providers.
- v5.2.4: Workspace and settings persistence pass from clean state.
- v5.2.5: Packaging smoke tests pass on desktop targets.
- v6.0.0: All release gates green.

## Release gates

### 1) UI wiring completeness

- [ ] Every visible control in index.html calls a real function.
- [ ] No placeholder handlers (alert-only, TODO stubs, no-op callbacks).
- [ ] Every control reports success/failure in UI log or status region.

Evidence:
- Add/update control mapping table in docs/releases/v6-control-map.md.

### 2) Frontend/backend command integrity

- [ ] Each invoke(...) action maps to a registered Tauri command.
- [ ] Payload keys match backend command signatures.
- [ ] Error responses are handled and shown to user.

Evidence:
- Command map reviewed and marked complete.

### 3) AI thread reliability

- [ ] Left and right pane threads dispatch and render responses.
- [ ] Provider + model selection is honored.
- [ ] Missing key/model errors are specific and actionable.
- [ ] Gemini/OpenAI/Ollama paths tested for configured providers.

Evidence:
- Manual script run with pass/fail notes.

### 4) Workspace and file operations

- [ ] Load workspace, browse tree, open editor, and save operations work.
- [ ] Import theory source workflow succeeds with expected files.
- [ ] Master axiom generation succeeds (local fallback and AI path).
- [ ] Save/restore version actions function from UI.

Evidence:
- Smoke test results captured in release notes.

### 5) Settings persistence and startup defaults

- [ ] Settings survive app restart.
- [ ] Unset root paths fall back to HOME-based defaults.
- [ ] Dialog default paths never fall back to src-tauri working directory.

Evidence:
- Cold-start test from clean config state documented.

### 6) Error handling quality

- [ ] No silent failures for core workflows.
- [ ] User-facing errors include root cause context.
- [ ] Recoverable failures provide clear next step.

### 7) Desktop UX stability

- [ ] Layout and resizing are stable at common desktop resolutions.
- [ ] Modal/tab/pane interactions do not trap focus or break scrolling.
- [ ] Keyboard input behavior is consistent across major flows.

### 8) Packaging readiness

- [ ] Linux package smoke test.
- [ ] Windows package smoke test.
- [ ] macOS package smoke test.

Note: If one platform is unavailable in CI/dev, record remaining platform checks as release blockers.

## Tagging rule

- Do not tag v6.0.0 until every checkbox is complete and validated.
- Patch versions (v5.2.x) should be used to mark completed stabilization milestones.
