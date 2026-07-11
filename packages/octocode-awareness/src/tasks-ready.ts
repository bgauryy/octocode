/** Durable plan tasks, dependencies, leases, and execution runs. */
import { randomUUID } from 'node:crypto';
import type { DatabaseSync } from 'node:sqlite';
import { utcNow } from './helpers.js';
import { CreateTaskParams, event, evictExpiredTaskClaims, getTask, hydrateTask, normalizeTaskPaths, PlanTaskRecord, PlanTaskStatus, required } from './tasks-catalog.js';

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
  const ownsTransaction = !db.isTransaction;
  if (ownsTransaction) db.exec('BEGIN IMMEDIATE');
  try {
    const rows = db.prepare(`SELECT t.task_id, t.plan_id, t.status, p.status AS plan_status
      FROM tasks t JOIN plans p ON p.plan_id = t.plan_id
      WHERE t.task_id IN (?, ?)`)
      .all(params.taskId, params.dependsOnTaskId) as unknown as Array<{
        task_id: string;
        plan_id: string;
        status: PlanTaskStatus;
        plan_status: string;
      }>;
    if (rows.length !== 2) throw new Error('both dependency tasks must exist');
    if (rows[0]!.plan_id !== rows[1]!.plan_id) throw new Error('task dependencies must stay within one plan');
    const task = rows.find((row) => row.task_id === params.taskId)!;
    const dependency = rows.find((row) => row.task_id === params.dependsOnTaskId)!;
    if (['COMPLETED', 'CANCELLED'].includes(task.plan_status)) {
      throw new Error(`cannot change dependencies in ${task.plan_status.toLowerCase()} plan ${task.plan_id}`);
    }
    if (!['OPEN', 'BLOCKED'].includes(task.status)) {
      throw new Error(`cannot change dependencies for task ${params.taskId} with status ${task.status}`);
    }
    if (dependency.status === 'CANCELLED') {
      throw new Error(`cannot depend on cancelled task ${params.dependsOnTaskId}`);
    }
    const cycle = db.prepare(`WITH RECURSIVE chain(task_id) AS (
        SELECT depends_on_task_id FROM task_dependencies WHERE task_id = ?
        UNION
        SELECT td.depends_on_task_id FROM task_dependencies td JOIN chain c ON td.task_id = c.task_id
      ) SELECT 1 FROM chain WHERE task_id = ? LIMIT 1`)
      .get(params.dependsOnTaskId, params.taskId);
    if (cycle) throw new Error('task dependency would create a cycle');
    const now = utcNow();
    const inserted = db.prepare(`INSERT OR IGNORE INTO task_dependencies
      (task_id, depends_on_task_id, created_by, created_at) VALUES (?, ?, ?, ?)`)
      .run(params.taskId, params.dependsOnTaskId, params.agentId, now) as { changes: number };
    if (inserted.changes > 0) {
      event(db, params.taskId, null, params.agentId, 'DEPENDENCY_ADDED', params.dependsOnTaskId, now);
    }
    if (ownsTransaction) db.exec('COMMIT');
  } catch (error) {
    if (ownsTransaction) {
      try { db.exec('ROLLBACK'); } catch { /* transaction did not open */ }
    }
    throw error;
  }
}

export function listTasks(
  db: DatabaseSync,
  params: { planId?: string | null; status?: PlanTaskStatus | null; agentId?: string | null; workspacePath?: string | null; limit?: number | null } = {},
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
  if (params.workspacePath) {
    where.push('EXISTS (SELECT 1 FROM plans p WHERE p.plan_id = t.plan_id AND p.workspace_path = ?)');
    binds.push(params.workspacePath);
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
  params: { planId?: string | null; status?: PlanTaskStatus | null; agentId?: string | null; workspacePath?: string | null } = {},
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
  if (params.workspacePath) {
    where.push('EXISTS (SELECT 1 FROM plans p WHERE p.plan_id = t.plan_id AND p.workspace_path = ?)');
    binds.push(params.workspacePath);
  }
  return (db.prepare(`SELECT COUNT(*) AS count FROM tasks t WHERE ${where.join(' AND ')}`)
    .get(...binds) as { count: number }).count;
}

export function listReadyTasks(
  db: DatabaseSync,
  params: { planId?: string | null; workspacePath?: string | null; limit?: number | null } = {},
): PlanTaskRecord[] {
  evictExpiredTaskClaims(db);
  const binds: Array<string | number> = [];
  const planWhere = params.planId ? 'AND t.plan_id = ?' : '';
  if (params.planId) binds.push(params.planId);
  const workspaceWhere = params.workspacePath ? 'AND p.workspace_path = ?' : '';
  if (params.workspacePath) binds.push(params.workspacePath);
  const limit = params.limit == null ? null : Math.max(1, Math.floor(params.limit));
  const limitSql = limit == null ? '' : 'LIMIT ?';
  if (limit != null) binds.push(limit);
  const rows = db.prepare(`SELECT t.* FROM tasks t JOIN plans p ON p.plan_id = t.plan_id
    WHERE t.status = 'OPEN' AND p.status = 'ACTIVE' ${planWhere} ${workspaceWhere}
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

export function countReadyTasks(db: DatabaseSync, params: { planId?: string | null; workspacePath?: string | null } = {}): number {
  evictExpiredTaskClaims(db);
  const binds: string[] = [];
  const planWhere = params.planId ? 'AND t.plan_id = ?' : '';
  if (params.planId) binds.push(params.planId);
  const workspaceWhere = params.workspacePath ? 'AND p.workspace_path = ?' : '';
  if (params.workspacePath) binds.push(params.workspacePath);
  return (db.prepare(`SELECT COUNT(*) AS count FROM tasks t JOIN plans p ON p.plan_id = t.plan_id
    WHERE t.status = 'OPEN' AND p.status = 'ACTIVE' ${planWhere} ${workspaceWhere}
      AND NOT EXISTS (SELECT 1 FROM task_claims c WHERE c.task_id = t.task_id)
      AND NOT EXISTS (
        SELECT 1 FROM task_dependencies td
        JOIN tasks dependency ON dependency.task_id = td.depends_on_task_id
        WHERE td.task_id = t.task_id AND dependency.status <> 'DONE'
      )`).get(...binds) as { count: number }).count;
}
