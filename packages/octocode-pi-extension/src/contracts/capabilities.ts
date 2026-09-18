/** Effective capability identities shared by parent catalogs and bounded workers. */
import { z } from 'zod';

const identity = z.string().trim().min(1).max(4096);
const description = z.string().max(100_000).optional();
export const McpToolIdentitySchema = z.strictObject({ server: identity, tool: identity });
export const CapabilitySkillSchema = z.strictObject({
  id: identity,
  name: identity,
  path: identity,
  description,
  revision: identity.optional(),
});
export const CapabilityMcpToolSchema = McpToolIdentitySchema.extend({
  description,
  instructions: description,
  inputSchema: z.unknown().optional(),
  schemaDigest: identity.optional(),
});
export const WorkerCapabilitySelectionSchema = z.strictObject({
  nativeTools: z.array(identity).max(500).optional().describe('Native tool names from the effective parent snapshot. Skill and MCP grants also require their native gateway tools.'),
  skills: z.array(identity).max(5000).optional().describe('Exact skill identities or unique active parent skill names. Load/enable a skill in the parent first; requires the native "skill" gateway.'),
  mcpTools: z.array(McpToolIdentitySchema).max(10_000).optional().describe('Exact MCP server/tool identities. Requires the native "MCPTool" gateway; activated proxy names are not native tools.'),
});

export type McpToolIdentity = z.infer<typeof McpToolIdentitySchema>;
export type WorkerCapabilitySelection = z.infer<typeof WorkerCapabilitySelectionSchema>;
export type CapabilitySkill = z.infer<typeof CapabilitySkillSchema>;
export type CapabilityMcpTool = z.infer<typeof CapabilityMcpToolSchema>;

/** Canonical worker-excluded tool names; matching is case-insensitive. */
export const FORBIDDEN_WORKER_TOOL_NAMES = Object.freeze([
  'agent', 'spawnAgent', 'spawnSubagent', 'callTool', 'callSkill', 'tool-smith', 'skill-smith',
] as const);
const forbiddenWorkerToolNamesLower = new Set(FORBIDDEN_WORKER_TOOL_NAMES.map(name => name.toLowerCase()));

export function mcpToolIdentityKey(value: McpToolIdentity): string {
  return JSON.stringify([value.server, value.tool]);
}

function requireUnique(values: string[], ctx: z.RefinementCtx, field: string): void {
  if (new Set(values).size !== values.length) {
    ctx.addIssue({ code: 'custom', path: [field], message: 'Capability identities must be unique.' });
  }
}

export const CapabilitySnapshotSchema = z.strictObject({
  schemaVersion: z.literal(1),
  revision: identity,
  nativeTools: z.array(identity).max(500),
  skills: z.array(CapabilitySkillSchema).max(5000),
  mcpTools: z.array(CapabilityMcpToolSchema).max(10_000),
}).superRefine((value, ctx) => {
  requireUnique(value.nativeTools, ctx, 'nativeTools');
  requireUnique(value.skills.map(skill => skill.id), ctx, 'skills');
  requireUnique(value.mcpTools.map(mcpToolIdentityKey), ctx, 'mcpTools');
});

export const WorkerCapabilityGrantSchema = z.strictObject({
  schemaVersion: z.literal(1),
  workerId: identity,
  revision: z.number().int().positive(),
  snapshotRevision: identity,
  nativeTools: z.array(identity).max(500),
  skills: z.array(identity).max(5000),
  mcpTools: z.array(McpToolIdentitySchema).max(10_000),
}).superRefine((value, ctx) => {
  requireUnique(value.nativeTools, ctx, 'nativeTools');
  requireUnique(value.skills, ctx, 'skills');
  requireUnique(value.mcpTools.map(mcpToolIdentityKey), ctx, 'mcpTools');
  if (value.nativeTools.some(isForbiddenWorkerTool)) {
    ctx.addIssue({ code: 'custom', path: ['nativeTools'], message: 'Recursive agent and smith tools cannot be granted to workers.' });
  }
});

export type CapabilitySnapshot = z.infer<typeof CapabilitySnapshotSchema>;
export type WorkerCapabilityGrant = z.infer<typeof WorkerCapabilityGrantSchema>;

