import type { DatabaseSync } from 'node:sqlite';
import { parseJsonList } from './helpers.js';
import { AwarenessQueryParams, AwarenessQueryRow, BindValue, LESSON_LABELS, MemoryDbRow, limitOf, stringList } from './repo-model.js';
import { addExactScope, addLabelsFilter, addMemoryFileFilter, addNullableScope, addStateFilter, addTextFilter, referenceHealth, scopeFromParams, withReferences, workspaceArtifactScope } from './repo-scope.js';

export function memoryRows(
  db: DatabaseSync,
  params: AwarenessQueryParams,
  options: { gotchas?: boolean; lessons?: boolean } = {},
): AwarenessQueryRow[] {
  const scope = scopeFromParams(params);
  const where = ["state = 'ACTIVE'"];
  const binds: BindValue[] = [];
  addNullableScope(where, binds, scope);
  addTextFilter(where, binds, params.query, ['task_context', 'observation', 'label', 'tags_json', 'failure_signature']);
  addMemoryFileFilter(where, binds, params.file, scope);

  if (options.gotchas) {
    where.push("(label = 'GOTCHA' OR failure_signature IS NOT NULL)");
  } else if (options.lessons) {
    where.push(`label IN (${LESSON_LABELS.map(() => '?').join(',')})`);
    binds.push(...LESSON_LABELS);
  } else {
    addLabelsFilter(where, binds, stringList(params.label));
  }

  const since = params.since?.trim();
  if (since) {
    where.push('created_at >= ?');
    binds.push(since);
  }

  const limit = limitOf(params.limit);
  const rows = db.prepare(
    `SELECT memory_id, agent_id, task_context, observation, importance, state, label, tags_json,
            workspace_path, artifact, repo, ref, failure_signature, created_at, updated_at
       FROM memories
      WHERE ${where.join(' AND ')}
      ORDER BY importance DESC, datetime(created_at) DESC
      LIMIT ?`
  ).all(...binds, limit) as unknown as MemoryDbRow[];

  return withReferences(db, rows).map(row => {
    const references = row.references ?? [];
    return {
      memory_id: row.memory_id,
      label: row.label,
      importance: row.importance,
      task_context: row.task_context,
      observation: row.observation,
      tags: parseJsonList(row.tags_json),
      references,
      ...referenceHealth(references, scope.workspacePath),
      failure_signature: row.failure_signature,
      agent_id: row.agent_id,
      workspace_path: row.workspace_path,
      artifact: row.artifact,
      repo: row.repo,
      ref: row.ref,
      created_at: row.created_at,
      updated_at: row.updated_at,
    };
  });
}

export function planRows(db: DatabaseSync, params: AwarenessQueryParams): AwarenessQueryRow[] {
  const scope = scopeFromParams(params);
  const where: string[] = [];
  const binds: BindValue[] = [];
  addExactScope(where, binds, workspaceArtifactScope(scope));
  addTextFilter(where, binds, params.query, ['plan_id', 'name', 'objective', 'lead_agent_id', 'doc_dir']);
  addStateFilter(where, binds, stringList(params.state), 'status', state => state.toUpperCase());
  const since = params.since?.trim();
  if (since) {
    where.push('created_at >= ?');
    binds.push(since);
  }
  const sqlWhere = where.length > 0 ? `WHERE ${where.join(' AND ')}` : '';
  const rows = db.prepare(
    `SELECT plan_id, name, objective, lead_agent_id, status, workspace_path, artifact,
            doc_dir, created_at, updated_at,
            (SELECT COUNT(*) FROM plan_members pm WHERE pm.plan_id = plans.plan_id) AS member_count,
            (SELECT COUNT(*) FROM tasks t WHERE t.plan_id = plans.plan_id) AS task_count
       FROM plans
       ${sqlWhere}
      ORDER BY datetime(updated_at) DESC
      LIMIT ?`
  ).all(...binds, limitOf(params.limit)) as unknown as Array<Record<string, string | number | null>>;

  return rows.map(row => ({
    plan_id: String(row['plan_id']),
    name: String(row['name']),
    objective: String(row['objective']),
    lead_agent_id: String(row['lead_agent_id']),
    status: String(row['status']),
    doc_dir: String(row['doc_dir']),
    member_count: Number(row['member_count']),
    task_count: Number(row['task_count']),
    workspace_path: row['workspace_path'] ?? null,
    artifact: row['artifact'] ?? null,
    created_at: String(row['created_at']),
    updated_at: String(row['updated_at']),
  }));
}

