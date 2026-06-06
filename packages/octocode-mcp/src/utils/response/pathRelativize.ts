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

const PATH_LIKE_KEYS = ['path', 'uri'] as const;

export function relativizeResultPaths(
  results: ReadonlyArray<{ data?: unknown } | null | undefined>
): string | undefined {
  const holders: Array<{ obj: Record<string, unknown>; key: string }> = [];
  for (const r of results) {
    const data = r?.data;
    if (!data || typeof data !== 'object') continue;
    for (const value of Object.values(data as Record<string, unknown>)) {
      if (!Array.isArray(value)) continue;
      for (const el of value) {
        if (!el || typeof el !== 'object') continue;
        const obj = el as Record<string, unknown>;
        const key = PATH_LIKE_KEYS.find(
          k => typeof obj[k] === 'string' && (obj[k] as string).startsWith('/')
        );
        if (key) holders.push({ obj, key });
      }
    }
  }
  if (holders.length < 2) return undefined;

  const base = commonDirPrefix(holders.map(h => h.obj[h.key] as string));
  if (base.length <= 1) return undefined;

  const cut = base.length + 1;
  for (const { obj, key } of holders) {
    const p = obj[key] as string;
    if (p.startsWith(base + '/')) obj[key] = p.slice(cut);
  }
  return base;
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
