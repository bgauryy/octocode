/**
 * repo-context.ts - generated repo-readable projections over awareness data.
 *
 * The SQLite store remains canonical. This module only reads it into lean views
 * for agents/humans and writes optional `.octocode/*` snapshots for workspaces
 * that choose to share or keep a local generated context folder.
 */

import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, realpathSync, writeFileSync } from 'node:fs';
import { isAbsolute, join, relative, resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { fileURLToPath } from 'node:url';
import { parseJsonList } from './helpers.js';
import { normalizeWorkspacePath } from './git.js';

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
  sections?: Record<string, AwarenessQuerySection>;
  filters: Record<string, unknown>;
}

export interface RepoContextInjectParams extends AwarenessQueryParams {
  outDir?: string | null;
  out_dir?: string | null;
  mode?: string | null;
  includeView?: boolean | null;
  include_view?: boolean | null;
  check?: boolean | null;
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
  manifest: Record<string, unknown>;
}

export type AwarenessQueryRow = Record<string, string | number | boolean | null | string[]>;

type BindValue = string | number;

interface Scope {
  workspacePath: string | null;
  workspacePaths: string[];
  artifact: string | null;
  repo: string | null;
  ref: string | null;
}

// One query fans out into many SQL views. Cache its normalized workspace scope
// by the request object so each view reuses the same realpath/Git discovery
// instead of spawning Git again for every section.
const SCOPE_CACHE = new WeakMap<AwarenessQueryParams, Scope>();

