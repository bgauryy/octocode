/**
 * verify.ts — Verify-gate operations for the awareness Stop hook.
 *
 * auditUnverified: returns runs with status='PENDING' (edited but not verified)
 *                  for an agent/workspace. The Stop hook (stop-verify.sh) blocks
 *                  conclude when count > 0.
 *
 * markVerified:    transitions a run PENDING → SUCCESS | FAILED so the gate
 *                  clears after the agent verifies its edits. Restricted to PENDING
 *                  transitions to prevent orphaning ACTIVE locks as SUCCESS.
 *                  A linked plan task moves VERIFY → DONE | FAILED with it.
 */

import { randomUUID } from 'node:crypto';
import type { DatabaseSync } from 'node:sqlite';
import { normalizeArtifact, utcNow } from './helpers.js';
import { normalizeWorkspacePath } from './git.js';
import type { RunStatus } from './types.js';
import {
  RUNS_UPDATE_PENDING_TO_FAILED,
  RUNS_UPDATE_ACTIVE_TO_FAILED,
  RUN_LOG_INSERT_ABANDONED,
  RUN_LOG_INSERT_STALE_ABANDONED,
  RUN_LOG_INSERT_VERIFIED,
  RUNS_UPDATE_PENDING_VERIFIED_BY_AGENT,
  RUNS_SELECT_STATUS,
  RUNS_SELECT_PENDING_IDS,
} from './sql/runs.js';

// ─── Public shapes ────────────────────────────────────────────────────────────

export interface UnverifiedIntent {
  run_id: string;
  agent_id: string;
  status: string;
  test_plan: string;
  context_ref: string | null;
  rationale: string;
  target_files: string[];
  workspace_path: string | null;
  artifact: string | null;
  created_at: string;
}

/**
 * VER-2: An ACTIVE run whose declared file presence has expired.
 * These are orphaned work units the old audit silently missed.
 */
export interface StaleActiveIntent {
  run_id: string;
  agent_id: string;
  status: 'ACTIVE';
  rationale: string;
  context_ref: string | null;
  target_files: string[];
  workspace_path: string | null;
  artifact: string | null;
  created_at: string;
  age_hours: number; // how long stuck ACTIVE with no live file presence
}

export interface AuditUnverifiedResult {
  ok: true;
  unverified: UnverifiedIntent[];    // status=PENDING: released, awaiting verify
  stale_active: StaleActiveIntent[]; // VER-2: ACTIVE with no live file presence
  count: number;                     // total = unverified.length + stale_active.length
}

export interface AuditUnverifiedParams {
  agentId?: string | null;
  workspacePath?: string | null;
  artifact?: string | null;
  abandon?: boolean;         // dismiss all PENDING runs as FAILED (clear orphaned)
  olderThanDays?: number | null;
  origins?: Array<'TASK' | 'WORK' | 'HOOK'>;
  before?: string | null;
}

export type VerifyStatus = 'SUCCESS' | 'FAILED';

export interface MarkVerifiedParams {
  runId?: string;            // verify one run; required unless allPending=true
  agentId?: string;
  allPending?: boolean;       // verify ALL pending runs for this agent/workspace
  workspacePath?: string | null;
  artifact?: string | null;
  message?: string;           // what was verified
  status?: VerifyStatus;
}

export interface MarkVerifiedOk {
  ok: true;
  // VER-1: null when allPending=true (no single task applies in batch mode).
  // Callers must guard for null when using allPending.
  run_id: string | null;
  run_ids?: string[];   // set when allPending=true
  count?: number;        // set when allPending=true
  status: RunStatus;
  updated_at: string;
}

export interface MarkVerifiedErr {
  ok: false;
  error: string;
  run_id: string | null;
}

export type MarkVerifiedResult = MarkVerifiedOk | MarkVerifiedErr;

// ─── Internal ─────────────────────────────────────────────────────────────────

const VALID_VERIFY_STATUSES = new Set<string>(['SUCCESS', 'FAILED']);

