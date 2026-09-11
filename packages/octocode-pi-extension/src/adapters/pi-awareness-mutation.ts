import fs from 'node:fs';
import { isPersistentStorageEnabledForExtension as isPersistentStorageEnabled } from '@octocodeai/config';
import { connectDb, insertEditLog, loadWorkspacePolicy } from '@octocodeai/octocode-awareness';
import { resolveAwarenessDatabase } from '../tools/awareness-context.js';
import { runAwarenessPreEdit, resolveAwarenessCoordinationScope } from '../assets.js';
import { openPersistentAwareness } from '../tools/storage-policy.js';
import { createAwarenessMutationGate } from '../tools/awareness-mutation-gate.js';
import { getAwarenessAgentId, getAwarenessAgentIdentity } from '../tools/awareness-shared.js';
import type { PiContext } from '../types.js';

export const awarenessMutationGate = createAwarenessMutationGate({
  enabled: isPersistentStorageEnabled,
  trackWork: (workspace) => loadWorkspacePolicy(workspace).policy.hooks.profile !== 'coordination',
  storeExists: (workspace) => {
    if (!isPersistentStorageEnabled()) return false;
    const scope = resolveAwarenessCoordinationScope(workspace);
    return fs.existsSync(resolveAwarenessDatabase(workspace, scope));
  },
  queryTarget: (target, workspace, agentId) => {
    const scope = resolveAwarenessCoordinationScope(workspace);
    const result = runAwarenessPreEdit({
      workspace,
      scope,
      dbPath: resolveAwarenessDatabase(workspace, scope),
      agentId,
      host: 'pi',
      event: { toolName: 'write', input: { path: target } },
    });
    return { blocked: result.blocked, message: result.message };
  },
  startWork: (target, workspace, agentId) => {
    const aw = openPersistentAwareness({ workspace, scope: resolveAwarenessCoordinationScope(workspace) });
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
    const aw = openPersistentAwareness({ workspace, scope: resolveAwarenessCoordinationScope(workspace) });
    try { aw.endWork({ filePath: target, agentId, runId }); }
    finally { aw.close(); }
  },
  recordEdit: (target, workspace, agentId) => {
    if (!isPersistentStorageEnabled()) return;
    const scope = resolveAwarenessCoordinationScope(workspace);
    const database = connectDb(resolveAwarenessDatabase(workspace, scope));
    try {
      insertEditLog(database, {
        agentId,
        filePath: target,
        operation: 'update',
        workspacePath: workspace,
        artifact: 'pi-native-hook',
      });
    } finally {
      database.close();
    }
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
    awareness = openPersistentAwareness({ workspace: cwd });
    const agentId = getAwarenessAgentId(cwdOverride === undefined ? ctx : undefined);
    if (action === 'join') awareness.joinAgent({ ...getAwarenessAgentIdentity(ctx), role: 'lead' });
    else awareness.leaveAgent({ agentId });
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
