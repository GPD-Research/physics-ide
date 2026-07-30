# App Layers

In this app, think of four layers:

## 1. Workspace
A workspace is the actual project folder on disk that the IDE opens.
It contains your files, notes, equations, theory markdown, and local versions folder.
When you switch workspaces, you are switching which folder the IDE is looking at.

## 2. Theory Version
A theory version is a stable milestone snapshot of a workspace.
In this design, versions are local snapshots saved under the workspace's versions folder.
A version means this hypothesis cycle is complete enough to preserve as a baseline.
Versions are what you keep, compare, restore, and optionally publish.

## 3. Hypothesis
A hypothesis is an experimental branch of ideas inside a theory version line.
It is where you test changes, explore alternatives, and run risky edits before promotion.
If the hypothesis succeeds, you bake the results into a new theory version.
If it fails, you terminate it and return to baseline.

## 4. Git vs GitHub
Git is local version-control machinery on your machine.
GitHub is a remote hosting service for Git repositories.

In this app's model:

- Git can be used locally for hypothesis branching.
- GitHub is optional.
- Versions are the authoritative app milestones.
- Only finalized work should be pushed, not active hypothesis branches.

## Practical Mental Model
1. Open workspace.
2. Create hypothesis for experiments.
3. Validate results.
4. Save as new version when mature.
5. Optionally push finalized version-state to GitHub if you use it.

## So:
- Workspace = where work lives
- Hypothesis = experiment track
- Version = accepted milestone
- Git = local branch mechanics
- GitHub = optional remote sharing/publishing
