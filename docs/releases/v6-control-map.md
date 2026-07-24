# v6 Control Mapping Matrix

Purpose: prove that every visible control has a real implementation path.

Status legend:
- mapped: control is wired to a real frontend function
- command-linked: frontend function calls a real backend command when required
- complete: success and failure paths validated
- blocked: implementation gap found

| UI control label | Location | Frontend handler | Backend command | Status | Notes |
| --- | --- | --- | --- | --- | --- |
| Load Workspace | left panel / workspace controls | loadWorkspace | save_root_directory + read_directory | pending | |
| Export Workspace Tree | left panel / workspace controls | exportWorkspaceTree | export_workspace_tree | pending | |
| Save as Version | left panel / workspace controls | saveVersion | save_as_version | pending | |
| Restore Version | left panel / workspace controls | restoreVersion | restore_version | pending | |
| Save Settings | settings modal footer | saveSettings | save_user_settings | pending | |
| Import Theory Source | settings modal / Locations tab | importTheorySource | import_theory_source_command | pending | |
| Generate Master Axiom | settings modal / Locations tab | generateAxiomFromTheory | generate_master_axiom_from_theory | pending | |
| Left chat send | left AI pane | submitPrompt | send_llm_prompt | pending | |
| Right chat send | right AI pane | submitPrompt | send_llm_prompt | pending | |
| Save Equation | equation panel | saveEquation | save_equation_to_md | pending | |
| Launch editor (file tree item) | file tree | openEditor | launch_file_editor | pending | |
| Open detached terminal | terminal panel action | spawnDetachedTerminal | detach_terminal_shell | pending | |

## Audit instructions

1. Enumerate every clickable/interactive control in index.html.
2. For each control, fill handler and backend command (if any).
3. Validate success and failure behavior.
4. Mark status complete only after manual verification.
