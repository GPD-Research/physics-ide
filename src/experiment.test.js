import test from 'node:test';
import assert from 'node:assert/strict';
import { requirementStatus, startBlockers, setupOverwrites, experimentTaskLimits, checksSummary } from './experiment.js';

test('requirements are checked against settings and the pane toggle, never switched on', () => {
  const cfg = { ai_file_access_mode: 'read_write', ai_run_enabled: true };
  const met = (reqs) => reqs.map((r) => r.met);
  assert.deepEqual(met(requirementStatus(['write', 'run', 'fetch'], cfg, 'left', true)), [true, true, false]);
  assert.deepEqual(met(requirementStatus(['write', 'read-only'], cfg, 'left', false)), [false, true]);
  assert.match(requirementStatus(['write'], { ai_file_access_mode: 'read_only' }, 'left', true)[0].detail, /read_only/);
  assert.deepEqual(met(requirementStatus(['read-only'], { ai_file_access_mode: 'disabled' }, 'right', true)), [true]);
  assert.equal(requirementStatus(['root'], cfg, 'left', true)[0].met, false);
  assert.deepEqual(cfg, { ai_file_access_mode: 'read_write', ai_run_enabled: true });
});

test('Start is blocked by file errors, unmet requirements, invalid setup and a busy pane', () => {
  const preview = { experiment: { errors: ['mode must be task or chat'] }, setup: [{ path: 'a.txt', state: 'invalid', issue: '`..` is not allowed' }, { path: 'b.txt', state: 'overwrite' }, { path: 'c.txt', state: 'same' }] };
  const blockers = startBlockers(preview, [{ name: 'run', met: false, detail: 'off' }], { busy: true });
  assert.equal(blockers.length, 4);
  assert.deepEqual(setupOverwrites(preview), ['b.txt']);
  assert.deepEqual(startBlockers({ experiment: { errors: [] }, setup: [] }, [{ name: 'run', met: true }]), []);
  const dup = startBlockers(
    { experiment: { errors: ['unknown requirement `root` (use write, read-only, run, fetch)', 'setup `../x`: `..` is not allowed'] }, setup: [{ path: '../x', state: 'invalid', issue: '`..` is not allowed' }] },
    [{ name: 'root', met: false, detail: 'unknown requirement' }]
  );
  assert.equal(dup.length, 2, 'parse errors are not repeated as requirement/setup blockers');
});

test('task limits come from the file for this run only', () => {
  assert.deepEqual(experimentTaskLimits({ max_rounds: 5, token_budget: 0 }, { ai_task_max_rounds: 12, ai_task_token_budget: 9000 }), { maxRounds: 5, tokenBudget: 0 });
  assert.deepEqual(experimentTaskLimits({}, { ai_task_max_rounds: 12, ai_task_token_budget: 9000 }), { maxRounds: 12, tokenBudget: 9000 });
  assert.equal(checksSummary({ pass: 3, fail: 1 }), '3 pass · 1 fail · 0 unchecked');
});
