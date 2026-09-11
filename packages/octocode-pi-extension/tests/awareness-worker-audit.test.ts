import { describe, expect, it, vi } from 'vitest';
import { inspectWorkerAwareness } from '../src/tools/awareness-worker-audit.js';

describe('terminal worker Awareness inspection', () => {
  it('reads debt by exact native identity with zero age and never creates verification', () => {
    const close = vi.fn();
    const auditChecks = vi.fn(() => ({ pendingCount: 1, pending: [{ runId: 'late-artifact-run' }], staleActiveCount: 1, staleActive: [{ runId: 'orphan' }] }));
    const result = inspectWorkerAwareness({ awarenessAgentId: 'native-worker', awarenessWorkspace: '/repo' }, () => ({ auditChecks, close }));
    expect(auditChecks).toHaveBeenCalledWith({ agentId: 'native-worker', minAgeMs: 0 });
    expect(result).toMatchObject({ agentId: 'native-worker', status: 'pending', pendingCount: 1, runIds: ['late-artifact-run'], staleActiveCount: 1, staleActiveRunIds: ['orphan'] });
    expect(close).toHaveBeenCalledOnce();
  });
  it('reports missing identity and unavailable stores as unknown, not clean', () => {
    const open = vi.fn(() => { throw new Error('memory-only'); });
    expect(inspectWorkerAwareness({}, open)).toMatchObject({ status: 'unavailable' });
    expect(open).not.toHaveBeenCalled();
    expect(inspectWorkerAwareness({ awarenessAgentId: 'a', awarenessWorkspace: '/repo' }, open)).toMatchObject({ status: 'unavailable' });
  });
  it('bounds IDs without hiding the real count or the executable inspection route', () => {
    const result = inspectWorkerAwareness({ awarenessAgentId: 'a', awarenessWorkspace: '/repo' }, () => ({
      auditChecks: () => ({ pendingCount: 51, pending: Array.from({ length: 51 }, (_, n) => ({ runId: `r-${n}` })), staleActiveCount: 0, staleActive: [] }), close() {},
    }));
    expect(result).toMatchObject({ pendingCount: 51, partial: true, next: { operation: 'work.verify', params: { action: 'audit' } } });
    expect(result.runIds).toHaveLength(20);
  });
});