interface MemoryDbRow {
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

const VIEW_SET = new Set<string>(AWARENESS_QUERY_VIEWS);
const CSV_VIEWS = ['memories', 'gotchas', 'lessons', 'plans', 'tasks', 'runs', 'agents', 'locks', 'signals', 'refinements', 'files', 'activity', 'workboard'] as const;
interface ProjectionMarkdownBudget {
  max_lines: number;
  role: string;
}

interface ProjectionMarkdownBudgetStatus extends ProjectionMarkdownBudget {
  actual_lines: number;
  within_budget: boolean;
}

const PROJECTION_MARKDOWN_BUDGETS: Record<string, ProjectionMarkdownBudget> = {
  'AGENTS.md': { max_lines: 80, role: 'agent start summary' },
  'MEMORY.md': { max_lines: 200, role: 'active memory index' },
  'GOTCHAS.md': { max_lines: 200, role: 'gotcha index' },
  'LEARN.md': { max_lines: 200, role: 'lesson/opportunity index' },
  'BOOKMARKS.md': { max_lines: 200, role: 'learnable resource index' },
  'DEVELOPER_REVIEW.md': { max_lines: 200, role: 'agent feedback to the instruction author' },
};
const ATTEND_COMPACT_BUDGET = { max_lines: 40, max_json_bytes: 2 * 1024 };
const WORKBOARD_BUDGET = { max_rows_per_column: 10 };
const LESSON_LABELS = [
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

function utcNow(): string {
  return new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');
}

function normalizeView(view: string | null | undefined): AwarenessQueryView {
  const normalized = (view ?? 'all').trim().toLowerCase().replace(/_/g, '-');
  if (VIEW_SET.has(normalized)) return normalized as AwarenessQueryView;
  throw new Error(`unknown octocode-awareness query view "${view}". Expected one of: ${AWARENESS_QUERY_VIEWS.join(', ')}`);
}

function normalizeFormat(format: string | null | undefined): AwarenessQueryFormat {
  const normalized = (format ?? 'json').trim().toLowerCase();
  if (normalized === 'json' || normalized === 'table' || normalized === 'csv' || normalized === 'markdown' || normalized === 'html') return normalized;
  throw new Error('--format must be json, table, csv, markdown, or html');
}

function normalizeMode(mode: string | null | undefined): RepoContextMode {
  const normalized = (mode ?? 'local').trim().toLowerCase();
  if (normalized === 'local' || normalized === 'share') return normalized;
  throw new Error('--mode must be local or share');
}

function limitOf(value: number | null | undefined, fallback = 50, max = 500): number {
  if (value == null || !Number.isFinite(value)) return fallback;
  return Math.min(max, Math.max(1, Math.floor(value)));
}

function stringList(value: string | string[] | null | undefined): string[] {
  if (Array.isArray(value)) return value.map(String).filter(Boolean);
  if (value == null || value === '') return [];
  return [String(value)];
}

function scopeFromParams(params: AwarenessQueryParams): Scope {
  const cached = SCOPE_CACHE.get(params);
  if (cached) return cached;
  const cwd = params.cwd ? resolve(params.cwd) : process.cwd();
  const rawWorkspace = params.workspacePath ?? params.workspace_path ?? params.workspace ?? cwd;
  const workspacePath = rawWorkspace ? resolve(String(rawWorkspace)) : null;
  const scope = {
    // Keep the raw resolved path for projection output / echo; the alias set
    // below carries the extra keys used for DB row matching.
    workspacePath,
    workspacePaths: workspacePath ? workspaceAliases(workspacePath, cwd) : [],
    artifact: params.artifact ? String(params.artifact) : null,
    repo: params.repo ? String(params.repo) : null,
    ref: params.ref ? String(params.ref) : null,
  };
  SCOPE_CACHE.set(params, scope);
  return scope;
}

function withScope(
  params: AwarenessQueryParams,
  overrides: Partial<AwarenessQueryParams>,
): AwarenessQueryParams {
  const derived = { ...params, ...overrides };
  SCOPE_CACHE.set(derived, scopeFromParams(params));
  return derived;
}

function workspaceAliases(workspacePath: string, cwd?: string): string[] {
  const aliases = new Set<string>([workspacePath]);
  try {
    aliases.add(realpathSync.native(workspacePath));
  } catch {
    try { aliases.add(realpathSync(workspacePath)); } catch { /* path may not exist yet */ }
  }
  // D1 fix: also match the git-root workspace key that write paths store via
  // fillScope, so reads run from a package/subdir meet the rows writes stored.
  // Additive — the raw resolved path stays in the set, so non-git and raw-path
  // rows keep matching.
  try {
    const gitRoot = normalizeWorkspacePath(workspacePath, cwd ?? workspacePath);
    if (gitRoot) aliases.add(gitRoot);
  } catch { /* leave aliases as-is if git detection fails */ }
  return [...aliases];
}

function addNullableScope(where: string[], binds: BindValue[], scope: Scope, alias = ''): void {
  const p = alias ? `${alias}.` : '';
  if (scope.workspacePaths.length > 0) {
    where.push(`(${p}workspace_path IN (${scope.workspacePaths.map(() => '?').join(',')}) OR ${p}workspace_path IS NULL)`);
    binds.push(...scope.workspacePaths);
  }
  if (scope.artifact) {
    where.push(`(${p}artifact = ? OR ${p}artifact IS NULL)`);
    binds.push(scope.artifact);
  }
  if (scope.repo) {
    where.push(`(${p}repo = ? OR ${p}repo IS NULL)`);
    binds.push(scope.repo);
  }
  if (scope.ref) {
    where.push(`(${p}ref = ? OR ${p}ref IS NULL)`);
    binds.push(scope.ref);
  }
}

function addExactScope(where: string[], binds: BindValue[], scope: Scope, alias = ''): void {
  const p = alias ? `${alias}.` : '';
  if (scope.workspacePaths.length > 0) {
    where.push(`${p}workspace_path IN (${scope.workspacePaths.map(() => '?').join(',')})`);
    binds.push(...scope.workspacePaths);
  }
  if (scope.artifact) {
    where.push(`(${p}artifact = ? OR ${p}artifact IS NULL)`);
    binds.push(scope.artifact);
  }
  if (scope.repo) {
    where.push(`(${p}repo = ? OR ${p}repo IS NULL)`);
    binds.push(scope.repo);
  }
  if (scope.ref) {
    where.push(`(${p}ref = ? OR ${p}ref IS NULL)`);
    binds.push(scope.ref);
  }
}

function addTextFilter(where: string[], binds: BindValue[], query: string | null | undefined, columns: string[]): void {
  const q = query?.trim();
  if (!q) return;
  where.push(`LOWER(${columns.map(c => `COALESCE(${c}, '')`).join(" || ' ' || ")}) LIKE LOWER(?)`);
  binds.push(`%${q}%`);
}

function addStateFilter(
  where: string[],
  binds: BindValue[],
  states: string[],
  column: string,
  normalize: (state: string) => string = (state) => state,
): void {
  if (states.length === 0) return;
  where.push(`${column} IN (${states.map(() => '?').join(',')})`);
  binds.push(...states.map(normalize));
}

function addLabelsFilter(where: string[], binds: BindValue[], labels: string[], column = 'label'): void {
  if (labels.length === 0) return;
  where.push(`${column} IN (${labels.map(() => '?').join(',')})`);
  binds.push(...labels.map(l => l.toUpperCase()));
}

function fileRefCandidates(file: string, workspacePath: string | null): string[] {
  const trimmed = file.trim();
  if (!trimmed) return [];
  if (trimmed.startsWith('file:')) return [trimmed];
  const absolute = isAbsolute(trimmed) ? resolve(trimmed) : resolve(workspacePath ?? process.cwd(), trimmed);
  return [`file:${absolute}`, `%${trimmed}%`];
}

function stripLocationSuffix(value: string): string {
  return value.trim().replace(/:(\d+)(?::\d+)?$/, '');
}

function localPathFromReference(reference: string, workspacePath: string | null): string | null {
  const trimmed = reference.trim();
  if (!trimmed) return null;
  let rawPath: string | null = null;
  if (/^file:\/\//i.test(trimmed)) {
    try {
      rawPath = fileURLToPath(trimmed);
    } catch {
      rawPath = trimmed.replace(/^file:\/+/i, '/');
    }
  } else if (/^file:/i.test(trimmed)) {
    rawPath = trimmed.slice('file:'.length);
  } else if (/^path:/i.test(trimmed)) {
    rawPath = trimmed.slice('path:'.length);
  } else if (/^[a-z][a-z0-9+.-]*:/i.test(trimmed)) {
    return null;
  } else if (isAbsolute(trimmed) || trimmed.startsWith('./') || trimmed.startsWith('../') || /^[^/\s]+\/.+/.test(trimmed)) {
    rawPath = trimmed;
  }
  if (!rawPath) return null;
  const clean = stripLocationSuffix(rawPath);
  return isAbsolute(clean) ? resolve(clean) : resolve(workspacePath ?? process.cwd(), clean);
}

function referenceHealth(references: string[], workspacePath: string | null): Record<string, string[] | number> {
  const fileReferences: string[] = [];
  const existingFiles: string[] = [];
  const missingFiles: string[] = [];
  const missingReferences: string[] = [];
  for (const reference of references) {
    const localPath = localPathFromReference(reference, workspacePath);
    if (!localPath) continue;
    fileReferences.push(localPath);
    if (existsSync(localPath)) {
      existingFiles.push(localPath);
    } else {
      missingFiles.push(localPath);
      missingReferences.push(reference);
    }
  }
  return {
    reference_count: references.length,
    file_reference_count: fileReferences.length,
    missing_reference_count: missingReferences.length,
    file_references: [...new Set(fileReferences)],
    existing_files: [...new Set(existingFiles)],
    missing_files: [...new Set(missingFiles)],
    missing_references: [...new Set(missingReferences)],
  };
}

function addMemoryFileFilter(where: string[], binds: BindValue[], file: string | null | undefined, scope: Scope): void {
  if (!file) return;
  const candidates = fileRefCandidates(file, scope.workspacePath);
  if (candidates.length === 0) return;
  where.push(`EXISTS (
    SELECT 1 FROM memory_refs r
    WHERE r.memory_id = memories.memory_id
      AND (${candidates.map(() => 'r.reference LIKE ?').join(' OR ')})
  )`);
  binds.push(...candidates);
}

function withReferences(db: DatabaseSync, rows: MemoryDbRow[]): MemoryDbRow[] {
  if (rows.length === 0) return rows;
  const ids = rows.map(row => row.memory_id);
  const refs = db.prepare(
    `SELECT memory_id, reference
       FROM memory_refs
      WHERE memory_id IN (${ids.map(() => '?').join(',')})
      ORDER BY memory_id, ordinal`
  ).all(...ids) as unknown as Array<{ memory_id: string; reference: string }>;
  const map = new Map<string, string[]>();
  for (const ref of refs) {
    const list = map.get(ref.memory_id) ?? [];
    list.push(ref.reference);
    map.set(ref.memory_id, list);
  }
  for (const row of rows) row.references = map.get(row.memory_id) ?? [];
  return rows;
}

function memoryRows(
  db: DatabaseSync,
  params: AwarenessQueryParams,
  options: { gotchas?: boolean; lessons?: boolean } = {},
): AwarenessQueryRow[] {
  const scope = scopeFromParams(params);
  const where = ["state = 'ACTIVE'"];
  const binds: BindValue[] = [];
  addNullableScope(where, binds, scope);
  addTextFilter(where, binds, params.query, ['task_context', 'observation', 'label', 'tags_json', 'failure_signature']);
  addMemoryFileFilter(where, binds, params.file, scope);

  if (options.gotchas) {
    where.push("(label = 'GOTCHA' OR failure_signature IS NOT NULL)");
  } else if (options.lessons) {
    where.push(`label IN (${LESSON_LABELS.map(() => '?').join(',')})`);
    binds.push(...LESSON_LABELS);
  } else {
    addLabelsFilter(where, binds, stringList(params.label));
  }

  const since = params.since?.trim();
  if (since) {
    where.push('created_at >= ?');
    binds.push(since);
  }

  const limit = limitOf(params.limit);
  const rows = db.prepare(
    `SELECT memory_id, agent_id, task_context, observation, importance, state, label, tags_json,
            workspace_path, artifact, repo, ref, failure_signature, created_at, updated_at
       FROM memories
      WHERE ${where.join(' AND ')}
      ORDER BY importance DESC, datetime(created_at) DESC
      LIMIT ?`
  ).all(...binds, limit) as unknown as MemoryDbRow[];

  return withReferences(db, rows).map(row => {
    const references = row.references ?? [];
    return {
      memory_id: row.memory_id,
      label: row.label,
      importance: row.importance,
      task_context: row.task_context,
      observation: row.observation,
      tags: parseJsonList(row.tags_json),
      references,
      ...referenceHealth(references, scope.workspacePath),
      failure_signature: row.failure_signature,
      agent_id: row.agent_id,
      workspace_path: row.workspace_path,
      artifact: row.artifact,
      repo: row.repo,
      ref: row.ref,
      created_at: row.created_at,
      updated_at: row.updated_at,
    };
  });
}

function planRows(db: DatabaseSync, params: AwarenessQueryParams): AwarenessQueryRow[] {
  const scope = scopeFromParams(params);
  const where: string[] = [];
  const binds: BindValue[] = [];
  addExactScope(where, binds, scope);
  addTextFilter(where, binds, params.query, ['plan_id', 'name', 'objective', 'lead_agent_id', 'doc_dir']);
  addStateFilter(where, binds, stringList(params.state), 'status', state => state.toUpperCase());
  const since = params.since?.trim();
  if (since) {
    where.push('created_at >= ?');
    binds.push(since);
  }
  const sqlWhere = where.length > 0 ? `WHERE ${where.join(' AND ')}` : '';
  const rows = db.prepare(
    `SELECT plan_id, name, objective, lead_agent_id, status, workspace_path, artifact,
            doc_dir, created_at, updated_at,
            (SELECT COUNT(*) FROM plan_members pm WHERE pm.plan_id = plans.plan_id) AS member_count,
            (SELECT COUNT(*) FROM tasks t WHERE t.plan_id = plans.plan_id) AS task_count
       FROM plans
       ${sqlWhere}
      ORDER BY datetime(updated_at) DESC
      LIMIT ?`
  ).all(...binds, limitOf(params.limit)) as unknown as Array<Record<string, string | number | null>>;

  return rows.map(row => ({
    plan_id: String(row['plan_id']),
    name: String(row['name']),
    objective: String(row['objective']),
    lead_agent_id: String(row['lead_agent_id']),
    status: String(row['status']),
    doc_dir: String(row['doc_dir']),
    member_count: Number(row['member_count']),
    task_count: Number(row['task_count']),
    workspace_path: row['workspace_path'] ?? null,
    artifact: row['artifact'] ?? null,
    created_at: String(row['created_at']),
    updated_at: String(row['updated_at']),
  }));
}

function taskRows(db: DatabaseSync, params: AwarenessQueryParams): AwarenessQueryRow[] {
  const scope = scopeFromParams(params);
  const where: string[] = [];
  const binds: BindValue[] = [];
  addExactScope(where, binds, scope, 'p');
  addTextFilter(where, binds, params.query, ['t.task_id', 't.title', 't.reasoning', 't.acceptance_criteria', 't.created_by', 'p.name']);
  addStateFilter(where, binds, stringList(params.state), 't.status', state => state.toUpperCase());
  const agentId = params.agentId ?? params.agent_id;
  if (agentId) { where.push('(t.created_by = ? OR c.agent_id = ?)'); binds.push(agentId, agentId); }
  const since = params.since?.trim();
  if (since) { where.push('t.created_at >= ?'); binds.push(since); }
  const sqlWhere = where.length > 0 ? `WHERE ${where.join(' AND ')}` : '';
  const rows = db.prepare(
    `SELECT t.*, p.name AS plan_name, p.status AS plan_status, p.workspace_path, p.artifact,
            c.agent_id AS claimed_by, c.run_id, c.expires_at AS claim_expires_at,
            COALESCE((SELECT json_group_array(tp.path) FROM task_paths tp WHERE tp.task_id = t.task_id), '[]') AS paths_json,
            COALESCE((SELECT json_group_array(td.depends_on_task_id) FROM task_dependencies td WHERE td.task_id = t.task_id), '[]') AS dependencies_json,
            CASE WHEN t.status = 'OPEN'
              AND c.task_id IS NULL
              AND NOT EXISTS (
                SELECT 1 FROM task_dependencies td
                JOIN tasks dependency ON dependency.task_id = td.depends_on_task_id
                WHERE td.task_id = t.task_id AND dependency.status <> 'DONE'
              ) THEN 1 ELSE 0 END AS ready
       FROM tasks t
       JOIN plans p ON p.plan_id = t.plan_id
       LEFT JOIN task_claims c ON c.task_id = t.task_id AND c.expires_at > strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
       ${sqlWhere}
      ORDER BY t.priority DESC, datetime(t.created_at), t.task_id
      LIMIT ?`
  ).all(...binds, limitOf(params.limit)) as unknown as Array<Record<string, string | number | null>>;

  return rows.map(row => ({
    task_id: String(row['task_id']),
    plan_id: String(row['plan_id']),
    plan_name: String(row['plan_name']),
    plan_status: String(row['plan_status']),
    title: String(row['title']),
    reasoning: String(row['reasoning']),
    acceptance_criteria: String(row['acceptance_criteria']),
    status: String(row['status']),
    priority: Number(row['priority']),
    created_by: String(row['created_by']),
    paths: parseJsonList(row['paths_json']),
    dependencies: parseJsonList(row['dependencies_json']),
    ready: Number(row['ready']) === 1,
    claimed_by: row['claimed_by'] ?? null,
    run_id: row['run_id'] ?? null,
    claim_expires_at: row['claim_expires_at'] ?? null,
    workspace_path: row['workspace_path'] ?? null,
    artifact: row['artifact'] ?? null,
    created_at: String(row['created_at']),
    updated_at: String(row['updated_at']),
    completed_at: row['completed_at'] ?? null,
  }));
}

function runRows(db: DatabaseSync, params: AwarenessQueryParams): AwarenessQueryRow[] {
  const scope = scopeFromParams(params);
  const where: string[] = [];
  const binds: BindValue[] = [];
  addExactScope(where, binds, scope, 'tr');
  const query = params.query?.trim();
  if (query) {
    where.push(`(LOWER(COALESCE(tr.run_id, '') || ' ' || COALESCE(tr.rationale, '') || ' ' ||
      COALESCE(tr.test_plan, '') || ' ' || COALESCE(tr.context_ref, '') || ' ' || COALESCE(tr.agent_id, '')) LIKE LOWER(?)
      OR EXISTS (SELECT 1 FROM run_files rfq WHERE rfq.run_id = tr.run_id AND LOWER(rfq.file_path) LIKE LOWER(?)))`);
    binds.push(`%${query}%`, `%${query}%`);
  }
  addStateFilter(where, binds, stringList(params.state), 'tr.status', state => state.toUpperCase());
  const agentId = params.agentId ?? params.agent_id;
  if (agentId) { where.push('tr.agent_id = ?'); binds.push(agentId); }
  const since = params.since?.trim();
  if (since) { where.push('tr.created_at >= ?'); binds.push(since); }
  const sqlWhere = where.length > 0 ? `WHERE ${where.join(' AND ')}` : '';
  const rows = db.prepare(
    `SELECT tr.run_id, tr.task_id, tr.agent_id, tr.session_id, tr.rationale, tr.test_plan,
            tr.context_ref, tr.status, tr.workspace_path, tr.artifact, tr.created_at, tr.updated_at,
            COALESCE((SELECT json_group_array(rf.file_path)
              FROM run_files rf WHERE rf.run_id = tr.run_id), '[]') AS files_json
       FROM task_runs tr ${sqlWhere}
      ORDER BY datetime(tr.created_at) DESC LIMIT ?`
  ).all(...binds, limitOf(params.limit)) as unknown as Array<Record<string, string | null>>;
  return rows.map(row => ({
    run_id: String(row['run_id']),
    task_id: row['task_id'] ?? null,
    agent_id: String(row['agent_id']),
    status: String(row['status']),
    rationale: String(row['rationale']),
    test_plan: String(row['test_plan']),
    context_ref: row['context_ref'] ?? null,
    files: parseJsonList(row['files_json']),
    workspace_path: row['workspace_path'] ?? null,
    artifact: row['artifact'] ?? null,
    created_at: String(row['created_at']),
    updated_at: String(row['updated_at']),
  }));
}

function lockRows(db: DatabaseSync, params: AwarenessQueryParams): AwarenessQueryRow[] {
  const scope = scopeFromParams(params);
  const where: string[] = [];
  const binds: BindValue[] = [];
  addExactScope(where, binds, scope, 't');
  addTextFilter(where, binds, params.query, ['l.file_path', 't.agent_id', 't.rationale']);
  const agentId = params.agentId ?? params.agent_id;
  if (agentId) {
    where.push('t.agent_id = ?');
    binds.push(agentId);
  }
  const sqlWhere = where.length > 0 ? `WHERE ${where.join(' AND ')}` : '';
  const rows = db.prepare(
    `SELECT l.lock_id, l.file_path, l.run_id, t.agent_id, t.session_id, 'EXCLUSIVE' AS lock_type,
            l.acquired_at, l.expires_at, t.task_id, t.workspace_path, t.artifact, t.status
       FROM locks l
       JOIN task_runs t ON t.run_id = l.run_id
       ${sqlWhere}
      ORDER BY datetime(l.acquired_at) DESC
      LIMIT ?`
  ).all(...binds, limitOf(params.limit)) as unknown as Array<Record<string, string | null>>;

  return rows.map(row => ({
    lock_id: String(row['lock_id']),
    file_path: String(row['file_path']),
    run_id: String(row['run_id']),
    task_id: row['task_id'] ?? null,
    agent_id: String(row['agent_id']),
    lock_type: String(row['lock_type']),
    run_status: String(row['status']),
    acquired_at: String(row['acquired_at']),
    expires_at: row['expires_at'] ?? null,
    workspace_path: row['workspace_path'] ?? null,
    artifact: row['artifact'] ?? null,
  }));
}

function agentRows(db: DatabaseSync, params: AwarenessQueryParams): AwarenessQueryRow[] {
  const scope = scopeFromParams(params);
  const where: string[] = [];
  const binds: BindValue[] = [];
  if (scope.workspacePaths.length > 0) {
    where.push(`(workspace_path IN (${scope.workspacePaths.map(() => '?').join(',')}) OR workspace_path IS NULL)`);
    binds.push(...scope.workspacePaths);
  }
  if (scope.artifact) {
    where.push('(artifact = ? OR artifact IS NULL)');
    binds.push(scope.artifact);
  }
  addTextFilter(where, binds, params.query, ['agent_id', 'agent_name', 'context']);
  const sqlWhere = where.length > 0 ? `WHERE ${where.join(' AND ')}` : '';
  const rows = db.prepare(
    `SELECT agent_id, agent_name, workspace_path, artifact, context, registered_at, last_seen_at
       FROM agents
       ${sqlWhere}
      ORDER BY datetime(last_seen_at) DESC
      LIMIT ?`
  ).all(...binds, limitOf(params.limit)) as unknown as AwarenessQueryRow[];
  return rows;
}

function signalRows(db: DatabaseSync, params: AwarenessQueryParams): AwarenessQueryRow[] {
  const scope = scopeFromParams(params);
  const where: string[] = [];
  const binds: BindValue[] = [];
  addExactScope(where, binds, scope);
  addTextFilter(where, binds, params.query, ['subject', 'body', 'kind', 'files_json', 'refs_json', 'from_agent', 'to_agent']);
  addStateFilter(where, binds, stringList(params.state), 'status', state => state.toLowerCase());
  const agentId = params.agentId ?? params.agent_id;
  if (agentId) {
    where.push('(from_agent = ? OR to_agent = ? OR to_agent IS NULL)');
    binds.push(agentId, agentId);
  }
  const since = params.since?.trim();
  if (since) {
    where.push('created_at >= ?');
    binds.push(since);
  }
  const includeBodies = Boolean(params.includeBodies ?? params.include_bodies);
  const sqlWhere = where.length > 0 ? `WHERE ${where.join(' AND ')}` : '';
  const rows = db.prepare(
    `SELECT signal_id, workspace_path, artifact, repo, ref, from_agent, to_agent, kind,
            subject, body, files_json, refs_json, thread_id, reply_to, importance, status, created_at
       FROM signals
       ${sqlWhere}
      ORDER BY datetime(created_at) DESC
      LIMIT ?`
  ).all(...binds, limitOf(params.limit)) as unknown as Array<Record<string, string | number | null>>;

  return rows.map(row => ({
    signal_id: String(row['signal_id']),
    kind: String(row['kind']),
    status: String(row['status']),
    subject: String(row['subject']),
    body: includeBodies ? row['body'] as string | null : summarize(String(row['body'] ?? ''), 160),
    from_agent: String(row['from_agent']),
    to_agent: row['to_agent'] as string | null,
    files: parseJsonList(row['files_json']),
    refs: parseJsonList(row['refs_json']),
    thread_id: String(row['thread_id']),
    reply_to: row['reply_to'] as string | null,
    importance: Number(row['importance']),
    workspace_path: row['workspace_path'] as string | null,
    artifact: row['artifact'] as string | null,
    repo: row['repo'] as string | null,
    ref: row['ref'] as string | null,
    created_at: String(row['created_at']),
  }));
}

function refinementRows(db: DatabaseSync, params: AwarenessQueryParams): AwarenessQueryRow[] {
  const scope = scopeFromParams(params);
  const where: string[] = [];
  const binds: BindValue[] = [];
  addExactScope(where, binds, scope);
  addTextFilter(where, binds, params.query, ['reasoning', 'remember', 'quality', 'state', 'files_json', 'agent_id']);
  addStateFilter(where, binds, stringList(params.state), 'state', state => state.toLowerCase());
  const since = params.since?.trim();
  if (since) {
    where.push('created_at >= ?');
    binds.push(since);
  }
  const sqlWhere = where.length > 0 ? `WHERE ${where.join(' AND ')}` : '';
  const rows = db.prepare(
    `SELECT refinement_id, agent_id, workspace_path, artifact, repo, ref, files_json,
            reasoning, remember, quality, state, created_at, updated_at
       FROM refinements
       ${sqlWhere}
      ORDER BY
        CASE state WHEN 'open' THEN 0 WHEN 'ongoing' THEN 1 ELSE 2 END,
        datetime(updated_at) DESC
      LIMIT ?`
  ).all(...binds, limitOf(params.limit)) as unknown as Array<Record<string, string | null>>;

  return rows.map(row => ({
    refinement_id: String(row['refinement_id']),
    agent_id: String(row['agent_id']),
    quality: String(row['quality']),
    state: String(row['state']),
    reasoning: String(row['reasoning']),
    remember: String(row['remember']),
    files: parseJsonList(row['files_json']),
    workspace_path: String(row['workspace_path']),
    artifact: row['artifact'] ?? null,
    repo: row['repo'] ?? null,
    ref: row['ref'] ?? null,
    created_at: String(row['created_at']),
    updated_at: String(row['updated_at']),
  }));
}

/** Pull the feedback clause out of a reflection narrative, if present. */
function extractInstructionsFeedback(observation: string): string {
  const marker = 'instructions feedback:';
  const idx = observation.toLowerCase().indexOf(marker);
  if (idx === -1) return observation;
  const after = observation.slice(idx + marker.length);
  // The narrative joins clauses with ' | ' and closes reflection bodies with ')'.
  const end = after.search(/\s\|\s|\)\s*$/);
  return (end === -1 ? after : after.slice(0, end)).trim();
}

/**
 * Feedback addressed to the human developer who authored the agent's operating
 * instructions. Primary source is the tracked `instructions`-quality refinement queue
 * (open/ongoing/done lifecycle); developer-review-tagged memories that no refinement
 * already represents are folded in so manually-tagged or historical feedback still shows.
 */
function developerReviewRows(db: DatabaseSync, params: AwarenessQueryParams): AwarenessQueryRow[] {
  const scope = scopeFromParams(params);
  const limit = limitOf(params.limit, 60, 500);

  const refWhere = ["quality = 'instructions'"];
  const refBinds: BindValue[] = [];
  addExactScope(refWhere, refBinds, scope);
  addTextFilter(refWhere, refBinds, params.query, ['reasoning', 'remember', 'files_json', 'agent_id']);
  addStateFilter(refWhere, refBinds, stringList(params.state), 'state', state => state.toLowerCase());
  const refRows = db.prepare(
    `SELECT refinement_id, agent_id, workspace_path, artifact, repo, ref, files_json,
            reasoning, remember, state, created_at, updated_at
       FROM refinements
      WHERE ${refWhere.join(' AND ')}
      ORDER BY CASE state WHEN 'open' THEN 0 WHEN 'ongoing' THEN 1 ELSE 2 END, datetime(updated_at) DESC
      LIMIT ?`
  ).all(...refBinds, limit) as unknown as Array<Record<string, string | null>>;

  const rows: AwarenessQueryRow[] = refRows.map(row => ({
    source: 'refinement',
    id: String(row['refinement_id']),
    refinement_id: String(row['refinement_id']),
    state: String(row['state']),
    feedback: String(row['remember']),
    context: String(row['reasoning']),
    files: parseJsonList(row['files_json']),
    agent_id: String(row['agent_id']),
    workspace_path: row['workspace_path'] ?? null,
    artifact: row['artifact'] ?? null,
    repo: row['repo'] ?? null,
    ref: row['ref'] ?? null,
    created_at: String(row['created_at']),
    updated_at: String(row['updated_at']),
  }));

  // Fold in developer-review memories a refinement doesn't already carry.
  const refTexts = refRows.map(row => String(row['remember'] ?? '').trim()).filter(Boolean);
  const memWhere = ["state = 'ACTIVE'", `tags_json LIKE '%"developer-review"%'`];
  const memBinds: BindValue[] = [];
  addNullableScope(memWhere, memBinds, scope);
  addTextFilter(memWhere, memBinds, params.query, ['task_context', 'observation']);
  const memRows = db.prepare(
    `SELECT memory_id, agent_id, task_context, observation, importance, created_at, updated_at
       FROM memories
      WHERE ${memWhere.join(' AND ')}
      ORDER BY importance DESC, datetime(created_at) DESC
      LIMIT ?`
  ).all(...memBinds, limit) as unknown as Array<Record<string, string | number | null>>;
  for (const row of memRows) {
    const observation = String(row['observation'] ?? '');
    if (refTexts.some(text => text && observation.includes(text))) continue;
    rows.push({
      source: 'memory',
      id: String(row['memory_id']),
      memory_id: String(row['memory_id']),
      state: 'recorded',
      feedback: extractInstructionsFeedback(observation),
      context: String(row['task_context'] ?? ''),
      importance: Number(row['importance'] ?? 0),
      files: [],
      agent_id: String(row['agent_id']),
      created_at: String(row['created_at']),
      updated_at: row['updated_at'] ?? null,
    });
  }

  return rows.slice(0, limit);
}

function trackFile(
  map: Map<string, AwarenessQueryRow>,
  filePath: string,
  source: string,
  date: string | null | undefined,
  workspacePath: string | null,
): void {
  const resolved = localPathFromReference(filePath, workspacePath);
  const clean = resolved ?? stripLocationSuffix(filePath.startsWith('file:') ? filePath.slice('file:'.length) : filePath);
  if (!clean) return;
  const fileExists = existsSync(clean);
  const row = map.get(clean) ?? {
    file_path: clean,
    file_exists: fileExists,
    missing_file: !fileExists,
    memories: 0,
    gotchas: 0,
    tasks: 0,
    runs: 0,
    locks: 0,
    refinements: 0,
    signals: 0,
    edits: 0,
    last_seen_at: null,
  };
  row['file_exists'] = Boolean(row['file_exists']) || fileExists;
  row['missing_file'] = !Boolean(row['file_exists']);
  const current = Number(row[source] ?? 0);
  row[source] = current + 1;
  if (date && (!row['last_seen_at'] || String(date) > String(row['last_seen_at']))) row['last_seen_at'] = date;
  map.set(clean, row);
}

function fileRows(db: DatabaseSync, params: AwarenessQueryParams): AwarenessQueryRow[] {
  const scope = scopeFromParams(params);
  const limit = limitOf(params.limit, 80, 500);
  const files = new Map<string, AwarenessQueryRow>();

  const memoryWhere = ["m.state = 'ACTIVE'", "r.reference LIKE 'file:%'"];
  const memoryBinds: BindValue[] = [];
  addNullableScope(memoryWhere, memoryBinds, scope, 'm');
  addTextFilter(memoryWhere, memoryBinds, params.query, ['r.reference', 'm.task_context', 'm.observation']);
  const memoryRefs = db.prepare(
    `SELECT r.reference, m.label, m.created_at
       FROM memory_refs r
       JOIN memories m ON m.memory_id = r.memory_id
      WHERE ${memoryWhere.join(' AND ')}
      ORDER BY datetime(m.created_at) DESC
      LIMIT ?`
  ).all(...memoryBinds, 1000) as unknown as Array<{ reference: string; label: string; created_at: string }>;
  for (const ref of memoryRefs) {
    trackFile(files, ref.reference, 'memories', ref.created_at, scope.workspacePath);
    if (ref.label === 'GOTCHA') trackFile(files, ref.reference, 'gotchas', ref.created_at, scope.workspacePath);
  }

  for (const row of taskRows(db, withScope(params, { limit: 500 }))) {
    for (const file of row['paths'] as string[]) trackFile(files, file, 'tasks', String(row['created_at']), scope.workspacePath);
  }
  for (const row of runRows(db, withScope(params, { limit: 500 }))) {
    for (const file of row['files'] as string[]) trackFile(files, file, 'runs', String(row['created_at']), scope.workspacePath);
  }
  for (const row of lockRows(db, withScope(params, { limit: 500 }))) {
    trackFile(files, String(row['file_path']), 'locks', String(row['acquired_at']), scope.workspacePath);
  }
  for (const row of refinementRows(db, withScope(params, { limit: 500 }))) {
    for (const file of row['files'] as string[]) trackFile(files, file, 'refinements', String(row['updated_at']), scope.workspacePath);
  }
  for (const row of signalRows(db, withScope(params, { limit: 500 }))) {
    for (const file of row['files'] as string[]) trackFile(files, file, 'signals', String(row['created_at']), scope.workspacePath);
  }

  const editWhere: string[] = [];
  const editBinds: BindValue[] = [];
  addExactScope(editWhere, editBinds, scope);
  addTextFilter(editWhere, editBinds, params.query, ['file_path', 'old_file_path', 'operation', 'agent_id']);
  const editSqlWhere = editWhere.length > 0 ? `WHERE ${editWhere.join(' AND ')}` : '';
  const edits = db.prepare(
    `SELECT file_path, old_file_path, operation, agent_id, created_at
       FROM edit_log
       ${editSqlWhere}
      ORDER BY datetime(created_at) DESC
      LIMIT ?`
  ).all(...editBinds, 1000) as unknown as Array<{ file_path: string; old_file_path: string | null; created_at: string }>;
  for (const edit of edits) {
    trackFile(files, edit.file_path, 'edits', edit.created_at, scope.workspacePath);
    if (edit.old_file_path) trackFile(files, edit.old_file_path, 'edits', edit.created_at, scope.workspacePath);
  }

  return [...files.values()]
    .sort((a, b) => {
      const scoreA = (a['missing_file'] ? 20 : 0) + Number(a['locks'] ?? 0) * 10 + Number(a['gotchas'] ?? 0) * 6 + Number(a['memories'] ?? 0) * 4 + Number(a['tasks'] ?? 0) * 3 + Number(a['runs'] ?? 0) + Number(a['edits'] ?? 0);
      const scoreB = (b['missing_file'] ? 20 : 0) + Number(b['locks'] ?? 0) * 10 + Number(b['gotchas'] ?? 0) * 6 + Number(b['memories'] ?? 0) * 4 + Number(b['tasks'] ?? 0) * 3 + Number(b['runs'] ?? 0) + Number(b['edits'] ?? 0);
      return scoreB - scoreA || String(b['last_seen_at'] ?? '').localeCompare(String(a['last_seen_at'] ?? ''));
    })
    .slice(0, limit);
}

function countWhere(db: DatabaseSync, table: string, where: string[], binds: BindValue[]): number {
  const sqlWhere = where.length > 0 ? `WHERE ${where.join(' AND ')}` : '';
  const row = db.prepare(`SELECT COUNT(*) AS count FROM ${table} ${sqlWhere}`).get(...binds) as { count: number };
  return row.count;
}

function repoProfileRows(db: DatabaseSync, params: AwarenessQueryParams): AwarenessQueryRow[] {
  const scope = scopeFromParams(params);
  const memWhere = ["state = 'ACTIVE'"];
  const memBinds: BindValue[] = [];
  addNullableScope(memWhere, memBinds, scope);

  const taskWhere: string[] = [];
  const taskBinds: BindValue[] = [];
  addExactScope(taskWhere, taskBinds, scope, 'p');

  const planWhere: string[] = [];
  const planBinds: BindValue[] = [];
  addExactScope(planWhere, planBinds, scope);

  const runWhere: string[] = [];
  const runBinds: BindValue[] = [];
  addExactScope(runWhere, runBinds, scope);

  const lockWhere: string[] = [];
  const lockBinds: BindValue[] = [];
  addExactScope(lockWhere, lockBinds, scope, 't');

  const refinementWhere = ["state IN ('open','ongoing')"];
  const refinementBinds: BindValue[] = [];
  addExactScope(refinementWhere, refinementBinds, scope);

  const signalWhere = ["status = 'open'"];
  const signalBinds: BindValue[] = [];
  addExactScope(signalWhere, signalBinds, scope);

  const trackedFiles = fileRows(db, withScope(params, { limit: 500 }));
  return [
    { metric: 'active_memories', count: countWhere(db, 'memories', memWhere, memBinds) },
    { metric: 'gotchas', count: memoryRows(db, withScope(params, { view: 'gotchas', limit: 500 }), { gotchas: true }).length },
    { metric: 'lessons', count: memoryRows(db, withScope(params, { view: 'lessons', limit: 500 }), { lessons: true }).length },
    { metric: 'plans', count: countWhere(db, 'plans', planWhere, planBinds) },
    { metric: 'tasks', count: countWhere(db, 'tasks t JOIN plans p ON p.plan_id = t.plan_id', taskWhere, taskBinds) },
    { metric: 'runs', count: countWhere(db, 'task_runs', runWhere, runBinds) },
    { metric: 'active_locks', count: countWhere(db, 'locks l JOIN task_runs t ON t.run_id = l.run_id', lockWhere, lockBinds) },
    { metric: 'open_refinements', count: countWhere(db, 'refinements', refinementWhere, refinementBinds) },
    { metric: 'open_signals', count: countWhere(db, 'signals', signalWhere, signalBinds) },
    { metric: 'known_agents', count: agentRows(db, withScope(params, { limit: 500 })).length },
    { metric: 'tracked_files', count: trackedFiles.length },
    { metric: 'missing_file_refs', count: trackedFiles.filter(row => row['missing_file']).length },
    { metric: 'developer_review', count: developerReviewRows(db, withScope(params, { limit: 500 })).length },
  ];
}

function activityRows(db: DatabaseSync, params: AwarenessQueryParams): AwarenessQueryRow[] {
  const limit = limitOf(params.limit);
  const rows: AwarenessQueryRow[] = [];
  for (const row of memoryRows(db, withScope(params, { limit }))) {
    rows.push({
      kind: 'memory',
      id: String(row['memory_id']),
      title: `${row['label']}: ${summarize(String(row['task_context']), 80)}`,
      detail: summarize(String(row['observation']), 180),
      agent_id: String(row['agent_id']),
      created_at: String(row['created_at']),
    });
  }
  for (const row of taskRows(db, withScope(params, { limit }))) {
    rows.push({
      kind: 'task',
      id: String(row['task_id']),
      title: `${row['status']}: ${summarize(String(row['title']), 100)}`,
      detail: summarize(String(row['reasoning']), 180),
      agent_id: String(row['claimed_by'] ?? row['created_by']),
      created_at: String(row['created_at']),
    });
  }
  for (const row of runRows(db, withScope(params, { limit }))) {
    rows.push({
      kind: 'run',
      id: String(row['run_id']),
      title: `${row['status']}: ${summarize(String(row['rationale']), 100)}`,
      detail: summarize(String(row['test_plan']), 180),
      agent_id: String(row['agent_id']),
      created_at: String(row['created_at']),
    });
  }
  for (const row of signalRows(db, withScope(params, { limit }))) {
    rows.push({
      kind: 'signal',
      id: String(row['signal_id']),
      title: `${row['kind']}: ${summarize(String(row['subject']), 100)}`,
      detail: summarize(String(row['body'] ?? ''), 180),
      agent_id: String(row['from_agent']),
      created_at: String(row['created_at']),
    });
  }
  for (const row of refinementRows(db, withScope(params, { limit }))) {
    rows.push({
      kind: 'refinement',
      id: String(row['refinement_id']),
      title: `${row['state']}: ${summarize(String(row['remember']), 100)}`,
      detail: summarize(String(row['reasoning']), 180),
      agent_id: String(row['agent_id']),
      created_at: String(row['updated_at']),
    });
  }
  return rows
    .sort((a, b) => String(b['created_at']).localeCompare(String(a['created_at'])))
    .slice(0, limit);
}

function rowFiles(row: AwarenessQueryRow): string[] {
  const raw = row['files'] ?? row['paths'];
  return Array.isArray(raw) ? raw.map(String) : [];
}

function displayPath(filePath: string, workspacePath: string | null): string {
  if (!workspacePath) return filePath.replace(/\\/g, '/');
  const rel = relative(workspacePath, filePath);
  return rel && rel !== '..' && !rel.startsWith(`..${process.platform === 'win32' ? '\\' : '/'}`) && !isAbsolute(rel)
    ? rel.replace(/\\/g, '/')
    : filePath.replace(/\\/g, '/');
}

function filesUnderWorkRows(db: DatabaseSync, params: AwarenessQueryParams): AwarenessQueryRow[] {
  const scope = scopeFromParams(params);
  const where = ["tr.status = 'ACTIVE'", 'rf.ended_at IS NULL', 'rf.expires_at > ?'];
  const binds: BindValue[] = [utcNow()];
  addExactScope(where, binds, scope, 'tr');
  addTextFilter(where, binds, params.query, ['rf.file_path', 'tr.agent_id', 'tr.rationale', 'rf.reason_override', 't.title', 'p.name']);
  const agentId = params.agentId ?? params.agent_id;
  if (agentId) { where.push('tr.agent_id = ?'); binds.push(agentId); }
  if (params.file?.trim()) {
    const root = scope.workspacePath ?? process.cwd();
    const filePath = isAbsolute(params.file) ? resolve(params.file) : resolve(root, params.file);
    where.push('rf.file_path = ?');
    binds.push(filePath);
  }
  const rows = db.prepare(
    `SELECT rf.file_path, rf.run_id, tr.agent_id, tr.task_id,
            COALESCE(rf.reason_override, t.reasoning, tr.rationale) AS reason,
            t.plan_id, p.name AS plan_name, tr.workspace_path,
            CASE WHEN l.lock_id IS NULL THEN 0 ELSE 1 END AS locked,
            l.expires_at AS lock_expires_at
       FROM run_files rf
       JOIN task_runs tr ON tr.run_id = rf.run_id
       LEFT JOIN tasks t ON t.task_id = tr.task_id
       LEFT JOIN plans p ON p.plan_id = t.plan_id
       LEFT JOIN locks l ON l.run_id = rf.run_id AND l.file_path = rf.file_path
         AND (l.expires_at IS NULL OR l.expires_at > ?)
      WHERE ${where.join(' AND ')}
      ORDER BY rf.file_path, datetime(rf.heartbeat_at) DESC, tr.agent_id, rf.run_id`
  ).all(utcNow(), ...binds) as unknown as Array<Record<string, string | number | null>>;

  const grouped = new Map<string, Array<Record<string, string | number | null>>>();
  for (const row of rows) {
    const path = String(row['file_path']);
    const peers = grouped.get(path) ?? [];
    peers.push(row);
    grouped.set(path, peers);
  }

  return [...grouped.entries()].map(([filePath, peers]) => {
    const shown = peers.slice(0, 3);
    const lock = peers.find(peer => Number(peer['locked']) === 1);
    const workspacePath = String(peers[0]?.['workspace_path'] ?? scope.workspacePath ?? '') || null;
    return {
      item_type: 'file',
      id: filePath,
      path: displayPath(filePath, workspacePath),
      peer_count: peers.length,
      agents: shown.map(peer => String(peer['agent_id'])),
      run_ids: shown.map(peer => String(peer['run_id'])),
      task_ids: shown.flatMap(peer => peer['task_id'] == null ? [] : [String(peer['task_id'])]),
      plan_ids: shown.flatMap(peer => peer['plan_id'] == null ? [] : [String(peer['plan_id'])]),
      plans: shown.flatMap(peer => peer['plan_name'] == null ? [] : [String(peer['plan_name'])]),
      reasons: shown.map(peer => summarize(String(peer['reason'] ?? ''), 80)),
      omitted_peer_count: Math.max(0, peers.length - shown.length),
      locked: Boolean(lock),
      lock_agent_id: lock == null ? null : String(lock['agent_id']),
      lock_expires_at: lock?.['lock_expires_at'] ?? null,
    };
  });
}

function pushLimited(
  columns: Record<string, AwarenessQueryRow[]>,
  counts: Record<string, number>,
  column: string,
  row: AwarenessQueryRow,
  limit: number,
): void {
  counts[column] = (counts[column] ?? 0) + 1;
  const rows = columns[column] ?? [];
  if (rows.length < limit) {
    rows.push({ column, ...row });
    columns[column] = rows;
  }
}

function workboardRows(db: DatabaseSync, params: AwarenessQueryParams): AwarenessQueryRow[] {
  const limit = limitOf(params.limit, 10, 50);
  const columns: Record<string, AwarenessQueryRow[]> = {
    Inbox: [],
    Verify: [],
    Ready: [],
    Claimed: [],
    FilesUnderWork: [],
    RecentDone: [],
    MemoryReview: [],
    DeveloperReview: [],
    ProjectionHealth: [],
  };
  const counts: Record<string, number> = {};

  for (const row of filesUnderWorkRows(db, withScope(params, { limit: 200 }))) {
    pushLimited(columns, counts, 'FilesUnderWork', row, limit);
  }

  const openSignals = signalRows(db, withScope(params, { state: ['open'], limit: 200, includeBodies: false }));
  for (const row of openSignals) {
    pushLimited(columns, counts, 'Inbox', {
      item_type: 'signal',
      id: String(row['signal_id']),
      title: `${row['kind']}: ${summarize(String(row['subject']), 100)}`,
      detail: summarize(String(row['body'] ?? ''), 180),
      agent_id: String(row['from_agent']),
      status: String(row['status']),
      raw_ids: [String(row['signal_id'])],
      files: rowFiles(row),
      created_at: String(row['created_at']),
    }, limit);
  }

  const handoffs = refinementRows(db, withScope(params, { state: ['open', 'ongoing'], limit: 200 }))
    .filter(row => String(row['quality']) === 'handoff');
  for (const row of handoffs) {
    pushLimited(columns, counts, 'Inbox', {
      item_type: 'refinement',
      id: String(row['refinement_id']),
      title: summarize(String(row['remember']), 100),
      detail: summarize(String(row['reasoning']), 180),
      agent_id: String(row['agent_id']),
      status: String(row['state']),
      quality: String(row['quality']),
      raw_ids: [String(row['refinement_id'])],
      files: rowFiles(row),
      created_at: String(row['created_at']),
      updated_at: String(row['updated_at']),
    }, limit);
  }

  for (const row of taskRows(db, withScope(params, { state: ['VERIFY'], limit: 500 }))) {
    pushLimited(columns, counts, 'Verify', {
      item_type: 'task',
      id: String(row['task_id']),
      title: summarize(String(row['title']), 120),
      detail: summarize(String(row['acceptance_criteria']), 180),
      plan_id: String(row['plan_id']),
      agent_id: String(row['claimed_by'] ?? row['created_by']),
      status: String(row['status']),
      raw_ids: [String(row['task_id']), ...(row['run_id'] ? [String(row['run_id'])] : [])],
      files: rowFiles(row),
      created_at: String(row['created_at']),
      updated_at: String(row['updated_at']),
    }, limit);
  }

  // Quick lock-only flows have no plan task; they still owe verification.
  for (const row of runRows(db, withScope(params, { state: ['PENDING'], limit: 500 }))
    .filter(row => row['task_id'] == null)) {
    pushLimited(columns, counts, 'Verify', {
      item_type: 'run',
      id: String(row['run_id']),
      title: summarize(String(row['rationale']), 120),
      detail: summarize(String(row['test_plan']), 180),
      agent_id: String(row['agent_id']),
      status: String(row['status']),
      raw_ids: [String(row['run_id'])],
      files: rowFiles(row),
      created_at: String(row['created_at']),
      updated_at: String(row['updated_at']),
    }, limit);
  }

  for (const row of taskRows(db, withScope(params, { state: ['OPEN'], limit: 500 }))
    .filter(row => row['ready'] === true)) {
    pushLimited(columns, counts, 'Ready', {
      item_type: 'task',
      id: String(row['task_id']),
      title: summarize(String(row['title']), 120),
      detail: summarize(String(row['reasoning']), 180),
      plan_id: String(row['plan_id']),
      agent_id: String(row['created_by']),
      status: String(row['status']),
      priority: Number(row['priority']),
      raw_ids: [String(row['task_id'])],
      files: rowFiles(row),
      created_at: String(row['created_at']),
      updated_at: String(row['updated_at']),
    }, limit);
  }

  for (const row of taskRows(db, withScope(params, { state: ['IN_PROGRESS'], limit: 500 }))) {
    pushLimited(columns, counts, 'Claimed', {
      item_type: 'task',
      id: String(row['task_id']),
      title: summarize(String(row['title']), 120),
      detail: summarize(String(row['reasoning']), 180),
      plan_id: String(row['plan_id']),
      agent_id: String(row['claimed_by']),
      status: String(row['status']),
      raw_ids: [String(row['task_id']), ...(row['run_id'] ? [String(row['run_id'])] : [])],
      files: rowFiles(row),
      created_at: String(row['created_at']),
      updated_at: String(row['updated_at']),
      expires_at: row['claim_expires_at'] ?? null,
    }, limit);
  }

  for (const row of taskRows(db, withScope(params, { state: ['DONE', 'FAILED', 'CANCELLED'], limit: 200 }))) {
    pushLimited(columns, counts, 'RecentDone', {
      item_type: 'task',
      id: String(row['task_id']),
      title: `${row['status']}: ${summarize(String(row['title']), 100)}`,
      detail: summarize(String(row['acceptance_criteria']), 180),
      plan_id: String(row['plan_id']),
      agent_id: String(row['created_by']),
      status: String(row['status']),
      raw_ids: [String(row['task_id'])],
      files: rowFiles(row),
      created_at: String(row['created_at']),
      updated_at: String(row['updated_at']),
    }, limit);
  }

  for (const row of memoryRows(db, withScope(params, { limit: 200 }))) {
    const failureSignature = String(row['failure_signature'] ?? '');
    const refs = Array.isArray(row['references']) ? row['references'] as string[] : [];
    const missingRefs = Array.isArray(row['missing_references']) ? row['missing_references'] as string[] : [];
    const tags = Array.isArray(row['tags']) ? row['tags'] as string[] : [];
    const reviewReasons = [
      refs.length === 0 ? 'missing_refs' : null,
      missingRefs.length > 0 ? 'stale_file_refs' : null,
      failureSignature ? 'failure_signature' : null,
      tags.includes('anti-bloat') ? 'policy_memory' : null,
    ].filter((reason): reason is string => Boolean(reason));
    if (reviewReasons.length === 0) continue;
    pushLimited(columns, counts, 'MemoryReview', {
      item_type: 'memory',
      id: String(row['memory_id']),
      title: `${row['label']}:${row['importance']} ${summarize(String(row['task_context']), 100)}`,
      detail: summarize(String(row['observation']), 180),
      agent_id: String(row['agent_id']),
      status: 'review',
      reasons: reviewReasons,
      missing_reference_count: missingRefs.length,
      missing_references: missingRefs,
      raw_ids: [String(row['memory_id'])],
      files: (Array.isArray(row['file_references']) ? row['file_references'] as string[] : refs.filter(ref => ref.startsWith('file:')).map(ref => ref.slice('file:'.length))),
      created_at: String(row['created_at']),
      updated_at: row['updated_at'] ?? null,
    }, limit);
  }

  for (const row of developerReviewRows(db, withScope(params, { state: ['open', 'ongoing'], limit: 200 }))) {
    pushLimited(columns, counts, 'DeveloperReview', {
      item_type: String(row['source']) === 'refinement' ? 'refinement' : 'memory',
      id: String(row['id']),
      title: summarize(String(row['feedback']), 120),
      detail: summarize(String(row['context']), 180),
      agent_id: String(row['agent_id']),
      status: String(row['state']),
      raw_ids: [String(row['id'])],
      files: rowFiles(row),
      created_at: String(row['created_at']),
      updated_at: row['updated_at'] ?? null,
    }, limit);
  }

  const profile = Object.fromEntries(repoProfileRows(db, params).map(row => [String(row['metric']), Number(row['count'] ?? 0)])) as Record<string, number>;
  const activeMemories = Number(profile['active_memories'] ?? 0);
  const taskCount = Number(profile['tasks'] ?? 0);
  const openRefinements = Number(profile['open_refinements'] ?? 0);
  const openSignalCount = Number(profile['open_signals'] ?? 0);
  const missingFileRefs = Number(profile['missing_file_refs'] ?? 0);
  const projectionWarnings = [
    missingFileRefs > 0 ? 'missing_file_refs' : null,
    activeMemories > 200 ? 'active_memories_over_200' : null,
    taskCount > 500 ? 'task_rows_over_500' : null,
    openRefinements > 40 ? 'open_refinements_over_40' : null,
  ].filter((warning): warning is string => Boolean(warning));
  pushLimited(columns, counts, 'ProjectionHealth', {
    item_type: 'projection',
    id: 'projection-health',
    title: projectionWarnings.length > 0 ? 'Projection/bloat review suggested' : 'Projection health nominal',
    detail: projectionWarnings.join(', ') || 'No profile threshold warnings.',
    status: projectionWarnings.length > 0 ? 'review' : 'ok',
    count: projectionWarnings.length,
    raw_ids: [],
    files: [],
    active_memories: activeMemories,
    missing_file_refs: missingFileRefs,
    tasks: taskCount,
    open_refinements: openRefinements,
    open_signals: openSignalCount,
    created_at: utcNow(),
  }, limit);

  return Object.entries(columns).flatMap(([column, rows]) => {
    const total = counts[column] ?? rows.length;
    return rows.map(row => ({
      ...row,
      column_total: total,
      omitted_count: Math.max(0, total - rows.length),
    }));
  });
}

function rowsForView(db: DatabaseSync, view: AwarenessQueryView, params: AwarenessQueryParams): AwarenessQueryRow[] {
  switch (view) {
    case 'repo-profile': return repoProfileRows(db, params);
    case 'memories': return memoryRows(db, params);
    case 'gotchas': return memoryRows(db, params, { gotchas: true });
    case 'lessons': return memoryRows(db, params, { lessons: true });
    case 'plans': return planRows(db, params);
    case 'tasks': return taskRows(db, params);
    case 'runs': return runRows(db, params);
    case 'locks': return lockRows(db, params);
    case 'agents': return agentRows(db, params);
    case 'signals': return signalRows(db, params);
    case 'refinements': return refinementRows(db, params);
    case 'files': return fileRows(db, params);
    case 'activity': return activityRows(db, params);
    case 'workboard': return workboardRows(db, params);
    case 'developer-review': return developerReviewRows(db, params);
    case 'all': return [];
  }
}

export function queryAwareness(db: DatabaseSync, params: AwarenessQueryParams = {}): AwarenessQueryResult {
  const view = normalizeView(params.view);
  const scope = scopeFromParams(params);
  const generatedAt = utcNow();
  const filters = {
    query: params.query ?? null,
    limit: limitOf(params.limit),
    agent_id: params.agentId ?? params.agent_id ?? null,
    state: stringList(params.state),
    label: stringList(params.label),
    file: params.file ?? null,
    since: params.since ?? null,
  };

  if (view === 'all') {
    const sections: Record<string, AwarenessQuerySection> = {};
    for (const section of AWARENESS_QUERY_VIEWS) {
      if (section === 'all') continue;
      const rows = rowsForView(db, section, params);
      sections[section] = { count: rows.length, rows };
    }
    const rows = Object.entries(sections).map(([name, section]) => ({ section: name, count: section.count }));
    return {
      ok: true,
      view,
      generated_at: generatedAt,
      workspace_path: scope.workspacePath,
      artifact: scope.artifact,
      repo: scope.repo,
      ref: scope.ref,
      count: rows.length,
      rows,
      sections,
      filters,
    };
  }

  const rows = rowsForView(db, view, params);
  return {
    ok: true,
    view,
    generated_at: generatedAt,
    workspace_path: scope.workspacePath,
    artifact: scope.artifact,
    repo: scope.repo,
    ref: scope.ref,
    count: rows.length,
    rows,
    filters,
  };
}

/**
 * Developer-review digest for the CLI `reflect developer-review` command: the same
 * rows that feed `.octocode/DEVELOPER_REVIEW.md`, plus the rendered Markdown doc.
 */
export function developerReviewDoc(
  db: DatabaseSync,
  params: AwarenessQueryParams = {},
): { rows: AwarenessQueryRow[]; open: number; resolved: number; markdown: string } {
  const rows = developerReviewRows(db, params);
  const open = rows.filter(row => String(row['state']) !== 'done').length;
  return {
    rows,
    open,
    resolved: rows.length - open,
    markdown: renderDeveloperReviewDoc(rows, PROJECTION_MARKDOWN_BUDGETS['DEVELOPER_REVIEW.md']!.max_lines),
  };
}

export function formatAwarenessQueryResult(result: AwarenessQueryResult, format: string | null | undefined): string {
  const normalized = normalizeFormat(format);
  if (normalized === 'json') return JSON.stringify(result, null, 2);
  if (normalized === 'csv') return toCsv(result.rows);
  if (normalized === 'table') return toTable(result.rows);
  if (normalized === 'html') return renderAwarenessHtml(result);
  return toMarkdown(result);
}

export function renderAwarenessHtml(result: AwarenessQueryResult): string {
  const title = `Octocode Awareness: ${result.view}`;
  const sectionNames = result.sections ? Object.keys(result.sections) : [result.view];
  const sections = result.sections
    ? Object.entries(result.sections).map(([name, section]) => renderHtmlSection(name, section.rows)).join('\n')
    : renderHtmlSection(result.view, result.rows);
  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>${escapeHtml(title)}</title>
  <style>
    :root { color-scheme: light dark; font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }
    body { margin: 0; background: Canvas; color: CanvasText; }
    header { padding: 24px 28px 12px; border-bottom: 1px solid color-mix(in srgb, CanvasText 16%, transparent); }
    .controls { display: flex; flex-wrap: wrap; gap: 10px; margin-top: 16px; align-items: center; }
    .controls input, .controls select, .controls label { font: inherit; font-size: 13px; }
    .controls input, .controls select { min-height: 34px; padding: 6px 8px; border: 1px solid color-mix(in srgb, CanvasText 24%, transparent); border-radius: 6px; background: Canvas; color: CanvasText; }
    .controls input { min-width: min(420px, 100%); flex: 1 1 260px; }
    .controls label { display: inline-flex; align-items: center; gap: 6px; white-space: nowrap; color: color-mix(in srgb, CanvasText 76%, transparent); }
    main { padding: 20px 28px 40px; display: grid; gap: 28px; }
    h1 { margin: 0 0 8px; font-size: 24px; letter-spacing: 0; }
    h2 { margin: 0 0 12px; font-size: 18px; letter-spacing: 0; }
    .meta { color: color-mix(in srgb, CanvasText 62%, transparent); font-size: 13px; }
    table { width: 100%; border-collapse: collapse; font-size: 13px; }
    th, td { text-align: left; vertical-align: top; padding: 8px 10px; border-bottom: 1px solid color-mix(in srgb, CanvasText 12%, transparent); }
    th { font-weight: 650; white-space: nowrap; }
    th button { all: unset; cursor: pointer; }
    th button::after { content: " v"; color: color-mix(in srgb, CanvasText 46%, transparent); }
    td { max-width: 460px; overflow-wrap: anywhere; }
    section { overflow-x: auto; }
    section[hidden], tr[hidden] { display: none; }
    code { font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size: 12px; }
  </style>
</head>
<body>
  <header>
    <h1>${escapeHtml(title)}</h1>
    <div class="meta">Generated ${escapeHtml(result.generated_at)} for <code>${escapeHtml(result.workspace_path ?? 'global')}</code></div>
    <div class="controls">
      <input id="global-filter" type="search" placeholder="Filter rows" autocomplete="off">
      <select id="section-filter" aria-label="Section">
        <option value="">All sections</option>
        ${sectionNames.map(name => `<option value="${escapeHtml(name)}">${escapeHtml(name)}</option>`).join('')}
      </select>
      <label><input id="missing-filter" type="checkbox"> Missing files</label>
    </div>
  </header>
  <main>
    ${sections}
  </main>
  <script>
    const filterInput = document.querySelector('#global-filter');
    const sectionFilter = document.querySelector('#section-filter');
    const missingFilter = document.querySelector('#missing-filter');
    function applyFilters() {
      const text = filterInput.value.trim().toLowerCase();
      const wantedSection = sectionFilter.value;
      const onlyMissing = missingFilter.checked;
      for (const section of document.querySelectorAll('section[data-section]')) {
        const sectionMatches = !wantedSection || section.dataset.section === wantedSection;
        let visible = 0;
        for (const row of section.querySelectorAll('tbody tr')) {
          const rowMatches = (!text || row.textContent.toLowerCase().includes(text)) && (!onlyMissing || row.dataset.missing === 'true');
          row.hidden = !(sectionMatches && rowMatches);
          if (!row.hidden) visible++;
        }
        section.hidden = !sectionMatches || visible === 0;
      }
    }
    for (const control of [filterInput, sectionFilter, missingFilter]) control.addEventListener('input', applyFilters);
    for (const button of document.querySelectorAll('th button[data-key]')) {
      button.addEventListener('click', () => {
        const table = button.closest('table');
        const tbody = table.querySelector('tbody');
        const index = Array.from(button.closest('tr').children).indexOf(button.closest('th'));
        const direction = button.dataset.direction === 'asc' ? 'desc' : 'asc';
        button.dataset.direction = direction;
        const rows = Array.from(tbody.querySelectorAll('tr'));
        rows.sort((a, b) => {
          const av = a.children[index]?.textContent.trim() ?? '';
          const bv = b.children[index]?.textContent.trim() ?? '';
          const an = Number(av);
          const bn = Number(bv);
          const cmp = Number.isFinite(an) && Number.isFinite(bn) ? an - bn : av.localeCompare(bv);
          return direction === 'asc' ? cmp : -cmp;
        });
        for (const row of rows) tbody.appendChild(row);
        applyFilters();
      });
    }
    applyFilters();
  </script>
</body>
</html>
`;
}

export function writeAwarenessView(
  db: DatabaseSync,
  params: AwarenessQueryParams & { out?: string | null; format?: string | null } = {},
): { ok: true; path: string; view: AwarenessQueryView; count: number } {
  const result = queryAwareness(db, params);
  const workspacePath = scopeFromParams(params).workspacePath ?? process.cwd();
  const outPath = resolveWorkspaceOutputPath(params.out, workspacePath, join(workspacePath, '.octocode', 'awareness', 'index.html'));
  mkdirSync(join(outPath, '..'), { recursive: true });
  writeFileSync(outPath, renderAwarenessHtml(result), 'utf8');
  return { ok: true, path: outPath, view: result.view, count: result.count };
}

function resolveWorkspaceOutputPath(output: string | null | undefined, workspacePath: string, defaultPath: string): string {
  const target = output?.trim() || defaultPath;
  return isAbsolute(target) ? resolve(target) : resolve(workspacePath, target);
}

export function injectRepoContext(db: DatabaseSync, params: RepoContextInjectParams = {}): RepoContextInjectResult {
  const scope = scopeFromParams(params);
  const workspacePath = scope.workspacePath ?? process.cwd();
  const rawOutDir = params.outDir ?? params.out_dir;
  const outDir = resolveWorkspaceOutputPath(rawOutDir, workspacePath, join(workspacePath, '.octocode'));
  const mode = normalizeMode(params.mode);
  const includeView = params.includeView ?? params.include_view ?? true;
  const check = params.check ?? true;
  const queryParams: AwarenessQueryParams = {
    ...params,
    workspacePath,
    limit: limitOf(params.limit, 50, 500),
    view: 'all',
  };
  SCOPE_CACHE.set(queryParams, scope);
  const all = queryAwareness(db, queryParams);
  const filesWritten: string[] = [];
  const writtenContent: Record<string, string> = {};
  const warnings: string[] = [];

  function write(relPath: string, content: string): void {
    const full = join(outDir, relPath);
    mkdirSync(join(full, '..'), { recursive: true });
    writeFileSync(full, content, 'utf8');
    writtenContent[relPath] = content;
    filesWritten.push(full);
  }

  const sections = all.sections ?? {};
  const counts = Object.fromEntries(Object.entries(sections).map(([name, section]) => [name, section.count]));

  write('AGENTS.md', renderRepoAgentsMd(all));
  write('MEMORY.md', renderRowsDoc('Memory', sections['memories']?.rows ?? [], 'Active awareness memories for this repo.', PROJECTION_MARKDOWN_BUDGETS['MEMORY.md']!.max_lines));
  write('GOTCHAS.md', renderRowsDoc('Gotchas', sections['gotchas']?.rows ?? [], 'Failures, traps, and sharp edges agents should check before editing.', PROJECTION_MARKDOWN_BUDGETS['GOTCHAS.md']!.max_lines));
  write('LEARN.md', renderRowsDoc('Learning And Opportunities', sections['lessons']?.rows ?? [], 'Decisions, architecture notes, workflows, and improvement ideas.', PROJECTION_MARKDOWN_BUDGETS['LEARN.md']!.max_lines));
  write('BOOKMARKS.md', renderBookmarksDoc(sections['memories']?.rows ?? []));
  write('DEVELOPER_REVIEW.md', renderDeveloperReviewDoc(sections['developer-review']?.rows ?? [], PROJECTION_MARKDOWN_BUDGETS['DEVELOPER_REVIEW.md']!.max_lines));

  for (const view of CSV_VIEWS) {
    write(join('awareness', 'csv', `${view}.csv`), toCsv(sections[view]?.rows ?? []));
  }

  if (includeView) {
    write(join('awareness', 'index.html'), renderAwarenessHtml(all));
  }

  write(join('references', 'repo-map.md'), renderReferenceDoc('Repo Map', [
    'Generated overview of awareness-tracked files and activity.',
    'Use `.octocode/awareness/csv/files.csv` when filtering or sorting by file path.',
    'Use the live command `octocode-awareness query files --workspace <repo>` when freshness matters.',
  ], sections['files']?.rows ?? []));
  write(join('references', 'commands.md'), renderReferenceDoc('Awareness Commands', [
    '`octocode-awareness query <view>` reads the SQLite store for agents and scripts.',
    '`octocode-awareness query all --format html --out .octocode/awareness/index.html` writes a static human browser view; use `npx @octocodeai/octocode-awareness` only when no local CLI exists.',
    '`octocode-awareness repo inject --out .octocode` regenerates these Markdown, CSV, and HTML projections.',
  ]));
  write(join('references', 'testing.md'), renderReferenceDoc('Testing And Verification', [
    'Treat generated memories as leads. Verify current files and command output before acting.',
    'Release locks with `verify mark` or `lock release --verified` after declared tests actually run.',
    'Record new durable failures with `reflect record --failure-signature` or `memory record --label GOTCHA`.',
  ]));
  write(join('references', 'architecture.md'), renderReferenceDoc('Architecture Notes', [
    'The SQLite awareness DB is canonical. Files under `.octocode/` are generated projections.',
    'Keep workspace AGENTS.md concise and point agents here for repo-specific memory indexes.',
    'Do not edit generated CSV/Markdown snapshots by hand; regenerate after important memory changes.',
  ]));

  if (check) {
    const ignored = gitCheckIgnored(workspacePath, outDir);
    if (ignored.ignored) {
      warnings.push(`generated path is gitignored: ${relative(workspacePath, outDir) || outDir}; remove the ignore intentionally if this repo should share .octocode`);
    }
    if (mode === 'share' && ignored.ignored) {
      warnings.push('mode=share requested, but git currently ignores the generated .octocode path');
    }
  }

  const projectionBudgets: Record<string, ProjectionMarkdownBudgetStatus> = Object.fromEntries(Object.entries(PROJECTION_MARKDOWN_BUDGETS).map(([relPath, budget]) => {
    const actualLines = lineCount(writtenContent[relPath] ?? '');
    return [relPath, {
      ...budget,
      actual_lines: actualLines,
      within_budget: actualLines <= budget.max_lines,
    }];
  }));
  for (const [relPath, budget] of Object.entries(projectionBudgets)) {
    if (!budget.within_budget) warnings.push(`projection budget exceeded: ${relPath} has ${budget.actual_lines}/${budget.max_lines} lines`);
  }

  const generatedAt = utcNow();
  const manifestRelPath = join('awareness', 'manifest.json');
  const manifestFiles = [
    ...filesWritten.map(file => relative(workspacePath, file)),
    relative(workspacePath, join(outDir, manifestRelPath)),
  ];
  const manifest = {
    schema_version: 1,
    generated_at: generatedAt,
    generator: '@octocodeai/octocode-awareness repo inject',
    mode,
    workspace_path: workspacePath,
    artifact: scope.artifact,
    repo: scope.repo,
    ref: scope.ref,
    source: {
      canonical: '~/.octocode/memory/awareness.sqlite3',
      projection: '.octocode',
    },
    policy: {
      gitignore_modified: false,
      share_decision: 'user-owned',
    },
    counts,
    budgets: {
      markdown: projectionBudgets,
      workboard: WORKBOARD_BUDGET,
      attend_compact: ATTEND_COMPACT_BUDGET,
    },
    files: manifestFiles,
    warnings,
  };
  write(manifestRelPath, JSON.stringify(manifest, null, 2) + '\n');

  return {
    ok: true,
    generated_at: generatedAt,
    workspace_path: workspacePath,
    out_dir: outDir,
    mode,
    count: filesWritten.length,
    files: filesWritten,
    warnings,
    manifest,
  };
}

function renderRepoAgentsMd(all: AwarenessQueryResult): string {
  const sections = all.sections ?? {};
  const profile = sections['repo-profile']?.rows ?? [];
  const counts = Object.fromEntries(profile.map(row => [String(row['metric']), row['count'] ?? 0]));
  const gotchas = (sections['gotchas']?.rows ?? []).slice(0, 5);
  const lessons = (sections['lessons']?.rows ?? []).slice(0, 5);
  const fileWorkRows = (sections['workboard']?.rows ?? [])
    .filter(row => row['column'] === 'FilesUnderWork');
  const fileWork = fileWorkRows.slice(0, 3);
  const locks = (sections['locks']?.rows ?? []).slice(0, 3);
  const lockTotal = (sections['locks']?.rows ?? []).length;
  const projectionWarnings = [
    Number(counts['missing_file_refs'] ?? 0) > 0 ? `Missing/stale file refs (${counts['missing_file_refs']}) — use \`query files --format table\` before trusting bookmarks.` : null,
    Number(counts['active_memories'] ?? 0) > 200 ? `Active memories high (${counts['active_memories']}) — prefer recall/CSV over full Markdown.` : null,
    Number(counts['tasks'] ?? 0) > 500 ? `Task history high (${counts['tasks']}) — use \`query workboard\`.` : null,
    Number(counts['open_refinements'] ?? 0) > 40 ? `Open refinements high (${counts['open_refinements']}) — filter CSV before promoting.` : null,
  ].filter((item): item is string => Boolean(item));
  const lines = [
    '# Octocode Awareness Map',
    '',
    '<!-- Generated by `octocode-awareness repo inject`. Regenerate instead of hand-editing. -->',
    '',
    'Digested awareness entrypoint. Root `AGENTS.md` should point here. SQLite is canonical; this folder is a capped wiki.',
    '',
    '## How To Use',
    '',
    '- Live: `octocode-awareness attend|work list|query|memory recall|workspace status --workspace <repo>`.',
    '- Wiki leads below are projections, not proof. After inject, append a root `AGENTS.md` → `.octocode/AGENTS.md` pointer if missing.',
    '',
    '## Snapshot',
    '',
    `- Memories ${counts['active_memories'] ?? 0} · Gotchas ${counts['gotchas'] ?? 0} · Lessons ${counts['lessons'] ?? 0} · MissingFiles ${counts['missing_file_refs'] ?? 0} · Locks ${counts['active_locks'] ?? 0} · Refinements ${counts['open_refinements'] ?? 0} · Signals ${counts['open_signals'] ?? 0} · DevReview ${counts['developer_review'] ?? 0}`,
    '',
    '## Retro Files Map',
    '',
    'Generated retrospective projections in this folder (SQLite is canonical; regenerate, don\'t hand-edit). File work, locks, signals, and tasks live only in the DB — use live `attend`, `work`, or `query`.',
    '',
    '- Gotchas → `.octocode/GOTCHAS.md` · live `query gotchas` / `memory recall`',
    '- Lessons → `.octocode/LEARN.md` · live `query lessons`',
    '- Memory index → `.octocode/MEMORY.md` · live `memory recall --smart`',
    '- Bookmarks → `.octocode/BOOKMARKS.md` · Files/missing refs → live `query files` or `awareness/csv/files.csv` · Workboard → live `query workboard`',
    '- Developer review → `.octocode/DEVELOPER_REVIEW.md` · live `reflect developer-review` — agent feedback on the instructions themselves',
    '',
    '## Read Before Editing',
    '',
    '- Run `attend`, then inspect `FilesUnderWork` before editing. Record ordinary work with `work start`; overlap is allowed and visible.',
    '- Exclusive locks are reserved for sensitive files. An active exclusive lock blocks conflicting work.',
    '- Read GOTCHAS + LEARN; run `query files --format table` or filter `awareness/csv/files.csv` for affected and missing paths.',
    '- Prefer live `attend` / `work list` / `query` when freshness matters; `repo inject` after important memories.',
    '',
    '## Projection Health',
    '',
    '- Canonical DB: `~/.octocode/memory/awareness.sqlite3`. Manifest: `.octocode/awareness/manifest.json`.',
    ...projectionWarnings.map(warning => `- ${warning}`),
    '',
  ];

