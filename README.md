# physics-IDE

physics-IDE is a Linux-first desktop research environment for developing, comparing, and testing scientific theories in a structured workspace. It combines a Tauri-based desktop shell with a lightweight web frontend and Rust-backed analysis utilities.

## Project Goals

The immediate direction for physics-IDE is to focus on a Linux-exclusive build and Debian-based deployment workflow. Windows support is currently deferred while the compilation process remains too complex to maintain effectively.

Version 8 is the release-candidate baseline. The next major milestone is version 9, the first official physics-IDE release, focused on production readiness and efficient project-aware AI operation.

The version 8 vision is to provide a flexible environment for:

- organizing theory material, notes, equations, and manuscripts in one place;
- ingesting theory sources from different paradigms without rejecting them up front;
- generating a structured master manuscript from imported markdown theory content;
- building a project knowledge index so the AI can navigate chapters, sections, and subsections coherently;
- linking theory content to reusable tools, scripts, experiments, and datasets;
- supporting educational and exploratory workflows across mainstream, hybrid, and non-standard theory families;
- connecting theory content to empirical data and transparent evaluation workflows;
- shipping a polished Linux desktop experience with reliable .deb packaging and installation.

## Current Progress

The project has moved from a simple desktop shell toward a more practical theory-development workspace.

### Latest progress (2026-08-16)

- completed v9 prompt assembly work with deterministic stable-to-dynamic ordering, preserved provider roles, source provenance, per-thread token estimates, and provider-reported usage diagnostics;
- validated native OpenAI prompt caching on `gpt-4.1`, `gpt-4.1-mini`, and `gpt-4o-2024-08-06`, each reporting 1,024 cached tokens and an 81% cache-hit ratio for the tested shared prefix;
- added the AI Request Inspector for prompt ordering, stable-prefix fingerprints, source contributions, cache probes, and provider usage totals;
- replaced recursive briefing-time workspace scans with compact visible-tree export and root-only fallback behavior;
- added deterministic `physics-ide.structural-context/v1` compilation with stable IDs, typed scientific records, source provenance, validation, and compact-core benchmarking;
- used blind A/B testing to reject full-corpus structural-core replacement for the BMI project when it was both larger and less useful than concise legacy context;
- added a project-keyed local SQLite FTS5 retrieval index under Tauri application data, with incremental refresh, bounded Unicode-safe chunks, explanatory neighbors, generated-context exclusions, safe local deletion, and legacy evidence fallback;
- validated the local vector foundation with statically linked `sqlite-vec`, a 384-dimensional cosine index, and FastEmbed model metadata without network-dependent tests;
- added an explicit 91 MB local model installer with immutable revision pinning, per-file SHA-256 verification, app-local storage, and a required offline inference activation probe;
- connected index refresh to incremental local embeddings with stable-ID reuse, transactional changed/deleted-vector invalidation, and inspector diagnostics;
- added hybrid retrieval using symmetric reciprocal-rank fusion across FTS5 and cosine-vector candidates, cached local inference, rank provenance, and lexical fallback;
- added deterministic typed graph-neighbor expansion from explicit project-source relation statements, with bounded provider context and inspector provenance;
- added an eight-case, provider-free four-family retrieval benchmark comparing Recall@3 and MRR@3 with acceptance thresholds, graph coverage, and per-case provenance;
- enforced configurable backend retrieval budgets with complete inclusion, exclusion, truncation, freshness, model, and source diagnostics;
- stopped automatic primer transport of master-axiom/project-awareness corpus excerpts while retaining explicit legacy recovery and comparison modes;
- added complete local-index lifecycle controls, non-mutating integrity/freshness inspection, versioned schema validation, and corruption/incompatibility quarantine recovery;
- replaced generated-primer editing/raw sync with a read-only Awareness Inspector, canonical refresh, and explicit bounded legacy recovery;
- restored idea-pad sync as an independent dynamic session-notes slot with visible-tree provenance while retaining Save/Save As recovery for user notes;
- moved model installation, indexing, rebuild, query, and benchmarks to blocking workers so large local projects remain responsive;
- improved natural-language mechanism retrieval with generic derivation terms, duplicate-copy collapse, direct-source preference, and heading-level multi-hit manuscript search;
- fixed a fatal UTF-8 manuscript-search crash caused by byte slicing across multibyte punctuation and added result content previews;
- expanded the backend regression suite to 71 passing tests plus an explicit model-install, real-vector persistence, hybrid-query, and real-model benchmark probe.

### Latest progress (2026-08-11)

