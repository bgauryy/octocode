/** Durable plan tasks, dependencies, leases, and execution runs. */
import { randomUUID } from 'node:crypto';
import type { DatabaseSync } from 'node:sqlite';
import { utcNow } from './helpers.js';
import { ensureRunSession } from './sessions.js';
import { DEFAULT_CLAIM_LEASE_MS, event, evictExpiredTaskClaims, getTask, MAX_CLAIM_LEASE_MS, PlanTaskRecord, required, TaskClaimRecord, TaskRunRecord } from './tasks-catalog.js';

export type ClaimTaskResult =
  | { ok: true; task: PlanTaskRecord; run: TaskRunRecord; claim: TaskClaimRecord }
  | { ok: false; error: string; task_id: string };

export function claimTask(
  db: DatabaseSync,
  params: { taskId: string; agentId: string; sessionId?: string | null; leaseMs?: number; testPlan?: string },
): ClaimTaskResult {
  const agentId = required(params.agentId, 'agent id');
  const now = utcNow();
  const leaseMs = Math.min(Math.max(1, params.leaseMs ?? DEFAULT_CLAIM_LEASE_MS), MAX_CLAIM_LEASE_MS);
  const expiresAt = new Date(Date.parse(now) + leaseMs).toISOString().replace(/\.\d{3}Z$/, 'Z');
  const runId = `run_${randomUUID().replace(/-/g, '')}`;

  db.exec('BEGIN IMMEDIATE');
  try {
    evictExpiredTaskClaims(db, now);
    const row = db.prepare(`SELECT t.*, p.workspace_path, p.artifact, p.status AS plan_status
      FROM tasks t JOIN plans p ON p.plan_id = t.plan_id WHERE t.task_id = ?`)
      .get(params.taskId) as Record<string, unknown> | undefined;
    if (!row) { db.exec('ROLLBACK'); return { ok: false, error: `task not found: ${params.taskId}`, task_id: params.taskId }; }
    const existing = db.prepare('SELECT agent_id FROM task_claims WHERE task_id = ?').get(params.taskId) as { agent_id: string } | undefined;
    if (existing) { db.exec('ROLLBACK'); return { ok: false, error: `task is already claimed by ${existing.agent_id}`, task_id: params.taskId }; }
    if (row['plan_status'] !== 'ACTIVE') {
      db.exec('ROLLBACK');
      return { ok: false, error: `task plan is not ACTIVE: status=${String(row['plan_status'])}`, task_id: params.taskId };
    }
    if (row['status'] !== 'OPEN') {
      db.exec('ROLLBACK');
      return { ok: false, error: `task is not ready: status=${String(row['status'])}`, task_id: params.taskId };
    }
    const blocked = db.prepare(`SELECT 1 FROM task_dependencies td
      JOIN tasks dependency ON dependency.task_id = td.depends_on_task_id
      WHERE td.task_id = ? AND dependency.status <> 'DONE' LIMIT 1`).get(params.taskId);
    if (blocked) { db.exec('ROLLBACK'); return { ok: false, error: 'task is blocked by unfinished dependencies', task_id: params.taskId }; }
    const workspacePath = String(row['workspace_path']);
    const artifact = row['artifact'] == null ? null : String(row['artifact']);
    const reasoning = String(row['reasoning']);
    const acceptanceCriteria = String(row['acceptance_criteria']);
    const planId = String(row['plan_id']);
    if (params.sessionId) {
      ensureRunSession(db, {
        sessionId: params.sessionId,
        agentId,
        workspacePath,
        artifact,
      });
    }
    db.prepare(`INSERT INTO task_runs
      (run_id, task_id, origin, agent_id, session_id, rationale, test_plan, status, workspace_path, artifact, created_at, updated_at)
      VALUES (?, ?, 'TASK', ?, ?, ?, ?, 'ACTIVE', ?, ?, ?, ?)`)
      .run(runId, params.taskId, agentId, params.sessionId ?? null, reasoning,
        params.testPlan?.trim() || acceptanceCriteria, workspacePath, artifact,
        now, now);
    db.prepare(`INSERT INTO task_claims(task_id, run_id, agent_id, claimed_at, heartbeat_at, expires_at)
      VALUES (?, ?, ?, ?, ?, ?)`)
      .run(params.taskId, runId, agentId, now, now, expiresAt);
    db.prepare("UPDATE tasks SET status = 'IN_PROGRESS', updated_at = ? WHERE task_id = ?")
      .run(now, params.taskId);
    db.prepare(`INSERT INTO plan_members(plan_id, agent_id, role, joined_at)
      VALUES (?, ?, 'CONTRIBUTOR', ?) ON CONFLICT(plan_id, agent_id) DO NOTHING`)
      .run(planId, agentId, now);
    event(db, params.taskId, runId, agentId, 'CLAIMED', 'task claimed', now);
    db.exec('COMMIT');
  } catch (error) {
    try { db.exec('ROLLBACK'); } catch { /* transaction did not open */ }
    throw error;
  }

  const task = getTask(db, params.taskId)!;
  const run = db.prepare('SELECT * FROM task_runs WHERE run_id = ?').get(runId) as unknown as TaskRunRecord;
  return { ok: true, task, run, claim: task.claim! };
}

