# v7.0.0 Release Notes (Draft)

Status: Draft pending final packaging smoke outcome.

## Highlights

- Added a dedicated top-menu AI Testing workflow to reduce main workspace UI clutter.
- Upgraded context probe behavior to use semantic evidence collection for better grounding.
- Added reusable per-theory probe suites for repeatable quality checks.
- Added post-run Open Report action to jump directly to generated probe reports.
- Hardened OpenAI model handling with safer fallback behavior for access/model errors.

## Quality and validation

- Repeated context-probe benchmarking in the v7 stream showed major reliability improvements.
- Frontend and backend automated tests were run during implementation and reported passing in-session.

## Packaging status

- Linux packaging smoke: attempted in the Linux dev container, but final pass/fail lines were not captured in this session.
- Windows packaging smoke: not executed in this environment.
- macOS packaging smoke: not executed in this environment.

Active Linux log:
- `/tmp/v7_tauri_build.log`

Before finalizing this release note, rerun Linux packaging and replace the Linux line above with the actual build result and artifact paths.

## Known limits

- Cross-platform packaging cannot be fully certified from this single Linux container.
- Final release sign-off should include explicit Windows/macOS evidence (or an accepted exception record).