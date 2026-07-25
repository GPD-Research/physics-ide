# physics-IDE

physics-IDE is a local-first desktop research environment for developing, comparing, and testing scientific theories in a structured workspace. It combines a Tauri-based desktop shell with a lightweight web frontend and Rust-backed analysis utilities.

## Project Goals

The long-term vision for physics-IDE is to provide a flexible environment for:

- organizing theory material, notes, equations, and manuscripts in one place;
- ingesting theory sources from different paradigms without rejecting them up front;
- generating master-axiom documents from theory markdown folders;
- supporting educational and exploratory workflows across mainstream, hybrid, and non-standard theory families;
- connecting theory content to empirical data and transparent evaluation workflows.

## Current Progress

The project has moved from a simple desktop shell toward a more practical theory-development workspace.

### Today's progress (2026-07-25)

- successfully deployed and validated two Ollama AI threads for local concurrent usage;
- enabled local save and run workflow for two Ollama models directly from the desktop environment;
- tweaked the integrated terminal behavior for a smoother development and model-execution workflow;
- completed additional stability and usability improvements across the desktop experience.

### Implemented so far

- a Tauri desktop app shell with a configurable interface and integrated terminal;
- settings for project roots, theory directories, master-axiom paths, and AI/provider configuration;
- a master-axiom generation flow that scans theory markdown content and produces a structured draft;
- a theory import pipeline that can ingest a source file and split plain-text manuscripts into markdown sections;
- an initial theory-mode classification layer that recognizes mainstream, hybrid, and left-field-style content;
- regression tests for scientific template generation, theory-style classification, and manuscript import.

### Current focus

- expanding the import pipeline for richer manuscript formats and section detection;
- improving support for diverse theory families and educational use cases;
- connecting imported theory structure to later empirical analysis and evaluation workflows.

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