  if (fileWork.length > 0) {
    lines.push('## Files Under Work', '');
    for (const row of fileWork) {
      const locked = row['locked'] ? ' · exclusive lock' : '';
      lines.push(`- ${row['path']} — ${row['peer_count']} worker(s): ${row['agents']} · ${summarize(String(row['reasons'] ?? 'reason not recorded'), 100)}${locked}`);
    }
    if (fileWorkRows.length > fileWork.length) lines.push(`- …and ${fileWorkRows.length - fileWork.length} more (live: \`work list\` or \`query workboard\`)`);
    lines.push('');
  }

  if (locks.length > 0) {
    lines.push('## Active Exclusive Locks', '');
    for (const lock of locks) lines.push(`- ${lock['file_path']} — ${lock['agent_id']}`);
    if (lockTotal > locks.length) lines.push(`- …and ${lockTotal - locks.length} more (live: \`query locks\`)`);
    lines.push('');
  }

  if (gotchas.length > 0) {
    lines.push('## Top Gotchas', '');
    for (const row of gotchas) lines.push(`- [${row['importance']}] ${summarize(String(row['observation']), 140)}`);
    lines.push('');
  }

  if (lessons.length > 0) {
    lines.push('## Top Lessons', '');
    for (const row of lessons) lines.push(`- [${row['label']}:${row['importance']}] ${summarize(String(row['observation']), 140)}`);
    lines.push('');
  }

