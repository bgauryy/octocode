/** Durable plan tasks, dependencies, leases, and execution runs. */

import { randomUUID } from 'node:crypto';
import { isAbsolute, relative, resolve, sep } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { utcNow } from './helpers.js';
import { normalizeWorkspacePath } from './git.js';

export type PlanTaskStatus = 'OPEN' | 'IN_PROGRESS' | 'BLOCKED' | 'VERIFY' | 'DONE' | 'FAILED' | 'CANCELLED';

export interface PlanTaskRecord {
  task_id: string;
  plan_id: string;
  title: string;
  reasoning: string;
  acceptance_criteria: string;
  status: PlanTaskStatus;
  priority: number;
  created_by: string;
  created_at: string;
  updated_at: string;
  completed_at: string | null;
  paths: string[];
  dependencies: string[];
  claim: TaskClaimRecord | null;
}

export interface TaskClaimRecord {
  task_id: string;
  run_id: string;
  agent_id: string;
  claimed_at: string;
  heartbeat_at: string;
  expires_at: string;
}

export interface TaskRunRecord {
  run_id: string;
  task_id: string | null;
  origin: 'TASK' | 'WORK' | 'HOOK';
  agent_id: string;
  session_id: string | null;
  rationale: string;
  test_plan: string;
  context_ref: string | null;
  status: 'PENDING' | 'ACTIVE' | 'SUCCESS' | 'FAILED';
  workspace_path: string | null;
  artifact: string | null;
  created_at: string;
  updated_at: string;
}

export interface CreateTaskParams {
  planId: string;
  title: string;
  reasoning: string;
  acceptanceCriteria: string;
  paths: string[];
  createdBy: string;
  priority?: number;
  dependsOn?: string[];
}

const DEFAULT_CLAIM_LEASE_MS = 30 * 60_000;
const MAX_CLAIM_LEASE_MS = 60 * 60_000;

function required(value: string, field: string): string {
  const normalized = value.trim();
  if (!normalized) throw new Error(`${field} is required`);
  return normalized;
}

function normalizeTaskPaths(workspacePath: string, paths: string[]): string[] {
  if (paths.length === 0) throw new Error('at least one task path is required');
  const root = resolve(workspacePath);
  const normalized = paths.map((input) => {
    const value = required(input, 'task path');
    const absolute = isAbsolute(value) ? resolve(value) : resolve(root, value);
    const rel = relative(root, absolute);
    if (!rel || rel === '..' || rel.startsWith(`..${sep}`) || isAbsolute(rel)) {
      throw new Error(`task path must be workspace-relative and below the workspace: ${input}`);
    }
    return rel.split(sep).join('/');
  });
  return [...new Set(normalized)];
}

function event(
  db: DatabaseSync,
  taskId: string,
  runId: string | null,
  agentId: string,
  eventType: string,
  message: string,
  now = utcNow(),
): void {
  db.prepare(`INSERT INTO task_events(event_id, task_id, run_id, agent_id, event_type, message, created_at)
    VALUES (?, ?, ?, ?, ?, ?, ?)`)
    .run(`tevt_${randomUUID().replace(/-/g, '')}`, taskId, runId, agentId, eventType, message, now);
}

