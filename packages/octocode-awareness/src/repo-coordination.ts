import type { DatabaseSync } from 'node:sqlite';
import { parseJsonList } from './helpers.js';
import { AwarenessQueryParams, AwarenessQueryRow, BindValue, limitOf, stringList } from './repo-model.js';
import { addExactScope, addNullableScope, addStateFilter, addTextFilter, scopeFromParams, workspaceArtifactScope } from './repo-scope.js';
import { summarize } from './repo-formats.js';

export function countPendingStandaloneRuns(db: DatabaseSync, params: AwarenessQueryParams): number {
  const scope = scopeFromParams(params);
  const where = ["tr.status = 'PENDING'", 'tr.task_id IS NULL'];
  const binds: BindValue[] = [];
  addExactScope(where, binds, workspaceArtifactScope(scope), 'tr');
  const query = params.query?.trim();
  if (query) {
    where.push(`(LOWER(COALESCE(tr.run_id, '') || ' ' || COALESCE(tr.rationale, '') || ' ' ||
      COALESCE(tr.test_plan, '') || ' ' || COALESCE(tr.context_ref, '') || ' ' || COALESCE(tr.agent_id, '')) LIKE LOWER(?)
      OR EXISTS (SELECT 1 FROM run_files rfq WHERE rfq.run_id = tr.run_id AND LOWER(rfq.file_path) LIKE LOWER(?)))`);
    binds.push(`%${query}%`, `%${query}%`);
  }
  const agentId = params.agentId ?? params.agent_id;
  if (agentId) { where.push('tr.agent_id = ?'); binds.push(agentId); }
  const since = params.since?.trim();
  if (since) { where.push('tr.created_at >= ?'); binds.push(since); }
  return (db.prepare(`SELECT COUNT(*) AS count FROM task_runs tr WHERE ${where.join(' AND ')}`)
    .get(...binds) as { count: number }).count;
}

export function lockRows(db: DatabaseSync, params: AwarenessQueryParams): AwarenessQueryRow[] {
  const scope = scopeFromParams(params);
  const where: string[] = [];
  const binds: BindValue[] = [];
  addExactScope(where, binds, workspaceArtifactScope(scope), 't');
  addTextFilter(where, binds, params.query, ['l.file_path', 't.agent_id', 't.rationale']);
  const agentId = params.agentId ?? params.agent_id;
  if (agentId) {
    where.push('t.agent_id = ?');
    binds.push(agentId);
  }
  const sqlWhere = where.length > 0 ? `WHERE ${where.join(' AND ')}` : '';
  const rows = db.prepare(
    `SELECT l.lock_id, l.file_path, l.run_id, t.agent_id, t.session_id, 'EXCLUSIVE' AS lock_type,
            l.acquired_at, l.expires_at, t.task_id, t.workspace_path, t.artifact, t.status
       FROM locks l
       JOIN task_runs t ON t.run_id = l.run_id
       ${sqlWhere}
      ORDER BY datetime(l.acquired_at) DESC
      LIMIT ?`
  ).all(...binds, limitOf(params.limit)) as unknown as Array<Record<string, string | null>>;

  return rows.map(row => ({
    lock_id: String(row['lock_id']),
    file_path: String(row['file_path']),
    run_id: String(row['run_id']),
    task_id: row['task_id'] ?? null,
    agent_id: String(row['agent_id']),
    lock_type: String(row['lock_type']),
    run_status: String(row['status']),
    acquired_at: String(row['acquired_at']),
    expires_at: row['expires_at'] ?? null,
    workspace_path: row['workspace_path'] ?? null,
    artifact: row['artifact'] ?? null,
  }));
}

export function agentRows(db: DatabaseSync, params: AwarenessQueryParams): AwarenessQueryRow[] {
  const scope = scopeFromParams(params);
  const where: string[] = [];
  const binds: BindValue[] = [];
  if (scope.workspacePaths.length > 0) {
    where.push(`(workspace_path IN (${scope.workspacePaths.map(() => '?').join(',')}) OR workspace_path IS NULL)`);
    binds.push(...scope.workspacePaths);
  }
  if (scope.artifact) {
    where.push('(artifact = ? OR artifact IS NULL)');
    binds.push(scope.artifact);
  }
  addTextFilter(where, binds, params.query, ['agent_id', 'agent_name', 'context']);
  const sqlWhere = where.length > 0 ? `WHERE ${where.join(' AND ')}` : '';
  const rows = db.prepare(
    `SELECT agent_id, agent_name, workspace_path, artifact, context, registered_at, last_seen_at
       FROM agents
       ${sqlWhere}
      ORDER BY datetime(last_seen_at) DESC
      LIMIT ?`
  ).all(...binds, limitOf(params.limit)) as unknown as AwarenessQueryRow[];
  return rows;
}