- added an AI file-access permission layer that limits file creation and modification to the active project root unless the user explicitly changes that root in Customize;
- made the AI advisory-only by default in the briefing packet language, so the app clearly distinguishes project reasoning from direct local workspace editing;
- replaced the decorative workspace tree output with a compact AI-friendly markdown project map that is easier for the AI to parse and reason over;
- added a read-only / read-write toggle in the settings panel so project-aware AI operations can be safely gated by user intent;
- upgraded chat input to support Shift+Enter line breaks and a more natural multiline workflow for structured prompting;
- added file attachment support for prompt threads, opened by default from the active project root and including text/image payload content in the prompt sent to the AI;
- tightened the user-facing idea-pad workflow so a session note can sync directly into AI context with an optional visible project-tree scope filter;
- added the Oceanic theme and refined Chromostereopsis for longer, more comfortable dark-theme use;
- verified the backend remains stable with the current regression suite passing after the AI access-control, prompt-attachment, and UI polish changes.

### Latest progress (2026-08-10)

- completed Gemini and OpenAI route validation hardening in Advanced AI Routing so save operations are blocked unless both pane routes validate;
- migrated Gemini defaults and route handling away from retired 2.0 IDs toward current-generation model selection behavior;
- added provider model catalog revision with validation so users can auto-filter to models that actually pass with their active key/project;
- added live per-provider validation progress feedback (countdown + progress bar) so long model sweeps visibly advance instead of appearing frozen;
- refactored Advanced AI Routing into a two-column layout (catalog left, pane controls right) for improved laptop usability;
- reduced translucency opacity across the glass UI surfaces for a lighter, more modern Linux desktop feel;
- completed regression coverage for routing UI/layout and catalog validation controls, with current tests green.

### Latest progress (2026-07-30)

- added a built-in Markdown Documents viewer with rendered preview, fuzzy search, and editor launch workflow;
- added single-document PDF export from Markdown Documents with output-directory selection;
- connected Markdown Documents and Manuscript Tools with cross-navigation buttons for rapid workflow switching;
- improved manuscript rendering/export behavior so PDF and DOCX are generated through Pandoc conversion;
- added explicit GitHub Username/PAT settings and aligned markdown save behavior with configured GitHub mode vs local-save mode;
- expanded the in-app Help system with:
   - GUI Button Glossary,
   - Push/Pull Context and Common Errors,
   - Startup Initial Setup Workflow and Checklist;
- completed a UI housekeeping pass with terminology alignment and broad tooltip coverage (including keyboard Enter hints on chat/search flows).

### Today's progress (2026-07-28)

- confirmed reliable Gemini model communication from the desktop app;
- fixed markdown file-opening from the project tree view;
- established the version 7 direction around project-aware AI memory and theory indexing;
- prepared the groundwork for a new Tools menu and manuscript-ordering workflow.

### Implemented so far

- a Tauri desktop app shell with a configurable interface and integrated terminal;
- settings for project roots, theory directories, master-axiom paths, and AI/provider configuration;
- a master-axiom generation flow that scans theory markdown content and produces a structured draft;
- a theory import pipeline that can ingest a source file and split plain-text manuscripts into markdown sections;
- an initial theory-mode classification layer that recognizes mainstream, hybrid, and left-field-style content;
- regression tests for scientific template generation, theory-style classification, and manuscript import.

### Current focus

- add a local embedding model and SQLite vector extension over the existing stable chunk/index contract;
- combine vector similarity, FTS5 lexical matching, and structural graph neighbors within explicit retrieval budgets;
- benchmark retrieval-selected structural context against concise prose before replacing any primer content;
- consolidate or deprecate manual primer controls only after automated awareness proves equivalent and preserves user-authored notes;
- implement lazy directory token-cost guidance and complete the v9 production release audit.

## Version 8 Implementation Order

1. Project knowledge index
   - Parse the master manuscript and theory markdown directory into a structured topic tree.
   - Generate chapter, section, and subsection summaries that can be used by the AI as a compact navigation layer.

2. Compact project digest
   - Create a token-efficient digest file that summarizes the theory corpus, assumptions, tools, and experiments.
   - Use this digest as a prompt context layer for AI sessions.

3. Manuscript composition workflow
   - Allow the user to reorder imported markdown files into a preferred master-document sequence.
   - Support logical sorting for numbered sections and appendix-style files.
   - Render a combined markdown document from the chosen order.
   - Export the result as Markdown, PDF, or DOCX from a new Tools menu workflow.
   - Support an optional AI training export that writes a replacement training artifact for project-aware AI context.

4. Tool and experiment registry
   - Add a project-level registry of reusable scripts, notebooks, and analysis tools.
   - Track prior experiments and link them to the theory topics they support.

