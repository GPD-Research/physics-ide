# v8.0.0 Final Build Plan

Current baseline: v7 stream (project-aware AI workflow stabilized)
Target milestone: v8.0.0

## v8 product statement

v8 should make physics-IDE feel intentionally organized and theory-agnostic:

- daily research stays in the main left/right wing workspace;
- occasional setup and maintenance flows move into the top-menu Tools dropdown;
- Customize becomes appearance and AI preference control;
- model/provider flexibility is fully user-driven without code edits.

## Locked UX architecture for v8

### 1) Main workspace wings = day-to-day theory work

Keep these in the main GUI:

- dual AI lanes and thread workflow;
- project tree, editing, scratchpad, iterative analysis loops;
- active testing actions that are used routinely inside a work session.

### 2) Tools menu = occasional workflows

Tools should contain actions that are not used every day:

- theory import setup and verification;
- master axiom generation/regeneration;
- briefing packet compile/refresh;
- manuscript render/export;
- snapshot/version utilities.

### 3) Customize menu = fast-changing preferences

Customize should focus on settings users may change often:

- AI provider/model preferences and custom model IDs;
- theme and color system;
- typography controls;
- translucency and visual polish controls.

## Core v8 feature tracks

### Track A: Theory Import Setup tool (Tools dropdown)

Goal: replace scattered setup steps with one explicit checklist surface, without forcing a strict wizard.

Deliverables:

- [ ] Add `Tools > Theory Import Setup` panel.
- [ ] Show explicit checklist stages:
	- `Import`
	- `Scan`
	- `Master axiom`
	- `Briefing`
	- `Run experiments`
	- `Score outcomes`
- [ ] Add status badges per stage (`Not started`, `In progress`, `Complete`, `Needs attention`).
- [ ] Add evidence text per stage (which file/artifact satisfied the step).
- [ ] Add one-click action buttons for each stage (run existing command wiring).
- [ ] Add `Run all missing` helper for first-time onboarding.

Verification rules (theory-agnostic):

- [ ] Import complete if theory markdown directory exists and contains markdown files.
- [ ] Scan complete if scan returns non-zero files and topic evidence.
- [ ] Master axiom complete if target file exists with required sections.
- [ ] Briefing complete if `ai_briefing.md`, `session_recap.md`, `project_awareness.md`, and `workspace_tree.txt` exist.
- [ ] Run experiments complete if at least one experiment artifact is recorded in configured experiment output location.
- [ ] Score outcomes complete if a scorecard/report artifact exists with timestamped run metadata.

### Track B: User-defined OpenAI/Gemini model IDs

Goal: users can run provider-supported model IDs directly with clear validation and reporting.

Deliverables:

- [ ] Freeform model ID inputs per pane/provider, while keeping presets.
- [ ] Per-pane validate action for configured model ID.
- [ ] Persist custom IDs in user settings.
- [ ] Runtime dispatch honors custom IDs first, then safe fallback behavior.
- [ ] Error text clearly explains unsupported model/endpoint combinations.

### Track C: Workflow consolidation and layout control

Goal: reduce UI clutter while preserving fast daily work.

Deliverables:

- [ ] Move mature, non-daily actions from main surfaces into Tools.
- [ ] Keep Tools grouped by category and make output paths obvious.
- [ ] Add View toggles for major left/right pane elements.
- [ ] Persist View states and provide `Reset layout` action.

### Track D: Visual and typography polish

Goal: modernize visual language without reducing readability.

Deliverables:

- [ ] Add translucent dropdown/panel styling controls.
- [ ] Introduce gradient-capable surface tokens.
- [ ] Add font role controls (title, descriptor, tooltip, small/body text).
- [ ] Ship a curated starter font set (readability-first plus a few expressive options).

## Existing automation baseline (as of v7)

Workflow coverage for `Import -> Scan -> Master axiom -> Briefing -> Run experiments -> Score outcomes`:

- import: semi-automated;
- scan: automated;
- master axiom: automated;
- briefing: automated;
- run experiments: partially automated primitives;
- score outcomes: mostly manual/gap.

v8 closes the usability gap by adding explicit checklist verification in one Tool surface.

## Theory repository strategy (parallel to v8 app work)

The app remains theory-agnostic. Model repos are validation content, not hardcoded behavior.

Parallel deliverables:

- [ ] Create `lambda-cdm` repository with canonical structure (`src/`, `data/`, `manuscript/`, root README).
- [ ] Create `ptolemaic-model-edu` repository with canonical structure and explicit educational/refuted labeling.
- [ ] Define shared model-pack metadata contract used by both repos.
- [ ] Add known-result benchmark artifacts for Lambda CDM to calibrate IDE workflows.
- [ ] Add known-limitation and pedagogical contrast tests for Ptolemaic model.

## Implementation phases

### Phase 1: Foundations (Tools entry + verifier backend)

- [ ] Add checklist verifier backend command and response schema.
- [ ] Implement file/artifact evidence checks for all six stages.
- [ ] Add initial Tools menu entry and panel shell.

### Phase 2: Action wiring and reliability

- [ ] Connect panel actions to existing import/axiom/briefing commands.
- [ ] Add safe handling for missing paths/config and actionable errors.
- [ ] Add `Run all missing` and `Next recommended step` guidance.

### Phase 3: Model flexibility + testing metadata

- [ ] Complete custom model ID inputs, validation, and persistence.
- [ ] Include provider/model IDs in probe suite runs and report headers.
- [ ] Add compatibility checks before batch probe execution.

### Phase 4: UI polish and migration cleanup

- [ ] Move remaining non-daily setup controls from Customize/main surfaces to Tools.
- [ ] Land translucency, gradients, and typography role controls.
- [ ] Keep main wings optimized for daily research cadence.

## v8 acceptance criteria

- [ ] Tools menu contains a functioning `Theory Import Setup` workflow panel.
- [ ] Checklist states are machine-verified, not only instructional text.
- [ ] Users can complete full onboarding/import without hunting across scattered menus.
- [ ] User-defined OpenAI and Gemini model IDs run without code edits.
- [ ] AI testing reports include exact provider/model metadata.
- [ ] Main GUI remains focused on day-to-day theory work.
- [ ] No theory-specific hardcoding is introduced for Lambda CDM or other models.

## Out of scope for v8

- Full, mandatory wizard flow with strict step-locking.
- Theory-specific parser branches that change core behavior by model name.

## Immediate next actions

- [ ] Open two new theory repositories (`lambda-cdm`, `ptolemaic-model-edu`).
- [ ] Add initial model-pack metadata template to both repos.
- [ ] Start v8 Phase 1 in app code: checklist verifier plus Tools menu entry.
