import type { DatabaseSync } from './sqlite.js';
import type { StoreRetirementBlocker, StoreRetirementBlockerCode } from './store-retirement-types.js';

/** Read every non-terminal lifecycle relation that can still own live work. */
export function storeRetirementBlockers(db: DatabaseSync, at: string): StoreRetirementBlocker[] {
  const checks: Array<{ code: StoreRetirementBlockerCode; sql: string; binds?: string[] }> = [
    { code: 'open_sessions', sql: 'SELECT COUNT(*) AS count FROM sessions WHERE ended_at IS NULL' },
    { code: 'active_agents', sql: "SELECT COUNT(*) AS count FROM awareness_agents WHERE status='ACTIVE'" },
    { code: 'unfinished_plans', sql: "SELECT COUNT(*) AS count FROM awareness_plans WHERE status NOT IN ('COMPLETED','CANCELLED')" },
    { code: 'unfinished_tasks', sql: "SELECT COUNT(*) AS count FROM awareness_tasks WHERE status NOT IN ('DONE','FAILED','CANCELLED')" },
    { code: 'active_runs', sql: "SELECT COUNT(*) AS count FROM task_runs WHERE status IN ('PENDING','ACTIVE')" },
    { code: 'live_task_claims', sql: 'SELECT COUNT(*) AS count FROM task_claims WHERE expires_at>?', binds: [at] },
    { code: 'live_run_files', sql: 'SELECT COUNT(*) AS count FROM run_files WHERE ended_at IS NULL AND expires_at>?', binds: [at] },
    { code: 'live_locks', sql: 'SELECT COUNT(*) AS count FROM awareness_locks WHERE expires_at IS NULL OR expires_at>?', binds: [at] },
    { code: 'pending_interactions', sql: "SELECT COUNT(*) AS count FROM pending_interactions WHERE status='pending' AND (expires_at IS NULL OR expires_at>?)", binds: [at] },
    { code: 'live_authorizations', sql: 'SELECT COUNT(*) AS count FROM authorization_receipts WHERE consumed_at IS NULL AND (expires_at IS NULL OR expires_at>?)', binds: [at] },
    { code: 'open_signals', sql: "SELECT COUNT(*) AS count FROM signals WHERE status='open'" },
    { code: 'open_history_captures', sql: "SELECT COUNT(*) AS count FROM local_history_operations WHERE status IN ('capturing','open')" },
    { code: 'active_history_restores', sql: "SELECT COUNT(*) AS count FROM local_history_restores WHERE status='applying' OR (status='ready' AND expires_at>?)", binds: [at] },
    { code: 'pending_experience_archives', sql: `SELECT COUNT(*) AS count FROM event_outbox sealed
      WHERE sealed.aggregate_kind='experience' AND sealed.event_type='experience.sealed'
        AND NOT EXISTS (
          SELECT 1 FROM event_outbox archived
          WHERE archived.workspace_path=sealed.workspace_path
            AND archived.aggregate_kind='experience'
            AND archived.aggregate_id=sealed.aggregate_id
            AND archived.event_type='experience.archive'
        )` },
  ];
  return checks.flatMap(({ code, sql, binds = [] }) => {
    const row = db.prepare(sql).get(...binds) as { count: number };
    const count = Number(row.count);
    return count > 0 ? [{ code, count }] : [];
  });
}
