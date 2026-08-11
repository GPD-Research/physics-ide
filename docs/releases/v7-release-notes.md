# v7.0.0 Release Notes (Draft)

Status: Release-candidate ready; Linux packaging smoke confirmed.

## Highlights

- Added a dedicated top-menu AI Testing workflow to reduce main workspace UI clutter.
- Upgraded context probe behavior to use semantic evidence collection for better grounding.
- Added reusable per-theory probe suites for repeatable quality checks.
- Added post-run Open Report action to jump directly to generated probe reports.
- Hardened OpenAI model handling with safer fallback behavior for access/model errors.

## Session update: AI access and prompt workflow (2026-08-11)

- Added a workspace-scoped AI file-access permission model so the AI can only create, modify, or delete files within the active project root unless the user deliberately changes that root in Customize.
- Clarified the AI advisory-only behavior in briefing context so the app distinguishes pure project reasoning from direct local file-edit capability.
- Replaced the decorative tree export with a compact AI-friendly markdown project map that is easier for the AI to parse and reason over.
- Added multiline chat input with Shift+Enter support for structured prompting and paragraph-level work.
- Added attached-file support for AI prompts, with native file-picker behavior rooted in the active project directory and prompt payloads including text/image content for inspection.
- Verified the updated backend remains stable with automated tests passing in-session.

## Quality and validation

- Repeated context-probe benchmarking in the v7 stream showed major reliability improvements.
- Frontend and backend automated tests were run during implementation and reported passing in-session.

## Packaging status

- Linux packaging smoke: confirmed pass by user on the latest deployed release build (excluding post-smoke changes from 2026-07-31).
- Windows packaging smoke: not executed in this environment.
- macOS packaging smoke: not executed in this environment.

Active Linux log:
- `/tmp/v7_tauri_build.log`

Linux packaging is considered validated for v7 release sign-off.

## Known limits

- Cross-platform packaging cannot be fully certified from this single Linux container.
- Windows/macOS remain explicit exceptions unless validated separately on native platforms or CI runners.