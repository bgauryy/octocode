import { createHash } from 'node:crypto';
import { utcNow, type SqliteLike } from './schema.js';

// This module owns normalized capability overrides. It retains its published
// mcp-state path for package compatibility while sharing the same precedence
// contract with skill enablement.

export const MCP_GLOBAL_SCOPE = '*';

interface ReadableSqlite extends SqliteLike {
  prepare(sql: string): {
    run(...params: unknown[]): unknown;
    get(...params: unknown[]): unknown;
    all(...params: unknown[]): unknown[];
  };
}

export interface McpServerOverride {
  scopeKey: string;
  serverKey: string;
  enabled: boolean;
}

export interface McpToolOverride extends McpServerOverride {
  toolName: string;
}

export interface SkillOverride {
  scopeKey: string;
  skillKey: string;
  enabled: boolean;
}

function assertKey(label: string, value: string): void {
  if (!value || value.length > 512 || value.includes('\0')) throw new Error(`Invalid ${label}`);
}

export function normalizeSkillKey(name: string): string {
  const key = name.replace(/\s+/g, ' ').trim().toLowerCase();
  assertKey('skill', key);
  return key;
}

function normalizeSkillSourceKey(name: string, sourceId: string): string {
  assertKey('skill source', sourceId);
  return `${normalizeSkillKey(name)}@${createHash('sha256').update(sourceId).digest('hex')}`;
}

export function setSkillEnabled(db: SqliteLike, scopeKey: string, skillName: string, enabled: boolean, sourceId?: string): void {
  assertKey('skill scope', scopeKey);
  const skillKey = sourceId === undefined ? normalizeSkillKey(skillName) : normalizeSkillSourceKey(skillName, sourceId);
  db.prepare(`INSERT INTO skill_overrides (scope_key, skill_key, enabled, updated_at)
    VALUES (?, ?, ?, ?)
    ON CONFLICT(scope_key, skill_key) DO UPDATE SET enabled=excluded.enabled, updated_at=excluded.updated_at`)
    .run(scopeKey, skillKey, enabled ? 1 : 0, utcNow());
}

export function getSkillEnablement(
  db: ReadableSqlite,
  scopeKey: string,
  skillName: string,
  configDefault = true,
  sourceId?: string,
): boolean {
  assertKey('skill scope', scopeKey);
  const skillKey = normalizeSkillKey(skillName);
  const sourceKey = sourceId === undefined ? undefined : normalizeSkillSourceKey(skillName, sourceId);
  const read = (scope: string, key: string) => db.prepare('SELECT enabled FROM skill_overrides WHERE scope_key=? AND skill_key=?').get(scope, key);
  const rows = sourceKey === undefined
    ? [read(scopeKey, skillKey), read(MCP_GLOBAL_SCOPE, skillKey)]
    : [read(scopeKey, sourceKey), read(scopeKey, skillKey), read(MCP_GLOBAL_SCOPE, sourceKey), read(MCP_GLOBAL_SCOPE, skillKey)];
  for (const row of rows) {
    if (row && typeof row === 'object') return Number((row as { enabled: number }).enabled) === 1;
  }
  return configDefault;
}

export function listSkillOverrides(db: ReadableSqlite, scopeKey: string): SkillOverride[] {
  return db.prepare('SELECT scope_key, skill_key, enabled FROM skill_overrides WHERE scope_key IN (?, ?) ORDER BY scope_key DESC, skill_key')
    .all(scopeKey, MCP_GLOBAL_SCOPE)
    .map((row) => {
      const value = row as { scope_key: string; skill_key: string; enabled: number };
      return { scopeKey: value.scope_key, skillKey: value.skill_key, enabled: value.enabled === 1 };
    });
}

export function setMcpServerEnabled(db: SqliteLike, scopeKey: string, serverKey: string, enabled: boolean): void {
  assertKey('MCP scope', scopeKey);
  assertKey('MCP server', serverKey);
  db.prepare(`INSERT INTO mcp_server_overrides (scope_key, server_key, enabled, updated_at)
    VALUES (?, ?, ?, ?)
    ON CONFLICT(scope_key, server_key) DO UPDATE SET enabled=excluded.enabled, updated_at=excluded.updated_at`)
    .run(scopeKey, serverKey, enabled ? 1 : 0, utcNow());
}

export function setMcpToolEnabled(db: SqliteLike, scopeKey: string, serverKey: string, toolName: string, enabled: boolean): void {
  assertKey('MCP scope', scopeKey);
  assertKey('MCP server', serverKey);
  assertKey('MCP tool', toolName);
  db.prepare(`INSERT INTO mcp_tool_overrides (scope_key, server_key, tool_name, enabled, updated_at)
    VALUES (?, ?, ?, ?, ?)
    ON CONFLICT(scope_key, server_key, tool_name) DO UPDATE SET enabled=excluded.enabled, updated_at=excluded.updated_at`)
    .run(scopeKey, serverKey, toolName, enabled ? 1 : 0, utcNow());
}

function readFlag(db: ReadableSqlite, table: string, scopeKey: string, serverKey: string, toolName?: string): boolean | undefined {
  const row = toolName === undefined
    ? db.prepare(`SELECT enabled FROM ${table} WHERE scope_key=? AND server_key=?`).get(scopeKey, serverKey)
    : db.prepare(`SELECT enabled FROM ${table} WHERE scope_key=? AND server_key=? AND tool_name=?`).get(scopeKey, serverKey, toolName);
  if (!row || typeof row !== 'object') return undefined;
  return Number((row as { enabled: number }).enabled) === 1;
}

/** Resolve the documented precedence; absence means inherit from the next level. */
export function getMcpEnablement(
  db: ReadableSqlite,
  scopeKey: string,
  serverKey: string,
  toolName: string | undefined,
  configDefault: boolean,
): boolean {
  const candidates: Array<boolean | undefined> = toolName
    ? [
        readFlag(db, 'mcp_tool_overrides', scopeKey, serverKey, toolName),
        readFlag(db, 'mcp_server_overrides', scopeKey, serverKey),
        readFlag(db, 'mcp_tool_overrides', MCP_GLOBAL_SCOPE, serverKey, toolName),
        readFlag(db, 'mcp_server_overrides', MCP_GLOBAL_SCOPE, serverKey),
      ]
    : [
        readFlag(db, 'mcp_server_overrides', scopeKey, serverKey),
        readFlag(db, 'mcp_server_overrides', MCP_GLOBAL_SCOPE, serverKey),
      ];
  return candidates.find((value) => value !== undefined) ?? configDefault;
}

export function listMcpOverrides(db: ReadableSqlite, scopeKey: string): {
  servers: McpServerOverride[];
  tools: McpToolOverride[];
} {
  const servers = db.prepare('SELECT scope_key, server_key, enabled FROM mcp_server_overrides WHERE scope_key=? ORDER BY server_key')
    .all(scopeKey).map((row) => {
      const value = row as { scope_key: string; server_key: string; enabled: number };
      return { scopeKey: value.scope_key, serverKey: value.server_key, enabled: value.enabled === 1 };
    });
  const tools = db.prepare('SELECT scope_key, server_key, tool_name, enabled FROM mcp_tool_overrides WHERE scope_key=? ORDER BY server_key, tool_name')
    .all(scopeKey).map((row) => {
      const value = row as { scope_key: string; server_key: string; tool_name: string; enabled: number };
      return { scopeKey: value.scope_key, serverKey: value.server_key, toolName: value.tool_name, enabled: value.enabled === 1 };
    });
  return { servers, tools };
}
