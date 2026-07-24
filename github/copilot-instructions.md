# Copilot Instructions for physics-ide

## Tech Stack
- Desktop application built with Tauri (Rust backend, web frontend).
- Local state management and file system storage for physics simulation states.

## Review Focus Areas
- **Rust Backend:** Check for proper error handling (`Result`/`Option`), safety with concurrency, unwrap/expect usage risks, and efficient memory management in simulation loops.
- **Tauri Bridge:** Look out for insecure IPC commands, missing payload validations, or serialization bottlenecks between the Rust core and the UI.
- **Code Quality:** Flag redundant code, dead imports, or state-sync bugs.
