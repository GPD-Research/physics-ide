import test from 'node:test';
import assert from 'node:assert/strict';
import { getModelOptions, getProviderLabel } from './ai-config.js';

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
