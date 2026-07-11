/** Durable collaborative plans and their narrative documents. */

import { randomUUID } from 'node:crypto';
import { existsSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { isAbsolute, join, relative, resolve, sep } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { normalizeArtifact, utcNow } from './helpers.js';
import { canonicalizePath, normalizeWorkspacePath } from './git.js';

export type PlanStatus = 'DRAFT' | 'ACTIVE' | 'PAUSED' | 'COMPLETED' | 'CANCELLED';
export type PlanMemberRole = 'LEAD' | 'CONTRIBUTOR';

export interface PlanRecord {
  plan_id: string;
  name: string;
  objective: string;
  lead_agent_id: string;
  status: PlanStatus;
  workspace_path: string;
  artifact: string | null;
  doc_dir: string;
  created_at: string;
  updated_at: string;
}

export interface PlanMemberRecord {
  agent_id: string;
  role: PlanMemberRole;
  joined_at: string;
}

export interface PlanDocRecord {
  relative_path: string;
  title: string;
  kind: 'PRIMARY' | 'SUPPORTING';
  ordinal: number;
}

export interface PlanDetail extends PlanRecord {
  members: PlanMemberRecord[];
  docs: PlanDocRecord[];
}

export interface CreatePlanParams {
  name: string;
  objective: string;
  leadAgentId: string;
  workspacePath: string;
  /**
   * Where to write the `.octocode/plan/**` scaffolding. Plan rows are always
   * scoped to the normalized workspace root so discovery works from any
   * subdirectory, but the filesystem side-effect must land under the path the
   * caller actually asked for — an explicit subdir (or isolated scratch dir)
   * must not silently receive its docs at the shared repo root. Must resolve
   * inside the workspace root; defaults to the root when omitted.
   */
  docsPath?: string | null;
  artifact?: string | null;
}

export interface JoinPlanParams {
  planId: string;
  agentId: string;
}

export interface RegisterPlanDocParams {
  planId: string;
  agentId: string;
  relativePath: string;
  title: string;
}

function required(value: string, field: string): string {
  const normalized = value.trim();
  if (!normalized) throw new Error(`${field} is required`);
  return normalized;
}

function slugify(value: string): string {
  const slug = value.toLowerCase().trim()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 64);
  return slug || 'plan';
}

function planTimestamp(now: string): string {
  return now.replace(/[-:]/g, '').replace('T', '-').replace(/\.\d+Z$/, 'Z');
}

function renderPrimaryPlanDoc(plan: PlanRecord): string {
  return `# ${plan.name}\n\n` +
    `Plan ID: \`${plan.plan_id}\`  \n` +
    `Lead agent: \`${plan.lead_agent_id}\`  \n` +
    `## Objective\n\n${plan.objective}\n\n` +
    `## Live work\n\n` +
    `Plan lifecycle, membership, and task state are live in the Awareness database. ` +
    `Use \`task list --plan-id ${plan.plan_id}\` ` +
    `or \`task ready --plan-id ${plan.plan_id}\`; do not duplicate an editable task checklist here.\n\n` +
    `## Supporting decisions\n\nAdd durable design notes under \`docs/\` and register them with the plan.\n`;
}

function rowToPlan(row: Record<string, unknown>): PlanRecord {
  return row as unknown as PlanRecord;
}

