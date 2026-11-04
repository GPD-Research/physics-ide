import test from 'node:test';
import assert from 'node:assert/strict';
import {
  GATE_ORDER, runGatePipeline, decideGateVerdict, gateReportPrompt, findProtocolIssues,
  gatePass, gateFail, gateSkip
} from './reply-gates.js';

test('gates run in G0..G6 order and unbuilt gates are explicit skips', async () => {
  const calls = [];
  const gate = (name) => () => { calls.push(name); return gatePass(); };
  const results = await runGatePipeline({ G6: gate('G6'), G1: gate('G1'), G0: gate('G0') }, {});
  assert.deepEqual(calls, ['G0', 'G1', 'G6']);
  assert.deepEqual(results.map((r) => r.gate), GATE_ORDER);
  assert.deepEqual(results.map((r) => r.status), ['pass', 'pass', 'skip', 'skip', 'skip', 'skip', 'pass']);
  assert.equal(results[2].reason, 'not built yet');
});

test('a failing G0 blocks later gates instead of judging an unparsed draft', async () => {
  let g1Ran = false;
  const results = await runGatePipeline({ G0: () => gateFail(['bad marker']), G1: () => { g1Ran = true; return gatePass(); } }, {});
  assert.equal(g1Ran, false);
  assert.equal(results[0].status, 'fail');
  assert.ok(results.slice(1).every((r) => r.status === 'skip' && r.reason === 'not run: G0 failed'));
});

test('a throwing or malformed gate is an error, never a pass', async () => {
  const results = await runGatePipeline({ G0: () => gatePass(), G1: () => { throw new Error('ledger locked'); }, G6: () => ({ ok: true }) }, {});
  assert.equal(results[1].status, 'error');
  assert.match(results[1].reason, /ledger locked/);
  assert.equal(results[6].status, 'error');
  assert.equal(decideGateVerdict(results, { rigor: 'enforce', attempt: 1, budget: 2 }), 'unverified');
});

test('a fail with no reason still carries an issue for the send-back report', async () => {
  const [r] = await runGatePipeline({ G0: () => ({ status: 'fail' }) }, {});
  assert.deepEqual(r.issues, ['gate failed without a reason']);
});

test('skips alone pass: an honest reply is not punished for gates that do not apply', () => {
  const results = [{ gate: 'G0', status: 'pass', issues: [] }, { gate: 'G2', status: 'skip', issues: [], reason: 'not built yet' }];
  assert.equal(decideGateVerdict(results, { rigor: 'enforce', attempt: 1, budget: 2 }), 'pass');
});

test('send-back budget: drafts within budget go back, the next failure renders UNVERIFIED', () => {
  const failed = [{ gate: 'G1', status: 'fail', issues: ['claims fixes.md was created'] }];
  const verdicts = [1, 2, 3].map((attempt) => decideGateVerdict(failed, { rigor: 'enforce', attempt, budget: 2 }));
  assert.deepEqual(verdicts, ['send_back', 'send_back', 'unverified']);
  assert.equal(decideGateVerdict(failed, { rigor: 'enforce', attempt: 1, budget: 0 }), 'unverified');
  assert.equal(decideGateVerdict(failed, { rigor: 'report-only', attempt: 1, budget: 2 }), 'report_only');
});

test('gate report lists only failing gates and is bounded', () => {
  const results = [
    { gate: 'G0', title: 't0', status: 'pass', issues: [] },
    { gate: 'G1', title: 't1', status: 'fail', issues: Array.from({ length: 50 }, (_, i) => `issue ${i} ${'x'.repeat(80)}`) }
  ];
  const prompt = gateReportPrompt(results, 2, 2);
  assert.match(prompt, /draft 2 of 3/);
  assert.match(prompt, /G1 — FAIL/);
  assert.ok(!prompt.includes('G0 — FAIL'));
  assert.ok(prompt.length <= 1500);
});

test('G0 accepts well-formed ops including END markers, EDIT hunks and fenced examples', () => {
  const reply = [
    '<<<FILE path="notes/a.md">>>', 'text with a fake <<<WRITE>>> marker inside the body', '<<<END FILE>>>',
    '<<<EDIT path="b.py">>>', '<<<<< SEARCH', 'x = 1', '=====', 'x = 2', '>>>>> REPLACE', '<<<END EDIT>>>',
    '<<<RUN cmd="python3 b.py" timeout="30">>>', '<<<READ path="c.txt">>>', '<<<SEARCH query="brane tension">>>',
    '<<<CALC>>>', 'print(1)', '<<<END CALC>>>', '<<<STEP 2>>>', '<<<DONE>>>',
    '```', '<<<WRITE path="doc example">>>', '```', 'Use `<<<FAKE>>>` in prose.'
  ].join('\n');
  assert.deepEqual(findProtocolIssues(reply), []);
});

test('G0 names pseudo-ops, missing attributes and unterminated or stray blocks', () => {
  const issues = findProtocolIssues([
    '<<<WRITE path="fixes.md">>>',
    '<<<READ>>>',
    '<<<RUN timeout="5">>>',
    '<<<END CALC>>>',
    '<<<FILE path="fixes.md">>>',
    'content but no end marker'
  ].join('\n'));
  assert.equal(issues.length, 5);
  assert.match(issues[0], /WRITE.*not an operation/);
  assert.match(issues[1], /no path=/);
  assert.match(issues[2], /no cmd=/);
  assert.match(issues[3], /never opened/);
  assert.match(issues[4], /no matching <<<END FILE>>>/);
});

test('gate result helpers carry status, issues and reasons', () => {
  assert.deepEqual(gatePass('3 ops'), { status: 'pass', issues: [], detail: '3 ops' });
  assert.deepEqual(gateFail(['x']), { status: 'fail', issues: ['x'], detail: '' });
  assert.deepEqual(gateSkip('not a Task-mode turn'), { status: 'skip', issues: [], reason: 'not a Task-mode turn' });
});
