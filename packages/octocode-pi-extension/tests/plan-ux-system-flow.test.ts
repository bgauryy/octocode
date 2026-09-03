import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, test } from 'vitest';
import { visibleWidth } from '@earendil-works/pi-tui';
import { createIsolatedAwarenessStore, createPiFlowHarness } from '@octocodeai/agent-testing';
import { openAwareness } from '@octocodeai/octocode-awareness';
import {
  clearPlan,
} from '../src/tools/active-plan.js';
import { buildPlanPageHtmlFromModel } from '../src/tools/plan-html.js';
import { buildPlanReadModel, getCurrentPlanReadModel, renderPlanContext, renderPlanReadModel } from '../src/tools/plan-read-model.js';
import { planPanelModelLines } from '../src/tools/plan-tool.js';
import { listLocalServerMounts, serveDirectory, stopLocalServer } from '../src/tools/local-server.js';
import { setInteractionStoreFactoryForTests } from '../src/tools/interaction-broker.js';
import { bindRuntimeRenderer } from '../src/tools/runtime-renderer.js';
import { createRuntimeStore, type ForegroundActivityInput } from '../src/tools/runtime-store.js';
import extension from '../src/index.js';
import type { PiContext, PiInstance } from '../src/types.js';

const roots: string[] = [];
afterEach(() => {
  for (const root of roots.splice(0)) {
    clearPlan(root);
    fs.rmSync(root, { recursive: true, force: true });
  }
});

function fixture(): { workspace: string; rfcPath: string; revision: string } {
  const workspace = fs.mkdtempSync(path.join(os.tmpdir(), 'plan-ux-system-'));
  roots.push(workspace);
  return fixtureAt(workspace);
}

function fixtureAt(workspace: string): { workspace: string; rfcPath: string; revision: string } {
  const rfcPath = path.join(workspace, '.octocode', 'rfc', 'ux-flow', 'RFC.md');
  const bytes = Buffer.from('# RFC\n\n## Summary\nValidate every planning surface.\n');
  fs.mkdirSync(path.dirname(rfcPath), { recursive: true });
  fs.writeFileSync(rfcPath, bytes);
  return { workspace, rfcPath, revision: createHash('sha256').update(bytes).digest('hex') };
}

