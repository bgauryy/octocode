import { isAbsolute, resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { canonicalizePath, fillScope, normalizeWorkspacePath } from './git.js';
import { normalizeArtifact, utcNow } from './helpers.js';

export const SESSION_CAPTURE_FILE_LIMIT = 20;
export const SESSION_CAPTURE_VISIBLE_FILE_LIMIT = 10;
export const SESSION_CAPTURE_RUN_DETAIL_LIMIT = 3;
export const SESSION_CAPTURE_RUN_FILE_LIMIT = 3;
export const SESSION_CAPTURE_TEXT_LIMIT = 120;
export const MAX_WAIT_MS = 3600_000;
export const MAX_RETRY_MS = 300_000;
export const DEFAULT_WAIT_MS = 60_000;
export const DEFAULT_RETRY_MS = 5_000;

export interface PruneStaleResult {
  pruned_locks: number;
  dry_run?: true;
  would_prune?: number;
  lock_ids?: string[];
}

export interface NotifyGetResult {
  ok: true;
  count: 0;
  notifications: [];
}

export interface SessionCaptureResult {
  ok: true;
  captured: boolean;
  refinement_id: string | null;
  pending_runs: number;
  active_runs: number;
  files: string[];
  dirty_files: string[];
  file_count?: number;
  dirty_file_count?: number;
  omitted_files?: number;
  omitted_dirty_files?: number;
  reason: string | null;
  consolidation_opportunities: number; // memories with novelty_score < 0.2 (candidates for supersede)
  deduplicated?: true;
}

export interface WaitForLockResult {
  ok: true;
  waited_ms: number;
  lock_free: boolean;
  conflicts?: Array<{ file_path: string; agent_id: string; expires_at: string | null }>;
}

export function compactText(value: string, max = SESSION_CAPTURE_TEXT_LIMIT): string {
  const normalized = value.replace(/\s+/g, ' ').trim();
  return normalized.length > max ? `${normalized.slice(0, max - 3)}...` : normalized;
}

export function listSummary(label: string, items: string[], visibleLimit = SESSION_CAPTURE_VISIBLE_FILE_LIMIT): string | null {
  if (items.length === 0) return null;
  const shown = items.slice(0, visibleLimit);
  const omitted = items.length - shown.length;
  return `${label}${omitted > 0 ? ` (showing ${shown.length} of ${items.length})` : ''}: ${shown.join(', ')}${omitted > 0 ? `; ${omitted} omitted` : ''}.`;
}

export function boundedMs(value: unknown, defaultMs: number, minMs: number, maxMs: number): number {
  if (value == null) return defaultMs;
  const numeric = Number(value);
  if (!Number.isFinite(numeric)) return minMs;
  return Math.min(Math.max(numeric, minMs), maxMs);
}

/** Delete expired exclusive locks without changing the independent work lifecycle. */
export function pruneStale(db: DatabaseSync, params: Record<string, unknown> = {}): PruneStaleResult {
  const dryRun = Boolean(params.dry_run ?? params.dryRun);
  const expiredOnly = Boolean(params.expired_only ?? params.expiredOnly);
  const olderThanMinutes = params.older_than_minutes != null ? Number(params.older_than_minutes) :
    params.olderThanMinutes != null ? Number(params.olderThanMinutes) : null;
  const agentId = typeof params.agent_id === 'string' ? params.agent_id :
    typeof params.agentId === 'string' ? params.agentId : null;
  const rawWorkspacePath = typeof params.workspace === 'string' ? params.workspace :
    typeof params.workspace_path === 'string' ? params.workspace_path :
      typeof params.workspacePath === 'string' ? params.workspacePath : null;
  const workspacePath = rawWorkspacePath ? normalizeWorkspacePath(rawWorkspacePath, rawWorkspacePath) : null;
  const artifact = normalizeArtifact(params.artifact);
  const rawTarget = params.target_file ?? params.targetFile;
  const targetFiles = (Array.isArray(rawTarget) ? rawTarget : rawTarget != null ? [rawTarget] : [])
    .map(String)
    .filter(Boolean)
    .map((file) => {
      const base = rawWorkspacePath ? resolve(rawWorkspacePath) : process.cwd();
      return canonicalizePath(isAbsolute(file) ? resolve(file) : resolve(base, file));
    });
  const now = utcNow();
  // Age cutoff: locks older than N minutes are considered stale even if not expired.
  const ageCutoff = olderThanMinutes != null && !expiredOnly
    ? new Date(Date.now() - olderThanMinutes * 60000).toISOString()
    : null;

  // Selection must be identical for dry-run and real prune, so previews are honest.
  const conditions: string[] = [];
  const binds: string[] = [];
  const staleClauses = ['(l.expires_at IS NOT NULL AND l.expires_at < ?)'];
  binds.push(now);
  if (ageCutoff) {
    staleClauses.push('(l.acquired_at < ?)');
    binds.push(ageCutoff);
  }
  conditions.push(`(${staleClauses.join(' OR ')})`);
  if (agentId) { conditions.push('t.agent_id = ?'); binds.push(agentId); }
  if (targetFiles.length > 0) {
    conditions.push(`l.file_path IN (${targetFiles.map(() => '?').join(',')})`);
    binds.push(...targetFiles);
  }
  if (workspacePath) { conditions.push('t.workspace_path = ?'); binds.push(workspacePath); }
  if (artifact) { conditions.push('(t.artifact = ? OR t.artifact IS NULL)'); binds.push(artifact); }
  const where = conditions.join(' AND ');
  const from = 'locks l JOIN task_runs t ON t.run_id = l.run_id';

  let staleLocks: Array<{ lock_id: string; run_id: string }> = [];
  try {
    staleLocks = db.prepare(
      `SELECT l.lock_id, l.run_id FROM ${from} WHERE ${where}`
    ).all(...binds) as Array<{ lock_id: string; run_id: string }>;
  } catch { /* non-critical stale-lock scan */ }

  if (dryRun) {
    return {
      pruned_locks: 0,
      dry_run: true,
      would_prune: staleLocks.length,
      lock_ids: staleLocks.map(lock => lock.lock_id).slice(0, 20),
    };
  }
  if (staleLocks.length === 0) {
    return { pruned_locks: 0 };
  }

  const ownsTransaction = !db.isTransaction;
  if (ownsTransaction) db.exec('BEGIN IMMEDIATE');
  try {
    staleLocks = db.prepare(
      `SELECT l.lock_id, l.run_id FROM ${from} WHERE ${where}`
    ).all(...binds) as Array<{ lock_id: string; run_id: string }>;
    if (staleLocks.length === 0) {
      if (ownsTransaction) db.exec('COMMIT');
      return { pruned_locks: 0 };
    }
    const ph = staleLocks.map(() => '?').join(',');
    db.prepare(`DELETE FROM locks WHERE lock_id IN (${ph})`).run(...staleLocks.map(l => l.lock_id));
    if (ownsTransaction) db.exec('COMMIT');
  } catch (e) {
    if (ownsTransaction) {
      try { db.exec('ROLLBACK'); } catch { /* already rolled back */ }
    }
    throw e;
  }

  return { pruned_locks: staleLocks.length };
}

// ─── Smart briefing ─────────────────────────────────────────────────────────

export interface BriefItem {
  kind: 'memory' | 'weakness' | 'refinement' | 'notification';
  text: string;
  importance?: number;
}

export interface NotifyGetBriefResult {
  ok: true;
  count: number;
  notifications: BriefItem[];
  additionalContext?: string;  // set when format:hook is requested
}

export function openRefinementCount(
  db: DatabaseSync,
  params: { workspacePath?: string | null; artifact?: string | null; repo?: string | null; ref?: string | null; cwd?: string; includeHandoffs?: boolean } = {},
): number {
  const scope = fillScope(
    { workspace_path: params.workspacePath ?? null, artifact: normalizeArtifact(params.artifact), repo: params.repo ?? null, ref: params.ref ?? null },
    params.cwd ?? process.cwd(),
  );
  const queryParams: (string | number)[] = [];
  let sql = "SELECT COUNT(*) AS c FROM refinements WHERE state IN ('open','ongoing')";
  if (!params.includeHandoffs) sql += " AND quality NOT IN ('handoff','instructions')";
  if (scope.workspace_path) {
    sql += ' AND (workspace_path = ? OR workspace_path IS NULL)';
    queryParams.push(scope.workspace_path);
  }
  if (scope.artifact) {
    sql += ' AND (artifact = ? OR artifact IS NULL)';
    queryParams.push(scope.artifact);
  }
  if (scope.repo) {
    sql += ' AND (repo = ? OR repo IS NULL)';
    queryParams.push(scope.repo);
  }
  if (scope.ref) {
    sql += ' AND (ref = ? OR ref IS NULL)';
    queryParams.push(scope.ref);
  }
  return (db.prepare(sql).get(...queryParams) as { c: number }).c;
}
