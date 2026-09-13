import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, expect, test, vi } from 'vitest';
import { createPiFlowHarness } from '@octocodeai/agent-testing';
import { contentDigest } from '@octocodeai/octocode-awareness/host';
import extension from '../src/index.js';
import type { PiContext, PiInstance } from '../src/types.js';
import { activePlanScope, clearPlan, setPlan } from '../src/tools/planning/plan-store.js';
import { createSessionArtifactContext, writeRehydrationLedger } from '../src/tools/session-artifacts.js';
import { hasPendingRehydration, rehydrateSession } from '../src/tools/rehydration-orchestrator.js';
import { registerCurrentContextSource } from '../src/tools/context-source-registry.js';
import { getCurrentPlanReadModel, renderPlanContext } from '../src/tools/plan-read-model.js';
import { installAuthenticatedWorkerCapabilityView } from './helpers/worker-capabilities.js';
import * as physiology from '../src/adapters/pi-physiology.js';

const roots: string[] = [];
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllEnvs();
  for (const root of roots.splice(0)) fs.rmSync(root, { recursive: true, force: true });
});

test('failed prompt assembly leaves a runtime advisory available on the successful retry', async () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'pi-advisory-retry-'));
  roots.push(tmp);
  vi.stubEnv('OCTOCODE_HOME', tmp);
  vi.stubEnv('OCTOCODE_AGENT_DIR', tmp);
  vi.stubEnv('OCTOCODE_PI_SUBAGENT', '0');
  let observer: physiology.PiPhysiologyObserver | undefined;
  const register = physiology.registerPiPhysiology;
  vi.spyOn(physiology, 'registerPiPhysiology').mockImplementation((...args) => {
    observer = register(...args);
    return observer;
  });
  const flow = createPiFlowHarness({ cwd: tmp, sessionId: 'advisory-retry' });
  await extension(flow.pi as unknown as PiInstance);
  flow.pi.setActiveTools(['bash']);
  const ctx = {
    cwd: tmp, hasUI: false,
    abort: vi.fn(),
    model: { contextWindow: 100_000 },
    getContextUsage: () => ({ tokens: 95_000, contextWindow: 100_000 }),
    sessionManager: { getSessionId: () => 'advisory-retry', getBranch: () => [] },
  } as unknown as PiContext;
  assert.ok(observer, 'the production extension registered its observer');
  await observer.sessionStart(ctx);
  const handlers = flow.handlers as unknown as Map<string, Array<(event: unknown, context: unknown) => Promise<unknown>>>;
  const invoke = async (systemPrompt: string) => await handlers.get('before_agent_start')!.at(-1)!({ systemPrompt, systemPromptOptions: { skills: [] } }, ctx) as { message?: { content: string } } | undefined;
  const abort = ctx.abort as ReturnType<typeof vi.fn>;
  const start = async (context = ctx) => {
    for (const handler of handlers.get('agent_start') ?? []) await handler({}, context);
  };
  try {
    assert.equal(await invoke('oversized base prompt '.repeat(30_000)), undefined);
    await start({ ...ctx, sessionManager: { getSessionId: () => 'other-session' } });
    expect(abort).not.toHaveBeenCalled();
    await start();
    expect(abort).toHaveBeenCalledTimes(1);
    assert.match((await invoke('Pi base prompt'))?.message?.content ?? '', /inspect_context_headroom/);
    assert.doesNotMatch((await invoke('Pi base prompt'))?.message?.content ?? '', /inspect_context_headroom/);
    await start();
    expect(abort).toHaveBeenCalledTimes(1);
    assert.equal(await invoke('oversized base prompt '.repeat(30_000)), undefined);
    for (const handler of handlers.get('session_start') ?? []) await handler({ reason: 'resume' }, ctx);
    await start();
    expect(abort).toHaveBeenCalledTimes(1);
  } finally {
    await observer.sessionShutdown(ctx);
    for (const handler of handlers.get('session_shutdown') ?? []) await handler({ reason: 'quit' }, ctx);
  }
});