export function taskRowWhere(params: AwarenessQueryParams): { where: string[]; binds: BindValue[] } {
  const scope = scopeFromParams(params);
  const where: string[] = [];
  const binds: BindValue[] = [];
  addExactScope(where, binds, workspaceArtifactScope(scope), 'p');
  addTextFilter(where, binds, params.query, ['t.task_id', 't.title', 't.reasoning', 't.acceptance_criteria', 't.created_by', 'p.name']);
  addStateFilter(where, binds, stringList(params.state), 't.status', state => state.toUpperCase());
  const agentId = params.agentId ?? params.agent_id;
  if (agentId) { where.push('(t.created_by = ? OR c.agent_id = ?)'); binds.push(agentId, agentId); }
  const since = params.since?.trim();
  if (since) { where.push('t.created_at >= ?'); binds.push(since); }
  return { where, binds };
}

export function taskRows(db: DatabaseSync, params: AwarenessQueryParams): AwarenessQueryRow[] {
  const { where, binds } = taskRowWhere(params);
  const sqlWhere = where.length > 0 ? `WHERE ${where.join(' AND ')}` : '';
  const rows = db.prepare(
    `SELECT t.*, p.name AS plan_name, p.status AS plan_status, p.workspace_path, p.artifact,
            COALESCE(c.agent_id, (
              SELECT tr.agent_id FROM task_runs tr
              WHERE tr.task_id = t.task_id AND tr.status = 'PENDING'
              ORDER BY datetime(tr.updated_at) DESC, tr.run_id DESC LIMIT 1
            )) AS claimed_by,
            COALESCE(c.run_id, (
              SELECT tr.run_id FROM task_runs tr
              WHERE tr.task_id = t.task_id AND tr.status = 'PENDING'
              ORDER BY datetime(tr.updated_at) DESC, tr.run_id DESC LIMIT 1
            )) AS run_id,
            c.expires_at AS claim_expires_at,
            COALESCE((SELECT json_group_array(tp.path) FROM task_paths tp WHERE tp.task_id = t.task_id), '[]') AS paths_json,
            COALESCE((SELECT json_group_array(td.depends_on_task_id) FROM task_dependencies td WHERE td.task_id = t.task_id), '[]') AS dependencies_json,
            CASE WHEN t.status = 'OPEN' AND p.status = 'ACTIVE'
              AND c.task_id IS NULL
              AND NOT EXISTS (
                SELECT 1 FROM task_dependencies td
                JOIN tasks dependency ON dependency.task_id = td.depends_on_task_id
                WHERE td.task_id = t.task_id AND dependency.status <> 'DONE'
              ) THEN 1 ELSE 0 END AS ready
       FROM tasks t
       JOIN plans p ON p.plan_id = t.plan_id
       LEFT JOIN task_claims c ON c.task_id = t.task_id AND c.expires_at > strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
       ${sqlWhere}
      ORDER BY t.priority DESC, datetime(t.created_at), t.task_id
      LIMIT ?`
  ).all(...binds, limitOf(params.limit)) as unknown as Array<Record<string, string | number | null>>;

  return rows.map(row => ({
    task_id: String(row['task_id']),
    plan_id: String(row['plan_id']),
    plan_name: String(row['plan_name']),
    plan_status: String(row['plan_status']),
    title: String(row['title']),
    reasoning: String(row['reasoning']),
    acceptance_criteria: String(row['acceptance_criteria']),
    status: String(row['status']),
    priority: Number(row['priority']),
    created_by: String(row['created_by']),
    paths: parseJsonList(row['paths_json']),
    dependencies: parseJsonList(row['dependencies_json']),
    ready: Number(row['ready']) === 1,
    claimed_by: row['claimed_by'] ?? null,
    run_id: row['run_id'] ?? null,
    claim_expires_at: row['claim_expires_at'] ?? null,
    workspace_path: row['workspace_path'] ?? null,
    artifact: row['artifact'] ?? null,
    created_at: String(row['created_at']),
    updated_at: String(row['updated_at']),
    completed_at: row['completed_at'] ?? null,
  }));
}

