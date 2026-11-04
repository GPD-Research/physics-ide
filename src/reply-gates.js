// Reply-gate pipeline (AI-RIGOR-STRATEGY Tier 1). Pure logic so the ordering, skip and
// send-back rules are unit-tested; index.html supplies the gate implementations.

export const GATE_ORDER = ['G0', 'G1', 'G2', 'G3', 'G4', 'G5', 'G6'];

export const GATE_TITLES = {
  G0: 'Protocol parse: every op marker is a recognised, well-formed, terminated operation',
  G1: 'Operation truth: claimed file/run actions resolve to successful ledger operations',
  G2: 'Claim extraction: substantive claims carry a status tag',
  G3: 'Evidence resolution: cited evidence exists and supports the claim',
  G4: 'Tool-backed checks: reported numbers come from a CALC/RUN',
  G5: 'Status consistency: wording matches the claim status',
  G6: 'Completion: Task-mode DONE is backed by outputs'
};

// A failing blocking gate stops the pipeline: later gates would judge a draft whose ops never parsed.
export const BLOCKING_GATES = new Set(['G0']);

export const gatePass = (detail = '') => ({ status: 'pass', issues: [], detail });
export const gateFail = (issues, detail = '') => ({ status: 'fail', issues: issues.map(String), detail });
export const gateSkip = (reason) => ({ status: 'skip', issues: [], reason: String(reason) });

const VALID = new Set(['pass', 'fail', 'skip', 'error']);

function normalise(gate, raw) {
  const r = raw && typeof raw === 'object' ? raw : {};
  const status = VALID.has(r.status) ? r.status : 'error';
  const issues = Array.isArray(r.issues) ? r.issues.map(String) : [];
  if (status === 'fail' && issues.length === 0) issues.push('gate failed without a reason');
  const out = { gate, title: GATE_TITLES[gate] || gate, status, issues };
  if (r.detail) out.detail = String(r.detail);
  if (r.reason) out.reason = String(r.reason);
  if (Array.isArray(r.warnings) && r.warnings.length) out.warnings = r.warnings.map(String);
  if (status === 'error' && !out.reason) out.reason = 'gate returned no verdict';
  return out;
}

// Runs every gate in GATE_ORDER. A gate with no implementation is an explicit skip, a throwing
// gate is an `error` (never a pass), and a failing blocking gate skips everything after it.
export async function runGatePipeline(gates, ctx) {
  const results = [];
  let blockedBy = null;
  for (const gate of GATE_ORDER) {
    if (blockedBy) {
      results.push(normalise(gate, gateSkip(`not run: ${blockedBy} failed`)));
      continue;
    }
    const fn = gates[gate];
    let raw;
    if (typeof fn !== 'function') raw = gateSkip('not built yet');
    else {
      try {
        raw = await fn(ctx);
      } catch (err) {
        raw = { status: 'error', reason: `gate error: ${err?.message || err}` };
      }
    }
    const result = normalise(gate, raw);
    results.push(result);
    if (result.status === 'fail' && BLOCKING_GATES.has(gate)) blockedBy = gate;
  }
  return results;
}

// pass | send_back | unverified | report_only. An `error` gate can never yield pass:
// a gate that could not check the reply leaves it unverified, not verified.
export function decideGateVerdict(results, { rigor = 'enforce', attempt = 1, budget = 2 } = {}) {
  const failed = results.some((r) => r.status === 'fail');
  const errored = results.some((r) => r.status === 'error');
  if (!failed && !errored) return 'pass';
  if (!failed) return 'unverified';
  if (rigor === 'report-only') return 'report_only';
  return attempt <= budget ? 'send_back' : 'unverified';
}