function evictExpiredTaskClaims(db: DatabaseSync, now = utcNow()): void {
  const expired = db.prepare(
    'SELECT task_id, run_id, agent_id FROM task_claims WHERE expires_at <= ?',
  ).all(now) as unknown as Array<{ task_id: string; run_id: string; agent_id: string }>;
  if (expired.length === 0) return;

  // SAVEPOINT (not BEGIN): claimTask already holds BEGIN IMMEDIATE when it calls us.
  db.exec('SAVEPOINT evict_expired_task_claims');
  try {
    for (const claim of expired) {
      db.prepare('DELETE FROM locks WHERE run_id = ?').run(claim.run_id);
      db.prepare(`UPDATE run_files SET heartbeat_at = ?, expires_at = ?, ended_at = ?
        WHERE run_id = ? AND ended_at IS NULL`).run(now, now, now, claim.run_id);
      db.prepare("UPDATE task_runs SET status = 'FAILED', updated_at = ? WHERE run_id = ? AND status = 'ACTIVE'")
        .run(now, claim.run_id);
      db.prepare("UPDATE tasks SET status = 'OPEN', updated_at = ? WHERE task_id = ? AND status = 'IN_PROGRESS'")
        .run(now, claim.task_id);
      db.prepare('DELETE FROM task_claims WHERE task_id = ?').run(claim.task_id);
      event(db, claim.task_id, claim.run_id, claim.agent_id, 'CLAIM_EXPIRED', 'claim lease expired', now);
    }
    db.exec('RELEASE SAVEPOINT evict_expired_task_claims');
  } catch (e) {
    try { db.exec('ROLLBACK TO SAVEPOINT evict_expired_task_claims'); } catch { /* already rolled back */ }
    try { db.exec('RELEASE SAVEPOINT evict_expired_task_claims'); } catch { /* already released */ }
    throw e;
  }
}

function hydrateTask(db: DatabaseSync, row: Record<string, unknown>): PlanTaskRecord {
  const taskId = String(row['task_id']);
  const paths = db.prepare('SELECT path FROM task_paths WHERE task_id = ? ORDER BY ordinal, path')
    .all(taskId).map((item) => String((item as Record<string, unknown>)['path']));
  const dependencies = db.prepare(
    'SELECT depends_on_task_id FROM task_dependencies WHERE task_id = ? ORDER BY created_at, depends_on_task_id',
  ).all(taskId).map((item) => String((item as Record<string, unknown>)['depends_on_task_id']));
  const claim = db.prepare('SELECT * FROM task_claims WHERE task_id = ?').get(taskId) as unknown as TaskClaimRecord | undefined;
  return { ...(row as unknown as Omit<PlanTaskRecord, 'paths' | 'dependencies' | 'claim'>), paths, dependencies, claim: claim ?? null };
}

export function getTask(db: DatabaseSync, taskId: string): PlanTaskRecord | null {
  evictExpiredTaskClaims(db);
  const row = db.prepare('SELECT * FROM tasks WHERE task_id = ?').get(taskId) as Record<string, unknown> | undefined;
  return row ? hydrateTask(db, row) : null;
}

/** Return one unambiguous live plan-task claim for hook attachment. */
export function activeTaskClaimForAgent(
  db: DatabaseSync,
  params: { agentId: string; workspacePath: string; artifact?: string | null },
): TaskClaimRecord | null {
  evictExpiredTaskClaims(db);
  const workspacePath = normalizeWorkspacePath(params.workspacePath, params.workspacePath)
    ?? resolve(params.workspacePath);
  const where = ['c.agent_id = ?', 'p.workspace_path = ?', 'c.expires_at > ?'];
  const binds: Array<string | null> = [params.agentId, workspacePath, utcNow()];
  if (params.artifact) {
    where.push('(p.artifact = ? OR p.artifact IS NULL)');
    binds.push(params.artifact);
  }
  const claims = db.prepare(`SELECT c.* FROM task_claims c
    JOIN tasks t ON t.task_id = c.task_id
    JOIN plans p ON p.plan_id = t.plan_id
    WHERE ${where.join(' AND ')} ORDER BY c.claimed_at DESC LIMIT 2`)
    .all(...binds) as unknown as TaskClaimRecord[];
  return claims.length === 1 ? claims[0]! : null;
}