export function createPlan(
  db: DatabaseSync,
  params: CreatePlanParams,
): { plan: PlanRecord; document_path: string; manifest_path: string } {
  const name = required(params.name, 'plan name');
  const objective = required(params.objective, 'plan objective');
  const leadAgentId = required(params.leadAgentId, 'lead agent id');
  const workspacePath = normalizeWorkspacePath(params.workspacePath, params.workspacePath)
    ?? resolve(params.workspacePath);
  const docsRoot = params.docsPath ? canonicalizePath(params.docsPath) : workspacePath;
  let docBase = '';
  if (docsRoot !== workspacePath) {
    docBase = relative(workspacePath, docsRoot);
    if (!docBase || docBase.startsWith('..') || isAbsolute(docBase)) {
      throw new Error(`plan docs path must be inside the workspace root ${workspacePath}: ${docsRoot}`);
    }
  }
  const now = utcNow();
  const planId = `plan_${randomUUID().replace(/-/g, '')}`;
  const docDir = `${docBase ? `${docBase.split(sep).join('/')}/` : ''}.octocode/plan/${planTimestamp(now)}-${slugify(name)}`;
  const absoluteDir = join(workspacePath, docDir);
  if (existsSync(absoluteDir)) {
    throw new Error(`plan document directory already exists: ${docDir}`);
  }

  const plan: PlanRecord = {
    plan_id: planId,
    name,
    objective,
    lead_agent_id: leadAgentId,
    status: 'ACTIVE',
    workspace_path: workspacePath,
    artifact: normalizeArtifact(params.artifact),
    doc_dir: docDir,
    created_at: now,
    updated_at: now,
  };

  mkdirSync(join(absoluteDir, 'docs'), { recursive: true });
  const planPath = join(absoluteDir, 'PLAN.md');
  const manifestPath = join(absoluteDir, 'manifest.json');
  try {
      writeFileSync(planPath, renderPrimaryPlanDoc(plan), { encoding: 'utf8', flag: 'wx' });
      writeFileSync(manifestPath, `${JSON.stringify({
        plan_id: planId,
      primary_doc: 'PLAN.md',
      supporting_docs_dir: 'docs',
      live_task_state: 'awareness.sqlite3',
    }, null, 2)}\n`, { encoding: 'utf8', flag: 'wx' });

    db.exec('BEGIN IMMEDIATE');
    try {
      db.prepare(`INSERT INTO plans
        (plan_id, name, objective, lead_agent_id, status, workspace_path, artifact, doc_dir, created_at, updated_at)
        VALUES (?, ?, ?, ?, 'ACTIVE', ?, ?, ?, ?, ?)`)
        .run(planId, name, objective, leadAgentId, workspacePath, plan.artifact, docDir, now, now);
      db.prepare(`INSERT INTO plan_members(plan_id, agent_id, role, joined_at)
        VALUES (?, ?, 'LEAD', ?)`)
        .run(planId, leadAgentId, now);
      db.prepare(`INSERT INTO plan_docs(plan_id, relative_path, title, kind, ordinal)
        VALUES (?, 'PLAN.md', ?, 'PRIMARY', 0)`)
        .run(planId, name);
      db.exec('COMMIT');
    } catch (error) {
      try { db.exec('ROLLBACK'); } catch { /* transaction did not open */ }
      throw error;
    }
  } catch (error) {
    rmSync(absoluteDir, { recursive: true, force: true });
    throw error;
  }

  return { plan, document_path: planPath, manifest_path: manifestPath };
}

export function getPlan(db: DatabaseSync, planId: string): PlanDetail | null {
  const row = db.prepare('SELECT * FROM plans WHERE plan_id = ?').get(planId) as Record<string, unknown> | undefined;
  if (!row) return null;
  const members = db.prepare(
    'SELECT agent_id, role, joined_at FROM plan_members WHERE plan_id = ? ORDER BY role, joined_at, agent_id',
  ).all(planId) as unknown as PlanMemberRecord[];
  const docs = db.prepare(
    'SELECT relative_path, title, kind, ordinal FROM plan_docs WHERE plan_id = ? ORDER BY ordinal, relative_path',
  ).all(planId) as unknown as PlanDocRecord[];
  return { ...rowToPlan(row), members, docs };
}

export function listPlans(
  db: DatabaseSync,
  params: { workspacePath?: string | null; artifact?: string | null; status?: PlanStatus | null; limit?: number | null } = {},
): PlanRecord[] {
  const where: string[] = ['1 = 1'];
  const binds: string[] = [];
  if (params.workspacePath) {
    where.push('workspace_path = ?');
    binds.push(normalizeWorkspacePath(params.workspacePath, params.workspacePath) ?? resolve(params.workspacePath));
  }
  const artifact = normalizeArtifact(params.artifact);
  if (artifact) { where.push('(artifact = ? OR artifact IS NULL)'); binds.push(artifact); }
  if (params.status) { where.push('status = ?'); binds.push(params.status); }
  const limit = params.limit == null ? null : Math.max(1, Math.floor(params.limit));
  const limitSql = limit == null ? '' : 'LIMIT ?';
  const queryBinds: Array<string | number> = limit == null ? binds : [...binds, limit];
  return db.prepare(
    `SELECT * FROM plans WHERE ${where.join(' AND ')} ORDER BY updated_at DESC, plan_id ${limitSql}`,
  ).all(...queryBinds).map((row) => rowToPlan(row as Record<string, unknown>));
}

export function countPlans(
  db: DatabaseSync,
  params: { workspacePath?: string | null; artifact?: string | null; status?: PlanStatus | null } = {},
): number {
  const where: string[] = ['1 = 1'];
  const binds: string[] = [];
  if (params.workspacePath) {
    where.push('workspace_path = ?');
    binds.push(normalizeWorkspacePath(params.workspacePath, params.workspacePath) ?? resolve(params.workspacePath));
  }
  const artifact = normalizeArtifact(params.artifact);
  if (artifact) { where.push('(artifact = ? OR artifact IS NULL)'); binds.push(artifact); }
  if (params.status) { where.push('status = ?'); binds.push(params.status); }
  return (db.prepare(`SELECT COUNT(*) AS count FROM plans WHERE ${where.join(' AND ')}`)
    .get(...binds) as { count: number }).count;
}

