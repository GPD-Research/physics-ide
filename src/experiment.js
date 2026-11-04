// Experiment-file helpers for the preview card (pure, unit-tested). Rust parses the file,
// writes setup files and evaluates `expect`; this only decides what the user sees and whether
// Start is allowed. Requirements are checked, never switched on.

export function requirementStatus(requires, cfg, pane, mayWrite) {
  const access = String(cfg?.ai_file_access_mode || 'disabled');
  const canWrite = Boolean(mayWrite) && access === 'read_write';
  return (requires || []).map((name) => {
    switch (name) {
      case 'write':
        return { name, met: canWrite, detail: canWrite ? 'pane may write; AI file access is Read/write' : !mayWrite ? `the ${pane} pane is read-only (its write toggle)` : `AI file access is ${access} (Settings: needs Read/write)` };
      case 'read-only':
        return { name, met: !canWrite, detail: !canWrite ? 'AI writes are blocked for this pane' : `the ${pane} pane can write; turn its write toggle off` };
      case 'run': {
        const met = cfg?.ai_run_enabled === true;
        return { name, met, detail: met ? 'AI may run scripts' : 'AI may run scripts is off (Settings)' };
      }
      case 'fetch': {
        const met = cfg?.ai_fetch_enabled === true;
        return { name, met, detail: met ? 'FETCH is on' : 'FETCH is off (Settings)' };
      }
      default:
        return { name, met: false, detail: 'unknown requirement' };
    }
  });
}

export function startBlockers(preview, reqs, { busy = false, taskActive = false, experimentActive = false } = {}) {
  const out = [];
  const exp = preview?.experiment || {};
  const errors = exp.errors || [];
  const errText = errors.join('\n');
  errors.forEach((e) => out.push(`File error: ${e}`));
  reqs.filter((r) => !r.met && !errText.includes(`requirement \`${r.name}\``)).forEach((r) => out.push(`Requirement ${r.name}: ${r.detail}`));
  (preview?.setup || []).filter((s) => s.state === 'invalid' && !errText.includes(`setup \`${s.path}\``)).forEach((s) => out.push(`Setup ${s.path}: ${s.issue}`));
  if (busy) out.push('the pane is still answering');
  if (taskActive) out.push('a task is already running in this pane');
  if (experimentActive) out.push('an experiment is already running in this pane');
  return out;
}

export function setupOverwrites(preview) {
  return (preview?.setup || []).filter((s) => s.state === 'overwrite').map((s) => s.path);
}

export function experimentTaskLimits(exp, cfg) {
  const rounds = Number(exp?.max_rounds) || Number(cfg?.ai_task_max_rounds) || 12;
  const budget = exp?.token_budget != null ? Number(exp.token_budget) : Number(cfg?.ai_task_token_budget) || 0;
  return { maxRounds: Math.max(1, rounds), tokenBudget: Math.max(0, budget || 0) };
}

export function checksSummary(r) {
  return `${r?.pass || 0} pass · ${r?.fail || 0} fail · ${r?.unchecked || 0} unchecked`;
}