export function signalRows(db: DatabaseSync, params: AwarenessQueryParams): AwarenessQueryRow[] {
  const scope = scopeFromParams(params);
  const where: string[] = [];
  const binds: BindValue[] = [];
  addExactScope(where, binds, scope);
  addTextFilter(where, binds, params.query, ['subject', 'body', 'kind', 'files_json', 'refs_json', 'from_agent', 'to_agent']);
  addStateFilter(where, binds, stringList(params.state), 'status', state => state.toLowerCase());
  const agentId = params.agentId ?? params.agent_id;
  if (agentId) {
    where.push('(from_agent = ? OR to_agent = ? OR to_agent IS NULL)');
    binds.push(agentId, agentId);
  }
  const since = params.since?.trim();
  if (since) {
    where.push('created_at >= ?');
    binds.push(since);
  }
  const includeBodies = Boolean(params.includeBodies ?? params.include_bodies);
  const sqlWhere = where.length > 0 ? `WHERE ${where.join(' AND ')}` : '';
  const rows = db.prepare(
    `SELECT signal_id, workspace_path, artifact, repo, ref, from_agent, to_agent, kind,
            subject, body, files_json, refs_json, thread_id, reply_to, importance, status, created_at
       FROM signals
       ${sqlWhere}
      ORDER BY datetime(created_at) DESC
      LIMIT ?`
  ).all(...binds, limitOf(params.limit)) as unknown as Array<Record<string, string | number | null>>;

  return rows.map(row => ({
    signal_id: String(row['signal_id']),
    kind: String(row['kind']),
    status: String(row['status']),
    subject: String(row['subject']),
    body: includeBodies ? row['body'] as string | null : summarize(String(row['body'] ?? ''), 160),
    from_agent: String(row['from_agent']),
    to_agent: row['to_agent'] as string | null,
    files: parseJsonList(row['files_json']),
    refs: parseJsonList(row['refs_json']),
    thread_id: String(row['thread_id']),
    reply_to: row['reply_to'] as string | null,
    importance: Number(row['importance']),
    workspace_path: row['workspace_path'] as string | null,
    artifact: row['artifact'] as string | null,
    repo: row['repo'] as string | null,
    ref: row['ref'] as string | null,
    created_at: String(row['created_at']),
  }));
}

export function refinementRows(db: DatabaseSync, params: AwarenessQueryParams): AwarenessQueryRow[] {
  const scope = scopeFromParams(params);
  const where: string[] = [];
  const binds: BindValue[] = [];
  addExactScope(where, binds, scope);
  addTextFilter(where, binds, params.query, ['reasoning', 'remember', 'quality', 'state', 'files_json', 'agent_id']);
  addStateFilter(where, binds, stringList(params.state), 'state', state => state.toLowerCase());
  const since = params.since?.trim();
  if (since) {
    where.push('created_at >= ?');
    binds.push(since);
  }
  const sqlWhere = where.length > 0 ? `WHERE ${where.join(' AND ')}` : '';
  const rows = db.prepare(
    `SELECT refinement_id, agent_id, workspace_path, artifact, repo, ref, files_json,
            reasoning, remember, quality, state, created_at, updated_at
       FROM refinements
       ${sqlWhere}
      ORDER BY
        CASE state WHEN 'open' THEN 0 WHEN 'ongoing' THEN 1 ELSE 2 END,
        datetime(updated_at) DESC
      LIMIT ?`
  ).all(...binds, limitOf(params.limit)) as unknown as Array<Record<string, string | null>>;

  return rows.map(row => ({
    refinement_id: String(row['refinement_id']),
    agent_id: String(row['agent_id']),
    quality: String(row['quality']),
    state: String(row['state']),
    reasoning: String(row['reasoning']),
    remember: String(row['remember']),
    files: parseJsonList(row['files_json']),
    workspace_path: String(row['workspace_path']),
    artifact: row['artifact'] ?? null,
    repo: row['repo'] ?? null,
    ref: row['ref'] ?? null,
    created_at: String(row['created_at']),
    updated_at: String(row['updated_at']),
  }));
}

