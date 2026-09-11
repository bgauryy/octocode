import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, test, vi } from 'vitest';
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

const roots: string[] = [];
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllEnvs();
  for (const root of roots.splice(0)) fs.rmSync(root, { recursive: true, force: true });
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
