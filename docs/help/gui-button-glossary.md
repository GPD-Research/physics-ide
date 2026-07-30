# GUI Button Glossary

This glossary defines the interactive controls in the Physics IDE UI.

Notes:
- Some controls are links in dropdown menus but behave like buttons.
- Some controls appear only inside modals.
- Some controls are generated dynamically (for example, profile list rows).

## File Menu

- New Theory Sheet: Reset to first-session mode and close the active theory context.
- Open Workspace: Choose and load a workspace folder from disk.
- Theory Profiles...: Open the profile manager modal.
- Save as Version: Save a local version snapshot of the current workspace state.
- Restore Version: Open version restore flow and restore a selected local version.
- Save as Hypothesis: Create or move into a hypothesis branch for experiments.
- Terminate Hypothesis: End the current hypothesis branch and return to baseline branch flow.
- Exit: Open the exit/session wrap-up dialog.

## Edit Menu

- Copy Math String: Placeholder action for copying equation text.
- Paste Context Buffer: Placeholder action for inserting external context text.

## View Menu

- Terminal (toggle): Show or hide the terminal panel.
- Inspectors (toggle): Show or hide the inspectors panel.
- Equation Tools (toggle): Show or hide equation tools panel.
- Scratchpad (toggle): Show or hide notes/scratchpad panel.
- Next Analytical Slot: Rotate to the next left-pane chat thread slot.
- Next Creative Slot: Rotate to the next right-pane chat thread slot.

## Tools Menu

- Markdown Documents: Open markdown browser/search/preview/edit modal.
- Manuscript Tools: Open manuscript assembly and render modal.
- AI Testing: Open probe/testing modal for context and lane checks.
- App Layers: Open conceptual help document about workspace/version/hypothesis layers.
- GUI Button Glossary: Open this glossary document.

## Top Bar and Workspace Controls

- Customize: Open settings/customization modal.
- Edit Project Title (pencil): Rename the displayed project title.
- Open Workspace: Load a directory as active workspace.
- Export ASCII Tree: Generate and save workspace tree text file.
- Pull: Run git pull in the active workspace repository.
- Push: Run git push in the active workspace repository (main/master guarded).

## Terminal Panel Controls

- Detach: Toggle terminal detached/attached mode.
- Shell: Open external shell/terminal app.

## Left and Right AI Chat Controls

- Slot Up (▲): Rotate to previous thread slot in a pane.
- Slot Down (▼): Rotate to next thread slot in a pane.
- Attach (+): Attach a file into the pane thread context.
- Send: Submit current prompt to the pane provider/model.

## Right Wing Tabs

- Context: Show context and workflow panel.
- Inspectors: Show analysis/inspection panel.
- Equation: Show equation editor/render/save panel.
- Notes: Show scratchpad notes panel.

## Context Panel Buttons

- View/Edit Primer: Open primer/briefing packet editor.
- Sync AI Context Now: Push primer context into both AI lanes.
- Open AI Testing Suite: Open AI Testing modal.

## Inspectors Panel Buttons

- Create Analysis Plan: Generate an empirical analysis plan scaffold.
- Edit Analysis Plan: Open current analysis plan for editing.
- Compute Metrics: Run configured cosmology metrics workflow.

## Equation Panel Buttons

- Quick Clean: Normalize or clean equation text formatting.
- Render Preview: Render equation text preview.
- View Axiom File: Load master axiom file content into equation panel.
- Edit Axiom File: Open master axiom file in editor.
- Save to Workspace: Save equation output to workspace markdown.

## Notes/Scratchpad Buttons

- Bold (B): Apply bold formatting to selected text.
- Italic (I): Apply italic formatting to selected text.
- Integral (∫ LaTeX): Insert integral LaTeX snippet.
- Divider: Insert note divider marker.
- Clear: Clear scratchpad content.
- Save: Save scratchpad to default scratchpad file.
- Save As...: Save scratchpad to a chosen path.
- Pass Into Primer: Append scratchpad content into primer packet content.

## Restore Version Modal

- Cancel: Close restore modal without changes.
- Restore: Execute restore for selected version.

## Customize Modal

### Tabs
- Locations: Path and theory source configuration tab.
- System & Apps: Editor/terminal/GitHub credentials tab.
- AI Models: Provider keys and model assignment tab.
- Appearance: Theme/color customization tab.

### Locations Tab Buttons
- Browse (Project Root): Choose project root directory.
- Browse (Theory Markdown Directory): Choose theory markdown folder.
- Browse (Master Axiom File): Choose axiom file path.
- Browse (Tools Directory): Choose tools folder.
- Browse (Import Source): Choose source document to import.
- Import Theory Source into Folder: Parse/import selected source into theory folder.
- Generate Master Axiom from Theory Folder: Generate master axiom from configured theory docs.

### Footer Buttons
- Cancel: Close settings without applying new edits.
- Save Settings: Persist settings to local app config.

## Theory Profiles Modal

- Save Current: Save current state as named profile.
- Load Selected: Load selected profile into active app state.
- Rename Selected: Rename selected profile.
- Delete Selected: Delete selected profile.
- Refresh: Reload profile list.
- Close: Close profile modal.
- Profile Row Button (dynamic): Select profile row and set target for load/rename/delete.

## Manuscript Tools Modal

- Maximize: Toggle modal expanded view.
- Browse (Source File): Pick a single markdown source file.
- Browse (Source Directory): Pick source directory for markdown discovery.
- Browse (Output Directory): Pick manuscript output directory.
- Move Up: Move selected manuscript item earlier in order.
- Move Down: Move selected manuscript item later in order.
- Refresh List: Re-scan and refresh discovered manuscript files.
- Close: Close manuscript tools modal.
- Render: Render compiled manuscript output.
- Include/Exclude (dynamic row button): Toggle whether selected file is included in final manuscript.

## AI Testing Modal

- Close: Close AI Testing modal.
- Save Suite: Save current probe setup as a named suite.
- Delete Suite: Delete selected saved suite.
- Run Selected Suite: Execute selected probe suite.
- Run Probe (Both Panes): Run a probe against both lanes now.
- Run Batch (Current Pair): Run batch probes for active lane pair.
- Reset Probe History: Clear probe history records.
- Reset Left Pane: Reset left pane context history.
- Reset Right Pane: Reset right pane context history.
- Open Report: Open latest generated probe report artifact.

## Security Note Modal

- Understood: Dismiss security notice.

## Exit / Session Wrap-Up Modal

- Refresh Draft: Regenerate session recap draft text.
- Cancel: Close exit dialog without exiting.
- Prepare & Exit: Finalize recap/briefing flow and exit session.

## Briefing Packet Editor Modal

- Refresh: Reload packet content from current sources.
- Save Packet: Save current packet edits.
- Sync To AI Threads: Push packet context to both lanes.
- Close: Close briefing editor.

## Help Modal

- Search: Run fuzzy search across help documents.
- Close: Close help modal.

## Markdown Documents Modal

- Browse: Choose markdown directory root.
- Load: Load markdown files from selected directory.
- Search: Search loaded markdown docs by title/content.
- Edit: Open selected markdown file in external editor workflow.
- Close: Close markdown documents modal.
- Document Row Button (dynamic): Select document and show rendered preview.
- Search Result Row Button (dynamic): Jump to selected result document.

## Miscellaneous Dynamic Buttons

- Attachment Remove (X): Remove an attached file from thread context.
