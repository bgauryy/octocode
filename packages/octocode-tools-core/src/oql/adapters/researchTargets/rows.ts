/**
 * Result/record shaping shared by every research-target adapter: turning a
 * backing tool's `data` payload into generic `OqlRecordResultRow`s, deriving
 * a citeable `id` per record type, and trimming parent-level metadata/shared
 * repository refs that ride alongside the array payload.
 */
import type { OqlRecordResultRow, QuerySource } from '../../types.js';

/** Known array-valued payload fields, in priority order. */
export const RECORD_ARRAY_KEYS = [
  'repositories',
  'pull_requests',
  'commits',
  'packages',
  'results',
  'locations',
  'references',
  'symbols',
  'strings',
  'entries',
  'incomingCalls',
  'outgoingCalls',
];

export const RECORD_PARENT_METADATA_EXCLUDE = new Set([
  ...RECORD_ARRAY_KEYS,
  'pagination',
  'contentPagination',
  'next',
]);

/** Expand a tool `data` payload into row items (an inner array if present). */
export function expandData(
  data: Record<string, unknown> | undefined
): unknown[] {
  if (!data) return [];
  for (const key of RECORD_ARRAY_KEYS) {
    const v = (data as Record<string, unknown>)[key];
    if (Array.isArray(v)) return v;
  }
  return [data];
}

export function sharedRepositoryRefs(
  parent: Record<string, unknown> | undefined
): Record<string, unknown> | undefined {
  const repositories = parent?.repositories;
  if (
    !repositories ||
    typeof repositories !== 'object' ||
    Array.isArray(repositories)
  ) {
    return undefined;
  }

  const compact: Record<string, unknown> = {};
  for (const [id, value] of Object.entries(repositories)) {
    if (!value || typeof value !== 'object' || Array.isArray(value)) continue;
    const source = value as Record<string, unknown>;
    const repo: Record<string, unknown> = {};
    for (const key of ['repository', 'repositoryDirectory', 'owner', 'repo']) {
      if (typeof source[key] === 'string') repo[key] = source[key];
    }
    if (Object.keys(repo).length > 0) compact[id] = repo;
  }

  return Object.keys(compact).length ? { repositories: compact } : undefined;
}

export function parentMetadata(
  data: Record<string, unknown> | undefined
): Record<string, unknown> | undefined {
  if (!data) return undefined;
  const metadata: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(data)) {
    if (RECORD_PARENT_METADATA_EXCLUDE.has(key)) continue;
    if (
      value === null ||
      typeof value === 'string' ||
      typeof value === 'number' ||
      typeof value === 'boolean'
    ) {
      metadata[key] = value;
    }
  }
  return Object.keys(metadata).length ? metadata : undefined;
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

/**
 * Concise lanes flatten rows to `{ value: "<id> <title…>" }` (e.g. PR rows
 * become "#3536 chore(...)"); keep a citeable identity from the leading token
 * instead of dropping the id entirely.
 */
function valueLeadingToken(d: Record<string, unknown>): string | undefined {
  return typeof d.value === 'string' && d.value.trim()
    ? d.value.trim().split(/\s+/, 1)[0]
    : undefined;
}

/** Citeable identity per record type, extracted from the backend payload. */
function stableId(
  recordType: OqlRecordResultRow['recordType'],
  d: Record<string, unknown>
): string | undefined {
  const s = (k: string): string | undefined =>
    typeof d[k] === 'string' || typeof d[k] === 'number'
      ? String(d[k])
      : undefined;
  switch (recordType) {
    case 'repository':
      return (
        s('fullName') ??
        (s('owner') && s('repo') ? `${s('owner')}/${s('repo')}` : s('url')) ??
        valueLeadingToken(d)
      );
    case 'package': {
      const name = s('name') ?? s('packageName');
      const ver = s('version');
      return name ? (ver ? `${name}@${ver}` : name) : valueLeadingToken(d);
    }
    case 'pullRequest':
      return s('number')
        ? `#${s('number')}`
        : (s('url') ?? valueLeadingToken(d));
    case 'commit':
      return (
        s('sha')?.slice(0, 12) ?? s('oid')?.slice(0, 12) ?? valueLeadingToken(d)
      );
    case 'materialized':
      return s('localPath') ?? s('repoRoot');
    case 'diff':
      // Whole-PR patch rows have no single path — cite the PR number instead.
      return (
        s('path') ??
        s('filename') ??
        (s('number') ? `#${s('number')}` : valueLeadingToken(d))
      );
    case 'semantics': {
      const uri = s('uri');
      const line = s('line') ?? s('startLine');
      return uri ? (line ? `${uri}:${line}` : uri) : undefined;
    }
    case 'research':
      return s('intent') ?? s('goal') ?? 'research';
    case 'graph':
      return s('intent') ? `graph:${s('intent')}` : 'graph';
  }
  return valueLeadingToken(d);
}

export function records(
  items: unknown[],
  recordType: OqlRecordResultRow['recordType'],
  source?: QuerySource,
  metadata?: Record<string, unknown>
): OqlRecordResultRow[] {
  return items.map(item => {
    const data = (
      item && typeof item === 'object'
        ? (item as Record<string, unknown>)
        : { value: item }
    ) as Record<string, unknown>;
    const id = stableId(recordType, data);
    return {
      kind: 'record' as const,
      recordType,
      ...(id ? { id } : {}),
      ...(source ? { source } : {}),
      ...(metadata ? { metadata } : {}),
      data,
    };
  });
}
