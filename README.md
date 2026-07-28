# physics-IDE

physics-IDE is a Linux-first desktop research environment for developing, comparing, and testing scientific theories in a structured workspace. It combines a Tauri-based desktop shell with a lightweight web frontend and Rust-backed analysis utilities.

## Project Goals

The immediate direction for physics-IDE is to focus on a Linux-exclusive build and Debian-based deployment workflow. Windows support is currently deferred while the compilation process remains too complex to maintain effectively.

The next major milestone is version 7: turning physics-IDE into a true imported-project research environment where the AI can understand the state of the theory, the available tools, and the history of analysis work without needing the original human operator to re-explain everything each session.

The version 7 vision is to provide a flexible environment for:

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

- formalizing the version 7 implementation plan for project-aware AI memory;
- adding a Tools menu for future project workflows and app functions;
- building a manuscript composition workflow that orders markdown chapters, sections, and subsections into a master document;
- connecting imported theory structure to reusable tools, experiments, and datasets;
- improving support for diverse theory families and educational use cases.

## Version 7 Implementation Order

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

4. Tool and experiment registry
   - Add a project-level registry of reusable scripts, notebooks, and analysis tools.
   - Track prior experiments and link them to the theory topics they support.

5. AI awareness integration
   - Inject the project digest, topic index, and tool registry into the AI briefing pipeline.
   - Make the AI prefer existing tools and previous analyses before proposing new ones.

6. UI polish and workflow consolidation
   - Add the new Tools menu and move version-7 functions into that drop-down as they are introduced.
   - Keep the Customize menu focused on configuration paths and app settings.

## Development

### Prerequisites

- Node.js and npm
- Rust toolchain
- Tauri prerequisites for your OS

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

## API Key Transparency

- Provider API keys entered in settings are stored locally on the device in application config as plain text.
- Keys are used only to send requests to the provider selected in the UI.
- For stronger secret handling, prefer environment variables or an OS keychain-backed workflow.