5. AI awareness integration
   - Inject the project digest, topic index, and tool registry into the AI briefing pipeline.
   - Make the AI prefer existing tools and previous analyses before proposing new ones.
   - Support in-thread file attachment so users can provide a selected document or image directly to an AI lane.

6. UI polish and workflow consolidation
   - Add the new Tools menu and move version-7 functions into that drop-down as they are introduced.
   - Keep the Customize menu focused on configuration paths and app settings.
   - Expand the terminal area in the left wing to make the workspace tools more usable.

## Development

### Prerequisites

- Node.js and npm
- Rust toolchain
- Tauri prerequisites for your OS
- Pandoc (required for real PDF and DOCX manuscript/markdown export)

### Run locally

```bash
npm install
npm run tauri dev
```

### Verify backend tests

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

## Notes

This repository is under active development. The current implementation is intentionally modular so new theory parsers, empirical evaluators, and scientific workflows can be added over time.

## Release Tracking

- Desktop stabilization toward v6.0.0 is tracked in docs/releases/v6-desktop-checklist.md.
- UI-to-function mapping coverage is tracked in docs/releases/v6-control-map.md.
- The v8 release-candidate plan is tracked in docs/releases/v8-model-flexibility-plan.md.
- The v9 first-official-release goals and execution roadmap are tracked in docs/releases/v9-official-release-goals.md.

## API Key Transparency

- Provider API keys entered in settings are stored locally on the device in encrypted form.
- Keys are decrypted only when needed for provider requests and are not displayed back in plain text in the UI.
- Keys are used only to send requests to the provider selected in the UI.

## Version 8 Release Candidate

Version 8 marks the current release-candidate milestone for physics-IDE.

- Project-aware AI behavior is stable enough to materially outperform a generic side-by-side browser LLM workflow for in-project theory work.
- The desktop workflow now supports a coherent paradigm for theoretical-physics modeling, iteration, and testing, with AI carrying repetitive context-heavy tasks while the human remains the primary director of theory evolution.
- The app is now tuned for a clean AI-first project workflow: idea-pad driven prompting, scoped project context, safe local file access guardrails, and a more natural desktop UX.

## Version 8 Goals

Version 8 is focused on refinement, flexibility, and polish.

- model freedom: user-selected OpenAI and Gemini model IDs without code edits;
- workflow consolidation: migrate mature workflows into the top-menu Tools dropdown to free left/right wing real estate;
- layout control: expand View controls so users can toggle pane elements such as file tree and primer-related surfaces;
- primer simplification: evaluate how much primer work can be automated by project-aware context, including an idea-pad-driven pathway that can append daily notes into primer context;
- UX coherence: keep customization centered on path/location setup while reducing repetitive manual context assembly.

Tracking references:

- docs/releases/v7-release-checklist.md
- docs/releases/v7-release-notes.md
- docs/releases/v8-model-flexibility-plan.md
- docs/releases/v9-official-release-goals.md

## Ubuntu Linux Full Build Guide (v8 RC)

Use this sequence on a local Ubuntu laptop for a clean production build.

1. Install system dependencies

```bash
sudo apt update
sudo apt install -y \
   build-essential \
   curl \
   wget \
   file \
   pandoc \
   libgtk-3-dev \
   libayatana-appindicator3-dev \
   librsvg2-dev \
   patchelf \
   libwebkit2gtk-4.1-dev
```

2. Install Node.js 20 LTS (if not already installed)

```bash
curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
sudo apt install -y nodejs
node -v
npm -v
```

3. Install Rust toolchain (if not already installed)

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
rustc -V
cargo -V
```

4. Clone and install project dependencies

```bash
git clone https://github.com/GPD-Research/physics-ide.git
cd physics-ide
npm install
```

5. Run automated checks before packaging

```bash
node --test src/ai-config.test.js
cargo test --manifest-path src-tauri/Cargo.toml
```

6. Build production desktop artifacts

```bash
npm run tauri -- build
```

7. Locate artifacts

- Debian package and related artifacts are produced under:
   - src-tauri/target/release/bundle/

8. Install local Debian package (if generated)

```bash
sudo dpkg -i src-tauri/target/release/bundle/deb/*.deb
sudo apt -f install -y
```

9. Launch and smoke-check

- open the installed app;
- verify workspace loading, AI provider settings, and AI Testing modal flows;
- run one context probe and confirm report generation/open-report behavior.

If build issues appear, capture full logs:

```bash
npm run tauri -- build > build.log 2>&1
tail -n 120 build.log
```