interface IntentDbRow {
  run_id: string;
  agent_id: string;
  status: string;
  test_plan: string;
  context_ref: string | null;
  rationale: string;
  workspace_path: string | null;
  artifact: string | null;
  created_at: string;
}

interface AgentStatusRow {
  agent_id: string;
  status: string;
}

/**
 * Batched target-file lookup — chunked IN queries instead of one SELECT per
 * run. Chunking keeps huge audits under SQLITE_MAX_VARIABLE_NUMBER.
 */
function targetFilesForRuns(db: DatabaseSync, runIds: string[]): Map<string, string[]> {
  const byRun = new Map<string, string[]>(runIds.map((id) => [id, []]));
  for (let offset = 0; offset < runIds.length; offset += 500) {
    const chunk = runIds.slice(offset, offset + 500);
    const rows = db.prepare(
      `SELECT run_id, file_path FROM run_files
       WHERE run_id IN (${chunk.map(() => '?').join(',')})
       ORDER BY file_path`,
    ).all(...chunk) as unknown as Array<{ run_id: string; file_path: string }>;
    for (const row of rows) byRun.get(row.run_id)?.push(row.file_path);
  }
  return byRun;
}

function closeRunFiles(db: DatabaseSync, runId: string, now: string): void {
  db.prepare('DELETE FROM locks WHERE run_id = ?').run(runId);
  db.prepare(`UPDATE run_files SET heartbeat_at = ?, expires_at = ?, ended_at = ?
    WHERE run_id = ? AND ended_at IS NULL`).run(now, now, now, runId);
}

function finishLinkedTask(
  db: DatabaseSync,
  runId: string,
  status: VerifyStatus,
  agentId: string,
  now: string,
  message?: string,
): void {
  const linked = db.prepare('SELECT task_id FROM task_runs WHERE run_id = ?')
    .get(runId) as { task_id: string | null } | undefined;
  if (!linked?.task_id) return;
  const taskStatus = status === 'SUCCESS' ? 'DONE' : 'FAILED';
  const updated = db.prepare(`UPDATE tasks SET status = ?, updated_at = ?, completed_at = ?
    WHERE task_id = ? AND status = 'VERIFY'`)
    .run(taskStatus, now, now, linked.task_id);
  if (updated.changes === 0) return;
  db.prepare(`INSERT INTO task_events(event_id, task_id, run_id, agent_id, event_type, message, created_at)
    VALUES (?, ?, ?, ?, ?, ?, ?)`)
    .run(`tevt_${randomUUID().replace(/-/g, '')}`, linked.task_id, runId, agentId,
      status === 'SUCCESS' ? 'VERIFIED' : 'VERIFICATION_FAILED', message ?? taskStatus, now);
}

function abandonLinkedTask(
  db: DatabaseSync,
  runId: string,
  agentId: string,
  now: string,
  message: string,
): void {
  const linked = db.prepare('SELECT task_id FROM task_runs WHERE run_id = ?')
    .get(runId) as { task_id: string | null } | undefined;
  if (!linked?.task_id) return;
  const updated = db.prepare(`UPDATE tasks SET status = 'FAILED', updated_at = ?, completed_at = ?
    WHERE task_id = ? AND status IN ('IN_PROGRESS', 'VERIFY')`)
    .run(now, now, linked.task_id);
  if (updated.changes === 0) return;
  db.prepare('DELETE FROM task_claims WHERE task_id = ?').run(linked.task_id);
  db.prepare(`INSERT INTO task_events(event_id, task_id, run_id, agent_id, event_type, message, created_at)
    VALUES (?, ?, ?, ?, 'ABANDONED', ?, ?)`)
    .run(`tevt_${randomUUID().replace(/-/g, '')}`, linked.task_id, runId, agentId, message, now);
}

// ─── Exports ──────────────────────────────────────────────────────────────────

/**
 * Return all run rows with status='PENDING', optionally scoped to an
 * agent and/or workspace. A non-zero count means the Stop hook should block
 * conclude.
 */