test('drives AskUser, browser Accept/Start, shared work, verification, and every surface through production adapters', async () => {
  const isolated = await createIsolatedAwarenessStore(
    ({ workspace, dbPath }) => openAwareness({ workspace, dbPath }),
    { close: (store) => store.close() },
  );
  roots.push(isolated.root);
  const previousDb = process.env['OCTOCODE_AGENT_DB_PATH'];
  const previousHome = process.env['OCTOCODE_HOME'];
  const previousAgent = process.env['OCTOCODE_AGENT_ID'];
  process.env['OCTOCODE_AGENT_DB_PATH'] = isolated.dbPath;
  process.env['OCTOCODE_HOME'] = isolated.root;
  process.env['OCTOCODE_AGENT_ID'] = 'pi:system-flow';
  const { rfcPath, revision } = fixtureAt(isolated.workspace);
  let clock = 1_000;
  const flow = createPiFlowHarness({
    cwd: isolated.workspace,
    scripted: {
      customs: [
        { status: 'selected', value: 'strict', label: 'Strict migration' },
        { status: 'selected', value: 'browser', label: 'Open in browser' },
        { status: 'selected', value: 'continue', label: 'Continue to next task' },
      ],
    },
  });
  const ctx = flow.context as unknown as PiContext;
  const store = createRuntimeStore(() => ++clock);
  bindRuntimeRenderer(ctx, store);
  const activity = (next: ForegroundActivityInput): void => store.getState().setActivity(next);
  try {
    await extension(flow.pi as unknown as PiInstance);
    activity({ kind: 'researching', planScope: isolated.workspace, detail: 'Inspecting contracts' });
    const answer = await flow.runTool('askUser', {
      queries: [{
        reasoning: 'choose migration policy before proposing',
        question: 'Choose migration policy',
        options: [
          { value: 'strict', label: 'Strict migration', recommended: true },
          { value: 'compatible', label: 'Compatible' },
        ],
      }],
    }) as { details: { status: string; value: string } };
    assert.deepEqual(answer.details, { status: 'selected', value: 'strict', label: 'Strict migration' });

    const proposed = await flow.runTool('plan', {
      queries: [{
        reasoning: 'propose exact RFC for review',
        action: 'propose',
        scope: 'shared',
        consequential: true,
        rfcPath,
        steps: [
          { text: 'Implement data contracts', activeForm: 'Implementing data contracts', paths: ['src/data.ts'], acceptance: 'data checked', checkCommand: 'test data' },
          { text: 'Validate terminal and browser UX', activeForm: 'Validating UX', dependsOn: [1], paths: ['src/ui.ts'], acceptance: 'UX checked', checkCommand: 'test ui' },
        ],
      }],
    }) as { details: { reviewUrl: string; revision: string } };
    assert.equal(proposed.details.revision, revision);
    assert.ok(listLocalServerMounts().length > 0);

    assert.equal((await flow.postBrowserMessage({ url: proposed.details.reviewUrl, message: `/octocode-plan accept ${revision}`, origin: 'https://attacker.invalid' })).status, 403);
    assert.equal((await flow.postBrowserMessage({ url: proposed.details.reviewUrl, message: `/octocode-plan accept ${revision}`, contentType: 'text/plain' })).status, 415);
    assert.equal((await flow.postBrowserMessage({ url: proposed.details.reviewUrl, message: `/octocode-plan accept ${revision}` })).status, 202);
    let model = getCurrentPlanReadModel(ctx);
    assert.equal(model.phase, 'accepted');
    assert.ok(model.tasks.every((step) => step.status !== 'doing'), 'Accept never starts work');
    const acceptedReceipt = model.authorization.acceptReceiptId;
    assert.equal((await flow.postBrowserMessage({ url: proposed.details.reviewUrl, message: `/octocode-plan accept ${revision}` })).status, 202);
    model = getCurrentPlanReadModel(ctx);
    assert.equal(model.authorization.acceptReceiptId, acceptedReceipt, 'duplicate browser click cannot mint new authority');

    await flow.restart();
    assert.equal(getCurrentPlanReadModel(flow.context as unknown as PiContext).phase, 'accepted', 'restart restores accepted plan');
    assert.equal((await flow.postBrowserMessage({ url: proposed.details.reviewUrl, message: `/octocode-plan start ${revision}` })).status, 202);
    model = getCurrentPlanReadModel(flow.context as unknown as PiContext);
    assert.equal(model.phase, 'executing', JSON.stringify(flow.eventsOf('ui.notification').slice(-5)));
    assert.ok(model.authorization.startReceiptId);
    assert.ok(model.coordination.awarenessPlanId);
    await flow.runTool('agent', { queries: [{ reasoning: 'inspect effective worker capability after Start', type: 'inspect' }] });
    assert.ok(flow.normalizedTranscript().some((event) => event.kind === 'tool.started' && (event.data as { name?: string }).name === 'agent'));
    await flow.restart('during-start');
    assert.equal(getCurrentPlanReadModel(flow.context as unknown as PiContext).phase, 'executing', 'executing authority survives immediate restart');

    await flow.runTool('plan', { queries: [{ reasoning: 'observed first check', action: 'complete', index: 1, receipt: { command: 'test data', status: 'SUCCESS', message: 'data passed' } }] });
    await flow.restart('during-work');
    const failedVerification = await flow.runTool('plan', { queries: [{ reasoning: 'record observed failing verification', action: 'complete', index: 2, receipt: { command: 'test ui', status: 'FAILED', message: 'ui failed' } }] }) as { isError?: boolean };
    assert.equal(failedVerification.isError, true);
    await flow.restart('during-verification');
    await flow.runTool('plan', { queries: [{ reasoning: 'observed final check', action: 'complete', index: 2, receipt: { command: 'test ui', status: 'SUCCESS', message: 'ui passed' } }] });
    model = getCurrentPlanReadModel(flow.context as unknown as PiContext);
    assert.equal(model.phase, 'complete');
    assert.deepEqual(model.tasks.map((step) => step.status), ['done', 'done']);

    const awarenessPlan = isolated.store.getPlan(model.coordination.awarenessPlanId!);
    assert.equal(awarenessPlan.status, 'DONE');
    assert.ok(isolated.store.listTasks({ planId: awarenessPlan.planId }).every((task) => task.verifiedAt));
    assert.match(renderPlanReadModel(model, 'terminal') as string, /Implement data contracts/);
    assert.match(buildPlanPageHtmlFromModel(model), /Implement data contracts/);
    assert.match(renderPlanContext(model), /phase=complete/);
    assert.deepEqual((renderPlanReadModel(model, 'rpc') as typeof model).tasks.map((task) => task.id), model.tasks.map((task) => task.id));
    assert.ok(flow.eventsOf('ui.widget').length > 0);
    assert.ok(flow.normalizedTranscript().some((event) => event.kind === 'command.expanded'));
    flow.assertSequence(['ui.dialog', 'browser.request', 'command.expanded', 'session.restarted', 'browser.request', 'command.expanded']);
  } finally {
    stopLocalServer();
    await isolated.cleanup();
    if (previousDb === undefined) delete process.env['OCTOCODE_AGENT_DB_PATH']; else process.env['OCTOCODE_AGENT_DB_PATH'] = previousDb;
    if (previousHome === undefined) delete process.env['OCTOCODE_HOME']; else process.env['OCTOCODE_HOME'] = previousHome;
    if (previousAgent === undefined) delete process.env['OCTOCODE_AGENT_ID']; else process.env['OCTOCODE_AGENT_ID'] = previousAgent;
  }
}, 15_000);

