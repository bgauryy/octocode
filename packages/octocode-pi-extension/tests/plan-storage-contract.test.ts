import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, expect, test } from 'vitest';
import { activePlanScope, adoptPlanFromBranch, clearPlan, getPlan, PLAN_ENTRY_TYPE, readPersistedPlanForTests } from '../src/tools/active-plan.js';
import { createSessionArtifactContext, readPlanProjection } from '../src/tools/session-artifacts.js';

const roots: string[] = [];
afterEach(() => { for (const root of roots.splice(0)) fs.rmSync(root, { recursive: true, force: true }); });

test('only current plan snapshots can restore branch and persisted state', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'pi-plan-storage-'));
  roots.push(root);
  const ctx = { cwd: root, sessionManager: { getSessionId: () => path.basename(root) } };
  const scope = activePlanScope(ctx);
  const artifacts = createSessionArtifactContext(ctx);
  const state = {
    version: 3, scope, phase: 'executing', branchSnapshotId: 'retired', generation: 1,
    steps: [{ id: 'step', text: 'retired task', status: 'doing' }],
    updatedAt: new Date().toISOString(),
    capturedAt: new Date().toISOString(),
  };
  try {
    expect(adoptPlanFromBranch(scope, [{ id: 'retired', type: 'custom', customType: PLAN_ENTRY_TYPE, data: state }])).toBe(false);
    expect(getPlan(scope)).toEqual([]);
    artifacts.writeJson('plan/state.json', {
      version: 1, sourceEntryId: 'retired', generation: 1, capturedAt: state.updatedAt, state,
    });
    expect(readPersistedPlanForTests(scope)).toEqual([]);

    const current = { ...state, version: 4, cleared: false, branchSnapshotId: 'current', generation: 2 };
    expect(adoptPlanFromBranch(scope, [{ id: 'current', type: 'custom', customType: PLAN_ENTRY_TYPE, data: current }])).toBe(true);
    expect(getPlan(scope).map(step => step.text)).toEqual(['retired task']);
    expect(readPersistedPlanForTests(scope).map(step => step.text)).toEqual(['retired task']);
    expect(readPlanProjection<{ version: number }>(artifacts)?.state.version).toBe(4);
  } finally {
    clearPlan(scope);
    roots.push(artifacts.root);
  }
});