export function auditUnverified(
  db: DatabaseSync,
  params: AuditUnverifiedParams = {},
): AuditUnverifiedResult {
  // Normalize (git-root + symlink canonicalized) so this matches the same
  // scope key that preFlightIntent/releaseFileLock wrote, regardless of
  // symlinks or whether the workspace became a git repo after the lock
  // was recorded — see canonicalizePath in git.ts.
  const workspacePath = params.workspacePath ? normalizeWorkspacePath(params.workspacePath, params.workspacePath) : null;
  const where: string[] = ["status = 'PENDING'"];
  const binds: (string | number)[] = [];
  let ageCutoff: string | null = null;
  if (params.olderThanDays != null) {
    if (!Number.isFinite(params.olderThanDays) || params.olderThanDays < 1) {
      throw new Error('olderThanDays must be a finite number >= 1');
    }
    ageCutoff = new Date(Date.now() - Math.floor(params.olderThanDays) * 86400000).toISOString();
    where.push('updated_at < ?');
    binds.push(ageCutoff);
  }
  if (params.origins?.length) {
    const origins = [...new Set(params.origins)];
    if (origins.some((origin) => !['TASK', 'WORK', 'HOOK'].includes(origin))) {
      throw new Error('origins must contain only TASK, WORK, or HOOK');
    }
    where.push(`origin IN (${origins.map(() => '?').join(',')})`);
    binds.push(...origins);
  }
  let before: string | null = null;
  if (params.before) {
    const parsed = new Date(params.before);
    if (Number.isNaN(parsed.getTime())) throw new Error('before must be a valid ISO timestamp');
    before = parsed.toISOString();
    where.push('created_at < ?');
    binds.push(before);
  }

  if (params.agentId) {
    where.push('agent_id = ?');
    binds.push(params.agentId);
  }
  if (workspacePath) {
    where.push('workspace_path = ?');
    binds.push(workspacePath);
  }
  const artifact = normalizeArtifact(params.artifact);
  if (artifact) {
    where.push('(artifact = ? OR artifact IS NULL)');
    binds.push(artifact);
  }

  const rows = db.prepare(
    `SELECT run_id, agent_id, status, test_plan, context_ref, rationale, workspace_path, artifact, created_at
     FROM task_runs
     WHERE ${where.join(' AND ')}
     ORDER BY created_at ASC`,
  ).all(...binds) as unknown as IntentDbRow[];

  const unverifiedFiles = targetFilesForRuns(db, rows.map((r) => r.run_id));
  const unverified: UnverifiedIntent[] = rows.map(r => ({
    run_id: r.run_id,
    agent_id: r.agent_id,
    status: r.status,
    test_plan: r.test_plan,
    context_ref: r.context_ref,
    rationale: r.rationale,
    target_files: unverifiedFiles.get(r.run_id) ?? [],
    workspace_path: r.workspace_path,
    artifact: r.artifact,
    created_at: r.created_at,
  }));

  if (params.abandon && unverified.length > 0) {
    const now = utcNow();
    const markFailed = db.prepare(RUNS_UPDATE_PENDING_TO_FAILED);
    const logAbandoned = db.prepare(RUN_LOG_INSERT_ABANDONED);
    for (const intent of unverified) {
      markFailed.run(now, intent.run_id);
      closeRunFiles(db, intent.run_id, now);
      abandonLinkedTask(db, intent.run_id, intent.agent_id, now, 'pending run abandoned by verification audit');
      try {
        logAbandoned.run(
          'evt_' + randomUUID().replace(/-/g, ''), intent.run_id, intent.agent_id, now,
        );
      } catch { /* non-critical audit log */ }
    }
  }

  // VER-2: Detect standalone ACTIVE runs whose file presence expired, plus task
  // runs whose claim lease and file presence both expired. Ordinary work may
  // validly have no lock, so lock absence is never verification debt.
  const staleActive: StaleActiveIntent[] = [];
  try {
    const nowIso = utcNow();
    const staleWhere: string[] = [
      "ai.status = 'ACTIVE'",
      'EXISTS (SELECT 1 FROM run_files any_rf WHERE any_rf.run_id = ai.run_id)',
      `NOT EXISTS (
        SELECT 1 FROM run_files active_rf
        WHERE active_rf.run_id = ai.run_id AND active_rf.ended_at IS NULL
          AND active_rf.expires_at > ?
      )`,
      `NOT EXISTS (
        SELECT 1 FROM task_claims tc
        WHERE tc.run_id = ai.run_id AND tc.expires_at > ?
      )`,
    ];
    const staleBinds: (string | number)[] = [nowIso, nowIso];
    if (params.agentId) { staleWhere.push('ai.agent_id = ?'); staleBinds.push(params.agentId); }
    if (workspacePath) { staleWhere.push('ai.workspace_path = ?'); staleBinds.push(workspacePath); }
    if (artifact) { staleWhere.push('(ai.artifact = ? OR ai.artifact IS NULL)'); staleBinds.push(artifact); }
    if (ageCutoff) { staleWhere.push('ai.updated_at < ?'); staleBinds.push(ageCutoff); }
    if (params.origins?.length) {
      const origins = [...new Set(params.origins)];
      staleWhere.push(`ai.origin IN (${origins.map(() => '?').join(',')})`);
      staleBinds.push(...origins);
    }
    if (before) { staleWhere.push('ai.created_at < ?'); staleBinds.push(before); }

    const staleRows = db.prepare(
      `SELECT ai.run_id, ai.agent_id, ai.rationale, ai.context_ref, ai.workspace_path, ai.artifact, ai.created_at
       FROM task_runs ai
       WHERE ${staleWhere.join(' AND ')}
       ORDER BY ai.created_at ASC`
    ).all(...staleBinds) as unknown as IntentDbRow[];

    const staleFiles = targetFilesForRuns(db, staleRows.map((r) => r.run_id));
    for (const r of staleRows) {
      const ageMs = Date.now() - new Date(r.created_at).getTime();
      staleActive.push({
        run_id: r.run_id,
        agent_id: r.agent_id,
        status: 'ACTIVE',
        rationale: r.rationale,
        context_ref: r.context_ref,
        target_files: staleFiles.get(r.run_id) ?? [],
        workspace_path: r.workspace_path,
        artifact: r.artifact,
        created_at: r.created_at,
        age_hours: Math.round(ageMs / 3600000 * 10) / 10,
      });
    }
  } catch (e) { if (!(e instanceof Error && e.message.includes('no such table'))) throw e; }

  if (params.abandon && staleActive.length > 0) {
    const now = utcNow();
    const markFailed = db.prepare(RUNS_UPDATE_ACTIVE_TO_FAILED);
    const logStaleAbandoned = db.prepare(RUN_LOG_INSERT_STALE_ABANDONED);
    for (const intent of staleActive) {
      markFailed.run(now, intent.run_id);
      closeRunFiles(db, intent.run_id, now);
      abandonLinkedTask(db, intent.run_id, intent.agent_id, now, 'stale task run abandoned by verification audit');
      try {
        logStaleAbandoned.run(
          'evt_' + randomUUID().replace(/-/g, ''), intent.run_id, intent.agent_id, now,
        );
      } catch { /* non-critical audit log */ }
    }
  }

  const total = unverified.length + staleActive.length;
  return { ok: true, unverified, stale_active: staleActive, count: total };
}

