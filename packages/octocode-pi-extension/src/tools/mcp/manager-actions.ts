import type { FooterDensity } from '../../ui-extras.js';
import type { PermissionLevel } from '../../contracts/protocols.js';
import { EFFORT_LEVELS, type EffortLevel } from '../effort-dial.js';

export type McpManagerAction = (
  | { action: 'open-plan' }
  | { action: 'enable' | 'disable'; server: string; tool?: string; scope: 'project' | 'global' }
  | { action: 'add'; server: string; scope: 'project' | 'global'; config: Record<string, unknown> }
  | { action: 'remove' | 'restart' | 'connect' | 'retry'; server: string; scope: 'project' | 'global' }
  | { action: 'enable-skill' | 'disable-skill'; skill: string; scope: 'project' | 'global' }
  | { action: 'set-footer-density'; density: FooterDensity; expectedRevision?: string }
  | { action: 'set-permission-level'; level: PermissionLevel; expectedRevision?: string }
  | { action: 'set-effort'; level: EffortLevel; expectedRevision?: string }
  | { action: 'set-theme'; theme: 'dark' | 'light'; expectedRevision?: string }
  | { action: 'review-hook'; source: string; hash: string; expectedRevision?: string }
  | { action: 'enable-hook' | 'disable-hook'; source: string; expectedRevision?: string }
  | { action: 'review-skill' | 'review-mcp'; source: string; hash: string; scope: 'project' | 'global' }
) & { capabilityRevision?: string };

const SERVER_NAME = /^[A-Za-z0-9_.-]{1,64}$/;
const ENV_NAME = /^[A-Za-z_][A-Za-z0-9_]*$/;
const HTTP_HEADER_NAME = /^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/;
const SKILL_NAME = /^[^\0\r\n]{1,160}$/;

export function parseMcpManagerAction(raw: unknown): McpManagerAction {
  const action = parseMcpManagerActionPayload(raw);
  const stamp = (raw as Record<string, unknown>)['capabilityRevision'];
  if (stamp !== undefined && (typeof stamp !== 'string' || !/^sha256:[a-f0-9]{64}$/.test(stamp))) throw new Error('Invalid capability revision');
  return { ...action, ...(typeof stamp === 'string' ? { capabilityRevision: stamp } : {}) };
}

function parseMcpManagerActionPayload(raw: unknown): McpManagerAction {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) throw new Error('Invalid MCP action');
  const value = raw as Record<string, unknown>;
  for (const key of Object.keys(value)) {
    if (!['action', 'server', 'scope', 'tool', 'config', 'skill', 'density', 'level', 'theme', 'source', 'hash', 'expectedRevision', 'capabilityRevision'].includes(key)) throw new Error(`Unsupported settings action field: ${key}`);
  }
  const action = value['action'];
  if (action === 'open-plan') return { action };
  const expectedRevision = typeof value['expectedRevision'] === 'string' ? value['expectedRevision'] : undefined;
  const server = value['server'];
  if (action === 'set-effort') {
    if (!EFFORT_LEVELS.includes(value['level'] as EffortLevel)) throw new Error('Invalid effort level');
    return { action, level: value['level'] as EffortLevel, ...(expectedRevision ? { expectedRevision } : {}) };
  }
  if (action === 'set-theme') {
    if (value['theme'] !== 'dark' && value['theme'] !== 'light') throw new Error('Invalid theme');
    return { action, theme: value['theme'], ...(expectedRevision ? { expectedRevision } : {}) };
  }
  if (action === 'set-footer-density') {
    if (!['compact', 'default', 'full'].includes(String(value['density']))) throw new Error('Invalid footer density');
    return { action, density: value['density'] as FooterDensity, ...(expectedRevision ? { expectedRevision } : {}) };
  }
  if (action === 'set-permission-level') {
    if (!['default', 'relaxed', 'strict'].includes(String(value['level']))) throw new Error('Invalid permission level');
    return { action, level: value['level'] as PermissionLevel, ...(expectedRevision ? { expectedRevision } : {}) };
  }
  if (action === 'review-hook' || action === 'enable-hook' || action === 'disable-hook') {
    if (typeof value['source'] !== 'string' || value['source'].length > 2048) throw new Error('Invalid hook source');
    if (action === 'review-hook' && (typeof value['hash'] !== 'string' || !/^(sha256:)?[a-f0-9]{64}$/.test(value['hash']))) throw new Error('Invalid hook review hash');
    return {
      action,
      source: value['source'],
      ...(action === 'review-hook' ? { hash: value['hash'] as string } : {}),
      ...(expectedRevision ? { expectedRevision } : {}),
    } as McpManagerAction;
  }
  if (value['scope'] !== undefined && value['scope'] !== 'project' && value['scope'] !== 'global') throw new Error('Invalid MCP scope');
  const scope = value['scope'] === 'global' ? 'global' : 'project';
  if (action === 'review-skill' || action === 'review-mcp') {
    if (typeof value['source'] !== 'string' || !/^sha256:[a-f0-9]{64}$/.test(value['source']) || typeof value['hash'] !== 'string' || !/^sha256:[a-f0-9]{64}$/.test(value['hash'])) throw new Error('Invalid linked source review');
    return { action, source: value['source'], hash: value['hash'], scope };
  }
  if (action === 'enable-skill' || action === 'disable-skill') {
    const skill = value['skill'];
    if (typeof skill !== 'string' || !SKILL_NAME.test(skill)) throw new Error('Invalid skill name');
    return { action, skill, scope };
  }
  if (!['enable', 'disable', 'add', 'remove', 'restart', 'connect', 'retry'].includes(String(action))) throw new Error('Unsupported settings action');
  if (typeof server !== 'string' || !SERVER_NAME.test(server)) throw new Error('Invalid MCP server');
  if (action === 'enable' || action === 'disable') {
    const tool = value['tool'];
    if (tool !== undefined && (typeof tool !== 'string' || !SERVER_NAME.test(tool))) throw new Error('Invalid MCP tool');
    return { action, server, scope, ...(typeof tool === 'string' ? { tool } : {}) };
  }
  if (action === 'add') {
    if (!value['config'] || typeof value['config'] !== 'object' || Array.isArray(value['config'])) throw new Error('MCP add requires config');
    const config = value['config'] as Record<string, unknown>;
    const allowed = new Set(['command', 'args', 'cwd', 'url', 'timeoutMs', 'description', 'instructions', 'envRefs', 'headerRefs', 'auth']);
    for (const key of Object.keys(config)) if (!allowed.has(key)) throw new Error(`Unsupported MCP config field: ${key}`);
    for (const field of ['envRefs', 'headerRefs'] as const) {
      const refs = config[field];
      if (refs === undefined) continue;
      if (!refs || typeof refs !== 'object' || Array.isArray(refs)) throw new Error(`${field} must be an object`);
      for (const [destination, source] of Object.entries(refs as Record<string, unknown>)) {
        const validDestination = field === 'envRefs' ? ENV_NAME.test(destination) : HTTP_HEADER_NAME.test(destination);
        if (!validDestination) throw new Error(`Invalid ${field} destination: ${destination}`);
        if (typeof source !== 'string' || !ENV_NAME.test(source)) throw new Error(`${field} values must be environment variable names`);
      }
    }
    return { action, server, scope, config };
  }
  return { action, server, scope } as McpManagerAction;
}