export function countTaskRows(db: DatabaseSync, params: AwarenessQueryParams, readyOnly = false): number {
  const { where, binds } = taskRowWhere(params);
  if (readyOnly) {
    where.push("t.status = 'OPEN'", "p.status = 'ACTIVE'", 'c.task_id IS NULL');
    where.push(`NOT EXISTS (
      SELECT 1 FROM task_dependencies td
      JOIN tasks dependency ON dependency.task_id = td.depends_on_task_id
      WHERE td.task_id = t.task_id AND dependency.status <> 'DONE'
    )`);
  }
  const sqlWhere = where.length > 0 ? `WHERE ${where.join(' AND ')}` : '';
  return (db.prepare(`SELECT COUNT(*) AS count
    FROM tasks t
    JOIN plans p ON p.plan_id = t.plan_id
    LEFT JOIN task_claims c ON c.task_id = t.task_id
      AND c.expires_at > strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    ${sqlWhere}`).get(...binds) as { count: number }).count;
}

export function runRows(db: DatabaseSync, params: AwarenessQueryParams): AwarenessQueryRow[] {
  const scope = scopeFromParams(params);
  const where: string[] = [];
  const binds: BindValue[] = [];
  addExactScope(where, binds, workspaceArtifactScope(scope), 'tr');
  const query = params.query?.trim();
  if (query) {
    where.push(`(LOWER(COALESCE(tr.run_id, '') || ' ' || COALESCE(tr.rationale, '') || ' ' ||
      COALESCE(tr.test_plan, '') || ' ' || COALESCE(tr.context_ref, '') || ' ' || COALESCE(tr.agent_id, '')) LIKE LOWER(?)
      OR EXISTS (SELECT 1 FROM run_files rfq WHERE rfq.run_id = tr.run_id AND LOWER(rfq.file_path) LIKE LOWER(?)))`);
    binds.push(`%${query}%`, `%${query}%`);
  }
  addStateFilter(where, binds, stringList(params.state), 'tr.status', state => state.toUpperCase());
  const agentId = params.agentId ?? params.agent_id;
  if (agentId) { where.push('tr.agent_id = ?'); binds.push(agentId); }
  const since = params.since?.trim();
  if (since) { where.push('tr.created_at >= ?'); binds.push(since); }
  const sqlWhere = where.length > 0 ? `WHERE ${where.join(' AND ')}` : '';
  const rows = db.prepare(
    `SELECT tr.run_id, tr.task_id, tr.agent_id, tr.session_id, tr.rationale, tr.test_plan,
            tr.context_ref, tr.status, tr.workspace_path, tr.artifact, tr.created_at, tr.updated_at,
            COALESCE((SELECT json_group_array(rf.file_path)
              FROM run_files rf WHERE rf.run_id = tr.run_id), '[]') AS files_json
       FROM task_runs tr ${sqlWhere}
      ORDER BY datetime(tr.created_at) DESC LIMIT ?`
  ).all(...binds, limitOf(params.limit)) as unknown as Array<Record<string, string | null>>;
  return rows.map(row => ({
    run_id: String(row['run_id']),
    task_id: row['task_id'] ?? null,
    agent_id: String(row['agent_id']),
    status: String(row['status']),
    rationale: String(row['rationale']),
    test_plan: String(row['test_plan']),
    context_ref: row['context_ref'] ?? null,
    files: parseJsonList(row['files_json']),
    workspace_path: row['workspace_path'] ?? null,
    artifact: row['artifact'] ?? null,
    created_at: String(row['created_at']),
    updated_at: String(row['updated_at']),
  }));
}