export function createTask(
  db: DatabaseSync,
  params: CreateTaskParams,
): { task: PlanTaskRecord } {
  const plan = db.prepare('SELECT workspace_path, status FROM plans WHERE plan_id = ?')
    .get(params.planId) as { workspace_path: string; status: string } | undefined;
  if (!plan) throw new Error(`plan not found: ${params.planId}`);
  if (['COMPLETED', 'CANCELLED'].includes(plan.status)) {
    throw new Error(`cannot add tasks to ${plan.status.toLowerCase()} plan ${params.planId}`);
  }
  const title = required(params.title, 'task title');
  const reasoning = required(params.reasoning, 'task reasoning');
  const createdBy = required(params.createdBy, 'task creator');
  const acceptance = required(params.acceptanceCriteria, 'task acceptance criteria');
  const paths = normalizeTaskPaths(plan.workspace_path, params.paths);
  const taskId = `task_${randomUUID().replace(/-/g, '')}`;
  const now = utcNow();

  db.exec('BEGIN IMMEDIATE');
  try {
    db.prepare(`INSERT INTO tasks
      (task_id, plan_id, title, reasoning, acceptance_criteria, status, priority, created_by, created_at, updated_at)
      VALUES (?, ?, ?, ?, ?, 'OPEN', ?, ?, ?, ?)`)
      .run(taskId, params.planId, title, reasoning, acceptance, params.priority ?? 0, createdBy, now, now);
    const insertPath = db.prepare('INSERT INTO task_paths(task_id, path, ordinal) VALUES (?, ?, ?)');
    paths.forEach((path, ordinal) => insertPath.run(taskId, path, ordinal));
    event(db, taskId, null, createdBy, 'CREATED', reasoning, now);
    for (const dependency of params.dependsOn ?? []) {
      addTaskDependency(db, { taskId, dependsOnTaskId: dependency, agentId: createdBy });
    }
    db.exec('COMMIT');
  } catch (error) {
    try { db.exec('ROLLBACK'); } catch { /* transaction did not open */ }
    throw error;
  }
  return { task: getTask(db, taskId)! };
}

export function addTaskDependency(
  db: DatabaseSync,
  params: { taskId: string; dependsOnTaskId: string; agentId: string },
): void {
  if (params.taskId === params.dependsOnTaskId) throw new Error('a task cannot depend on itself');
  const rows = db.prepare('SELECT task_id, plan_id FROM tasks WHERE task_id IN (?, ?)')
    .all(params.taskId, params.dependsOnTaskId) as unknown as Array<{ task_id: string; plan_id: string }>;
  if (rows.length !== 2) throw new Error('both dependency tasks must exist');
  if (rows[0]!.plan_id !== rows[1]!.plan_id) throw new Error('task dependencies must stay within one plan');
  const cycle = db.prepare(`WITH RECURSIVE chain(task_id) AS (
      SELECT depends_on_task_id FROM task_dependencies WHERE task_id = ?
      UNION
      SELECT td.depends_on_task_id FROM task_dependencies td JOIN chain c ON td.task_id = c.task_id
    ) SELECT 1 FROM chain WHERE task_id = ? LIMIT 1`)
    .get(params.dependsOnTaskId, params.taskId);
  if (cycle) throw new Error('task dependency would create a cycle');
  const now = utcNow();
  db.prepare(`INSERT OR IGNORE INTO task_dependencies
    (task_id, depends_on_task_id, created_by, created_at) VALUES (?, ?, ?, ?)`)
    .run(params.taskId, params.dependsOnTaskId, params.agentId, now);
  event(db, params.taskId, null, params.agentId, 'DEPENDENCY_ADDED', params.dependsOnTaskId, now);
}

export function listTasks(
  db: DatabaseSync,
  params: { planId?: string | null; status?: PlanTaskStatus | null; agentId?: string | null; limit?: number | null } = {},
): PlanTaskRecord[] {
  evictExpiredTaskClaims(db);
  const where: string[] = ['1 = 1'];
  const binds: Array<string | number> = [];
  if (params.planId) { where.push('t.plan_id = ?'); binds.push(params.planId); }
  if (params.status) { where.push('t.status = ?'); binds.push(params.status); }
  if (params.agentId) {
    where.push('EXISTS (SELECT 1 FROM task_claims c WHERE c.task_id = t.task_id AND c.agent_id = ?)');
    binds.push(params.agentId);
  }
  const limit = params.limit == null ? null : Math.max(1, Math.floor(params.limit));
  const limitSql = limit == null ? '' : 'LIMIT ?';
  const queryBinds: Array<string | number> = limit == null ? binds : [...binds, limit];
  return db.prepare(`SELECT t.* FROM tasks t WHERE ${where.join(' AND ')}
    ORDER BY t.priority DESC, t.created_at, t.task_id ${limitSql}`)
    .all(...queryBinds).map((row) => hydrateTask(db, row as Record<string, unknown>));
}

