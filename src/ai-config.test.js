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

test('advanced routing UI exposes a validation-driven catalog revision button', () => {
  assert.ok(html.includes('Revise List with Validation'), 'expected validation-driven catalog revision button');
});

test('advanced routing UI uses a two-column layout', () => {
  assert.ok(html.includes('class="ai-routing-layout"'), 'expected two-column routing layout container');
  assert.ok(html.includes('class="ai-routing-pane-stack"'), 'expected pane stack in right column');
});

test('provider catalog sections include validation progress indicators', () => {
  assert.ok(html.includes('id="ai-catalog-openai-progress-bar"'), 'expected OpenAI catalog progress bar');
  assert.ok(html.includes('id="ai-catalog-gemini-progress-bar"'), 'expected Gemini catalog progress bar');
  assert.ok(html.includes('Remaining ${remaining}/${total}'), 'expected iterative countdown progress text');
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

test('ai thread bubbles use a transparent thread background without theme-specific overrides', () => {
  assert.ok(html.includes('color-mix(in srgb, var(--thread-bg, var(--bg-panel)) 75%, transparent)'), 'expected AI chat bubble background to use theme-agnostic 75% opacity thread tint');
  assert.ok(html.includes('.chat-bubble.ai'), 'expected AI chat bubbles to get a dedicated class for transparent thread styling');
});

test('workflow and menu structure matches the simplified AI workflow UX', () => {
  assert.ok(html.includes('AI Testing Suite'), 'expected Tools menu to expose the AI Testing Suite menu item');
  assert.ok(html.includes('data-tab="workflow"') && html.includes('>Workflow<'), 'expected the right-wing tab to be renamed to Workflow');
  assert.ok(html.includes('View/Edit Primer') && html.includes('openBriefingPacketEditor'), 'expected primer editor to live in the View menu');
  assert.ok(html.includes('Sync AI Context Now'), 'expected sync context control to remain in the workflow panel');
});

test('chat output renderer includes a math normalization path for pseudo-latex content', () => {
  assert.ok(html.includes('normalizeAiMathOutput'), 'expected a math normalization helper for raw model output');
  assert.ok(html.includes('renderMathContentToHtml'), 'expected a chat renderer that converts math to KaTeX-friendly HTML');
});
