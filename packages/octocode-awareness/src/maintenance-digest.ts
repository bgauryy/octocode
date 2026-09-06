import { randomUUID } from 'node:crypto';
import { existsSync } from 'node:fs';
import { isAbsolute, resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { hasFts, rebuildFts } from './db-maintenance.js';
import { checkpointWal } from '@octocodeai/agent-contracts/sqlite';
import { normalizeWorkspacePath } from './git.js';
import { assertKnownOptions, normalizeArtifact } from './helpers.js';
import { pruneStale } from './maintenance-stale.js';
import type { DigestResult, MaintenancePressure } from './maintenance-digest-types.js';
import { auditUnverified } from './verify-audit.js';
import { closeRunFiles, failStaleLinkedTask } from './verify-shared.js';
import { RUN_LOG_INSERT_VERIFIED, RUNS_UPDATE_ACTIVE_TO_FAILED } from './sql/runs.js';

// ─── Explicit maintenance digest ─────────────────────────────────────────

export const MIN_RETENTION_DAYS = 1;
export const MAX_RETENTION_DAYS = 3650;

const DIGEST_OPTION_KEYS = [
  'retention_days',
  'refinement_handoff_retention_days',
  'handoff_signal_retention_days',
  'refinement_done_retention_days',
  'operational_retention_days',
  'pressure_age_days',
  'fail_stale_active_runs',
  'workspace_path',
  'workspace_normalized',
  'artifact',
  'dry_run',
] as const;

export function retentionWindow(
  params: Record<string, unknown>,
  snakeName: string,
  fallback: number,
): number {
  const raw = params[snakeName] ?? fallback;
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
  assertKnownOptions(params, DIGEST_OPTION_KEYS, 'maintenance pressure');
  const requestedDays = Number(params.pressure_age_days ?? 1);
  const pressureAgeDays = Number.isFinite(requestedDays) ? Math.min(3650, Math.max(1, Math.floor(requestedDays))) : 1;
  const cutoff = new Date(Date.now() - pressureAgeDays * 86400000).toISOString();
  const rawWorkspacePath = typeof params.workspace_path === 'string' ? params.workspace_path : null;
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
  const staleActive = auditUnverified(db, {
    workspacePath,
    artifact,
    olderThanDays: pressureAgeDays,
  }).stale_active;
  const signalCount = (db.prepare(
    `SELECT COUNT(*) AS count FROM signals
      WHERE status = 'open' AND created_at < ?${scopeSql}`
  ).get(cutoff, ...scopeBinds) as { count: number }).count;
  const signalRows = db.prepare(
    `SELECT signal_id FROM signals
      WHERE status = 'open' AND created_at < ?${scopeSql}
      ORDER BY datetime(created_at), signal_id LIMIT 3`
  ).all(cutoff, ...scopeBinds) as unknown as Array<{ signal_id: string }>;
  const handoffSignalCount = (db.prepare(
    `SELECT COUNT(*) AS count FROM signals
      WHERE kind = 'handoff' AND status = 'open' AND created_at < ?${scopeSql}`
  ).get(cutoff, ...scopeBinds) as { count: number }).count;
  const handoffSignalRows = db.prepare(
    `SELECT signal_id FROM signals
      WHERE kind = 'handoff' AND status = 'open' AND created_at < ?${scopeSql}
      ORDER BY datetime(created_at), signal_id LIMIT 3`
  ).all(cutoff, ...scopeBinds) as unknown as Array<{ signal_id: string }>;
  const referenceRows = db.prepare(
    `SELECT m.memory_id, r.reference
       FROM awareness_memories m
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
    stale_active_runs: staleActive.length,
    stale_open_signals: signalCount,
    stale_handoff_signals: handoffSignalCount,
    stale_missing_refs: staleMemoryIds.size,
    samples: {
      run_ids: pendingRows.map(row => row.run_id),
      active_run_ids: staleActive.slice(0, 3).map(row => row.run_id),
      signal_ids: signalRows.map(row => row.signal_id),
      handoff_signal_ids: handoffSignalRows.map(row => row.signal_id),
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
  assertKnownOptions(params, DIGEST_OPTION_KEYS, 'maintenance digest');
  const retentionDays = retentionWindow(params, 'retention_days', 90);
  const handoffRetentionDays = retentionWindow(params, 'refinement_handoff_retention_days', 7);
  const handoffSignalRetentionDays = retentionWindow(params, 'handoff_signal_retention_days', 1);
  const doneRetentionDays = retentionWindow(params, 'refinement_done_retention_days', 30);
  const operationalRetentionDays = retentionWindow(params, 'operational_retention_days', 90);
  retentionWindow(params, 'pressure_age_days', 1);
  const rawFailStaleActiveRuns = params.fail_stale_active_runs;
  if (rawFailStaleActiveRuns != null && typeof rawFailStaleActiveRuns !== 'boolean') {
    throw new Error('fail_stale_active_runs must be boolean');
  }
  const failStaleActiveRuns = rawFailStaleActiveRuns !== false;
  const rawWorkspacePath = typeof params.workspace_path === 'string' ? params.workspace_path : null;
  const workspacePath = rawWorkspacePath ? normalizeWorkspacePath(rawWorkspacePath, rawWorkspacePath) : null;
  const artifact = normalizeArtifact(params.artifact);
  const now = new Date().toISOString();
  const cutoff = new Date(Date.now() - retentionDays * 86400000).toISOString();
  const handoffCutoff = new Date(Date.now() - handoffRetentionDays * 86400000).toISOString();
  const handoffSignalCutoff = new Date(Date.now() - handoffSignalRetentionDays * 86400000).toISOString();
  const doneCutoff = new Date(Date.now() - doneRetentionDays * 86400000).toISOString();
  const operationalCutoff = new Date(Date.now() - operationalRetentionDays * 86400000).toISOString();
  const pressure = inspectMaintenancePressure(db, params);
  const pressureFields = {
    pressure_age_days: pressure.pressure_age_days,
    stale_pending_runs: pressure.stale_pending_runs,
    stale_active_runs: pressure.stale_active_runs,
    stale_open_signals: pressure.stale_open_signals,
    stale_handoff_signals: pressure.stale_handoff_signals,
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
      `SELECT COUNT(*) AS c FROM awareness_memories WHERE valid_to IS NOT NULL AND valid_to < ? AND state = 'ACTIVE'${memoryScopeSql}`
    ).get(now, ...memoryScopeBinds) as { c: number }).c;
    const wouldPruneOld = (db.prepare(
      `SELECT COUNT(*) AS c FROM awareness_memories WHERE state = 'SUPERSEDED' AND updated_at < ?${memoryScopeSql}`
    ).get(cutoff, ...memoryScopeBinds) as { c: number }).c;
    const lockDryRun = pruneStale(db, {
      ...(workspacePath ? { workspace_path: workspacePath } : {}),
      ...(artifact ? { artifact } : {}),
      expired_only: true,
      dry_run: true,
    });
    const wouldPruneLocks = lockDryRun.would_prune ?? 0;
    // Stale handoff refinements are dead letters (handoffs now live in signals):
    // prune them past retention in ANY state — their addressed identity never returns.
    const wouldPruneRefinements = (db.prepare(`SELECT COUNT(*) AS c FROM refinements
       WHERE ((quality = 'handoff' AND updated_at < ?)
          OR (quality IN ('good','bad') AND state = 'done' AND updated_at < ?))${refinementScopeSql}`)
      .get(handoffCutoff, doneCutoff, ...refinementScopeBinds) as { c: number }).c;
    const wouldResolveHandoffSignals = (db.prepare(
      `SELECT COUNT(*) AS c FROM signals WHERE kind = 'handoff' AND status = 'open' AND created_at < ?${memoryScopeSql}`
    ).get(handoffSignalCutoff, ...memoryScopeBinds) as { c: number }).c;
    const staleActiveRunIds = auditUnverified(db, {
      workspacePath,
      artifact,
      olderThanDays: pressure.pressure_age_days,
    }).stale_active.map(row => row.run_id);
    const wouldFailStaleActiveRuns = failStaleActiveRuns ? staleActiveRunIds.length : 0;
    const wouldPruneRuns = (db.prepare(`SELECT COUNT(*) AS c FROM task_runs
      WHERE task_id IS NULL AND origin IN ('WORK','HOOK')
        AND status IN ('SUCCESS','FAILED') AND updated_at < ?${memoryScopeSql}`)
      .get(operationalCutoff, ...memoryScopeBinds) as { c: number }).c;
    const expireMemoryIds = (db.prepare(
      `SELECT memory_id FROM awareness_memories
       WHERE valid_to IS NOT NULL AND valid_to < ? AND state = 'ACTIVE'${memoryScopeSql}
       ORDER BY datetime(valid_to), memory_id LIMIT ?`
    ).all(now, ...memoryScopeBinds, candidateLimit) as Array<{ memory_id: string }>).map(row => row.memory_id);
    const purgeMemoryIds = (db.prepare(
      `SELECT memory_id FROM awareness_memories
       WHERE state = 'SUPERSEDED' AND updated_at < ?${memoryScopeSql}
       ORDER BY datetime(updated_at), memory_id LIMIT ?`
    ).all(cutoff, ...memoryScopeBinds, candidateLimit) as Array<{ memory_id: string }>).map(row => row.memory_id);
    const refinementIds = (db.prepare(
      `SELECT refinement_id FROM refinements
       WHERE ((quality = 'handoff' AND updated_at < ?)
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
      resolved_handoff_signals: 0,
      failed_stale_active_runs: 0,
      pruned_runs: 0,
      fts_rebuilt: false,
      dry_run: true,
      would_archive: wouldArchive,
      would_prune_old: wouldPruneOld,
      would_prune_locks: wouldPruneLocks,
      would_prune_refinements: wouldPruneRefinements,
      would_resolve_handoff_signals: wouldResolveHandoffSignals,
      would_fail_stale_active_runs: wouldFailStaleActiveRuns,
      would_prune_runs: wouldPruneRuns,
      candidate_limit: candidateLimit,
      candidate_ids: {
        expire_memory_ids: expireMemoryIds,
        purge_memory_ids: purgeMemoryIds,
        locks: lockDryRun.locks ?? [],
        refinement_ids: refinementIds,
        run_ids: runIds,
        stale_active_run_ids: staleActiveRunIds.slice(0, candidateLimit),
      },
      ...pressureFields,
    };
  }

  let archiveRes: { changes: number } = { changes: 0 };
  let deleteRes: { changes: number } = { changes: 0 };
  let prunedLocks = 0;
  let pruneRefinementsRes: { changes: number } = { changes: 0 };
  let resolvedHandoffSignals = 0;
  let failedStaleActiveRuns = 0;
  let pruneRunsRes: { changes: number } = { changes: 0 };
  let ftsRebuilt = false;
  const ownsDigestTransaction = !db.isTransaction;
  if (ownsDigestTransaction) db.exec('BEGIN IMMEDIATE');
  try {
    // 1. Archive expired memories (valid_to < now)
    archiveRes = db.prepare(
      `UPDATE awareness_memories
       SET state = 'SUPERSEDED', expired_at = ?, updated_at = ?
       WHERE valid_to IS NOT NULL AND valid_to < ? AND state = 'ACTIVE'${memoryScopeSql}`
    ).run(now, now, now, ...memoryScopeBinds) as { changes: number };

    // 2. Hard-delete old SUPERSEDED entries to keep the DB lean
    deleteRes = db.prepare(
      `DELETE FROM awareness_memories
       WHERE state = 'SUPERSEDED' AND updated_at < ?${memoryScopeSql}`
    ).run(cutoff, ...memoryScopeBinds) as { changes: number };

    // 3. Prune expired locks inside the same caller-owned transaction.
    prunedLocks = pruneStale(db, {
      ...(workspacePath ? { workspace_path: workspacePath } : {}),
      ...(artifact ? { artifact } : {}),
      expired_only: true,
    }).pruned_locks;

    // 4. Prune stale handoff refinements past retention in ANY state (dead
    // letters — handoffs live in signals now) and completed repo-fix refinements.
    pruneRefinementsRes = db.prepare(
      `DELETE FROM refinements
       WHERE ((quality = 'handoff' AND updated_at < ?)
          OR (quality IN ('good','bad') AND state = 'done' AND updated_at < ?))${refinementScopeSql}`
    ).run(handoffCutoff, doneCutoff, ...refinementScopeBinds) as { changes: number };

    // 4b. TTL: auto-resolve open handoff signals past retention so the broadcast
    // inbox cannot accumulate unbounded stale handoffs.
    resolvedHandoffSignals = (db.prepare(
      `UPDATE signals SET status = 'resolved', resolved_at = ?
       WHERE kind = 'handoff' AND status = 'open' AND created_at < ?${memoryScopeSql}`
    ).run(now, handoffSignalCutoff, ...memoryScopeBinds) as { changes: number }).changes;

    // 4c. Stale ACTIVE runs with expired presence are not live work. Mark them
    // FAILED with an audit receipt so verify audit stops replaying old sessions.
    if (failStaleActiveRuns) {
      const staleActive = auditUnverified(db, {
        workspacePath,
        artifact,
        olderThanDays: pressure.pressure_age_days,
      }).stale_active;
      for (const run of staleActive) {
        const failed = db.prepare(RUNS_UPDATE_ACTIVE_TO_FAILED).run(now, run.run_id) as { changes: number };
        if (failed.changes !== 1) continue;
        closeRunFiles(db, run.run_id, now);
        const message = `maintenance digest: stale ACTIVE run had no live file presence after ${pressure.pressure_age_days}d`;
        failStaleLinkedTask(db, run.run_id, run.agent_id, now, message);
        try {
          db.prepare(RUN_LOG_INSERT_VERIFIED).run(
            'evt_' + randomUUID().replace(/-/g, ''), run.run_id, run.agent_id, message, now,
          );
        } catch { /* non-critical audit log */ }
        failedStaleActiveRuns += 1;
      }
    }

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

  const finalPressure = inspectMaintenancePressure(db, params);
  const finalPressureFields = {
    pressure_age_days: finalPressure.pressure_age_days,
    stale_pending_runs: finalPressure.stale_pending_runs,
    stale_active_runs: finalPressure.stale_active_runs,
    stale_open_signals: finalPressure.stale_open_signals,
    stale_handoff_signals: finalPressure.stale_handoff_signals,
    stale_missing_refs: finalPressure.stale_missing_refs,
    pressure_samples: finalPressure.samples,
  };

  return {
    ok: true,
    archived_memories: archiveRes.changes,
    pruned_old: deleteRes.changes,
    pruned_locks: prunedLocks,
    pruned_refinements: pruneRefinementsRes.changes,
    resolved_handoff_signals: resolvedHandoffSignals,
    failed_stale_active_runs: failedStaleActiveRuns,
    pruned_runs: pruneRunsRes.changes,
    fts_rebuilt: ftsRebuilt,
    ...finalPressureFields,
  };
}
