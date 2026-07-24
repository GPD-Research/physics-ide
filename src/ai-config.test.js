import test from 'node:test';
import assert from 'node:assert/strict';
import { getModelOptions, getProviderLabel } from './ai-config.js';

test('openai provider exposes OpenAI model options', () => {
  const models = getModelOptions('openai');
  assert.deepEqual(models, [
    { value: 'gpt-4o-mini', label: 'GPT-4o Mini (Light / Fast)' },
    { value: 'gpt-4o', label: 'GPT-4o Flagship (Heavy / Data Crunching)' },
  ]);
});

test('provider labels are human-readable', () => {
  assert.equal(getProviderLabel('openai'), 'OpenAI');
  assert.equal(getProviderLabel('gemini'), 'Gemini');
  assert.equal(getProviderLabel('ollama'), 'Ollama');
});

test('ollama provider exposes deep reasoning and conversational local options', () => {
  const models = getModelOptions('ollama');
  assert.ok(models.some(model => model.value === 'deepseek-r1:14b'));
  assert.ok(models.some(model => model.value === 'qwen2.5:7b-instruct'));
});
