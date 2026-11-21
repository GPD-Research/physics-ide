# Planned: Theory Map (import and generate)

Status: planned. Starts after the gates and tests stage (`AI-RIGOR-STRATEGY.md`, Tier 1–2) is
completed or stable. Not scheduled before that.

## Idea

A visual status-and-plan map of a theory: a flowchart from axioms through derived results,
parameters and predictions to data and kill tests. Each box carries a status and the file/section
and check that back it. Each causal link is drawn with a green check (verified, with the file or
test that verifies it) or a "?" (not yet resolved). An intuitive leap can be saved as a tentative
branch on "?" links, pending a connection that may come from a parallel branch.

The reference implementation is being built directly in GPD-Research/BMI-Synthesis-Lab
(`v2/map.yaml` + `viewer/`). The IDE version can be simpler.

## IDE scope

1. **Import.** Read a curated theory map (BMI's `map.yaml` schema) as a trusted structural
   context alongside, or instead of, the auto-extracted `structural_context.v1.json`.
2. **Generate.** Build a draft map for any project from its theory files: the AI proposes nodes,
   links and statuses as a diff; the user accepts or rejects each one. Nothing is written without
   acceptance.
3. **View.** A pane that renders the map: status colours, check/"?" on links, click a box to open
   its file at the section.
4. **Check.** The map's consistency checks (referenced files and sections exist, parameter list
   matches the ledger, dead nodes match the retraction record) can run as a gate.

## Later

- Similarity and foresight: compare the map's structural features with a curated catalogue of
  known frameworks (e.g. braneworld families) to show likely next stages and the known traps at
  each, before the theory reaches them.

## Notes

- Keep the schema generic (no BMI-specific field names) so both import and generate use it.