export function heartbeatTaskClaim(
  db: DatabaseSync,
  params: { taskId: string; runId: string; agentId: string; leaseMs?: number },
): TaskClaimRecord {
  const now = utcNow();
  const leaseMs = Math.min(Math.max(1, params.leaseMs ?? DEFAULT_CLAIM_LEASE_MS), MAX_CLAIM_LEASE_MS);
  const expiresAt = new Date(Date.parse(now) + leaseMs).toISOString().replace(/\.\d{3}Z$/, 'Z');
  let found = false;
  db.exec('BEGIN IMMEDIATE');
  try {
    evictExpiredTaskClaims(db, now);
    const result = db.prepare(`UPDATE task_claims SET heartbeat_at = ?, expires_at = ?
      WHERE task_id = ? AND run_id = ? AND agent_id = ? AND expires_at > ?`)
      .run(now, expiresAt, params.taskId, params.runId, params.agentId, now) as { changes: number };
    found = result.changes > 0;
    db.exec('COMMIT');
  } catch (error) {
    try { db.exec('ROLLBACK'); } catch { /* transaction did not begin */ }
    throw error;
  }
  if (!found) throw new Error('active task claim not found for this agent and run');
  return db.prepare('SELECT * FROM task_claims WHERE task_id = ?').get(params.taskId) as unknown as TaskClaimRecord;
}

export function submitTask(
  db: DatabaseSync,
  params: { taskId: string; runId: string; agentId: string; message?: string },
): { task: PlanTaskRecord; run: TaskRunRecord } {
  const now = utcNow();
  // Evict outside the write TX so a rejected submit does not undo lease expiry cleanup.
  evictExpiredTaskClaims(db, now);
  db.exec('BEGIN IMMEDIATE');
  try {
    const claim = db.prepare(
      `SELECT 1 AS ok FROM task_claims
       WHERE task_id = ? AND run_id = ? AND agent_id = ? AND expires_at > ?`,
    ).get(params.taskId, params.runId, params.agentId, now);
    if (!claim) {
      db.exec('ROLLBACK');
      throw new Error('only the active claimant can submit this task');
    }
    db.prepare('DELETE FROM locks WHERE run_id = ?').run(params.runId);
    db.prepare(`UPDATE run_files SET heartbeat_at = ?, expires_at = ?, ended_at = ?
      WHERE run_id = ? AND ended_at IS NULL`).run(now, now, now, params.runId);
    db.prepare("UPDATE task_runs SET status = 'PENDING', updated_at = ? WHERE run_id = ? AND status = 'ACTIVE'")
      .run(now, params.runId);
    db.prepare("UPDATE tasks SET status = 'VERIFY', updated_at = ? WHERE task_id = ?")
      .run(now, params.taskId);
    db.prepare('DELETE FROM task_claims WHERE task_id = ?').run(params.taskId);
    event(db, params.taskId, params.runId, params.agentId, 'SUBMITTED', params.message?.trim() || 'submitted for verification', now);
    db.exec('COMMIT');
  } catch (error) {
    try { db.exec('ROLLBACK'); } catch { /* transaction did not open */ }
    throw error;
  }
  return {
    task: getTask(db, params.taskId)!,
    run: db.prepare('SELECT * FROM task_runs WHERE run_id = ?').get(params.runId) as unknown as TaskRunRecord,
  };
}

export function releaseTaskClaim(
  db: DatabaseSync,
  params: { taskId: string; runId: string; agentId: string; blockedReason?: string | null },
): PlanTaskRecord {
  const now = utcNow();
  const blockedReason = params.blockedReason?.trim();
  // Evict outside the write TX so a rejected release does not undo lease expiry cleanup.
  evictExpiredTaskClaims(db, now);
  db.exec('BEGIN IMMEDIATE');
  try {
    const claim = db.prepare(
      `SELECT 1 AS ok FROM task_claims
       WHERE task_id = ? AND run_id = ? AND agent_id = ? AND expires_at > ?`,
    ).get(params.taskId, params.runId, params.agentId, now);
    if (!claim) {
      db.exec('ROLLBACK');
      throw new Error('only the active claimant can release this task');
    }
    db.prepare('DELETE FROM locks WHERE run_id = ?').run(params.runId);
    db.prepare(`UPDATE run_files SET heartbeat_at = ?, expires_at = ?, ended_at = ?
      WHERE run_id = ? AND ended_at IS NULL`).run(now, now, now, params.runId);
    db.prepare("UPDATE task_runs SET status = 'FAILED', updated_at = ? WHERE run_id = ? AND status = 'ACTIVE'")
      .run(now, params.runId);
    db.prepare('UPDATE tasks SET status = ?, updated_at = ? WHERE task_id = ?')
      .run(blockedReason ? 'BLOCKED' : 'OPEN', now, params.taskId);
    db.prepare('DELETE FROM task_claims WHERE task_id = ?').run(params.taskId);
    event(db, params.taskId, params.runId, params.agentId, blockedReason ? 'BLOCKED' : 'RELEASED', blockedReason || 'claim released', now);
    db.exec('COMMIT');
  } catch (error) {
    try { db.exec('ROLLBACK'); } catch { /* transaction did not open */ }
    throw error;
  }
  return getTask(db, params.taskId)!;
}