  lines.push('## References', '');
  lines.push('- `.octocode/MEMORY.md` · `.octocode/GOTCHAS.md` · `.octocode/LEARN.md` · `.octocode/BOOKMARKS.md` · `.octocode/DEVELOPER_REVIEW.md`');
  lines.push('- `.octocode/awareness/manifest.json` · `.octocode/references/`');
  lines.push('');
  return lines.join('\n');
}

function renderRowsDoc(title: string, rows: AwarenessQueryRow[], description: string, maxLines?: number): string {
  // Prefer higher-importance rows when a line budget forces omission.
  const ranked = [...rows].sort((a, b) => {
    const imp = Number(b['importance'] ?? 0) - Number(a['importance'] ?? 0);
    if (imp !== 0) return imp;
    return String(a['memory_id'] ?? a['plan_id'] ?? a['task_id'] ?? a['run_id'] ?? '')
      .localeCompare(String(b['memory_id'] ?? b['plan_id'] ?? b['task_id'] ?? b['run_id'] ?? ''));
  });
  const lines = [
    `# ${title}`,
    '',
    '<!-- Generated by `octocode-awareness repo inject`. Regenerate instead of hand-editing. -->',
    '',
    description,
    '',
    `Count: ${rows.length}`,
    '',
  ];
  let omitted = 0;
  for (const row of ranked) {
    const id = String(row['memory_id'] ?? row['refinement_id'] ?? row['plan_id'] ?? row['task_id'] ?? row['run_id'] ?? row['signal_id'] ?? row['file_path'] ?? 'item');
    const label = row['label'] ? `[${row['label']}:${row['importance'] ?? ''}] ` : '';
    const titleText = row['task_context'] ?? row['subject'] ?? row['remember'] ?? row['name'] ?? row['title'] ?? row['rationale'] ?? row['file_path'] ?? id;
    const block = [`## ${label}${summarize(String(titleText), 100)}`];
    if (row['observation']) block.push('', summarize(String(row['observation']), 500));
    if (row['failure_signature']) block.push('', `Failure signature: \`${row['failure_signature']}\``);
    const refs = Array.isArray(row['references']) ? row['references'] as string[] : [];
    if (refs.length > 0) block.push('', `Refs: ${refs.join(', ')}`);
    const missingRefs = Array.isArray(row['missing_references']) ? row['missing_references'] as string[] : [];
    if (missingRefs.length > 0) block.push('', `Missing refs: ${missingRefs.join(', ')}`);
    block.push('', `Source id: \`${id}\``, '');
    const needsOmittedLine = omitted === 0 && rows.length > 0;
    const reserve = maxLines ? (needsOmittedLine ? 3 : 1) : 0;
    if (maxLines && lines.length + block.length + reserve > maxLines) {
      omitted++;
      continue;
    }
    lines.push(...block);
  }
  if (omitted > 0) {
    const note = `Omitted by projection cap: ${omitted}. Use CSV/HTML/query views for full rows.`;
    if (!maxLines || lines.length + 2 <= maxLines) lines.push(note, '');
  }
  return lines.join('\n');
}

