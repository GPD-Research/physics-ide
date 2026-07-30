# Startup Initial Setup Workflow and Checklist

Use this checklist when running Physics IDE for the first time on a new machine or new project.

## Phase 1: Open a Project Workspace

1. Click Open Workspace/Open Folder.
2. Select your project root directory.
3. Confirm file tree appears and Project Status root updates.

Success criteria:
- Files are visible in the left panel.
- Workspace actions (Export Tree, Pull, Push) are enabled by context.

## Phase 2: Configure Core Paths

Open Customize and configure Locations:

1. Project Root Directory.
2. Theory Markdown Directory.
3. Master Axiom File path.
4. Tools Directory (optional but recommended).

Success criteria:
- Paths are saved without errors.
- Opening Customize again shows persisted values.

## Phase 3: Configure System and GitHub Access

In Customize -> System & Apps:

1. Set preferred CLI editor.
2. Set preferred terminal application.
3. Optional: set GitHub Username.
4. Optional: set GitHub API Key/PAT.

Behavior note:
- If GitHub credentials are set, markdown edit-close can use git add + commit flow.
- If not set, markdown edits use local-save mode.

Success criteria:
- Settings save successfully.
- Pull/Push works in repositories with valid remotes and credentials.

## Phase 4: Configure AI Providers

In Customize -> AI Models:

1. Paste Gemini API key and/or OpenAI API key.
2. Choose provider per pane.
3. Choose model per pane.
4. Save Preferences.

Success criteria:
- Left and right pane prompts return responses.
- No provider auth/model errors in terminal log.

## Phase 5: Verify Working Surfaces

1. Open Theory Profiles modal and verify list loads.
2. Open Markdown Documents modal and load docs directory.
3. Open Tools -> Help -> App Layers.
4. Open Tools -> Help -> GUI Button Glossary.

Success criteria:
- Help docs render in modal.
- Markdown docs list/search/preview functions operate.

## Phase 6: Establish Versioning Workflow

1. Save as Hypothesis for experimental work.
2. Save as Version for local milestones.
3. Restore Version to validate rollback path.
4. Use Push only from main/master when publishing finalized state.

Success criteria:
- Hypothesis and version actions complete without backend errors.
- Local snapshots appear in versions list.

## Optional Phase 7: Build Project Context Packet

1. Export ASCII Tree.
2. Ensure master axiom file exists.
3. Open/View primer and sync context to AI threads.

Success criteria:
- Context/briefing tools run and AI threads receive shared project context.

## Startup Troubleshooting Quick List

If setup fails, check in order:

1. Workspace root loaded correctly.
2. Required paths exist on disk.
3. API keys are valid and not expired.
4. Git repository and remote are configured.
5. Active branch policy for push (main/master).

## Daily Start Shortcut (After Initial Setup)

1. Open workspace.
2. Pull latest changes.
3. Load theory profile.
4. Verify AI lanes.
5. Continue hypothesis/version cycle.