export function countTasks(
  db: DatabaseSync,
  params: { planId?: string | null; status?: PlanTaskStatus | null; agentId?: string | null } = {},
): number {
  evictExpiredTaskClaims(db);
  const where: string[] = ['1 = 1'];
  const binds: string[] = [];
  if (params.planId) { where.push('t.plan_id = ?'); binds.push(params.planId); }
  if (params.status) { where.push('t.status = ?'); binds.push(params.status); }
  if (params.agentId) {
    where.push('EXISTS (SELECT 1 FROM task_claims c WHERE c.task_id = t.task_id AND c.agent_id = ?)');
    binds.push(params.agentId);
  }
  return (db.prepare(`SELECT COUNT(*) AS count FROM tasks t WHERE ${where.join(' AND ')}`)
    .get(...binds) as { count: number }).count;
}

export function listReadyTasks(
  db: DatabaseSync,
  params: { planId?: string | null; limit?: number | null } = {},
): PlanTaskRecord[] {
  evictExpiredTaskClaims(db);
  const binds: Array<string | number> = [];
  const planWhere = params.planId ? 'AND t.plan_id = ?' : '';
  if (params.planId) binds.push(params.planId);
  const limit = params.limit == null ? null : Math.max(1, Math.floor(params.limit));
  const limitSql = limit == null ? '' : 'LIMIT ?';
  if (limit != null) binds.push(limit);
  const rows = db.prepare(`SELECT t.* FROM tasks t JOIN plans p ON p.plan_id = t.plan_id
    WHERE t.status = 'OPEN' AND p.status = 'ACTIVE' ${planWhere}
      AND NOT EXISTS (SELECT 1 FROM task_claims c WHERE c.task_id = t.task_id)
      AND NOT EXISTS (
        SELECT 1 FROM task_dependencies td
        JOIN tasks dependency ON dependency.task_id = td.depends_on_task_id
        WHERE td.task_id = t.task_id AND dependency.status <> 'DONE'
      )
    ORDER BY t.priority DESC, t.created_at, t.task_id ${limitSql}`)
    .all(...binds) as unknown as Record<string, unknown>[];
  return rows.map((row) => hydrateTask(db, row));
}

export function countReadyTasks(db: DatabaseSync, params: { planId?: string | null } = {}): number {
  evictExpiredTaskClaims(db);
  const binds: string[] = [];
  const planWhere = params.planId ? 'AND t.plan_id = ?' : '';
  if (params.planId) binds.push(params.planId);
  return (db.prepare(`SELECT COUNT(*) AS count FROM tasks t JOIN plans p ON p.plan_id = t.plan_id
    WHERE t.status = 'OPEN' AND p.status = 'ACTIVE' ${planWhere}
      AND NOT EXISTS (SELECT 1 FROM task_claims c WHERE c.task_id = t.task_id)
      AND NOT EXISTS (
        SELECT 1 FROM task_dependencies td
        JOIN tasks dependency ON dependency.task_id = td.depends_on_task_id
        WHERE td.task_id = t.task_id AND dependency.status <> 'DONE'
      )`).get(...binds) as { count: number }).count;
}

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
      db.prepare(`INSERT OR IGNORE INTO sessions(session_id, agent_id, workspace_path, artifact, started_at)
        VALUES (?, ?, ?, ?, ?)`)
        .run(params.sessionId, agentId, workspacePath, artifact, now);
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
  const result = db.prepare(`UPDATE task_claims SET heartbeat_at = ?, expires_at = ?
    WHERE task_id = ? AND run_id = ? AND agent_id = ?`)
    .run(now, expiresAt, params.taskId, params.runId, params.agentId) as { changes: number };
  if (result.changes === 0) throw new Error('active task claim not found for this agent and run');
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
