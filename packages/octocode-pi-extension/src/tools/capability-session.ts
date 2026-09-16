import path from 'node:path';
import { CapabilitySnapshotSchema, type CapabilitySnapshot } from '../contracts/capabilities.js';
import { capabilityDefinitionRevision } from '../contracts/capability-sources.js';
import type { DiscoveredSkill } from './skill-discovery.js';
import type { McpCatalogSnapshotV1 } from './mcp/catalog.js';

const snapshots = new Map<string, CapabilitySnapshot>();

/** One revision of the effective catalog; timestamps and credentials are excluded. */
export function buildEffectiveCapabilitySnapshot(nativeTools: string[], skills: DiscoveredSkill[], mcp?: McpCatalogSnapshotV1): CapabilitySnapshot {
  const content = {
    nativeTools: [...new Set(nativeTools)].sort(),
    skills: skills.map(skill => ({ id: skill.sourceId ?? skill.path, name: skill.name, path: skill.path, description: skill.description, ...(skill.revision ? { revision: skill.revision } : {}) })).sort((a, b) => a.id.localeCompare(b.id)),
    mcpTools: (mcp?.servers ?? []).flatMap(server => server.tools.map(tool => ({ server: server.name, tool: tool.name, description: tool.description, instructions: server.instructions, inputSchema: tool.inputSchema, schemaDigest: tool.schemaDigest }))).sort((a, b) => a.server.localeCompare(b.server) || a.tool.localeCompare(b.tool)),
  };
  return CapabilitySnapshotSchema.parse({ schemaVersion: 1, revision: capabilityDefinitionRevision(content), ...content });
}

export function publishSessionCapabilities(cwd: string, snapshot: CapabilitySnapshot): void {
  snapshots.set(path.resolve(cwd), snapshot);
}

export function getSessionCapabilities(cwd: string): CapabilitySnapshot | undefined {
  const snapshot = snapshots.get(path.resolve(cwd));
  return snapshot ? structuredClone(snapshot) : undefined;
}

export function clearSessionCapabilities(cwd: string): void {
  snapshots.delete(path.resolve(cwd));
}