for (const worker of [false, true]) for (const firstTurn of [false, true]) for (const retained of worker ? [false] : [false, true]) {
  test(`recovery reaches ${worker ? 'worker' : 'main'} on ${firstTurn ? 'first' : 'frozen'} turn with retained plan=${retained}`, async () => {
    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'pi-turn-recovery-'));
    roots.push(tmp);
    vi.stubEnv('OCTOCODE_HOME', tmp);
    vi.stubEnv('OCTOCODE_AGENT_DIR', tmp);
    const previous = process.env['OCTOCODE_PI_SUBAGENT'];
    if (worker) process.env['OCTOCODE_PI_SUBAGENT'] = '1';
    else delete process.env['OCTOCODE_PI_SUBAGENT'];
    const branch: never[] = [];
    const ctx = { cwd: tmp!, hasUI: false, sessionManager: { getSessionId: () => 'recovery-hook', getBranch: () => branch } };
    const scope = activePlanScope(ctx as PiContext);
    let unregister: (() => void) | undefined;
    try {
      if (worker) await installAuthenticatedWorkerCapabilityView(['bash'], false);
      const flow = createPiFlowHarness({ cwd: tmp, sessionId: 'recovery-hook' });
      await extension(flow.pi as unknown as PiInstance);
      const pi = flow.pi;
      const handlers = flow.handlers as unknown as Map<string, Array<(event: unknown, context: unknown) => Promise<unknown>>>;
      pi.setActiveTools(['bash']);
      const invoke = async (systemPrompt = 'bounded role') => await handlers.get('before_agent_start')!.at(-1)!({ systemPrompt, systemPromptOptions: { skills: [] } }, ctx) as { systemPrompt?: string; message?: { content: string; details: { estimates: { total: number } } } };
      setPlan(scope, Array.from({ length: 40 }, (_, i) => ({ text: `large-plan-marker-${i} ${'p'.repeat(160)}`, reasoning: 'r'.repeat(500), acceptance: 'a'.repeat(500) })));
      const planContent = renderPlanContext(getCurrentPlanReadModel(ctx as PiContext, scope));
      assert.ok(planContent.length > 32_000 && planContent.length <= 60_000, 'exercise the gap between recovery and plan budgets');
      const initial = firstTurn ? undefined : await invoke();
      if (retained) branch.push({ type: 'custom_message', id: 'retained-plan', parentId: null, timestamp: new Date().toISOString(), customType: 'octocode-context-update', content: planContent, display: false, details: { segments: [{ id: 'active-plan', digest: contentDigest(planContent) }] } } as never);
      const content = 'worker-owned recovery evidence';
      const segment = { version: 1 as const, id: 'owned-memory', kind: 'memory-lead' as const, origin: 'test-owner', authority: 'external-data' as const, scope: 'session' as const, visibility: 'inspectable' as const, rehydrate: 'always' as const, tokenBudget: 100, digest: contentDigest(content) };
      unregister = registerCurrentContextSource(ctx as PiContext, { ...segment, readCurrent: () => content });
      writeRehydrationLedger(createSessionArtifactContext(ctx), { capturedAt: new Date().toISOString(), segments: [segment], segmentContents: { [segment.id]: content }, pendingInteractionIds: [], consumerCursors: {} });
      rehydrateSession(ctx as PiContext, 'compaction');
      assert.equal(hasPendingRehydration(ctx as PiContext), true);
      if (!worker && firstTurn && !retained) {
        assert.equal(await invoke('oversized base prompt '.repeat(30_000)), undefined, 'the hook reports its budget failure without a projection');
        assert.equal(hasPendingRehydration(ctx as PiContext), true, 'a rejected prompt must not consume staged recovery');
      }
      const recovered = await invoke();
      assert.match(recovered.message?.content ?? '', /worker-owned recovery evidence/);
      assert.equal(hasPendingRehydration(ctx as PiContext), false);
      if (worker || retained) assert.doesNotMatch(recovered.message?.content ?? '', /large-plan-marker/);
      else assert.ok(recovered.message?.content.includes(planContent), 'restore every current plan row and contract');
      assert.equal(recovered.message!.details.estimates.total, Math.ceil(recovered.message!.content.length / 4));
      if (initial) assert.equal(recovered.systemPrompt, initial.systemPrompt);
      assert.equal((await invoke()).message, undefined);
    } finally {
      unregister?.();
      clearPlan(scope);
      if (previous === undefined) delete process.env['OCTOCODE_PI_SUBAGENT'];
      else process.env['OCTOCODE_PI_SUBAGENT'] = previous;
    }
  });
}
