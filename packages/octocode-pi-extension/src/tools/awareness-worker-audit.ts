import { openPersistentAwareness } from './storage-policy.js';
import { loadWorkspacePolicy } from '@octocodeai/octocode-awareness/host';

interface WorkerIdentity { awarenessAgentId?: string; awarenessWorkspace?: string; awarenessDatabase?: string }
interface AuditStore {
  auditChecks(params: { agentId: string; minAgeMs: number }): { pendingCount: number; pending: Array<{ runId: string }>; staleActiveCount: number; staleActive: Array<{ runId: string }> };
  close(): void;
}

export interface WorkerAwarenessInspection {
  agentId?: string;
  status: 'clear' | 'pending' | 'unavailable';
  observedAt: string;
  pendingCount?: number;
  staleActiveCount?: number;
  staleActiveRunIds?: string[];
  runIds?: string[];
  partial?: boolean;
  next?: { operation: 'work.verify'; params: { action: 'audit' } };
}

/** Routine worker completion does not opt a session into verification bookkeeping. */
export function inspectWorkerAwarenessAutomatically(identity: WorkerIdentity): WorkerAwarenessInspection | undefined {
  if (!identity.awarenessWorkspace) return undefined;
  try {
    if (loadWorkspacePolicy(identity.awarenessWorkspace).policy.hooks.profile === 'coordination') return undefined;
  } catch {
    return { agentId: identity.awarenessAgentId, status: 'unavailable', observedAt: new Date().toISOString() };
  }
  return inspectWorkerAwareness(identity);
}

/** Read after terminal artifacts. A clean debt inspection is not successful verification. */
export function inspectWorkerAwareness(
  identity: WorkerIdentity,
  open: (options: { workspace: string; dbPath?: string }) => AuditStore = openPersistentAwareness,
): WorkerAwarenessInspection {
  const { awarenessAgentId: agentId, awarenessWorkspace: workspace } = identity;
  const base = { ...(agentId ? { agentId } : {}), observedAt: new Date().toISOString() };
  if (!agentId || !workspace) return { ...base, status: 'unavailable' };
  let store: AuditStore | undefined;
  try {
    store = open({ workspace, dbPath: identity.awarenessDatabase });
    const audit = store.auditChecks({ agentId, minAgeMs: 0 });
    return {
      ...base, status: audit.pendingCount || audit.staleActiveCount ? 'pending' : 'clear', pendingCount: audit.pendingCount,
      staleActiveCount: audit.staleActiveCount, staleActiveRunIds: audit.staleActive.slice(0, 20).map(row => row.runId),
      runIds: audit.pending.slice(0, 20).map(row => row.runId), partial: audit.pendingCount > 20 || audit.staleActiveCount > 20,
      next: { operation: 'work.verify', params: { action: 'audit' } },
    };
  } catch {
    return { ...base, status: 'unavailable' };
  } finally { store?.close(); }
}
