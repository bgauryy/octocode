/**
 * verify.ts — Verify-gate operations for the awareness Stop hook.
 *
 * auditUnverified: returns tasks with status='PENDING' (edited but not verified)
 *                  for an agent/workspace. The Stop hook (stop-verify.sh) blocks
 *                  conclude when count > 0.
 *
 * markVerified:    transitions a task PENDING → SUCCESS | FAILED so the gate
 *                  clears after the agent verifies its edits. Restricted to PENDING
 *                  transitions to prevent orphaning ACTIVE locks as SUCCESS.
 */

import { randomUUID } from 'node:crypto';
import type { DatabaseSync } from 'node:sqlite';
import { normalizeArtifact, utcNow, parseJsonList } from './helpers.js';
import { normalizeWorkspacePath } from './git.js';
import type { TaskStatus } from './types.js';
import {
  TASKS_UPDATE_PENDING_TO_FAILED,
  TASKS_UPDATE_ACTIVE_TO_FAILED,
  TASK_LOG_INSERT_ABANDONED,
  TASK_LOG_INSERT_STALE_ABANDONED,
  TASK_LOG_INSERT_VERIFIED,
  TASKS_UPDATE_PENDING_VERIFIED,
  TASKS_UPDATE_PENDING_VERIFIED_BY_AGENT,
  TASKS_SELECT_STATUS,
  TASKS_SELECT_PENDING_IDS,
} from './sql/tasks.js';

// ─── Public shapes ────────────────────────────────────────────────────────────

export interface UnverifiedIntent {
  task_id: string;
  agent_id: string;
  status: string;
  test_plan: string;
  plan_doc_ref: string | null;
  rationale: string;
  target_files: string[];
  workspace_path: string | null;
  artifact: string | null;
  created_at: string;
}

/**
 * VER-2: An ACTIVE task whose locks have all been evicted (expired).
 * These are orphaned sessions the old audit silently missed.
 */
export interface StaleActiveIntent {
  task_id: string;
  agent_id: string;
  status: 'ACTIVE';
  rationale: string;
  plan_doc_ref: string | null;
  target_files: string[];
  workspace_path: string | null;
  artifact: string | null;
  created_at: string;
  age_hours: number; // how long stuck ACTIVE with no live locks
}

export interface AuditUnverifiedResult {
  ok: true;
  unverified: UnverifiedIntent[];    // status=PENDING: released, awaiting verify
  stale_active: StaleActiveIntent[]; // VER-2: ACTIVE with no live locks
  count: number;                     // total = unverified.length + stale_active.length
}

export interface AuditUnverifiedParams {
  agentId?: string | null;
  workspacePath?: string | null;
  artifact?: string | null;
  abandon?: boolean;         // dismiss all PENDING tasks as FAILED (clear orphaned)
}

export type VerifyStatus = 'SUCCESS' | 'FAILED';

export interface MarkVerifiedParams {
  taskId?: string;            // verify one task; required unless allPending=true
  agentId?: string;
  allPending?: boolean;       // verify ALL pending tasks for this agent/workspace
  workspacePath?: string | null;
  artifact?: string | null;
  message?: string;           // what was verified
  status?: VerifyStatus;
}

export interface MarkVerifiedOk {
  ok: true;
  // VER-1: null when allPending=true (no single task applies in batch mode).
  // Callers must guard for null when using allPending.
  task_id: string | null;
  task_ids?: string[];   // set when allPending=true
  count?: number;        // set when allPending=true
  status: TaskStatus;
  updated_at: string;
}

export interface MarkVerifiedErr {
  ok: false;
  error: string;
  task_id: string | null;
}

export type MarkVerifiedResult = MarkVerifiedOk | MarkVerifiedErr;

// ─── Internal ─────────────────────────────────────────────────────────────────

const VALID_VERIFY_STATUSES = new Set<string>(['SUCCESS', 'FAILED']);

interface IntentDbRow {
  task_id: string;
  agent_id: string;
  status: string;
  test_plan: string;
  plan_doc_ref: string | null;
  rationale: string;
  workspace_path: string | null;
  artifact: string | null;
  files_json: string;
  created_at: string;
}

interface AgentStatusRow {
  agent_id: string;
  status: string;
}

// ─── Exports ──────────────────────────────────────────────────────────────────