/** Pull the feedback clause out of a reflection narrative, if present. */
export function extractInstructionsFeedback(observation: string): string {
  const marker = 'instructions feedback:';
  const idx = observation.toLowerCase().indexOf(marker);
  if (idx === -1) return observation;
  const after = observation.slice(idx + marker.length);
  // The narrative joins clauses with ' | ' and closes reflection bodies with ')'.
  const end = after.search(/\s\|\s|\)\s*$/);
  return (end === -1 ? after : after.slice(0, end)).trim();
}

/**
 * Feedback addressed to the human developer who authored the agent's operating
 * instructions. Primary source is the tracked `instructions`-quality refinement queue
 * (open/ongoing/done lifecycle); developer-review-tagged memories that no refinement
 * already represents are folded in so manually-tagged or historical feedback still shows.
 */
export function developerReviewRows(db: DatabaseSync, params: AwarenessQueryParams): AwarenessQueryRow[] {
  const scope = scopeFromParams(params);
  const limit = limitOf(params.limit, 60, 500);

  const refWhere = ["quality = 'instructions'"];
  const refBinds: BindValue[] = [];
  addExactScope(refWhere, refBinds, scope);
  addTextFilter(refWhere, refBinds, params.query, ['reasoning', 'remember', 'files_json', 'agent_id']);
  addStateFilter(refWhere, refBinds, stringList(params.state), 'state', state => state.toLowerCase());
  const refRows = db.prepare(
    `SELECT refinement_id, agent_id, workspace_path, artifact, repo, ref, files_json,
            reasoning, remember, state, created_at, updated_at
       FROM refinements
      WHERE ${refWhere.join(' AND ')}
      ORDER BY CASE state WHEN 'open' THEN 0 WHEN 'ongoing' THEN 1 ELSE 2 END, datetime(updated_at) DESC
      LIMIT ?`
  ).all(...refBinds, limit) as unknown as Array<Record<string, string | null>>;

  const rows: AwarenessQueryRow[] = refRows.map(row => ({
    source: 'refinement',
    id: String(row['refinement_id']),
    refinement_id: String(row['refinement_id']),
    state: String(row['state']),
    feedback: String(row['remember']),
    context: String(row['reasoning']),
    files: parseJsonList(row['files_json']),
    agent_id: String(row['agent_id']),
    workspace_path: row['workspace_path'] ?? null,
    artifact: row['artifact'] ?? null,
    repo: row['repo'] ?? null,
    ref: row['ref'] ?? null,
    created_at: String(row['created_at']),
    updated_at: String(row['updated_at']),
  }));

  // Fold in developer-review memories a refinement doesn't already carry.
  const refTexts = refRows.map(row => String(row['remember'] ?? '').trim()).filter(Boolean);
  const memWhere = ["state = 'ACTIVE'", `tags_json LIKE '%"developer-review"%'`];
  const memBinds: BindValue[] = [];
  addNullableScope(memWhere, memBinds, scope);
  addTextFilter(memWhere, memBinds, params.query, ['task_context', 'observation']);
  const memRows = db.prepare(
    `SELECT memory_id, agent_id, task_context, observation, importance, created_at, updated_at
       FROM memories
      WHERE ${memWhere.join(' AND ')}
      ORDER BY importance DESC, datetime(created_at) DESC
      LIMIT ?`
  ).all(...memBinds, limit) as unknown as Array<Record<string, string | number | null>>;
  for (const row of memRows) {
    const observation = String(row['observation'] ?? '');
    if (refTexts.some(text => text && observation.includes(text))) continue;
    rows.push({
      source: 'memory',
      id: String(row['memory_id']),
      memory_id: String(row['memory_id']),
      state: 'recorded',
      feedback: extractInstructionsFeedback(observation),
      context: String(row['task_context'] ?? ''),
      importance: Number(row['importance'] ?? 0),
      files: [],
      agent_id: String(row['agent_id']),
      created_at: String(row['created_at']),
      updated_at: row['updated_at'] ?? null,
    });
  }

  return rows.slice(0, limit);
}
