import { describe, expect, it } from 'vitest';
import {
  CapabilitySnapshotSchema,
  createWorkerCapabilityGrant,
  FORBIDDEN_WORKER_TOOL_NAMES,
  isForbiddenWorkerTool,
  projectWorkerCapabilitySnapshot,
} from '../../src/contracts/capabilities.js';

const snapshot = {
  schemaVersion: 1 as const,
  revision: 'catalog-1',
  nativeTools: ['MCPTool', 'skill', 'bash'],
  skills: [{ id: 'skill-research', name: 'research', path: '/skills/research', revision: 'one' }],
  mcpTools: [{ server: 'octocode', tool: 'localSearch' }, { server: 'private', tool: 'secrets' }],
};

describe('worker capability contracts', () => {
  it('validates strict snapshots and rejects duplicate identities', () => {
    expect(CapabilitySnapshotSchema.parse(snapshot)).toEqual(snapshot);
    expect(() => CapabilitySnapshotSchema.parse({ ...snapshot, unknown: true })).toThrow();
    expect(() => CapabilitySnapshotSchema.parse({ ...snapshot, nativeTools: ['bash', 'bash'] })).toThrow();
  });

  it('preserves empty selections and rejects unavailable or recursive capabilities', () => {
    const grant = createWorkerCapabilityGrant(snapshot, { workerId: 'worker-1', selection: {} });
    expect(grant).toMatchObject({ nativeTools: [], skills: [], mcpTools: [] });
    expect(() => createWorkerCapabilityGrant(snapshot, { workerId: 'worker-1', selection: { skills: ['missing'] } })).toThrow(/unavailable/i);
    expect(() => createWorkerCapabilityGrant({ ...snapshot, nativeTools: ['agent'] }, { workerId: 'worker-1', selection: { nativeTools: ['agent'] } })).toThrow(/recursive/i);
  });

  it('resolves a unique active skill name to its exact capability identity', () => {
    const grant = createWorkerCapabilityGrant(snapshot, {
      workerId: 'worker-1', selection: { nativeTools: ['skill'], skills: ['research'] },
    });
    expect(grant.skills).toEqual(['skill-research']);
  });

  it('fails closed when an active skill name is ambiguous', () => {
    expect(() => createWorkerCapabilityGrant({
      ...snapshot,
      skills: [...snapshot.skills, { id: 'skill-research-copy', name: 'research', path: '/skills/research-copy', revision: 'two' }],
    }, {
      workerId: 'worker-1', selection: { nativeTools: ['skill'], skills: ['research'] },
    })).toThrow(/ambiguous.*exact skill identity/i);
  });

  it('explains that activated MCP proxies must be granted through MCPTool', () => {
    expect(() => createWorkerCapabilityGrant(snapshot, {
      workerId: 'worker-1', selection: { nativeTools: ['mcp__octocode__localSearch__hash'] },
    })).toThrow(/capabilities\.mcpTools.*MCPTool/i);
  });

  it('keeps the canonical forbidden names and rejects case variants', () => {
    expect(FORBIDDEN_WORKER_TOOL_NAMES).toEqual([
      'agent', 'spawnAgent', 'spawnSubagent', 'callTool', 'callSkill', 'tool-smith', 'skill-smith',
    ]);
    for (const name of FORBIDDEN_WORKER_TOOL_NAMES) {
      expect(isForbiddenWorkerTool(name)).toBe(true);
      expect(isForbiddenWorkerTool(name.toUpperCase())).toBe(true);
    }
  });

  it('projects only granted identities and intersects with current enablement', () => {
    const grant = createWorkerCapabilityGrant(snapshot, {
      workerId: 'worker-1', selection: { nativeTools: ['MCPTool'], skills: ['skill-research'], mcpTools: [{ server: 'octocode', tool: 'localSearch' }] },
    });
    expect(projectWorkerCapabilitySnapshot(snapshot, grant)).toEqual({
      ...snapshot, nativeTools: ['MCPTool'], mcpTools: [{ server: 'octocode', tool: 'localSearch' }],
    });
    expect(projectWorkerCapabilitySnapshot({ ...snapshot, skills: [], mcpTools: [] }, grant)).toMatchObject({ skills: [], mcpTools: [] });
  });
});
