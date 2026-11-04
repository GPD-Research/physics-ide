import test from 'node:test';
import assert from 'node:assert/strict';
import { asciiMeter, runMeter, describeEvery, formatWatchReport } from './watch-mode.js';

test('ascii meter is bounded and proportional', () => {
  assert.equal(asciiMeter(0, 8), '[--------]');
  assert.equal(asciiMeter(0.5, 8), '[####----]');
  assert.equal(asciiMeter(3, 8), '[########]');
  assert.equal(runMeter(30, 60), '[##########----------] 30s / 60s limit');
});

test('schedule labels', () => {
  assert.equal(describeEvery(0), 'on input change');
  assert.equal(describeEvery(600), 'every 10 min');
  assert.equal(describeEvery(7200), 'every 2 h');
  assert.equal(describeEvery(45), 'every 45 s');
});

const watch = { id: 3, cmd: 'python3 sims/tension.py' };

test('moved report lists only non-steady results with z and cites the op', () => {
  const text = formatWatchReport({
    status: 'moved', op_id: 57, sigma: 2, changed_inputs: ['params/brane.yaml'],
    changes: [
      { name: 'sigma_T', kind: 'moved', old: { value: 1.23, err: 0.05, unit: 'GeV' }, new: { value: 1.51, err: 0.04, unit: 'GeV' }, z: 4.37 },
      { name: 'w0', kind: 'steady', old: { value: -1, err: 0.1, unit: '' }, new: { value: -0.99, err: 0.1, unit: '' }, z: 0.07 },
      { name: 'n_s', kind: 'missing', old: { value: 0.965, err: null, unit: '' }, new: null, z: null, note: 'no longer printed' }
    ]
  }, watch);
  assert.match(text, /Watch #3 — moved\*\* \(op#57\)/);
  assert.match(text, /Changed inputs: params\/brane\.yaml/);
  assert.match(text, /\| sigma_T \| moved \| 1\.23±0\.05 GeV \| 1\.51±0\.04 GeV \| 0\.28 \| 4\.4σ \|/);
  assert.match(text, /\| n_s \| missing \|/);
  assert.ok(!text.includes('| w0 |'));
  assert.match(text, /1 other RESULT\(s\) within 2σ/);
});

test('failed report keeps the baseline and forbids reporting values', () => {
  const text = formatWatchReport({ status: 'failed', op_id: 9, note: 'exit 1: Traceback', changes: [], changed_inputs: [] }, watch);
  assert.match(text, /The run failed: exit 1: Traceback\. The baseline is unchanged\./);
  assert.match(text, /do not report values from this run/);
});
