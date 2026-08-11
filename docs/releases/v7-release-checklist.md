# v7.0.0 Release Checklist

Current baseline: v6 stream
Target milestone: v7.0.0

This checklist defines the final release gates for v7.0.0.

## Scope locked for v7

- AI Testing moved into dedicated top-menu workflow.
- Context probe improved with semantic evidence grounding.
- Reusable per-theory probe suites added.
- Batch/suite report generation and Open Report action added.
- OpenAI model fallback behavior hardened around access errors.

## Release gates

### 1) Functional stability

- [x] Core desktop workflows still execute after v7 UI and AI changes.
- [x] Context probe runs complete with report output.
- [x] Probe suite save/load/run flows are wired and usable.

Evidence:
- Test and run updates were validated during v7 implementation cycles.

### 2) Automated test baseline

- [x] Frontend test command passes: `node --test src/ai-config.test.js`.
- [x] Rust backend tests pass: `cargo test`.

Evidence:
- Prior test runs during v7 implementation reported green status.

### 3) Packaging smoke checks

- [x] Linux package smoke test (user-confirmed pass on current .deb build, 2026-08-10).
- [ ] Windows package smoke test (blocked in this environment).
- [ ] macOS package smoke test (blocked in this environment).

Evidence:
- Active Linux build log path: `/tmp/v7_tauri_build.log`.
- User sign-off recorded: latest local .deb install and smoke test passed on 2026-08-10.
- Windows/macOS checks require platform-native validation or CI runners.

### 4) Release documentation

- [x] v7 release checklist created.
- [x] v7 release notes drafted.
- [x] Final packaging outcomes copied into checklist + notes.

## Tagging rule

- Linux smoke requirement is satisfied.
- Windows/macOS remain unexecuted in this environment; treat as accepted platform exceptions unless separate validation is required.