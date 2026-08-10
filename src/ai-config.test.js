import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { getModelOptions, getProviderLabel } from './ai-config.js';

const testDir = path.dirname(fileURLToPath(import.meta.url));
const htmlPath = path.join(testDir, 'index.html');
const html = readFileSync(htmlPath, 'utf8');

test('advanced routing UI uses current Gemini 2.5 presets', () => {
  assert.ok(html.includes('id="settings-left-model"'), 'expected left pane preset selector');
  assert.ok(html.includes('value="gemini-2.5-flash"'), 'expected modern Gemini 2.5 flash preset');
  assert.ok(html.includes('value="gemini-2.5-pro"'), 'expected modern Gemini 2.5 pro preset');
  assert.ok(!html.includes('<option value="gemini-2.0-flash">'), 'did not expect retired Gemini 2.0 flash preset option');
});

test('openai provider exposes OpenAI model options', () => {
  const models = getModelOptions('openai');
  assert.deepEqual(models, [
    { value: 'gpt-4.1-mini', label: 'GPT-4.1 Mini (Fast / Cost-Efficient)' },
    { value: 'gpt-4.1', label: 'GPT-4.1 Flagship (Heavy / Data Crunching)' },
    { value: 'gpt-4o-mini-2024-07-18', label: 'GPT-4o Mini 2024-07-18 (Light / Fast)' },
  ]);
});

test('provider labels are human-readable', () => {
  assert.equal(getProviderLabel('openai'), 'OpenAI');
  assert.equal(getProviderLabel('gemini'), 'Gemini');
  assert.equal(getProviderLabel('unknown'), 'Provider');
});
