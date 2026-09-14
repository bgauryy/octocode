import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import {
  bindSpawnedWorkerCapabilities,
  configureWorkerCapabilities,
  disposeWorkerCapabilityRuntime,
  getParentWorkerCapabilities,
  initializeWorkerCapabilityRuntime,
  inspectWorkerCapabilityGrants,
  resolveWorkerCapabilitySelection,
  WORKER_CAPABILITY_BINDING_ENV,
} from '../src/tools/worker-capabilities.js';
import { createWorkerBrokerClient } from '../src/tools/mcp/broker.js';
import { executeAgentLifecycle } from '../src/tools/agents/lifecycle.js';
import type { CapabilitySnapshot } from '@octocodeai/agent-contracts/capabilities';

const snapshot: CapabilitySnapshot = {
  schemaVersion: 1, revision: 'catalog-one', nativeTools: ['MCPTool', 'skill', 'file', 'bash'],
  skills: [
    { id: 'research-id', name: 'octocode-research', path: '/skills/research/SKILL.md' },
    { id: 'finance-id', name: 'finance', path: '/skills/finance/SKILL.md' },
  ],
  mcpTools: [
    { server: 'octocode', tool: 'localSearch' },
    { server: 'octocode', tool: 'localFetch' },
    { server: 'octocode', tool: 'ghSearch' },
    { server: 'finance', tool: 'trade' },
  ],
};

describe('task-specific worker runtime', () => {
  beforeEach(async () => initializeWorkerCapabilityRuntime({ snapshot, dispatchMcp: async () => ({ content: [{ type: 'text', text: 'parent transport result' }] }) }));
  afterEach(disposeWorkerCapabilityRuntime);

  it('selects focused role defaults from the current enabled snapshot', () => {
    expect(resolveWorkerCapabilitySelection({ resourceMode: 'octocode', capabilityProfile: 'implementer', tools: ['MCPTool', 'skill', 'web'] }, snapshot)).toEqual({
      nativeTools: ['MCPTool', 'skill'], skills: ['research-id'], mcpTools: [{ server: 'octocode', tool: 'localSearch' }, { server: 'octocode', tool: 'localFetch' }],
    });
    expect(resolveWorkerCapabilitySelection({ resourceMode: 'octocode', capabilityProfile: 'custom', tools: ['skill'] }, snapshot).skills).toEqual([]);
  });

  it('exposes the current parent snapshot revision before any worker exists', async () => {
    const result = await executeAgentLifecycle({ type: 'inspect' });
    expect(JSON.stringify(result.content)).toContain('Parent capability snapshotRevision: catalog-one');
  });

  it('keeps explicit empty capability arrays and lean resources empty', () => {
    const empty = bindSpawnedWorkerCapabilities('empty', { resourceMode: 'octocode', tools: ['MCPTool', 'skill'], capabilityProfile: 'researcher', capabilities: { nativeTools: [], skills: [], mcpTools: [] } });
    expect(empty.params.tools).toEqual([]);
    expect(empty.params.skills).toEqual([]);
    const lean = bindSpawnedWorkerCapabilities('lean', { resourceMode: 'lean', tools: ['bash'], skills: ['/skills/research'], capabilities: { nativeTools: [] } });
    expect(lean.params.skills).toEqual([]);
    expect(lean.grant).toMatchObject({ nativeTools: [], skills: [], mcpTools: [] });
    expect(lean.env[WORKER_CAPABILITY_BINDING_ENV]).toBeDefined();
    expect(() => configureWorkerCapabilities('lean', { snapshotRevision: snapshot.revision, selection: { skills: ['research-id'] } })).toThrow(/Lean workers/);
    expect(() => configureWorkerCapabilities('lean', { snapshotRevision: snapshot.revision, selection: { nativeTools: ['MCPTool'] } })).toThrow(/Lean workers/);
  });

  it('rejects resource grants whose native gateway is absent', () => {
    expect(() => resolveWorkerCapabilitySelection({
      resourceMode: 'octocode', capabilityProfile: 'researcher', tools: ['skill'],
      capabilities: { nativeTools: [], skills: ['research-id'] },
    }, snapshot)).toThrow(/skill grants require the native tool "skill"/i);
    expect(() => resolveWorkerCapabilitySelection({
      resourceMode: 'octocode', capabilityProfile: 'researcher', tools: ['MCPTool'],
      capabilities: { nativeTools: [], mcpTools: [{ server: 'octocode', tool: 'localSearch' }] },
    }, snapshot)).toThrow(/MCP grants require the native tool "MCPTool"/i);
  });

  it('accepts exact skill identities and keeps binding credentials out of public records', () => {
    const child = bindSpawnedWorkerCapabilities('one', {
      resourceMode: 'octocode', tools: ['skill'], capabilities: { skills: ['finance-id'] },
    });
    expect(child.params.skills).toEqual(['/skills/finance/SKILL.md']);
    const binding = JSON.parse(child.env[WORKER_CAPABILITY_BINDING_ENV]!);
    expect(binding.workerId).toBe('one');
    expect(JSON.stringify(inspectWorkerCapabilityGrants())).not.toContain(binding.token);
    expect(() => bindSpawnedWorkerCapabilities('bad', { resourceMode: 'octocode', tools: ['skill'], capabilities: { skills: ['unknown-id'] } })).toThrow(/unavailable/);
    expect(() => bindSpawnedWorkerCapabilities('bad-native', { resourceMode: 'octocode', tools: [], capabilities: { nativeTools: ['agent'] } })).toThrow(/recursive/i);
  });

  it('applies parent configuration across real broker requests while preserving worker identity', async () => {
    const child = bindSpawnedWorkerCapabilities('worker-one', { resourceMode: 'octocode', tools: ['MCPTool', 'skill'], capabilityProfile: 'implementer' });
    const client = createWorkerBrokerClient(JSON.parse(child.env[WORKER_CAPABILITY_BINDING_ENV]!));
    await client.readCapabilities(true);
    expect(() => configureWorkerCapabilities('worker-one', {
      snapshotRevision: snapshot.revision,
      selection: { nativeTools: [], mcpTools: [{ server: 'octocode', tool: 'localSearch' }] },
    })).toThrow(/MCP grants require the native tool "MCPTool"/i);
    expect(getParentWorkerCapabilities('worker-one')?.grant.revision).toBe(child.grant!.revision);
    const updated = configureWorkerCapabilities('worker-one', { snapshotRevision: snapshot.revision, selection: { nativeTools: ['skill'], skills: ['finance-id'] } });
    expect(updated.revision).toBeGreaterThan(child.grant!.revision);
    await expect(client.dispatch({ action: 'call', server: 'octocode', tool: 'localSearch' })).rejects.toThrow(/not granted/);
    expect(getParentWorkerCapabilities('worker-one')?.grant.skills).toEqual([]);
    const resumed = await client.readCapabilities(true);
    expect(resumed.grant).toMatchObject({ workerId: 'worker-one', revision: updated.revision, skills: ['finance-id'], nativeTools: ['skill'] });
  });
});
