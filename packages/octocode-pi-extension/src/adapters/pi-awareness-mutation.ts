import fs from 'node:fs';
import { isPersistentStorageEnabledForExtension as isPersistentStorageEnabled } from '@octocodeai/config';
import { loadWorkspacePolicy, runPreEditLockGate } from '@octocodeai/octocode-awareness/host';
import { resolveAwarenessStorageBindings } from '../tools/awareness-context.js';
import { openPersistentAwareness } from '../tools/storage-policy.js';
import { createAwarenessMutationGate } from '../tools/awareness-mutation-gate.js';
import { getAwarenessAgentId, getAwarenessAgentIdentity } from '../tools/awareness-shared.js';
import type { PiContext } from '../types.js';

function openBoundAwareness(workspace: string): ReturnType<typeof openPersistentAwareness> {
  const storage = resolveAwarenessStorageBindings(workspace);
  return openPersistentAwareness({
    workspace: storage.workspace,
    scope: storage.scope,
    dbPath: storage.database,
  });
}

export const awarenessMutationGate = createAwarenessMutationGate({
  enabled: isPersistentStorageEnabled,
  trackWork: (workspace) => loadWorkspacePolicy(workspace).policy.hooks.profile !== 'coordination',
  storeExists: (workspace) => {
    if (!isPersistentStorageEnabled()) return false;
    return fs.existsSync(resolveAwarenessStorageBindings(workspace).database);
  },
  queryTarget: (target, workspace, agentId) => {
    const storage = resolveAwarenessStorageBindings(workspace);
    const result = runPreEditLockGate({
      workspace: storage.workspace,
      scope: storage.scope,
      dbPath: storage.database,
      agentId,
      host: 'pi',
      event: { toolName: 'write', input: { path: target } },
    });
    return { blocked: result.blocked, message: result.message };
  },
  startWork: (target, workspace, agentId) => {
    const aw = openBoundAwareness(workspace);
    try {
      const existing = aw.listWork({ filePath: target, agentId })[0];
      const work = aw.startWork({
        filePath: target,
        agentId,
        ...(existing ? { runId: existing.runId } : {}),
        reason: 'Automatic Pi mutation presence',
        testPlan: 'Inspect the resulting file and run applicable repository checks before marking this mutation verified',
      });
      return existing ? null : work.runId;
    } finally {
      aw.close();
    }
  },
  endWork: (target, workspace, agentId, runId) => {
    const aw = openBoundAwareness(workspace);
    try { aw.endWork({ filePath: target, agentId, runId }); }
    finally { aw.close(); }
  },
  warn: (message) => console.warn(`[octocode] ${message}`),
});

export function updateAwarenessRegistry(
  action: 'join' | 'leave',
  ctx?: PiContext,
  cwdOverride?: string,
): void {
  const cwd = cwdOverride ?? ctx?.cwd ?? process.cwd();
  let awareness: ReturnType<typeof openPersistentAwareness> | undefined;
  try {
    awareness = openBoundAwareness(cwd);
    const identity = getAwarenessAgentIdentity(cwdOverride === undefined ? ctx : undefined);
    if (action === 'join') awareness.joinAgent({ ...identity, role: 'lead' });
    else awareness.leaveAgent({ agentId: identity.agentId });
  } catch { /* Awareness unresolved — skip */ }
  finally { awareness?.close(); }
}

export function runAwarenessMutationGate(
  event: { toolName?: string; input?: Record<string, unknown> },
  ctx?: PiContext,
): { block?: boolean; reason?: string } | void {
  const workspace = ctx?.cwd ?? process.cwd();
  return awarenessMutationGate.preflight(event, workspace, getAwarenessAgentId(ctx));
}