/**
 * Transition a PENDING task to SUCCESS or FAILED.
 *
 * Only operates on PENDING tasks — attempting to verify an ACTIVE, SUCCESS,
 * or FAILED task returns ok=false with a descriptive error so the agent knows
 * exactly what went wrong.
 */
export function markVerified(
  db: DatabaseSync,
  params: MarkVerifiedParams,
): MarkVerifiedResult {
  const { agentId = 'agent', allPending = false, message } = params;
  const workspacePath = params.workspacePath ? normalizeWorkspacePath(params.workspacePath, params.workspacePath) : null;
  const artifact = normalizeArtifact(params.artifact);
  const runId = params.runId ?? '';
  const status = params.status ?? 'SUCCESS';

  if (!VALID_VERIFY_STATUSES.has(status)) {
    return {
      ok: false,
      error: `invalid status "${status}" — must be SUCCESS or FAILED`,
      run_id: runId || null,
    };
  }

  const receipt = message?.trim() ?? '';
  if (status === 'SUCCESS' && !receipt) {
    return {
      ok: false,
      error: 'SUCCESS verification requires a non-empty evidence receipt in message',
      run_id: runId || null,
    };
  }
  if (allPending && !workspacePath && !artifact) {
    return {
      ok: false,
      error: '--all-pending requires --workspace or --artifact; use explicit run ids for cross-workspace verification',
      run_id: null,
    };
  }

  // --all-pending: verify every PENDING run for this agent/workspace at once
  if (allPending) {
    const dynWhere = [
      workspacePath ? ' AND workspace_path = ?' : '',
      artifact ? ' AND (artifact = ? OR artifact IS NULL)' : '',
    ].join('');
    const selectSql = RUNS_SELECT_PENDING_IDS.replace('{DYNAMIC_WHERE}', dynWhere);
    const selectBinds: (string | number)[] = [agentId];
    if (workspacePath) selectBinds.push(workspacePath);
    if (artifact) selectBinds.push(artifact);

    db.exec('BEGIN IMMEDIATE');
    try {
      const rows = db.prepare(selectSql).all(...selectBinds) as unknown as Array<{ run_id: string }>;
      const now = utcNow();
      const ids: string[] = [];
      for (const row of rows) {
        const upd = db.prepare(RUNS_UPDATE_PENDING_VERIFIED_BY_AGENT).run(
          status, now, row.run_id, agentId,
        ) as { changes: number };
        if (upd.changes === 0) continue;
        closeRunFiles(db, row.run_id, now);
        finishLinkedTask(db, row.run_id, status, agentId, now, receipt || undefined);
        ids.push(row.run_id);
        if (receipt) {
          try {
            db.prepare(RUN_LOG_INSERT_VERIFIED).run(
              'evt_' + randomUUID().replace(/-/g, ''), row.run_id, agentId, receipt, now,
            );
          } catch { /* non-critical audit log */ }
        }
      }
      db.exec('COMMIT');
      // VER-1: Return null for run_id — no single task applies in allPending batch mode.
      return { ok: true, run_id: null, run_ids: ids, count: ids.length, status: status as RunStatus, updated_at: now };
    } catch (e) {
      try { db.exec('ROLLBACK'); } catch { /* not in transaction */ }
      throw e;
    }
  }

  if (!runId) {
    return { ok: false, error: '--run-id is required (or use --all-pending)', run_id: null };
  }

  const now = utcNow();
  db.exec('BEGIN IMMEDIATE');
  try {
    const result = db.prepare(RUNS_UPDATE_PENDING_VERIFIED_BY_AGENT).run(
      status, now, runId, agentId,
    ) as { changes: number };

    if (result.changes === 0) {
      db.exec('ROLLBACK');
      // Distinguish: no such run / wrong agent / not PENDING
      const row = db.prepare(RUNS_SELECT_STATUS).get(runId) as unknown as AgentStatusRow | undefined;

      if (!row) {
        return { ok: false, error: `no run found with run_id=${runId}`, run_id: runId };
      }
      if (row.agent_id !== agentId) {
        return {
          ok: false,
          error: `run ${runId} belongs to agent "${row.agent_id}", not "${agentId}"`,
          run_id: runId,
        };
      }
      return {
        ok: false,
        error: `run ${runId} has status "${row.status}" — only PENDING runs can be verified`,
        run_id: runId,
      };
    }

    if (receipt) {
      try {
        db.prepare(RUN_LOG_INSERT_VERIFIED).run(
          'evt_' + randomUUID().replace(/-/g, ''), runId, agentId, receipt, now,
        );
      } catch { /* non-critical audit log */ }
    }

    closeRunFiles(db, runId, now);
    finishLinkedTask(db, runId, status, agentId, now, receipt || undefined);
    db.exec('COMMIT');
    return { ok: true, run_id: runId, status: status as RunStatus, updated_at: now };
  } catch (e) {
    try { db.exec('ROLLBACK'); } catch { /* not in transaction */ }
    throw e;
  }
}