export function joinPlan(db: DatabaseSync, params: JoinPlanParams): PlanMemberRecord {
  const agentId = required(params.agentId, 'agent id');
  if (!getPlan(db, params.planId)) throw new Error(`plan not found: ${params.planId}`);
  const now = utcNow();
  db.prepare(`INSERT INTO plan_members(plan_id, agent_id, role, joined_at)
    VALUES (?, ?, 'CONTRIBUTOR', ?)
    ON CONFLICT(plan_id, agent_id) DO NOTHING`)
    .run(params.planId, agentId, now);
  return db.prepare(
    'SELECT agent_id, role, joined_at FROM plan_members WHERE plan_id = ? AND agent_id = ?',
  ).get(params.planId, agentId) as unknown as PlanMemberRecord;
}

export function registerPlanDocument(
  db: DatabaseSync,
  params: RegisterPlanDocParams,
): PlanDocRecord {
  const plan = getPlan(db, params.planId);
  if (!plan) throw new Error(`plan not found: ${params.planId}`);
  const member = db.prepare('SELECT 1 FROM plan_members WHERE plan_id = ? AND agent_id = ?')
    .get(params.planId, required(params.agentId, 'agent id'));
  if (!member) throw new Error(`agent ${params.agentId} must join plan ${params.planId} before registering docs`);
  const relativePath = required(params.relativePath, 'document path').replace(/\\/g, '/');
  if (isAbsolute(relativePath)) throw new Error('plan document path must be relative to the plan folder');
  const planDir = resolve(plan.workspace_path, plan.doc_dir);
  const absolutePath = resolve(planDir, relativePath);
  const withinPlan = relative(planDir, absolutePath);
  if (!withinPlan || withinPlan === '..' || withinPlan.startsWith('../') || isAbsolute(withinPlan)) {
    throw new Error('plan document path must stay inside the plan folder');
  }
  if (!existsSync(absolutePath)) throw new Error(`plan document does not exist: ${relativePath}`);
  const nextOrdinal = (db.prepare(
    'SELECT COALESCE(MAX(ordinal), 0) + 1 AS ordinal FROM plan_docs WHERE plan_id = ?',
  ).get(params.planId) as { ordinal: number }).ordinal;
  db.prepare(`INSERT INTO plan_docs(plan_id, relative_path, title, kind, ordinal)
    VALUES (?, ?, ?, 'SUPPORTING', ?)
    ON CONFLICT(plan_id, relative_path) DO UPDATE SET title = excluded.title`)
    .run(params.planId, relativePath, required(params.title, 'document title'), nextOrdinal);
  return db.prepare(
    'SELECT relative_path, title, kind, ordinal FROM plan_docs WHERE plan_id = ? AND relative_path = ?',
  ).get(params.planId, relativePath) as unknown as PlanDocRecord;
}

export function updatePlanStatus(
  db: DatabaseSync,
  params: { planId: string; status: PlanStatus; agentId: string },
): PlanRecord {
  const plan = getPlan(db, params.planId);
  if (!plan) throw new Error(`plan not found: ${params.planId}`);
  if (plan.lead_agent_id !== params.agentId) {
    throw new Error(`only lead agent ${plan.lead_agent_id} can change plan status`);
  }
  const now = utcNow();
  db.exec('BEGIN IMMEDIATE');
  try {
    if (params.status === 'COMPLETED') {
      const unfinished = db.prepare(`SELECT COUNT(*) AS count FROM tasks
        WHERE plan_id = ? AND status NOT IN ('DONE', 'CANCELLED')`)
        .get(params.planId) as { count: number };
      if (unfinished.count > 0) {
        throw new Error(`cannot complete plan ${params.planId} with ${unfinished.count} unfinished task(s)`);
      }
    }
    if (params.status === 'CANCELLED') {
      const active = db.prepare(`SELECT COUNT(*) AS count FROM task_claims c
        JOIN tasks t ON t.task_id = c.task_id
        WHERE t.plan_id = ? AND c.expires_at > ?`)
        .get(params.planId, now) as { count: number };
      if (active.count > 0) {
        throw new Error(`cannot cancel plan ${params.planId} with ${active.count} active task run(s)`);
      }
      db.prepare(`UPDATE tasks SET status = 'CANCELLED', completed_at = ?, updated_at = ?
        WHERE plan_id = ? AND status IN ('OPEN', 'BLOCKED', 'VERIFY')`)
        .run(now, now, params.planId);
    }
    db.prepare('UPDATE plans SET status = ?, updated_at = ? WHERE plan_id = ?')
      .run(params.status, now, params.planId);
    db.exec('COMMIT');
  } catch (error) {
    try { db.exec('ROLLBACK'); } catch { /* transaction did not open */ }
    throw error;
  }
  return getPlan(db, params.planId)!;
}
