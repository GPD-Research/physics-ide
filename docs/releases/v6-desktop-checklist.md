# v6.0.0 Desktop Stabilization Checklist

Current iteration baseline: v5.2.1
Target milestone: v6.0.0

This checklist defines release gates for desktop readiness. Every item must be complete before tagging v6.0.0.

Validation status: Completed and user-validated in live desktop retest on 2026-07-25.

## Version cadence (v5.2.x -> v6.0.0)

- v5.2.1: Baseline stabilization start.
- v5.2.2: 100% UI control wiring map complete and verified.
- v5.2.3: Threaded AI reliability pass across configured providers.
- v5.2.4: Workspace and settings persistence pass from clean state.
- v5.2.5: Packaging smoke tests pass on desktop targets.
- v6.0.0: All release gates green.

## Release gates

### 1) UI wiring completeness

- [x] Every visible control in index.html calls a real function.
- [x] No placeholder handlers (alert-only, TODO stubs, no-op callbacks).
- [x] Every control reports success/failure in UI log or status region.

Evidence:
- Add/update control mapping table in docs/releases/v6-control-map.md.

### 2) Frontend/backend command integrity

- [x] Each invoke(...) action maps to a registered Tauri command.
- [x] Payload keys match backend command signatures.
- [x] Error responses are handled and shown to user.

Evidence:
- Command map reviewed and marked complete.

### 3) AI thread reliability

- [x] Left and right pane threads dispatch and render responses.
- [x] Provider + model selection is honored.
- [x] Missing key/model errors are specific and actionable.
- [x] Gemini/OpenAI/Ollama paths tested for configured providers.

Evidence:
- Manual script run with pass/fail notes.

### 4) Workspace and file operations

- [x] Load workspace, browse tree, open editor, and save operations work.
- [x] Import theory source workflow succeeds with expected files.
- [x] Master axiom generation succeeds (local fallback and AI path).
- [x] Save/restore version actions function from UI.

Evidence:
- Smoke test results captured in release notes.

### 5) Settings persistence and startup defaults

- [x] Settings survive app restart.
- [x] Unset root paths fall back to HOME-based defaults.
- [x] Dialog default paths never fall back to src-tauri working directory.

Evidence:
- Cold-start test from clean config state documented.

### 6) Error handling quality

- [x] No silent failures for core workflows.
- [x] User-facing errors include root cause context.
- [x] Recoverable failures provide clear next step.

### 7) Desktop UX stability

- [x] Layout and resizing are stable at common desktop resolutions.
- [x] Modal/tab/pane interactions do not trap focus or break scrolling.
- [x] Keyboard input behavior is consistent across major flows.

### 8) Packaging readiness

- [x] Linux package smoke test.
- [x] Windows package smoke test.
- [x] macOS package smoke test.

Note: If one platform is unavailable in CI/dev, record remaining platform checks as release blockers.

## Tagging rule

- Do not tag v6.0.0 until every checkbox is complete and validated.
- Patch versions (v5.2.x) should be used to mark completed stabilization milestones.
