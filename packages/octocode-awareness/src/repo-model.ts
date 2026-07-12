

export const AWARENESS_QUERY_VIEWS = [
  'all',
  'repo-profile',
  'memories',
  'gotchas',
  'lessons',
  'plans',
  'tasks',
  'runs',
  'locks',
  'agents',
  'signals',
  'refinements',
  'files',
  'activity',
  'workboard',
  'developer-review',
] as const;

export type AwarenessQueryView = typeof AWARENESS_QUERY_VIEWS[number];
export type AwarenessQueryFormat = 'json' | 'table' | 'csv' | 'markdown' | 'html';
export type RepoContextMode = 'local' | 'share';

export interface AwarenessQueryParams {
  view?: string | null;
  workspacePath?: string | null;
  workspace_path?: string | null;
  workspace?: string | null;
  artifact?: string | null;
  repo?: string | null;
  ref?: string | null;
  query?: string | null;
  limit?: number | null;
  agentId?: string | null;
  agent_id?: string | null;
  state?: string | string[] | null;
  label?: string | string[] | null;
  file?: string | null;
  since?: string | null;
  includeBodies?: boolean | null;
  include_bodies?: boolean | null;
  cwd?: string | null;
}

export interface AwarenessQuerySection {
  count: number;
  rows: AwarenessQueryRow[];
  total: number | null;
  omitted_count: number | null;
  is_partial: boolean;
  continuation: string | null;
}

export interface AwarenessQueryResult {
  ok: true;
  view: AwarenessQueryView;
  generated_at: string;
  workspace_path: string | null;
  artifact: string | null;
  repo: string | null;
  ref: string | null;
  count: number;
  rows: AwarenessQueryRow[];
  total: number | null;
  omitted_count: number | null;
  is_partial: boolean;
  continuation: string | null;
  sections?: Record<string, AwarenessQuerySection>;
  filters: Record<string, unknown>;
}

export interface RepoContextInjectParams extends AwarenessQueryParams {
  outDir?: string | null;
  out_dir?: string | null;
  mode?: string | null;
  includeView?: boolean | null;
  include_view?: boolean | null;
  pruneOrphans?: boolean | null;
  prune_orphans?: boolean | null;
  check?: boolean | null;
  /** Actual resolved store path for this run; reported verbatim as manifest source.canonical. */
  dbPath?: string | null;
}

export interface RepoContextInjectResult {
  ok: true;
  generated_at: string;
  workspace_path: string;
  out_dir: string;
  mode: RepoContextMode;
  count: number;
  files: string[];
  warnings: string[];
  orphan_candidates: string[];
  pruned_orphans: string[];
  manifest: Record<string, unknown>;
}

export type AwarenessQueryRow = Record<string, string | number | boolean | null | string[]>;

export type BindValue = string | number;

export interface Scope {
  workspacePath: string | null;
  workspacePaths: string[];
  artifact: string | null;
  repo: string | null;
  ref: string | null;
}

// One query fans out into many SQL views. Cache its normalized workspace scope
// by the request object so each view reuses the same realpath/Git discovery
// instead of spawning Git again for every section.
export const SCOPE_CACHE = new WeakMap<AwarenessQueryParams, Scope>();

export interface MemoryDbRow {
  memory_id: string;
  agent_id: string;
  task_context: string;
  observation: string;
  importance: number;
  state: string;
  label: string;
  tags_json: string;
  workspace_path: string | null;
  artifact: string | null;
  repo: string | null;
  ref: string | null;
  failure_signature: string | null;
  created_at: string;
  updated_at: string | null;
  references?: string[];
}

export const VIEW_SET = new Set<string>(AWARENESS_QUERY_VIEWS);
export const CSV_VIEWS = ['memories', 'gotchas', 'lessons', 'plans', 'tasks', 'runs', 'agents', 'locks', 'signals', 'refinements', 'files', 'activity', 'workboard'] as const;
export interface ProjectionMarkdownBudget {
  max_lines: number;
  role: string;
}

export interface ProjectionMarkdownBudgetStatus extends ProjectionMarkdownBudget {
  actual_lines: number;
  within_budget: boolean;
}