/**
 * Return all tasks rows with status='PENDING', optionally scoped to an
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
    `SELECT task_id, agent_id, status, test_plan, plan_doc_ref, rationale, workspace_path, artifact, files_json, created_at
     FROM tasks
     WHERE ${where.join(' AND ')}
     ORDER BY created_at ASC`,
  ).all(...binds) as unknown as IntentDbRow[];

  const unverified: UnverifiedIntent[] = rows.map(r => ({
    task_id: r.task_id,
    agent_id: r.agent_id,
    status: r.status,
    test_plan: r.test_plan,
    plan_doc_ref: r.plan_doc_ref,
    rationale: r.rationale,
    target_files: parseJsonList(r.files_json),
    workspace_path: r.workspace_path,
    artifact: r.artifact,
    created_at: r.created_at,
  }));

  if (params.abandon && unverified.length > 0) {
    const now = utcNow();
    for (const intent of unverified) {
      db.prepare(TASKS_UPDATE_PENDING_TO_FAILED).run(now, intent.task_id);
      try {
        db.prepare(TASK_LOG_INSERT_ABANDONED).run(
          'evt_' + randomUUID().replace(/-/g, ''), intent.task_id, intent.agent_id, now,
        );
      } catch { /* non-critical audit log */ }
    }
  }

  // VER-2: Detect ACTIVE tasks whose locks have all expired/been evicted.
  const staleActive: StaleActiveIntent[] = [];
  try {
    const nowIso = utcNow();
    const staleWhere: string[] = [
      "ai.status = 'ACTIVE'",
      `NOT EXISTS (
        SELECT 1 FROM locks fl
        WHERE fl.task_id = ai.task_id
          AND (fl.expires_at IS NULL OR fl.expires_at > ?)
      )`,
    ];
    const staleBinds: (string | number)[] = [nowIso];
    if (params.agentId) { staleWhere.push('ai.agent_id = ?'); staleBinds.push(params.agentId); }
    if (workspacePath) { staleWhere.push('ai.workspace_path = ?'); staleBinds.push(workspacePath); }
    if (artifact) { staleWhere.push('(ai.artifact = ? OR ai.artifact IS NULL)'); staleBinds.push(artifact); }

    const staleRows = db.prepare(
      `SELECT ai.task_id, ai.agent_id, ai.rationale, ai.plan_doc_ref, ai.workspace_path, ai.artifact, ai.files_json, ai.created_at
       FROM tasks ai
       WHERE ${staleWhere.join(' AND ')}
       ORDER BY ai.created_at ASC`
    ).all(...staleBinds) as unknown as IntentDbRow[];

    for (const r of staleRows) {
      const ageMs = Date.now() - new Date(r.created_at).getTime();
      staleActive.push({
        task_id: r.task_id,
        agent_id: r.agent_id,
        status: 'ACTIVE',
        rationale: r.rationale,
        plan_doc_ref: r.plan_doc_ref,
        target_files: parseJsonList(r.files_json),
        workspace_path: r.workspace_path,
        artifact: r.artifact,
        created_at: r.created_at,
        age_hours: Math.round(ageMs / 3600000 * 10) / 10,
      });
    }
  } catch (e) { if (!(e instanceof Error && e.message.includes('no such table'))) throw e; }

  if (params.abandon && staleActive.length > 0) {
    const now = utcNow();
    for (const intent of staleActive) {
      db.prepare(TASKS_UPDATE_ACTIVE_TO_FAILED).run(now, intent.task_id);
      try {
        db.prepare(TASK_LOG_INSERT_STALE_ABANDONED).run(
          'evt_' + randomUUID().replace(/-/g, ''), intent.task_id, intent.agent_id, now,
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
  const taskId = params.taskId ?? '';
  const status = params.status ?? 'SUCCESS';

  if (!VALID_VERIFY_STATUSES.has(status)) {
    return {
      ok: false,
      error: `invalid status "${status}" — must be SUCCESS or FAILED`,
      task_id: taskId || null,
    };
  }

  // --all-pending: verify every PENDING task for this agent/workspace at once
  if (allPending) {
    const dynWhere = [
      workspacePath ? ' AND workspace_path = ?' : '',
      artifact ? ' AND (artifact = ? OR artifact IS NULL)' : '',
    ].join('');
    const selectSql = TASKS_SELECT_PENDING_IDS.replace('{DYNAMIC_WHERE}', dynWhere);
    const selectBinds: (string | number)[] = [agentId];
    if (workspacePath) selectBinds.push(workspacePath);
    if (artifact) selectBinds.push(artifact);

    const rows = db.prepare(selectSql).all(...selectBinds) as unknown as Array<{ task_id: string }>;
    const now = utcNow();
    const ids: string[] = [];
    for (const row of rows) {
      db.prepare(TASKS_UPDATE_PENDING_VERIFIED).run(status, now, row.task_id);
      ids.push(row.task_id);
      if (message) {
        try {
          db.prepare(TASK_LOG_INSERT_VERIFIED).run(
            'evt_' + randomUUID().replace(/-/g, ''), row.task_id, agentId, message, now,
          );
        } catch { /* non-critical audit log */ }
      }
    }
    // VER-1: Return null for task_id — no single task applies in allPending batch mode.
    return { ok: true, task_id: null, task_ids: ids, count: ids.length, status: status as TaskStatus, updated_at: now };
  }

  if (!taskId) {
    return { ok: false, error: '--task-id is required (or use --all-pending)', task_id: null };
  }

  const now = utcNow();
  const result = db.prepare(TASKS_UPDATE_PENDING_VERIFIED_BY_AGENT).run(
    status, now, taskId, agentId,
  ) as { changes: number };

  if (result.changes === 0) {
    // Distinguish: no such task / wrong agent / not PENDING
    const row = db.prepare(TASKS_SELECT_STATUS).get(taskId) as unknown as AgentStatusRow | undefined;

    if (!row) {
      return { ok: false, error: `no task found with task_id=${taskId}`, task_id: taskId };
    }
    if (row.agent_id !== agentId) {
      return {
        ok: false,
        error: `task ${taskId} belongs to agent "${row.agent_id}", not "${agentId}"`,
        task_id: taskId,
      };
    }
    return {
      ok: false,
      error: `task ${taskId} has status "${row.status}" — only PENDING tasks can be verified`,
      task_id: taskId,
    };
  }

  if (message) {
    try {
      db.prepare(TASK_LOG_INSERT_VERIFIED).run(
        'evt_' + randomUUID().replace(/-/g, ''), taskId, agentId, message, now,
      );
    } catch { /* non-critical audit log */ }
  }

  return { ok: true, task_id: taskId, status: status as TaskStatus, updated_at: now };
}
