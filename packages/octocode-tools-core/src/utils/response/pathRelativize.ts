import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

export function commonDirPrefix(paths: readonly string[]): string {
  if (paths.length === 0) return '';
  let prefix = paths[0] ?? '';
  for (let i = 1; i < paths.length; i++) {
    const p = paths[i] ?? '';
    let j = 0;
    const max = Math.min(prefix.length, p.length);
    while (j < max && prefix[j] === p[j]) j++;
    prefix = prefix.slice(0, j);
    if (prefix === '') break;
  }
  const lastSlash = prefix.lastIndexOf('/');
  return lastSlash > 0 ? prefix.slice(0, lastSlash) : '';
}

const PATH_LIKE_KEYS = ['path', 'uri', 'absolutePath'] as const;

// Preserve absolute paths inside these top-level keys so agents can pass them
// directly to local tool calls (localSearchCode, localViewStructure, etc.).
const SKIP_TRAVERSAL_KEYS = new Set(['next', 'location']);

function collectPathHolders(
  node: unknown,
  holders: Array<{ obj: Record<string, unknown>; key: string }>,
  depth: number
): void {
  if (depth > 8 || !node || typeof node !== 'object') return;
  if (Array.isArray(node)) {
    for (const el of node) collectPathHolders(el, holders, depth + 1);
    return;
  }
  const obj = node as Record<string, unknown>;
  const absolutePath = normalizeLocalAbsolutePath(obj);
  if (absolutePath) {
    obj.absolutePath = absolutePath;
    obj.uri = pathToFileURL(absolutePath).href;
    holders.push({ obj, key: 'absolutePath' });
  }
  for (const [key, value] of Object.entries(obj)) {
    if (SKIP_TRAVERSAL_KEYS.has(key)) continue;
    if (value && typeof value === 'object') {
      collectPathHolders(value, holders, depth + 1);
    }
  }
}

function normalizeLocalAbsolutePath(
  obj: Record<string, unknown>
): string | undefined {
  const existing = obj.absolutePath;
  if (typeof existing === 'string' && path.isAbsolute(existing)) {
    return existing;
  }

  const uri = obj.uri;
  if (typeof uri === 'string' && uri.startsWith('file://')) {
    try {
      return fileURLToPath(uri);
    } catch {
      return undefined;
    }
  }

  const value = obj.path;
  if (typeof value === 'string' && path.isAbsolute(value)) return value;
  return undefined;
}

export function relativizeResultPaths(
  results: ReadonlyArray<{ data?: unknown } | null | undefined>
): string | undefined {
  const holders: Array<{ obj: Record<string, unknown>; key: string }> = [];
  for (const r of results) {
    collectPathHolders(r?.data, holders, 0);
  }
  if (holders.length === 0) return undefined;

  const base = commonDirPrefix(holders.map(h => h.obj[h.key] as string));
  if (base.length <= 1) return undefined;

  const prefix = base + '/';
  const cut = prefix.length;
  for (const { obj } of holders) {
    const absolutePath = obj.absolutePath;
    if (typeof absolutePath !== 'string' || !absolutePath.startsWith(prefix)) {
      continue;
    }
    obj.path = absolutePath.slice(cut);
    obj.absolutePath = absolutePath;
    obj.uri = pathToFileURL(absolutePath).href;
  }

  stripBaseFromStringElements(results, prefix);

  return base;
}

function stripBaseFromStringElements(
  results: ReadonlyArray<{ data?: unknown } | null | undefined>,
  prefix: string
): void {
  function walk(node: unknown, depth: number): void {
    if (depth > 8 || !node || typeof node !== 'object') return;
    if (Array.isArray(node)) {
      for (let i = 0; i < node.length; i++) {
        const v = node[i];
        if (typeof v === 'string') {
          const fileUriPrefix = 'file://' + prefix;
          if (v.includes(fileUriPrefix))
            (node as unknown[])[i] = v.replaceAll(fileUriPrefix, '');
          else if (v.includes(prefix))
            (node as unknown[])[i] = v.replaceAll(prefix, '');
        } else {
          walk(v, depth + 1);
        }
      }
      return;
    }
    const obj = node as Record<string, unknown>;
    for (const key of Object.keys(obj)) {
      if (SKIP_TRAVERSAL_KEYS.has(key)) continue;
      const v = obj[key];
      if (typeof v === 'string') {
        if (
          !PATH_LIKE_KEYS.includes(key as (typeof PATH_LIKE_KEYS)[number]) &&
          v.includes(prefix)
        ) {
          obj[key] = v.replaceAll(prefix, '');
        }
      } else {
        walk(v, depth + 1);
      }
    }
  }
  for (const r of results) walk(r?.data, 0);
}

function collectLeaves(
  results: ReadonlyArray<{ data?: unknown } | null | undefined>
): Array<Record<string, unknown>> {
  const leaves: Array<Record<string, unknown>> = [];
  for (const r of results) {
    const data = r?.data;
    if (!data || typeof data !== 'object') continue;
    for (const value of Object.values(data as Record<string, unknown>)) {
      if (!Array.isArray(value)) continue;
      for (const el of value) {
        if (el && typeof el === 'object' && !Array.isArray(el)) {
          leaves.push(el as Record<string, unknown>);
        }
      }
    }
  }
  return leaves;
}

type SharedValue = string | number | boolean;

function isHoistableScalar(v: unknown): v is SharedValue {
  return (
    (typeof v === 'string' && v !== '') ||
    typeof v === 'number' ||
    typeof v === 'boolean'
  );
}

const HOIST_EXCLUDED_KEYS = new Set<string>([
  ...PATH_LIKE_KEYS,
  'owner',
  'repo',
  'name',
  'id',
  // 'type' is REQUIRED by some tool output schemas (e.g. localViewStructure's
  // entries[].type) — hoisting it out of a homogeneous entry list (all-files
  // dir, filesOnly/directoriesOnly) made structuredContent fail MCP output
  // validation and killed the whole batch with -32602.
  'type',
]);

export function hoistSharedFields(
  results: ReadonlyArray<{ data?: unknown } | null | undefined>
): Record<string, SharedValue> | undefined {
  const leaves = collectLeaves(results);
  if (leaves.length < 2) return undefined;

  const first = leaves[0]!;
  let shared: Record<string, SharedValue> | undefined;
  for (const key of Object.keys(first)) {
    if (HOIST_EXCLUDED_KEYS.has(key)) continue;
    const v = first[key];
    if (!isHoistableScalar(v)) continue;
    if (leaves.every(l => l[key] === v)) {
      (shared ??= {})[key] = v;
    }
  }
  if (!shared) return undefined;

  const keys = Object.keys(shared);
  for (const leaf of leaves) {
    for (const key of keys) delete leaf[key];
  }
  return shared;
}