function bookmarkKind(reference: string): string {
  const lower = reference.toLowerCase();
  if (/^(github|gh|repo):/.test(lower) || lower.includes('github.com/')) return 'Repos';
  if (/^https?:\/\//.test(lower)) return 'URLs';
  if (/^(file|path):/.test(lower) || lower.startsWith('/') || lower.startsWith('./')) return 'Files';
  if (/^(doc|docs|paper|book|resource|skill):/.test(lower)) return 'Docs';
  if (/^[a-z][a-z0-9+.-]*:/.test(lower)) return 'URIs';
  return 'Other';
}

function renderBookmarksDoc(memoryRows: AwarenessQueryRow[]): string {
  const byRef = new Map<string, { kind: string; sourceIds: string[]; labels: string[]; titles: string[]; missing: boolean }>();
  for (const row of memoryRows) {
    const refs = Array.isArray(row['references']) ? row['references'] as string[] : [];
    const missingRefs = new Set(Array.isArray(row['missing_references']) ? row['missing_references'] as string[] : []);
    const sourceId = String(row['memory_id'] ?? 'memory');
    const label = `${row['label'] ?? 'MEMORY'}:${row['importance'] ?? ''}`.replace(/:$/, '');
    const title = summarize(String(row['task_context'] ?? row['observation'] ?? sourceId), 90);
    for (const rawRef of refs) {
      const ref = rawRef.trim();
      if (!ref) continue;
      const entry = byRef.get(ref) ?? { kind: bookmarkKind(ref), sourceIds: [], labels: [], titles: [], missing: false };
      if (!entry.sourceIds.includes(sourceId)) entry.sourceIds.push(sourceId);
      if (!entry.labels.includes(label)) entry.labels.push(label);
      if (!entry.titles.includes(title)) entry.titles.push(title);
      entry.missing = entry.missing || missingRefs.has(ref);
      byRef.set(ref, entry);
    }
  }

  const entries = [...byRef.entries()]
    .sort((a, b) => (
      a[1].kind.localeCompare(b[1].kind)
      || b[1].sourceIds.length - a[1].sourceIds.length
      || a[0].localeCompare(b[0])
    ))
    .slice(0, 80);
  const omitted = Math.max(0, byRef.size - entries.length);
  const lines = [
    '# Bookmarks',
    '',
    '<!-- Generated by `octocode-awareness repo inject`. Regenerate instead of hand-editing. -->',
    '',
    'Learnable resource leads from awareness memory references: URLs, repos, file paths, docs, papers, skills, and other URIs.',
    'SQLite remains canonical; verify each bookmark against current source or primary material before relying on it.',
    '',
    `Count: ${byRef.size}`,
    omitted > 0 ? `Omitted by cap: ${omitted}` : null,
    '',
  ].filter((line): line is string => line !== null);

  let currentKind = '';
  for (const [ref, entry] of entries) {
    if (entry.kind !== currentKind) {
      currentKind = entry.kind;
      lines.push(`## ${currentKind}`, '');
    }
    const sourceText = entry.sourceIds.slice(0, 3).join(', ');
    const titleText = entry.titles.slice(0, 2).join(' | ');
    const labelText = entry.labels.slice(0, 3).join(', ');
    const health = entry.missing ? ' [missing file]' : '';
    lines.push(`- \`${ref}\`${health} - ${labelText}; source: ${sourceText}; ${titleText}`);
  }
  lines.push('');
  return lines.join('\n');
}

function renderDeveloperReviewDoc(rows: AwarenessQueryRow[], maxLines: number): string {
  const open = rows.filter(row => String(row['state']) !== 'done');
  const resolved = rows.filter(row => String(row['state']) === 'done');
  const lines = [
    '# Developer Review',
    '',
    '<!-- Generated by `octocode-awareness repo inject`. Regenerate instead of hand-editing. -->',
    '',
    'Feedback from agents to the human who authors this repo\'s agent instructions',
    '(root `AGENTS.md`, `.octocode/AGENTS.md`, SKILL.md, system prompt, task briefs).',
    'Each item is where the *instructions* — not the code, not the harness — were ambiguous,',
    'wrong, over-constraining, or missing context. Recorded via `reflect record --fix-instructions`.',
    '',
    `Open: ${open.length} · Resolved: ${resolved.length}`,
    '',
  ];

  if (rows.length === 0) {
    lines.push(
      'No instruction feedback yet.',
      '',
      'Agents: when your instructions cost you time or a wrong turn, record it:',
      '`octocode-awareness reflect record --outcome partial --task "<what you did>" \\',
      '  --fix-instructions "<what the instructions should have said>"`',
      '',
    );
    return lines.join('\n');
  }

  function renderSection(title: string, sectionRows: AwarenessQueryRow[]): void {
    if (sectionRows.length === 0) return;
    lines.push(`## ${title} (${sectionRows.length})`, '');
    for (const row of sectionRows) {
      const source = String(row['source']);
      const state = String(row['state']);
      const agent = String(row['agent_id'] ?? 'agent');
      const files = rowFiles(row);
      const block = [`- ${summarize(String(row['feedback']), 240)}`];
      const meta = [`from ${agent}`, `via ${source}${state && state !== 'recorded' ? `:${state}` : ''}`, `id \`${row['id']}\``];
      if (files.length > 0) meta.push(`files: ${files.slice(0, 3).join(', ')}${files.length > 3 ? ` +${files.length - 3}` : ''}`);
      block.push(`  - ${meta.join(' · ')}`);
      if (maxLines && lines.length + block.length + 1 > maxLines) {
        lines.push(`- …more omitted by projection cap. Use \`query developer-review\` or \`reflect developer-review\`.`, '');
        return;
      }
      lines.push(...block);
    }
    lines.push('');
  }

  renderSection('Open', open);
  renderSection('Resolved', resolved);
  return lines.join('\n');
}

function renderReferenceDoc(title: string, bullets: string[], rows: AwarenessQueryRow[] = []): string {
  const lines = [
    `# ${title}`,
    '',
    '<!-- Generated by `octocode-awareness repo inject`. Regenerate instead of hand-editing. -->',
    '',
    ...bullets.map(item => `- ${item}`),
    '',
  ];
  if (rows.length > 0) {
    lines.push('## Top Rows', '');
    for (const row of rows.slice(0, 25)) {
      const primary = row['file_path'] ?? row['title'] ?? row['metric'] ?? JSON.stringify(row);
      lines.push(`- ${primary}`);
    }
    lines.push('');
  }
  return lines.join('\n');
}

function renderHtmlSection(name: string, rows: AwarenessQueryRow[]): string {
  return `<section data-section="${escapeHtml(name)}">
  <h2>${escapeHtml(name)} (${rows.length})</h2>
  ${rows.length === 0 ? '<p class="meta">No rows.</p>' : `<table>${renderHtmlTable(rows)}</table>`}
</section>`;
}

function renderHtmlTable(rows: AwarenessQueryRow[]): string {
  const keys = keysForRows(rows).slice(0, 12);
  const header = `<thead><tr>${keys.map(key => `<th><button type="button" data-key="${escapeHtml(key)}">${escapeHtml(key)}</button></th>`).join('')}</tr></thead>`;
  const body = rows.map(row => {
    const missing = row['missing_file'] === true || Number(row['missing_reference_count'] ?? 0) > 0 || (Array.isArray(row['missing_references']) && row['missing_references'].length > 0);
    return `<tr data-missing="${missing ? 'true' : 'false'}">${keys.map(key => `<td>${escapeHtml(cellToString(row[key]))}</td>`).join('')}</tr>`;
  }).join('\n');
  return `${header}<tbody>${body}</tbody>`;
}

function keysForRows(rows: AwarenessQueryRow[]): string[] {
  const keys: string[] = [];
  for (const row of rows) {
    for (const key of Object.keys(row)) {
      if (!keys.includes(key)) keys.push(key);
    }
  }
  return keys;
}

function toCsv(rows: AwarenessQueryRow[]): string {
  if (rows.length === 0) return '';
  const keys = keysForRows(rows);
  return [
    keys.map(csvCell).join(','),
    ...rows.map(row => keys.map(key => csvCell(cellToString(row[key]))).join(',')),
  ].join('\n') + '\n';
}

function toTable(rows: AwarenessQueryRow[]): string {
  if (rows.length === 0) return 'No rows.\n';
  const keys = keysForRows(rows).slice(0, 10);
  const widths = keys.map(key => Math.min(40, Math.max(key.length, ...rows.map(row => cellToString(row[key]).length))));
  const line = keys.map((key, i) => key.padEnd(widths[i] ?? key.length)).join('  ');
  const sep = widths.map(w => '-'.repeat(w)).join('  ');
  const body = rows.map(row => keys.map((key, i) => truncate(cellToString(row[key]), widths[i] ?? 40).padEnd(widths[i] ?? 40)).join('  '));
  return [line, sep, ...body].join('\n') + '\n';
}

function toMarkdown(result: AwarenessQueryResult): string {
  const lines = [
    `# Awareness ${result.view}`,
    '',
    `Generated: ${result.generated_at}`,
    `Workspace: ${result.workspace_path ?? 'global'}`,
    '',
  ];
  if (result.sections) {
    for (const [name, section] of Object.entries(result.sections)) {
      lines.push(`## ${name} (${section.count})`, '', markdownRows(section.rows), '');
    }
  } else {
    lines.push(markdownRows(result.rows), '');
  }
  return lines.join('\n');
}

function markdownRows(rows: AwarenessQueryRow[]): string {
  if (rows.length === 0) return '_No rows._';
  return rows.map(row => {
    const id = row['memory_id'] ?? row['plan_id'] ?? row['task_id'] ?? row['run_id'] ?? row['signal_id'] ?? row['refinement_id'] ?? row['file_path'] ?? row['metric'] ?? 'row';
    const label = row['label'] ? `[${cellToString(row['label'])}:${cellToString(row['importance'])}] ` : '';
    const title = row['task_context'] ?? row['subject'] ?? row['remember'] ?? row['name'] ?? row['title'] ?? row['rationale'] ?? row['metric'] ?? '';
    const text = row['observation'] ?? row['objective'] ?? row['reasoning'] ?? row['count'] ?? '';
    const extras: string[] = [];
    if (row['failure_signature']) extras.push(`failure=${cellToString(row['failure_signature'])}`);
  if (Array.isArray(row['references']) && row['references'].length > 0) extras.push(`refs=${(row['references'] as string[]).join(', ')}`);
  if (Array.isArray(row['missing_references']) && row['missing_references'].length > 0) extras.push(`missing=${(row['missing_references'] as string[]).join(', ')}`);
    const suffix = extras.length > 0 ? ` (${extras.join('; ')})` : '';
    return `- \`${cellToString(id)}\` ${label}${summarize(cellToString(title), 100)} - ${summarize(cellToString(text), 220)}${suffix}`;
  }).join('\n');
}

function csvCell(value: unknown): string {
  const s = String(value ?? '');
  return /[",\n\r]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}

function cellToString(value: unknown): string {
  if (Array.isArray(value)) return value.join('; ');
  if (value == null) return '';
  if (typeof value === 'object') return JSON.stringify(value);
  return String(value);
}

function truncate(value: string, width: number): string {
  if (value.length <= width) return value;
  if (width <= 3) return '.'.repeat(width);
  return value.slice(0, width - 3) + '...';
}

function summarize(value: string, max: number): string {
  const flat = value.replace(/\s+/g, ' ').trim();
  if (flat.length <= max) return flat;
  return flat.slice(0, Math.max(0, max - 3)).trimEnd() + '...';
}

function lineCount(value: string): number {
  if (!value) return 0;
  return value.split(/\r\n|\r|\n/).length;
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

function gitCheckIgnored(cwd: string, path: string): { ignored: boolean } {
  const candidate = isAbsolute(path) ? relative(cwd, path) : path;
  const result = spawnSync('git', ['check-ignore', '-q', candidate], { cwd, encoding: 'utf8' });
  return { ignored: result.status === 0 };
}
