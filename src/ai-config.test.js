import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { getModelOptions, getProviderLabel } from './ai-config.js';
import { claimsFileWriteWithoutOperation } from './ai-file-claims.js';

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

test('detects unexecuted file-write claims from pseudo commands and success prose', () => {
  assert.equal(claimsFileWriteWithoutOperation('The file manuscript/md/fixes.md has been created.'), true);
  assert.equal(claimsFileWriteWithoutOperation('write path="manuscript/md/fixes.md"\ncontent="""# Audit\n"""'), true);
  assert.equal(claimsFileWriteWithoutOperation('The file manuscript/md/fixes.md has not been created.'), false);
  assert.equal(claimsFileWriteWithoutOperation('The file manuscript/md/fixes.md is missing.'), false);
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

test('ai thread snapshots wrap long code blocks and inline content within the bubble', () => {
  assert.ok(html.includes('.chat-bubble pre'), 'expected the bubble to constrain fenced code blocks');
  assert.ok(html.includes('.chat-bubble code'), 'expected the bubble to constrain inline code and snapshot content');
  assert.ok(html.includes('overflow-wrap: anywhere'), 'expected long snapshot content to wrap before reaching the bubble edge');
});

test('reply gates run after file ops and render a gate card instead of trusting the model narration', () => {
  assert.ok(html.includes("invoke('verify_action_claims'"), 'expected the G1 operation-truth gate to query the ledger');
  assert.ok(html.includes('function renderGateCard'), 'expected a gate card renderer under AI bubbles');
  assert.ok(html.includes("source: 'gate_send_back'"), 'expected failed drafts to be sent back to the model');
  assert.ok(html.includes('findUnrecognisedOpMarkers'), 'expected G0 to name pseudo-op markers such as <<<WRITE>>>');
  assert.ok(html.includes('const threadId = ledgerThreadId(pane, index);'), 'expected ops to carry the thread session key into the ledger');
  assert.ok(html.includes("invoke('record_gate_run'"), 'expected every gate verdict to be persisted');
  assert.ok(html.includes("invoke('record_refused_op'"), 'expected refused ops to get a ledger row');
  assert.ok(html.includes("window.physicsIdeReplyGates = replyGates;"), 'expected the tested gate pipeline module to be loaded');
});

test('watch mode: proposals never run until armed, reports go through the gated dispatch path', () => {
  assert.ok(html.includes("invoke('watch_propose'"), 'expected WATCH lines and the panel to create proposals only');
  assert.ok(html.includes("invoke('watch_pause_all'"), 'expected armed watches to pause at app start');
  assert.ok(html.includes("source: 'watch_report'"), 'expected moved/failed runs to be sent to the AI as watch reports');
  assert.ok(html.includes('It is NOT running: the user must arm it'), 'expected the File System result to say a proposal is not running');
  assert.ok(html.includes("window.physicsIdeWatch = watchMode;"), 'expected the tested watch helpers to be loaded');
});

test('FETCH is a settings-gated, ledgered, untrusted read', () => {
  assert.ok(html.includes("invoke('fetch_ai_url'"), 'expected FETCH ops to go through the Rust allowlist command');
  assert.ok(html.includes('UNTRUSTED EXTERNAL CONTENT'), 'expected fetched text to be labelled untrusted');
  assert.ok(html.includes('id="settings-ai-fetch-enabled"') && html.includes('id="settings-ai-fetch-allowlist"'), 'expected FETCH settings');
  assert.ok(html.includes('ui.adsabs.harvard.edu') && html.includes('wikipedia.org'), 'expected ADS and Wikipedia on the default allowlist');
});

test('thread tools can export an audit of the ledger and gate runs', () => {
  assert.ok(html.includes("add('Export audit…', () => exportThreadAudit(pane)"), 'expected an Export audit item in Thread tools');
  assert.ok(html.includes("invoke('export_thread_audit'"), 'expected the export to go through the Rust ledger command');
});

test('with reply gates on, G1 alone judges write claims and an UNVERIFIED reply does not auto-continue', () => {
  const apply = html.slice(html.indexOf('async function applyAssistantFileBlocks('), html.indexOf('function findUnrecognisedOpMarkers('));
  assert.ok(apply.includes("paneRigorLevel(pane) === 'off' && Boolean(window.physicsIdeAiConfig?.claimsFileWriteWithoutOperation(reply))"), 'expected the ledger-blind write-claim nudge only when gates are off');
  assert.ok(html.includes("gateHeld = verdict === 'unverified' && results.some((r) => r.status === 'fail');"), 'expected an UNVERIFIED verdict to hold the reply');
  assert.ok(html.indexOf('if (fileOps.needsFollowUp && gateHeld)') < html.indexOf('if (fileOps.needsFollowUp && depth < AI_FILE_OPS_MAX_AUTO_CONTINUE)'), 'expected the hold to come before the file-ops auto-continue');
});

test('Load experiment reopens the folder the last experiment came from', () => {
  const load = html.slice(html.indexOf('async function loadExperiment('), html.indexOf('function experimentBlockers('));
  assert.ok(load.includes('defaultPath: (localStorage.getItem(LAST_EXPERIMENT_DIR_KEY) ||'), 'expected the picker to start in the remembered folder');
  assert.ok(load.includes('localStorage.setItem(LAST_EXPERIMENT_DIR_KEY, folder)'), 'expected the chosen file\'s folder to be remembered');
});

test('Reset thread starts a new ledger thread so earlier ops cannot back later claims', () => {
  const reset = html.slice(html.indexOf('async function resetSinglePaneContext('), html.indexOf('async function runContextProbe('));
  assert.ok(reset.includes('threadSessions[pane] = [null, null, null];'), 'expected reset to drop the ledger thread key');
  assert.ok(reset.includes('threadTranscripts[pane] = [[], [], []];'), 'expected reset to start a fresh transcript');
  assert.ok(reset.indexOf('await autosaveThread(pane, i)') < reset.indexOf('threadSessions[pane] = [null, null, null];'), 'expected pending autosaves to flush under the old key first');
  assert.ok(reset.includes("finishExperiment(pane, 'thread reset')"), 'expected an active experiment to finish before its thread is reset');
  assert.ok(html.includes('threadLogsText(pane, exp.logStart || 0)'), 'expected experiment audits to carry only this run\'s terminal lines');
});

test('experiments load through a preview with an explicit Start and app-checked expectations', () => {
  assert.ok(html.includes("add('Load experiment…', () => loadExperiment(pane)"), 'expected Load experiment in Thread tools');
  assert.ok(html.includes("invoke('experiment_load'") && html.includes("invoke('experiment_start'") && html.includes("invoke('experiment_finish'"), 'expected the Rust experiment commands');
  assert.ok(html.includes('id="experiment-start-btn" onclick="startLoadedExperiment()"'), 'expected Start to be a button click, never automatic');
  assert.ok(html.includes('confirm(`Overwrite ${overwrites.length} existing file(s)'), 'expected a second confirmation before setup overwrites');
  assert.ok(html.includes('await lastGateRecord[pane];'), 'expected the audit to wait for the last gate run to be recorded');
});

test('main window stays hidden behind a splash until startup finishes', () => {
  const conf = JSON.parse(readFileSync(path.join(testDir, '..', 'src-tauri', 'tauri.conf.json'), 'utf8'));
  const main = conf.app.windows.find((w) => w.label === 'main');
  assert.equal(main.visible, false, 'expected the main window to start hidden');
  assert.equal(main.maximized, true, 'expected the main window to open maximized');
  assert.ok(readFileSync(path.join(testDir, 'splash.html'), 'utf8').includes('__APP_VERSION__'), 'expected the splash to show the version');
  assert.ok(html.includes("} finally {\n        invoke('startup_ready')"), 'expected initializeApp to reveal the window even when startup fails');
});