test('terminal plan widget keeps every task visible and width-safe throughout a complex mocked agent flow', () => {
  const steps = [
    { id: 'research', text: 'Research the existing data, session, agent, and instruction contracts', activeForm: 'Researching all contracts', status: 'done' as const },
    { id: 'implement', text: 'Implement a deliberately long cross-layer change with mocked agent responses', activeForm: 'Implementing the cross-layer change', status: 'doing' as const, dependsOnStepIds: ['research'] },
    { id: 'browser', text: 'Validate browser callback revision and origin handling', status: 'todo' as const, dependsOnStepIds: ['implement'] },
    { id: 'rpc', text: 'Validate RPC cancellation, restart, and duplicate answer handling', status: 'todo' as const, dependsOnStepIds: ['implement'] },
    { id: 'verify', text: 'Run focused, package, and real CLI verification', status: 'todo' as const, dependsOnStepIds: ['browser', 'rpc'] },
  ];

  for (const width of [24, 40, 80, 120, 200]) {
    const model = buildPlanReadModel({
      steps,
      review: { phase: 'executing', branchSnapshotId: 'widget-test', generation: 0, decisions: [], blockingQuestions: [], comments: [] },
      coordination: { mode: 'local', sourcePlanKey: 'widget-test', coordinationWorkspace: '' },
    });
    const lines = planPanelModelLines(model, undefined, width);
    for (const line of lines) assert.ok(visibleWidth(line) <= width, `line fits width ${width}: ${line}`);
    const normalized = lines.join(' ').replace(/\s+/g, ' ');
    for (const step of steps) {
      const label = step.status === 'doing' ? step.activeForm : step.text;
      assert.ok(normalized.includes(label), `complete task label remains visible at width ${width}: ${label}`);
    }
    assert.match(normalized, /▶/, 'the active lane remains identifiable at every width');
  }
});

