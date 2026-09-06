import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, test } from 'vitest';
import { visibleWidth } from '@earendil-works/pi-tui';
import { createIsolatedAwarenessStore, createPiFlowHarness } from '@octocodeai/agent-testing';
import { openAwarenessStore } from '@octocodeai/octocode-awareness';
import {
  clearPlan,
} from '../src/tools/active-plan.js';
import { openPlanReview } from '../src/tools/plan-tool.js';
import { buildPlanPageHtmlFromModel, setPlanOpenerForTests } from '../src/tools/plan-html.js';
import { buildPlanReadModel, getCurrentPlanReadModel, renderPlanContext, renderPlanReadModel } from '../src/tools/plan-read-model.js';
import { buildPlanFooterSegments } from '../src/extension-ui.js';
import { renderFooterView } from '../src/tui/footer-view.js';
import { getLocalServerBaseUrl, listLocalServerMounts, serveDirectory, stopLocalServer } from '../src/tools/local-server.js';
import { setInteractionStoreFactoryForTests } from '../src/tools/interaction-broker.js';
import { bindRuntimeRenderer } from '../src/tools/runtime-renderer.js';
import { createRuntimeStore, type ForegroundActivityInput } from '../src/tools/runtime-store.js';
import extension from '../src/index.js';
import type { PiContext, PiInstance } from '../src/types.js';

const roots: string[] = [];
beforeEach(() => setPlanOpenerForTests(async () => ({ ok: true })));

afterEach(() => {
  setPlanOpenerForTests(undefined);
  stopLocalServer();
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

test('drives AskUser, explicit browser Start, shared work, verification, and every surface through production adapters', async () => {
  const isolated = await createIsolatedAwarenessStore(
    ({ workspace, dbPath }) => openAwarenessStore({ workspace, dbPath }),
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
        { status: 'cancelled' },
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
          { text: 'Implement data contracts', activeForm: 'Implementing data contracts', paths: ['src/data.ts'], reasoning: 'establish the reviewed data contract before dependent UX work', acceptance: 'data checked', checkCommand: 'test data' },
          { text: 'Validate terminal and browser UX', activeForm: 'Validating UX', dependsOn: [1], paths: ['src/ui.ts'], reasoning: 'verify each user surface against the completed data contract', acceptance: 'UX checked', checkCommand: 'test ui' },
        ],
      }],
    }) as { details: { revision: string } };
    assert.equal(proposed.details.revision, revision);
    assert.equal(listLocalServerMounts().length, 0, 'proposing never opens a browser automatically');
    await openPlanReview(ctx);
    const reviewMount = listLocalServerMounts()[0]!;
    const reviewUrl = `${getLocalServerBaseUrl()!}${reviewMount.name}/`;

    assert.equal((await postPlanAction(reviewUrl, { action: 'start', revision }, 'https://attacker.invalid')).status, 403);
    assert.equal((await postPlanAction(reviewUrl, { action: 'start', revision }, undefined, 'text/plain')).status, 415);
    assert.equal((await postPlanAction(reviewUrl, { action: 'start', revision })).status, 200);
    let model = getCurrentPlanReadModel(ctx);
    assert.equal(model.phase, 'executing', JSON.stringify(flow.eventsOf('ui.notification').slice(-5)));
    assert.ok(model.authorization.acceptReceiptId);
    assert.ok(model.authorization.startReceiptId);
    const startReceipt = model.authorization.startReceiptId;
    assert.equal((await postPlanAction(reviewUrl, { action: 'start', revision })).status, 200);
    model = getCurrentPlanReadModel(ctx);
    assert.equal(model.authorization.startReceiptId, startReceipt, 'duplicate browser click cannot mint new authority');

    await flow.restart();
    assert.equal(getCurrentPlanReadModel(flow.context as unknown as PiContext).phase, 'executing', 'restart restores started plan');
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
    const retryAfterFailedVerification = await flow.runTool('plan', { queries: [{ reasoning: 'record a later successful observation without hiding verification debt', action: 'complete', index: 2, receipt: { command: 'test ui', status: 'SUCCESS', message: 'ui passed' } }] }) as { isError?: boolean };
    assert.equal(retryAfterFailedVerification.isError, true, 'a failed shared verification remains debt until the canonical task lifecycle is resolved');
    model = getCurrentPlanReadModel(flow.context as unknown as PiContext);
    assert.equal(model.phase, 'executing');
    assert.deepEqual(model.tasks.map((step) => step.status), ['done', 'blocked']);

    assert.ok(model.coordination.awarenessPlanId);
    assert.equal(model.tasks[0]?.status, 'done');
    assert.equal(model.tasks[1]?.status, 'blocked');
    const verificationStore = openAwarenessStore({ workspace: model.coordination.workspace });
    try {
      const awarenessPlan = verificationStore.getPlan(model.coordination.awarenessPlanId!);
      assert.equal(awarenessPlan.status, 'ACTIVE');
      const sharedTasks = verificationStore.listTasks({ planId: awarenessPlan.planId });
      assert.ok(sharedTasks[0]?.verifiedAt);
      assert.equal(sharedTasks[1]?.status, 'FAILED');
      assert.equal(sharedTasks[1]?.verifiedAt, null);
    } finally {
      verificationStore.close();
    }
    assert.match(renderPlanReadModel(model, 'terminal') as string, /Implement data contracts/);
    assert.match(buildPlanPageHtmlFromModel(model), /Implement data contracts/);
    assert.match(renderPlanContext(model), /phase=executing/);
    assert.deepEqual((renderPlanReadModel(model, 'rpc') as typeof model).tasks.map((task) => task.id), model.tasks.map((task) => task.id));
    assert.equal(flow.eventsOf('ui.widget').length, 0, 'plan state never creates a duplicate persistent widget');
    assert.equal(flow.eventsOf('command.expanded').length, 0, 'browser actions never inject slash commands');
    flow.assertSequence(['ui.dialog', 'session.restarted']);
  } finally {
    stopLocalServer();
    await isolated.cleanup();
    if (previousDb === undefined) delete process.env['OCTOCODE_AGENT_DB_PATH']; else process.env['OCTOCODE_AGENT_DB_PATH'] = previousDb;
    if (previousHome === undefined) delete process.env['OCTOCODE_HOME']; else process.env['OCTOCODE_HOME'] = previousHome;
    if (previousAgent === undefined) delete process.env['OCTOCODE_AGENT_ID']; else process.env['OCTOCODE_AGENT_ID'] = previousAgent;
  }
