/**
 * Cross-target query-shaping helpers used by several research-target
 * adapters: error-message extraction, cloned-path resolution, `from`/owner-
 * repo splitting, and forwarding OQL page/limit params into backing-tool
 * params bags.
 */
import { statSync } from 'node:fs';
import nodePath from 'node:path';
import type { CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { OqlQuery, QuerySource } from '../../types.js';

export function errorText(value: unknown, fallback: string): string {
  if (typeof value === 'string' && value.trim()) return value;
  if (value instanceof Error && value.message) return value.message;
  if (value && typeof value === 'object') {
    const record = value as { error?: unknown; message?: unknown };
    if (typeof record.error === 'string' && record.error.trim()) {
      return record.error;
    }
    if (typeof record.message === 'string' && record.message.trim()) {
      return record.message;
    }
    const serialized = JSON.stringify(value);
    if (serialized) return serialized;
  }
  return fallback;
}

export function materializedClonePath(
  result: CallToolResult,
  localPath: string | undefined
): string | undefined {
  if (!localPath || nodePath.isAbsolute(localPath)) return localPath;
  const sc = result.structuredContent as { base?: string } | undefined;
  return sc?.base ? nodePath.join(sc.base, localPath) : localPath;
}

export function isExistingDirectory(path: string): boolean {
  try {
    const resolved = nodePath.isAbsolute(path) ? path : nodePath.resolve(path);
    return statSync(resolved).isDirectory();
  } catch {
    return false;
  }
}

export function splitRepo(source: QuerySource | undefined): {
  owner?: string;
  repo?: string;
} {
  if (!source || source.kind !== 'github') return {};
  if (source.repo && source.repo.includes('/')) {
    const [owner, repo] = source.repo.split('/');
    return { owner, repo };
  }
  return { owner: source.owner };
}

export function params(query: OqlQuery): Record<string, unknown> {
  return query.params ?? {};
}

export function withOqlPaging(
  query: OqlQuery,
  limitKey?: 'limit' | 'perPage'
): Record<string, unknown> {
  const out: Record<string, unknown> = { ...params(query) };
  if (out.page === undefined && query.page !== undefined) {
    out.page = query.page;
  }
  if (limitKey && out[limitKey] === undefined) {
    const limit = query.limit ?? query.itemsPerPage;
    if (limit !== undefined) out[limitKey] = limit;
  }
  return out;
}