export const PROJECTION_MARKDOWN_BUDGETS: Record<string, ProjectionMarkdownBudget> = {
  'AGENTS.md': { max_lines: 80, role: 'agent start summary' },
  'KNOWLEDGE.md': { max_lines: 200, role: 'bounded verified-knowledge leads' },
};
export const ATTEND_COMPACT_BUDGET = { max_lines: 40, max_json_bytes: 2 * 1024 };
export const WORKBOARD_BUDGET = { max_rows_per_column: 10 };
export const LESSON_LABELS = [
  'DECISION',
  'ARCHITECTURE',
  'WORKFLOW',
  'IMPROVEMENT',
  'DOCS',
  'TEST',
  'BUILD',
  'CONFIG',
  'PERFORMANCE',
  'REFACTOR',
  'API',
  'RELEASE',
  'FEATURE',
  'SUGGESTION',
  'SECURITY',
  'OVERRIDE',
];

export function utcNow(): string {
  return new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');
}

export function normalizeView(view: string | null | undefined): AwarenessQueryView {
  const normalized = (view ?? 'all').trim().toLowerCase().replace(/_/g, '-');
  if (VIEW_SET.has(normalized)) return normalized as AwarenessQueryView;
  throw new Error(`unknown octocode-awareness query view "${view}". Expected one of: ${AWARENESS_QUERY_VIEWS.join(', ')}`);
}

export function normalizeFormat(format: string | null | undefined): AwarenessQueryFormat {
  const normalized = (format ?? 'json').trim().toLowerCase();
  if (normalized === 'json' || normalized === 'table' || normalized === 'csv' || normalized === 'markdown' || normalized === 'html') return normalized;
  throw new Error('--format must be json, table, csv, markdown, or html');
}

export function normalizeMode(mode: string | null | undefined): RepoContextMode {
  const normalized = (mode ?? 'local').trim().toLowerCase();
  if (normalized === 'local' || normalized === 'share') return normalized;
  throw new Error('--mode must be local or share');
}

export function limitOf(value: number | null | undefined, fallback = 50, max = 501): number {
  if (value == null || !Number.isFinite(value)) return fallback;
  return Math.min(max, Math.max(1, Math.floor(value)));
}

export interface QueryCompleteness {
  rows: AwarenessQueryRow[];
  total: number | null;
  omitted_count: number | null;
  is_partial: boolean;
  continuation: string | null;
}

export function continuationFor(view: AwarenessQueryView, requestedLimit: number): string {
  if (requestedLimit < 500) {
    return `query ${view} --limit ${Math.min(500, Math.max(requestedLimit + 1, requestedLimit * 2))}; narrow filters if the result remains partial`;
  }
  return `query ${view} reached the 500-row safety cap; narrow workspace, state, label, file, time, or text filters`;
}

export function boundedRows(
  view: AwarenessQueryView,
  probedRows: AwarenessQueryRow[],
  requestedLimit: number,
): QueryCompleteness {
  if (view === 'workboard') {
    const columnTotals = new Map<string, number>();
    for (const row of probedRows) {
      const column = String(row['column'] ?? 'Other');
      if (!columnTotals.has(column)) columnTotals.set(column, Number(row['column_total'] ?? 1));
    }
    const total = [...columnTotals.values()].reduce((sum, count) => sum + count, 0);
    const omitted = Math.max(0, total - probedRows.length);
    return {
      rows: probedRows,
      total,
      omitted_count: omitted,
      is_partial: omitted > 0,
      continuation: omitted > 0 ? 'drill into the named workboard lane with its targeted command; lane output is intentionally bounded' : null,
    };
  }
  const hasMore = probedRows.length > requestedLimit;
  const rows = probedRows.slice(0, requestedLimit);
  return {
    rows,
    total: hasMore ? null : rows.length,
    omitted_count: hasMore ? null : 0,
    is_partial: hasMore,
    continuation: hasMore ? continuationFor(view, requestedLimit) : null,
  };
}

export function stringList(value: string | string[] | null | undefined): string[] {
  if (Array.isArray(value)) return value.map(String).filter(Boolean);
  if (value == null || value === '') return [];
  return [String(value)];
}
