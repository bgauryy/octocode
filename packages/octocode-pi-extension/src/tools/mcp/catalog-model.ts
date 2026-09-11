import { createHash } from 'node:crypto';

export const MCP_CATALOG_SNAPSHOT_VERSION = 1 as const;

export interface McpCatalogSourceIdentity {
  scope: string;
  path: string;
}

export interface McpCatalogToolSnapshot {
  name: string;
  description?: string;
  inputSchema: unknown;
  schemaDigest: string;
}

export interface McpCatalogServerSnapshot {
  name: string;
  configSignature: string;
  instructions?: string;
  tools: McpCatalogToolSnapshot[];
}

export interface McpCatalogSnapshotV1 {
  version: typeof MCP_CATALOG_SNAPSHOT_VERSION;
  workspaceKey: string;
  capturedAt: string;
  configDigest: string;
  servers: McpCatalogServerSnapshot[];
}

export interface McpCatalogToolInput {
  name: string;
  description?: string;
  inputSchema: unknown;
}

export interface McpCatalogServerInput {
  name: string;
  instructions?: string;
  tools: McpCatalogToolInput[];
}

export interface BuildMcpCatalogSnapshotOptions {
  cwd: string;
  sources: McpCatalogSourceIdentity[];
  configSignatures: Record<string, string>;
  servers: McpCatalogServerInput[];
  capturedAt?: string;
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

export function sha256(text: string): string {
  return createHash('sha256').update(text).digest('hex');
}

function canonicalize(value: unknown, seen: Set<object>): unknown {
  if (value === null || typeof value === 'string' || typeof value === 'boolean') return value;
  if (typeof value === 'number') return Number.isFinite(value) ? value : null;
  if (Array.isArray(value)) return value.map((item) => canonicalize(item, seen));
  if (!isRecord(value)) return null;
  if (seen.has(value)) throw new Error('MCP schema must not contain cycles');
  seen.add(value);
  const normalized: Record<string, unknown> = {};
  for (const key of Object.keys(value).sort()) {
    const item = value[key];
    if (item !== undefined) normalized[key] = canonicalize(item, seen);
  }
  seen.delete(value);
  return normalized;
}

export function stableJson(value: unknown): string {
  return JSON.stringify(canonicalize(value, new Set()));
}

export function stableSchemaDigest(schema: unknown): string {
  return sha256(stableJson(schema));
}
