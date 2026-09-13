import { existsSync } from 'node:fs';
import { isAbsolute, resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { normalizeWorkspacePath } from './git.js';
import { assertKnownOptions, normalizeArtifact } from './helpers.js';
import { auditUnverified } from './verify-audit.js';

export interface MaintenancePressure {
  pressure_age_days: number;
  cutoff: string;
  stale_pending_runs: number;
  stale_active_runs: number;
  stale_open_signals: number;
  stale_handoff_signals: number;
  stale_missing_refs: number;
  samples: {
    run_ids: string[];
    active_run_ids: string[];
    signal_ids: string[];
    handoff_signal_ids: string[];
    memory_ids: string[];
  };
}

const PRESSURE_OPTION_KEYS = ['pressure_age_days', 'workspace_path', 'workspace_normalized', 'artifact'] as const;

/** Read-only pressure sensor. It never changes lifecycle state. */
export function inspectMaintenancePressure(
  db: DatabaseSync,
  params: Record<string, unknown> = {},
): MaintenancePressure {
  assertKnownOptions(params, PRESSURE_OPTION_KEYS, 'maintenance pressure');
  const requestedDays = Number(params.pressure_age_days ?? 1);
  const pressureAgeDays = Number.isFinite(requestedDays) ? Math.min(3650, Math.max(1, Math.floor(requestedDays))) : 1;
  const cutoff = new Date(Date.now() - pressureAgeDays * 86_400_000).toISOString();
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

  const pendingCount = Number((db.prepare(`SELECT COUNT(*) AS count FROM task_runs
    WHERE status = 'PENDING' AND updated_at < ?${scopeSql}`).get(cutoff, ...scopeBinds) as { count: number }).count);
  const pendingRows = db.prepare(`SELECT run_id FROM task_runs
    WHERE status = 'PENDING' AND updated_at < ?${scopeSql}
    ORDER BY datetime(updated_at), run_id LIMIT 3`).all(cutoff, ...scopeBinds) as Array<{ run_id: string }>;
  const staleActive = auditUnverified(db, {
    workspacePath,
    artifact,
    olderThanDays: pressureAgeDays,
  }).stale_active;
  const signalCount = Number((db.prepare(`SELECT COUNT(*) AS count FROM signals
    WHERE status = 'open' AND created_at < ?${scopeSql}`).get(cutoff, ...scopeBinds) as { count: number }).count);
  const signalRows = db.prepare(`SELECT signal_id FROM signals
    WHERE status = 'open' AND created_at < ?${scopeSql}
    ORDER BY datetime(created_at), signal_id LIMIT 3`).all(cutoff, ...scopeBinds) as Array<{ signal_id: string }>;
  const handoffSignalCount = Number((db.prepare(`SELECT COUNT(*) AS count FROM signals
    WHERE kind = 'handoff' AND status = 'open' AND created_at < ?${scopeSql}`)
    .get(cutoff, ...scopeBinds) as { count: number }).count);
  const handoffSignalRows = db.prepare(`SELECT signal_id FROM signals
    WHERE kind = 'handoff' AND status = 'open' AND created_at < ?${scopeSql}
    ORDER BY datetime(created_at), signal_id LIMIT 3`).all(cutoff, ...scopeBinds) as Array<{ signal_id: string }>;
  const referenceRows = db.prepare(`SELECT m.memory_id, r.reference
    FROM awareness_memories m JOIN memory_refs r ON r.memory_id = m.memory_id
    WHERE m.state = 'ACTIVE' AND r.reference LIKE 'file:%'
      AND COALESCE(m.updated_at, m.created_at) < ?
      ${scopeSql.replaceAll('workspace_path', 'm.workspace_path').replaceAll('artifact', 'm.artifact')}
    ORDER BY datetime(COALESCE(m.updated_at, m.created_at)), m.memory_id LIMIT 1000`)
    .all(cutoff, ...scopeBinds) as Array<{ memory_id: string; reference: string }>;
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