test('registered AskUser widget covers recommended, free-text, cancel, and noninteractive pending flows', async () => {
  const isolated = await createIsolatedAwarenessStore(
    ({ workspace, dbPath }) => openAwareness({ workspace, dbPath }),
    { close: (store) => store.close() },
  );
  roots.push(isolated.root);
  const workspace = isolated.workspace;
  setInteractionStoreFactoryForTests((storeWorkspace) => openAwareness({ workspace: storeWorkspace, dbPath: isolated.dbPath }));
  const flow = createPiFlowHarness({
    cwd: workspace,
    scripted: { customs: [
      { inputs: ['\r'] },
      { inputs: ['complex answer', '\r'] },
      { inputs: ['\x1b[D'] },
      { inputs: ['\x1b'] },
      { inputs: [] },
    ] },
  });
  try {
    await extension(flow.pi as unknown as PiInstance);

  const recommended = await flow.runTool('askUser', { queries: [{
    reasoning: 'choose safe default',
    question: 'Which path?',
    options: [
      { value: 'fast', label: 'Fast' },
      { value: 'safe', label: 'Safe', recommended: true },
    ],
  }] }) as { details: { status: string; value: string } };
  assert.deepEqual(recommended.details, { status: 'selected', value: 'safe', label: 'Safe' });

  const text = await flow.runTool('askUser', { queries: [{ reasoning: 'collect nuance', question: 'Explain the constraint' }] }) as { details: { status: string; value: string } };
  assert.deepEqual(text.details, { status: 'text', value: 'complex answer' });
  const back = await flow.runTool('askUser', { queries: [{ reasoning: 'allow returning to the prior decision', question: 'Choose final scope', options: [{ value: 'repo', label: 'Repository' }] }] }) as { details: { status: string } };
  assert.equal(back.details.status, 'back');
  const cancelled = await flow.runTool('askUser', { queries: [{ reasoning: 'allow refusal', question: 'Proceed?', options: [{ value: 'yes', label: 'Yes' }] }] }) as { details: { status: string } };
  assert.equal(cancelled.details.status, 'cancelled');
  const timedOut = await flow.runTool('askUser', { queries: [{ reasoning: 'bound an unattended prompt', question: 'Still there?', timeoutMs: 5 }] }) as { details: { status: string } };
  assert.equal(timedOut.details?.status, 'timed_out', JSON.stringify(timedOut));

  const rpc = createPiFlowHarness({ cwd: workspace, hasUI: false, mode: 'rpc', sessionId: 'rpc-question' });
  await extension(rpc.pi as unknown as PiInstance);
  const unavailable = await rpc.runTool('askUser', { queries: [{ reasoning: 'RPC must not fake a default', question: 'Ship?', options: [{ value: 'yes', label: 'Yes', recommended: true }] }] }) as { content: Array<{ text: string }>; details: { status: string; reason: string } };
  assert.deepEqual(unavailable.details, {
    status: 'unavailable',
    mode: 'rpc',
    reason: 'interaction-answer-route-unavailable',
  });
  assert.match(unavailable.content[0]!.text, /no durable .*answer route/i);
  assert.match(unavailable.content[0]!.text, /ask the user inline/i);
  } finally {
    setInteractionStoreFactoryForTests();
    await isolated.cleanup();
  }
});

test('registered command Start fails closed without a displayed revision and stale RFC bytes cannot reuse accepted authority', async () => {
  const { workspace, rfcPath, revision } = fixture();
  const flow = createPiFlowHarness({ cwd: workspace, scripted: { customs: [{ status: 'selected', value: 'terminal', label: 'Keep in terminal' }] } });
  await extension(flow.pi as unknown as PiInstance);
  await flow.runTool('plan', {
    queries: [{ reasoning: 'propose exact revision', action: 'propose', consequential: true, rfcPath, steps: ['Implement'] }],
  });
  await flow.expandPrompt(`/octocode-plan accept ${revision}`);
  assert.equal(getCurrentPlanReadModel(flow.context as unknown as PiContext).phase, 'accepted');

  await flow.expandPrompt('/octocode-plan start');
  assert.equal(getCurrentPlanReadModel(flow.context as unknown as PiContext).phase, 'accepted');
  fs.appendFileSync(rfcPath, '\nchanged');
  await flow.expandPrompt(`/octocode-plan start ${revision}`);
  const stale = getCurrentPlanReadModel(flow.context as unknown as PiContext);
  assert.equal(stale.phase, 'accepted');
  assert.deepEqual(stale.tasks.map((step) => step.status), ['todo']);
  assert.ok(flow.eventsOf('command.finished').length >= 3);

  const unavailable = await serveDirectory('unavailable-flow', workspace, { indexFile: 'missing.html' });
  assert.ok(unavailable);
  assert.equal((await flow.postBrowserMessage({ url: unavailable!.url, message: '/octocode-plan show' })).status, 404);
  stopLocalServer();
});