export function gateReportPrompt(results, attempt, budget) {
  const lines = [`GATE REPORT (draft ${attempt} of ${budget + 1} failed; the app withheld it)`];
  results.filter((r) => r.status === 'fail').forEach((r) => {
    lines.push(`${r.gate} — FAIL: ${r.title}`);
    r.issues.slice(0, 6).forEach((i) => lines.push(`  - ${i}`));
  });
  lines.push('Fix the draft: perform the operation with a real FILE/APPEND/EDIT/RUN/CALC block, or restate honestly that it was not done. Cite operations as op#N from the File System results. A negative or "not checked" answer is acceptable; an unbacked success claim is not. Do not add new claims.');
  return lines.join('\n').slice(0, 1500);
}

export const KNOWN_OP_MARKERS = new Set(['FILE', 'END FILE', 'APPEND', 'END APPEND', 'EDIT', 'END EDIT', 'READ', 'RUN', 'CALC', 'END CALC', 'SEARCH', 'REMEMBER', 'END REMEMBER', 'PLAN', 'END PLAN', 'STEP', 'DONE', 'BLOCKED', 'FIND', 'REPLACE', 'END HUNK', 'FETCH']);

const BLOCK_OPS = ['FILE', 'APPEND', 'EDIT', 'CALC', 'REMEMBER', 'PLAN'];
const EDIT_INNER = new Set(['FIND', 'REPLACE', 'END HUNK']);
const PATH_OPS = { FILE: 'path', APPEND: 'path', EDIT: 'path', READ: 'path', RUN: 'cmd', SEARCH: 'query', FETCH: 'url' };

export function fencedRanges(source) {
  const ranges = [];
  const fence = /^\s*```/gm;
  let open = null;
  let m;
  while ((m = fence.exec(source)) !== null) {
    if (open === null) open = m.index;
    else { ranges.push([open, m.index]); open = null; }
  }
  if (open !== null) ranges.push([open, source.length]);
  return ranges;
}

// G0: markers that look like operations but would never execute (unknown verb, missing
// required attribute, block with no END marker). Fenced code is documentation, not ops.
export function findProtocolIssues(reply) {
  const source = String(reply || '');
  const fenced = fencedRanges(source);
  const inFence = (i) => fenced.some(([a, b]) => i >= a && i < b);
  const issues = [];
  const re = /<<<\s*([A-Z]+(?: [A-Z]+)?)(?=[\s>])([^>\n]*)>>>+/g;
  const opened = [];
  let m;
  while ((m = re.exec(source)) !== null) {
    if (inFence(m.index) || source[m.index - 1] === '`') continue;
    let word = m[1].trim();
    let attrs = m[2];
    if (!KNOWN_OP_MARKERS.has(word) && word.includes(' ') && !word.startsWith('END ')) {
      const [head, ...rest] = word.split(' ');
      if (KNOWN_OP_MARKERS.has(head)) { word = head; attrs = ` ${rest.join(' ')}${attrs}`; }
    }
    const snippet = `\`${m[0].slice(0, 60)}\``;
    const body = opened.length ? opened[opened.length - 1].word : null;
    // Inside a FILE/APPEND/CALC/… body everything is content except that block's own END (and EDIT hunks).
    if (body && word !== `END ${body}` && !(body === 'EDIT' && EDIT_INNER.has(word))) continue;
    if (!KNOWN_OP_MARKERS.has(word)) {
      issues.push(`${snippet} is not an operation the app executes; use <<<FILE path="…">>> … <<<END FILE>>> (or APPEND/EDIT/READ/RUN/CALC/SEARCH/REMEMBER).`);
      continue;
    }
    const attr = PATH_OPS[word];
    if (attr && !new RegExp(`\\b${attr}="[^"\\n]+"`).test(attrs)) {
      issues.push(`${snippet} has no ${attr}="…" attribute, so it was not executed.`);
      continue;
    }
    if (BLOCK_OPS.includes(word)) opened.push({ word, snippet });
    else if (word.startsWith('END ') && BLOCK_OPS.includes(word.slice(4))) {
      if (body === word.slice(4)) opened.pop();
      else issues.push(`${snippet} closes a ${word.slice(4)} block that was never opened.`);
    }
  }
  opened.forEach((o) => issues.push(`${o.snippet} has no matching <<<END ${o.word}>>>, so the block was not executed.`));
  return issues;
}
