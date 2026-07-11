import { existsSync } from 'node:fs';
import { isAbsolute, resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { checkpointWal, hasFts, rebuildFts } from './db.js';
import { normalizeWorkspacePath } from './git.js';
import { normalizeArtifact } from './helpers.js';
import { pruneStale } from './maintenance-stale.js';

// ─── Explicit maintenance digest ─────────────────────────────────────────

export interface DigestResult {
  ok: true;
  archived_memories: number;   // valid_to expired (or would_archive in dry_run)
  pruned_old: number;          // SUPERSEDED older than retention_days
  pruned_locks: number;        // expired file locks
  pruned_refinements: number;  // old handoffs and done refinements
  pruned_runs: number;         // old terminal standalone WORK/HOOK rows
  fts_rebuilt: boolean;
  dry_run?: true;
  would_archive?: number;
  would_prune_old?: number;
  would_prune_locks?: number;
  would_prune_refinements?: number;
  would_prune_runs?: number;
  pressure_age_days?: number;
  stale_pending_runs?: number;
  stale_open_signals?: number;
  stale_missing_refs?: number;
  pressure_samples?: MaintenancePressure['samples'];
  candidate_limit?: number;
  candidate_ids?: {
    expire_memory_ids: string[];
    purge_memory_ids: string[];
    lock_ids: string[];
    refinement_ids: string[];
    run_ids: string[];
  };
}

export interface MaintenancePressure {
  pressure_age_days: number;
  cutoff: string;
  stale_pending_runs: number;
  stale_open_signals: number;
  stale_missing_refs: number;
  samples: {
    run_ids: string[];
    signal_ids: string[];
    memory_ids: string[];
  };
}

export const MIN_RETENTION_DAYS = 1;
export const MAX_RETENTION_DAYS = 3650;

export function retentionWindow(
  params: Record<string, unknown>,
  snakeName: string,
  camelName: string,
  fallback: number,
): number {
  const raw = params[snakeName] ?? params[camelName] ?? fallback;
  const value = Number(raw);
  if (!Number.isInteger(value) || value < MIN_RETENTION_DAYS || value > MAX_RETENTION_DAYS) {
    throw new Error(`${snakeName} must be an integer in ${MIN_RETENTION_DAYS}..${MAX_RETENTION_DAYS}`);
  }
  return value;
}

/**
 * Read-only pressure sensor. It never expires, verifies, resolves, or deletes work;
 * callers receive bounded ids and must use the owning lifecycle command.
 */
export function inspectMaintenancePressure(
  db: DatabaseSync,
  params: Record<string, unknown> = {},
): MaintenancePressure {
  const requestedDays = Number(params.pressure_age_days ?? params.pressureAgeDays ?? 1);
  const pressureAgeDays = Number.isFinite(requestedDays) ? Math.min(3650, Math.max(1, Math.floor(requestedDays))) : 1;
  const cutoff = new Date(Date.now() - pressureAgeDays * 86400000).toISOString();
  const rawWorkspacePath = typeof params.workspace === 'string' ? params.workspace :
    typeof params.workspace_path === 'string' ? params.workspace_path :
      typeof params.workspacePath === 'string' ? params.workspacePath : null;
  const workspacePath = rawWorkspacePath
    ? (params.workspace_normalized === true ? resolve(rawWorkspacePath) : normalizeWorkspacePath(rawWorkspacePath, rawWorkspacePath))
    : null;
  const artifact = normalizeArtifact(params.artifact);
  const scope: string[] = [];
  const scopeBinds: string[] = [];
  if (workspacePath) { scope.push('workspace_path = ?'); scopeBinds.push(workspacePath); }
  if (artifact) { scope.push('artifact = ?'); scopeBinds.push(artifact); }
  const scopeSql = scope.length > 0 ? ` AND ${scope.join(' AND ')}` : '';

  const pendingCount = (db.prepare(
    `SELECT COUNT(*) AS count FROM task_runs
      WHERE status = 'PENDING' AND updated_at < ?${scopeSql}`
  ).get(cutoff, ...scopeBinds) as { count: number }).count;
  const pendingRows = db.prepare(
    `SELECT run_id FROM task_runs
      WHERE status = 'PENDING' AND updated_at < ?${scopeSql}
      ORDER BY datetime(updated_at), run_id LIMIT 3`
  ).all(cutoff, ...scopeBinds) as unknown as Array<{ run_id: string }>;
  const signalCount = (db.prepare(
    `SELECT COUNT(*) AS count FROM signals
      WHERE status = 'open' AND created_at < ?${scopeSql}`
  ).get(cutoff, ...scopeBinds) as { count: number }).count;
  const signalRows = db.prepare(
    `SELECT signal_id FROM signals
      WHERE status = 'open' AND created_at < ?${scopeSql}
      ORDER BY datetime(created_at), signal_id LIMIT 3`
  ).all(cutoff, ...scopeBinds) as unknown as Array<{ signal_id: string }>;
  const referenceRows = db.prepare(
    `SELECT m.memory_id, r.reference
       FROM memories m
       JOIN memory_refs r ON r.memory_id = m.memory_id
      WHERE m.state = 'ACTIVE'
        AND r.reference LIKE 'file:%'
        AND COALESCE(m.updated_at, m.created_at) < ?
        ${scopeSql.replaceAll('workspace_path', 'm.workspace_path').replaceAll('artifact', 'm.artifact')}
      ORDER BY datetime(COALESCE(m.updated_at, m.created_at)), m.memory_id
      LIMIT 1000`
  ).all(cutoff, ...scopeBinds) as unknown as Array<{ memory_id: string; reference: string }>;
  const staleMemoryIds = new Set<string>();
  for (const row of referenceRows) {
    const raw = row.reference.slice('file:'.length).replace(/(?::\d+(?::\d+)?|#L\d+(?:-L?\d+)?)$/, '');
    const path = isAbsolute(raw) ? raw : resolve(workspacePath ?? process.cwd(), raw);
    if (!existsSync(path)) staleMemoryIds.add(row.memory_id);
  }

  return {
    pressure_age_days: pressureAgeDays,
    cutoff,
    stale_pending_runs: pendingCount,
    stale_open_signals: signalCount,
    stale_missing_refs: staleMemoryIds.size,
    samples: {
      run_ids: pendingRows.map(row => row.run_id),
      signal_ids: signalRows.map(row => row.signal_id),
      memory_ids: [...staleMemoryIds].slice(0, 3),
    },
  };
}

/**
 * Explicit maintenance operation. Callers preview with dry_run before deciding
 * whether to apply it; prompt hooks are preview-only.
 * 1. Archive memories whose valid_to has passed
 * 2. Hard-delete SUPERSEDED memories older than retention_days
 * 3. Prune expired file locks
 * 4. Prune old session handoffs and completed refinements
 * 5. Rebuild / optimize the FTS5 index
 */
export function digest(
  db: DatabaseSync,
  params: Record<string, unknown> = {},
): DigestResult {
  const retentionDays = retentionWindow(params, 'retention_days', 'retentionDays', 90);
  const handoffRetentionDays = retentionWindow(params, 'refinement_handoff_retention_days', 'refinementHandoffRetentionDays', 7);
  const doneRetentionDays = retentionWindow(params, 'refinement_done_retention_days', 'refinementDoneRetentionDays', 30);
  const operationalRetentionDays = retentionWindow(params, 'operational_retention_days', 'operationalRetentionDays', 90);
  retentionWindow(params, 'pressure_age_days', 'pressureAgeDays', 1);
  const rawWorkspacePath = typeof params.workspace === 'string' ? params.workspace :
    typeof params.workspace_path === 'string' ? params.workspace_path :
      typeof params.workspacePath === 'string' ? params.workspacePath : null;
  const workspacePath = rawWorkspacePath ? normalizeWorkspacePath(rawWorkspacePath, rawWorkspacePath) : null;
  const artifact = normalizeArtifact(params.artifact);
  const now = new Date().toISOString();
  const cutoff = new Date(Date.now() - retentionDays * 86400000).toISOString();
  const handoffCutoff = new Date(Date.now() - handoffRetentionDays * 86400000).toISOString();
  const doneCutoff = new Date(Date.now() - doneRetentionDays * 86400000).toISOString();
  const operationalCutoff = new Date(Date.now() - operationalRetentionDays * 86400000).toISOString();
  const pressure = inspectMaintenancePressure(db, params);
  const pressureFields = {
    pressure_age_days: pressure.pressure_age_days,
    stale_pending_runs: pressure.stale_pending_runs,
    stale_open_signals: pressure.stale_open_signals,
    stale_missing_refs: pressure.stale_missing_refs,
    pressure_samples: pressure.samples,
  };
  const memoryScope: string[] = [];
  const memoryScopeBinds: string[] = [];
  if (workspacePath) { memoryScope.push('workspace_path = ?'); memoryScopeBinds.push(workspacePath); }
  if (artifact) { memoryScope.push('artifact = ?'); memoryScopeBinds.push(artifact); }
  const memoryScopeSql = memoryScope.length > 0 ? ` AND ${memoryScope.join(' AND ')}` : '';
  const refinementScope: string[] = [];
  const refinementScopeBinds: string[] = [];
  if (workspacePath) { refinementScope.push('workspace_path = ?'); refinementScopeBinds.push(workspacePath); }
  if (artifact) { refinementScope.push('artifact = ?'); refinementScopeBinds.push(artifact); }
  const refinementScopeSql = refinementScope.length > 0 ? ` AND ${refinementScope.join(' AND ')}` : '';

  // dry_run: count what would change without mutating anything
  if (params.dry_run) {
    const candidateLimit = 20;
    const wouldArchive = (db.prepare(
      `SELECT COUNT(*) AS c FROM memories WHERE valid_to IS NOT NULL AND valid_to < ? AND state = 'ACTIVE'${memoryScopeSql}`
    ).get(now, ...memoryScopeBinds) as { c: number }).c;
    const wouldPruneOld = (db.prepare(
      `SELECT COUNT(*) AS c FROM memories WHERE state = 'SUPERSEDED' AND updated_at < ?${memoryScopeSql}`
    ).get(cutoff, ...memoryScopeBinds) as { c: number }).c;
    const lockDryRun = pruneStale(db, {
      ...(workspacePath ? { workspace: workspacePath } : {}),
      ...(artifact ? { artifact } : {}),
      expired_only: true,
      dry_run: true,
    });
    const wouldPruneLocks = lockDryRun.would_prune ?? 0;
    const wouldPruneRefinements = (db.prepare(`SELECT COUNT(*) AS c FROM refinements
       WHERE ((quality = 'handoff' AND state = 'done' AND updated_at < ?)
          OR (quality IN ('good','bad') AND state = 'done' AND updated_at < ?))${refinementScopeSql}`)
      .get(handoffCutoff, doneCutoff, ...refinementScopeBinds) as { c: number }).c;
    const wouldPruneRuns = (db.prepare(`SELECT COUNT(*) AS c FROM task_runs
      WHERE task_id IS NULL AND origin IN ('WORK','HOOK')
        AND status IN ('SUCCESS','FAILED') AND updated_at < ?${memoryScopeSql}`)
      .get(operationalCutoff, ...memoryScopeBinds) as { c: number }).c;
    const expireMemoryIds = (db.prepare(
      `SELECT memory_id FROM memories
       WHERE valid_to IS NOT NULL AND valid_to < ? AND state = 'ACTIVE'${memoryScopeSql}
       ORDER BY datetime(valid_to), memory_id LIMIT ?`
    ).all(now, ...memoryScopeBinds, candidateLimit) as Array<{ memory_id: string }>).map(row => row.memory_id);
    const purgeMemoryIds = (db.prepare(
      `SELECT memory_id FROM memories
       WHERE state = 'SUPERSEDED' AND updated_at < ?${memoryScopeSql}
       ORDER BY datetime(updated_at), memory_id LIMIT ?`
    ).all(cutoff, ...memoryScopeBinds, candidateLimit) as Array<{ memory_id: string }>).map(row => row.memory_id);
    const refinementIds = (db.prepare(
      `SELECT refinement_id FROM refinements
       WHERE ((quality = 'handoff' AND state = 'done' AND updated_at < ?)
          OR (quality IN ('good','bad') AND state = 'done' AND updated_at < ?))${refinementScopeSql}
       ORDER BY datetime(updated_at), refinement_id LIMIT ?`
    ).all(handoffCutoff, doneCutoff, ...refinementScopeBinds, candidateLimit) as Array<{ refinement_id: string }>).map(row => row.refinement_id);
    const runIds = (db.prepare(
      `SELECT run_id FROM task_runs
       WHERE task_id IS NULL AND origin IN ('WORK','HOOK')
         AND status IN ('SUCCESS','FAILED') AND updated_at < ?${memoryScopeSql}
       ORDER BY datetime(updated_at), run_id LIMIT ?`
    ).all(operationalCutoff, ...memoryScopeBinds, candidateLimit) as Array<{ run_id: string }>).map(row => row.run_id);
    return {
      ok: true,
      archived_memories: 0,
      pruned_old: 0,
      pruned_locks: 0,
      pruned_refinements: 0,
      pruned_runs: 0,
      fts_rebuilt: false,
      dry_run: true,
      would_archive: wouldArchive,
      would_prune_old: wouldPruneOld,
      would_prune_locks: wouldPruneLocks,
      would_prune_refinements: wouldPruneRefinements,
      would_prune_runs: wouldPruneRuns,
      candidate_limit: candidateLimit,
      candidate_ids: {
        expire_memory_ids: expireMemoryIds,
        purge_memory_ids: purgeMemoryIds,
        lock_ids: lockDryRun.lock_ids ?? [],
        refinement_ids: refinementIds,
        run_ids: runIds,
      },
      ...pressureFields,
    };
  }

  let archiveRes: { changes: number } = { changes: 0 };
  let deleteRes: { changes: number } = { changes: 0 };
  let prunedLocks = 0;
  let pruneRefinementsRes: { changes: number } = { changes: 0 };
  let pruneRunsRes: { changes: number } = { changes: 0 };
  let ftsRebuilt = false;
  const ownsDigestTransaction = !db.isTransaction;
  if (ownsDigestTransaction) db.exec('BEGIN IMMEDIATE');
  try {
    // 1. Archive expired memories (valid_to < now)
    archiveRes = db.prepare(
      `UPDATE memories
       SET state = 'SUPERSEDED', expired_at = ?, updated_at = ?
       WHERE valid_to IS NOT NULL AND valid_to < ? AND state = 'ACTIVE'${memoryScopeSql}`
    ).run(now, now, now, ...memoryScopeBinds) as { changes: number };

    // 2. Hard-delete old SUPERSEDED entries to keep the DB lean
    deleteRes = db.prepare(
      `DELETE FROM memories
       WHERE state = 'SUPERSEDED' AND updated_at < ?${memoryScopeSql}`
    ).run(cutoff, ...memoryScopeBinds) as { changes: number };

    // 3. Prune expired locks inside the same caller-owned transaction.
    prunedLocks = pruneStale(db, {
      ...(workspacePath ? { workspace: workspacePath } : {}),
      ...(artifact ? { artifact } : {}),
      expired_only: true,
    }).pruned_locks;

    // 4. Prune only terminal session handoffs and completed repo-fix refinements.
    // Open/ongoing handoffs remain until their owner consumes and closes them.
    pruneRefinementsRes = db.prepare(
      `DELETE FROM refinements
       WHERE ((quality = 'handoff' AND state = 'done' AND updated_at < ?)
          OR (quality IN ('good','bad') AND state = 'done' AND updated_at < ?))${refinementScopeSql}`
    ).run(handoffCutoff, doneCutoff, ...refinementScopeBinds) as { changes: number };

    // 5. Compact terminal standalone execution rows. Run-file presence cascades;
    // verification receipts remain in run_log with run_id set null by the FK.
    pruneRunsRes = db.prepare(`DELETE FROM task_runs
      WHERE task_id IS NULL AND origin IN ('WORK','HOOK')
        AND status IN ('SUCCESS','FAILED') AND updated_at < ?${memoryScopeSql}`)
      .run(operationalCutoff, ...memoryScopeBinds) as { changes: number };

    // 6. Rebuild FTS5 from the same committed memory snapshot. A failure is
    // fatal so deleted source rows and the index can never diverge.
    if (hasFts(db)) {
      rebuildFts(db);
      ftsRebuilt = true;
    }
    if (ownsDigestTransaction) db.exec('COMMIT');
  } catch (error) {
    if (ownsDigestTransaction) {
      try { db.exec('ROLLBACK'); } catch { /* already rolled back */ }
    }
    throw error;
  }

  // 7. Absorb WAL pages after bulk maintenance writes (non-fatal on :memory:).
  if (ownsDigestTransaction) checkpointWal(db);

  return {
    ok: true,
    archived_memories: archiveRes.changes,
    pruned_old: deleteRes.changes,
    pruned_locks: prunedLocks,
    pruned_refinements: pruneRefinementsRes.changes,
    pruned_runs: pruneRunsRes.changes,
    fts_rebuilt: ftsRebuilt,
    ...pressureFields,
  };
}
