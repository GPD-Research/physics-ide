# v9.0.0 Release Notes

First public Linux release of Physics IDE, distributed as a `.deb` built by the tag-driven
`.github/workflows/release.yml` (`physics-ide_<version>_amd64.deb`, plus the stable
`physics-ide_latest_amd64.deb` asset linked from gpd-research.com).

## Highlights since v8

- Local AI (Ollama) as a first-class per-pane provider: hardware probe, curated catalog with
  quick↔deep fit estimates, quantization selector (q4_0 … fp16), one-click terminal install,
  user-selectable model storage directory; Linux and macOS.
- Agentic workflow inside the project root: `FILE` / `READ` / `APPEND` / `EDIT` blocks,
  allowlisted `RUN` with inline-code guardrails, `CALC` scratch computation in `.scratch/` with
  inline figures, `SEARCH` over the local retrieval index, `REMEMBER` into an append-only
  `project_memory.md`, shadow-git checkpoints with a viewer and revert.
- Task mode: per-pane Task panel with plan tracking, round and token budgets,
  Pause / Approve-each-step / Stop, and clean stops on provider errors.
- Deterministic master-axiom compilation (every governing equation with labels and variable
  definitions), injected whole into the primer; bounded per-message retrieval of the
  manuscript with hygiene against app-generated files.
- Manuscript Tools: Math Fixer with progress bar, full-screen Combine-files layout, Master
  Axiom mode with custom source / output directory / md-pdf-docx format.
- Document management: unified Document ▾ actions, built-in editor, Session Docs menu,
  thread autosave with read-only recall, Add to notes, configurable word processor.
- Responsiveness: every disk / network / subprocess Tauri command runs off the UI thread.

## Release gates

- [x] Interactive smoke test of file ops, SEARCH/memory, checkpoints, task mode, CALC and
      regression spot-checks passed on the reference laptop (2026-10-14).
- [x] `cargo check --lib --tests` and `node --check` on the frontend.
- [ ] `.deb` from the release workflow installs and launches (verify on first tag).

## Cutting the release

1. Merge the version-bump PR, then `git tag v9.0.0 && git push origin v9.0.0`.
2. Watch the Release workflow; confirm the two `.deb` assets on the GitHub Release.
3. The site link (`releases/latest/download/physics-ide_latest_amd64.deb`) resolves once the
   release exists — it 404s until then.
