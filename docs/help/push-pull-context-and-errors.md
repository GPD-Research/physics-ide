# Push/Pull Context and Common Errors

This help page explains how Push and Pull work in the Physics IDE and what common errors mean.

## Mental Model

- Git commands run against the currently loaded workspace folder.
- The loaded workspace should be a local Git repository clone.
- Push/Pull targets are determined by that repository's configured remote (usually origin).

In short:
- Workspace path answers "where am I running git?"
- Git remote answers "where is this repo syncing to?"

## How the IDE Chooses Context

1. You load a workspace with Open Workspace/Open Folder.
2. The app saves that path as active runtime workspace context.
3. Pull and Push run inside that active path.

If no workspace is loaded, push/pull actions are blocked.

## Preconditions for Pull/Push

Before using Pull/Push, confirm:

1. The loaded folder contains a .git repository.
2. A remote is configured (typically origin).
3. You have credentials configured (credential helper, SSH key, or HTTPS PAT flow).
4. Network access to the remote host is available.
5. For Push in this app: active branch is main or master.

## Branch Guard in This App

Push is intentionally blocked when you are on a non-main branch.

Meaning:
- Hypothesis branches are local exploration tracks.
- Finalized work should be merged/restored into main/master before Push.

## Common Errors and What to Do

## No workspace loaded

Typical symptom:
- "No workspace loaded"

Fix:
1. Click Open Workspace/Open Folder.
2. Select the intended project root.
3. Retry Pull or Push.

## Not a git repository

Typical symptom:
- "not a git repository"

Fix:
1. Confirm you opened the repository root (contains .git).
2. If repository is not initialized, run git init or clone the repo first.

## No remote configured

Typical symptom:
- pull/push fails with remote/origin errors.

Fix:
1. Check remotes in terminal: git remote -v
2. Add remote if missing:
- git remote add origin <repo-url>

## Authentication failed / permission denied

Typical symptoms:
- "Authentication failed"
- "Permission denied"

Fix:
1. Verify GitHub username/PAT or SSH key setup.
2. Confirm PAT scopes allow repo read/write as needed.
3. Re-auth with your credential helper if tokens changed.

## Non-fast-forward push rejected

Typical symptom:
- push rejected because remote has commits you do not have.

Fix:
1. Pull first (or fetch + rebase/merge).
2. Resolve conflicts if prompted.
3. Retry Push.

## Merge conflict on pull

Typical symptom:
- pull stops with conflict markers.

Fix:
1. Open conflicted files.
2. Resolve conflict sections.
3. Commit resolution.
4. Retry workflow.

## Push blocked on branch guard

Typical symptom:
- "Push blocked on branch ..."

Fix:
1. Move finalized changes into main/master.
2. Checkout main/master.
3. Retry Push.

## Clean Workflow Checklist

1. Open the correct workspace root.
2. Pull before making major changes.
3. Commit local changes with clear messages.
4. Ensure active branch is main/master for final publish.
5. Push.

## Quick Verification Commands (Terminal)

Use these commands if troubleshooting:

```bash
git rev-parse --is-inside-work-tree
git branch --show-current
git remote -v
git status
```

## Final Reminder

Git is local mechanics. GitHub is remote hosting.
If local repository context is correct and credentials are valid, Pull/Push from this IDE should work for the active project.