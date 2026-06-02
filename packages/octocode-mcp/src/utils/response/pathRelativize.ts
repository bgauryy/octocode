/**
 * Lean-output transforms for the structured response. Two format-agnostic,
 * lossless hoists over the same flattened leaf set (one array level under each
 * result's `data` — `files[]`, `entries[]`, `matches[]`, `locations[]`):
 *
 *   - `relativizeResultPaths` — hoist the common directory of absolute
 *     `path`/`uri` fields into a top-level `base`. Reconstruction: `abs =
 *     ${base}/${path}`.
 *   - `hoistSharedFields` — hoist scalar fields whose value is identical across
 *     every leaf into a top-level `shared` map, removing them from each leaf.
 *     Reconstruction: each leaf re-gains every `shared` key.
 *
 * Both relativize the structured records the model reads, so output stays short
 * and readable without any presentation-layer transform.
 */

/** Longest common directory prefix (no trailing slash) of absolute paths. */
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
  // Only keep up to the last slash so the boundary is a real directory.
  const lastSlash = prefix.lastIndexOf('/');
  return lastSlash > 0 ? prefix.slice(0, lastSlash) : '';
}

/**
 * Path-like fields, in priority order. `uri` covers the LSP tools
 * (goto/references/call-hierarchy) whose absolute file path lives under `uri`
 * rather than `path`; relativizing it earns the same `base` leanness the
 * local/GitHub tools already get. (HTTP `url` fields are intentionally NOT
 * here — they never start with `/`, so they're never relativized anyway.)
 */
const PATH_LIKE_KEYS = ['path', 'uri'] as const;

/**
 * Relativize absolute `path`/`uri` fields inside the canonical response (the
 * payload Claude Code surfaces to the model) against their longest common
 * directory, returning that directory as `base`. Mutates the path-bearing
 * objects in place. Reconstruction is exact: `abs = ${base}/${path}`.
 *
 * Walks one array level under each result's `data` (covers `files[]`,
 * `entries[]`); leaf nodes (matches) carry no `path` so they are untouched.
 * Repo-relative paths (not starting with `/`) and single-path payloads are
 * left alone — there is nothing to hoist.
 */
export function relativizeResultPaths(
  results: ReadonlyArray<{ data?: unknown } | null | undefined>
): string | undefined {
  // Each holder remembers WHICH path-like key (`path` or `uri`) carried its
  // absolute value, so a mixed bulk relativizes each element on its own field.
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

/**
 * Collect the leaf objects one array level under each result's `data` — the
 * same set `relativizeResultPaths` walks. Covers `files[]`, `entries[]`,
 * `matches[]`, `locations[]`, etc.; non-array data values and nested arrays of
 * non-objects are skipped.
 */
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

/**
 * Keys never hoisted into `shared`. `path`/`uri` are owned by `base`
 * relativization. `owner`/`repo`/`name`/`id` are chaining-identity keys: an
 * agent uses them verbatim in the next tool call, so — unlike `base`, which
 * leaves a reconstructable shortened path — deleting them from each leaf would
 * break tool-chaining and the per-item structured contract. So `shared` only
 * collapses incidental constants (e.g. `type`/`permissions`/`language`).
 */
const HOIST_EXCLUDED_KEYS = new Set<string>([
  ...PATH_LIKE_KEYS,
  'owner',
  'repo',
  'name',
  'id',
]);

/**
 * Hoist scalar fields that carry one identical, non-empty value across EVERY
 * leaf object into a single top-level `shared` map, deleting them from each
 * leaf. Lossless: a consumer reconstructs each leaf by merging `shared` back
 * in. Identity / path keys ({@link HOIST_EXCLUDED_KEYS}) are never hoisted.
 * Returns undefined when there is nothing to hoist (fewer than two leaves, or
 * no field shared by all).
 */
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