// This intentionally exercises real persistence, local HTTP callbacks, and
// repeated extension restarts. It can exceed Vitest's default under the full
// workspace suite even though the same production flow is healthy; give this
// integration boundary a bounded, explicit budget rather than a flaky default.
}, 30_000);

test('terminal footer keeps current work visible and width-safe while the canonical plan retains every task', () => {
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
    const lines = renderFooterView({ rows: [buildPlanFooterSegments(model)] }, { width });
    for (const line of lines) assert.ok(visibleWidth(line) <= width, `line fits width ${width}: ${line}`);
    const normalized = lines.join(' ').replace(/\s+/g, ' ');
    assert.ok(normalized.includes('Implementing the cross-layer change'), `current task remains visible at width ${width}`);
    assert.ok(!normalized.includes(steps[0]!.text), 'completed detail stays in the durable full plan');
    assert.ok(!normalized.includes(steps[4]!.text), 'later work stays collapsed in the persistent panel');
    assert.match(normalized, /task 2/, 'the active lane remains identifiable at every width');
    const full = renderPlanReadModel(model, 'terminal') as string;
    for (const step of steps) assert.ok(full.includes(step.text), 'the full plan retains every task');
  }
});

test('registered AskUser widget covers recommended, free-text, cancel, and noninteractive pending flows', async () => {
  const isolated = await createIsolatedAwarenessStore(
    ({ workspace, dbPath }) => openAwarenessStore({ workspace, dbPath }),
    { close: (store) => store.close() },
  );
  roots.push(isolated.root);
  const workspace = isolated.workspace;
  setInteractionStoreFactoryForTests((storeWorkspace) => openAwarenessStore({ workspace: storeWorkspace, dbPath: isolated.dbPath }));
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

test('browser Start requires a displayed revision and stale RFC bytes cannot reuse authority', async () => {
  const { workspace, rfcPath, revision } = fixture();
  const flow = createPiFlowHarness({ cwd: workspace, scripted: { customs: [{ status: 'cancelled' }] } });
  await extension(flow.pi as unknown as PiInstance);
  await flow.runTool('plan', {
    queries: [{ reasoning: 'propose exact revision', action: 'propose', consequential: true, rfcPath, steps: ['Implement'] }],
  });
  const url = await openPlanReview(flow.context as unknown as PiContext);
  assert.ok(url);
  assert.equal((await postPlanAction(url, { action: 'start' })).status, 400);
  assert.equal(getCurrentPlanReadModel(flow.context as unknown as PiContext).phase, 'in_review');
  fs.appendFileSync(rfcPath, '\nchanged');
  assert.equal((await postPlanAction(url, { action: 'start', revision })).status, 400);
  const stale = getCurrentPlanReadModel(flow.context as unknown as PiContext);
  assert.equal(stale.phase, 'in_review');
  assert.deepEqual(stale.tasks.map((step) => step.status), ['todo']);
  assert.equal(flow.eventsOf('command.finished').length, 0);

  const unavailable = await serveDirectory('unavailable-flow', workspace, { indexFile: 'missing.html' });
  assert.ok(unavailable);
  assert.equal((await flow.postBrowserMessage({ url: unavailable!.url, message: '/octocode-plan show' })).status, 404);
  stopLocalServer();
});

async function postPlanAction(url: string, action: unknown, origin = new URL(url).origin, contentType = 'application/json'): Promise<Response> {
  return fetch(new URL('__octocode/action', url), {
    method: 'POST', headers: { origin, 'content-type': contentType }, body: JSON.stringify(action),
  });
}
