/**
 * TSV leanness finalizer — strips redundancy from row payloads before they
 * are serialized for the LLM. Three lossless transforms, applied in order:
 *
 *   P2  drop columns that are empty in every row        (e.g. accessed/created)
 *   P3  hoist columns whose value is identical in every  -> `shared` map
 *       row (e.g. owner/repo of a single-repo search)    (emitted once)
 *   P1  relativize an absolute `path` column against the -> `base` prefix
 *       longest common directory                         (emitted once)
 *
 * Reconstruction is exact: an absolute path = `${base}/${row.path}`. The
 * caller keeps the absolute paths in `structuredContent` untouched, so
 * tool-chaining (LSP URIs, content fetches) is unaffected — only the
 * compact TSV the agent reads becomes leaner.
 */

export interface TsvFinalizeResult {
  columns: string[];
  rows: Array<Record<string, unknown>>;
  /** Common directory the relativized `path` cells hang off of. */
  base?: string;
  /** Columns hoisted out because every row shared one value. */
  shared?: Record<string, string>;
}

function isEmptyCell(v: unknown): boolean {
  return v === undefined || v === null || v === '';
}

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
 * Path-like columns/fields, in priority order. `uri` covers the LSP tools
 * (goto/references/call-hierarchy) whose absolute file path lives under `uri`
 * rather than `path`; relativizing it earns the same `base` leanness the
 * local/GitHub tools already get. (HTTP `url` columns are intentionally NOT
 * here — they never start with `/`, so they're never relativized anyway.)
 */
const PATH_LIKE_KEYS = ['path', 'uri'] as const;

export function finalizeTsv(
  columns: readonly string[],
  rows: Array<Record<string, unknown>>,
  opts: { pathColumn?: string } = {}
): TsvFinalizeResult {
  const pathColumn =
    opts.pathColumn ?? PATH_LIKE_KEYS.find(k => columns.includes(k)) ?? 'path';
  let cols = [...columns];

  // Nothing to de-duplicate across a single (or zero) row.
  if (rows.length < 2) {
    return {
      columns: cols.filter(c => rows.some(r => !isEmptyCell(r[c]))),
      rows,
    };
  }
  const firstRow = rows[0];
  if (!firstRow) return { columns: cols, rows };

  // P1 — relativize an absolute path column against the common directory.
  let base: string | undefined;
  if (cols.includes(pathColumn)) {
    const absVals = rows
      .map(r => r[pathColumn])
      .filter((v): v is string => typeof v === 'string' && v.startsWith('/'));
    // Only worth hoisting when most rows share a deep-enough prefix.
    if (absVals.length >= 2) {
      const prefix = commonDirPrefix(absVals);
      if (prefix.length > 1) {
        base = prefix;
        const cut = prefix.length + 1; // drop the trailing slash too
        for (const r of rows) {
          const v = r[pathColumn];
          if (typeof v === 'string' && v.startsWith(prefix + '/')) {
            r[pathColumn] = v.slice(cut);
          }
        }
      }
    }
  }

  // P3 — hoist columns that carry one identical, non-empty value everywhere.
  // The path column is never hoisted (it varies / is handled by `base`).
  let shared: Record<string, string> | undefined;
  for (const col of [...cols]) {
    if (col === pathColumn) continue;
    const first = firstRow[col];
    if (isEmptyCell(first)) continue;
    if (rows.every(r => r[col] === first)) {
      (shared ??= {})[col] = String(first);
      cols = cols.filter(c => c !== col);
    }
  }

  // P2 — drop columns that are empty in every remaining row.
  cols = cols.filter(col => rows.some(r => !isEmptyCell(r[col])));

  return { columns: cols, rows, base, shared };
}

/**
 * Relativize absolute `path` fields inside the canonical `structuredContent`
 * (the payload Claude Code surfaces to the model) against their longest common
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