/** Dynamic tool/skill authoring can spawn smiths, so it is parent-owned too. */
export function isForbiddenWorkerTool(name: string): boolean {
  return forbiddenWorkerToolNamesLower.has(name.toLowerCase());
}

export function createWorkerCapabilityGrant(
  rawSnapshot: CapabilitySnapshot,
  options: { workerId: string; revision?: number; selection: WorkerCapabilitySelection; snapshotRevision?: string },
): WorkerCapabilityGrant {
  const snapshot = CapabilitySnapshotSchema.parse(rawSnapshot);
  const selection = WorkerCapabilitySelectionSchema.parse(options.selection);
  if (options.snapshotRevision !== undefined && options.snapshotRevision !== snapshot.revision) {
    throw new Error('Capability snapshot revision is stale; inspect the current parent catalog before granting access.');
  }
  const native = new Set(snapshot.nativeTools);
  const skills = new Set(snapshot.skills.map(skill => skill.id));
  const skillIdsByName = new Map<string, string[]>();
  for (const skill of snapshot.skills) {
    const ids = skillIdsByName.get(skill.name) ?? [];
    ids.push(skill.id);
    skillIdsByName.set(skill.name, ids);
  }
  const mcp = new Set(snapshot.mcpTools.map(mcpToolIdentityKey));
  for (const tool of selection.nativeTools ?? []) {
    if (isForbiddenWorkerTool(tool)) throw new Error(`Recursive worker tool cannot be granted: ${tool}`);
    if (!native.has(tool)) {
      if (tool.startsWith('mcp__')) {
        throw new Error('Activated MCP proxy tools are not native worker tools; select the server/tool under capabilities.mcpTools and include "MCPTool".');
      }
      throw new Error(`Native tool is unavailable in the effective parent snapshot: ${tool}`);
    }
  }
  const selectedSkillIds = (selection.skills ?? []).map(selector => {
    if (skills.has(selector)) return selector;
    const matches = skillIdsByName.get(selector) ?? [];
    if (matches.length === 1) return matches[0]!;
    if (matches.length > 1) throw new Error(`Skill name is ambiguous in the effective parent snapshot: ${selector}. Use an exact skill identity.`);
    throw new Error(`Skill is unavailable in the effective parent snapshot: ${selector}`);
  });
  for (const target of selection.mcpTools ?? []) {
    if (!mcp.has(mcpToolIdentityKey(target))) throw new Error(`MCP tool is unavailable in the effective parent snapshot: ${target.server}/${target.tool}`);
  }
  return WorkerCapabilityGrantSchema.parse({
    schemaVersion: 1, workerId: options.workerId, revision: options.revision ?? 1,
    snapshotRevision: snapshot.revision,
    nativeTools: selection.nativeTools ?? [], skills: selectedSkillIds, mcpTools: selection.mcpTools ?? [],
  });
}

/** Intersect every projection with live parent enablement; a grant never enables a source. */
export function projectWorkerCapabilitySnapshot(
  rawSnapshot: CapabilitySnapshot,
  rawGrant: WorkerCapabilityGrant,
): CapabilitySnapshot {
  const snapshot = CapabilitySnapshotSchema.parse(rawSnapshot);
  const grant = WorkerCapabilityGrantSchema.parse(rawGrant);
  const native = new Set(grant.nativeTools);
  const skills = new Set(grant.skills);
  const mcp = new Set(grant.mcpTools.map(mcpToolIdentityKey));
  return {
    ...snapshot,
    nativeTools: snapshot.nativeTools.filter(name => native.has(name) && !isForbiddenWorkerTool(name)),
    skills: snapshot.skills.filter(skill => skills.has(skill.id)),
    mcpTools: snapshot.mcpTools.filter(tool => mcp.has(mcpToolIdentityKey(tool))),
  };
}

export function intersectWorkerCapabilityGrant(snapshot: CapabilitySnapshot, grant: WorkerCapabilityGrant): WorkerCapabilityGrant {
  const projected = projectWorkerCapabilitySnapshot(snapshot, grant);
  return {
    ...grant, snapshotRevision: snapshot.revision,
    nativeTools: projected.nativeTools, skills: projected.skills.map(skill => skill.id),
    mcpTools: projected.mcpTools.map(({ server, tool }) => ({ server, tool })),
  };
}
