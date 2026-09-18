import type { DatabaseSync } from 'node:sqlite';
import { parseJsonList } from './helpers.js';
import { AwarenessQueryParams, AwarenessQueryRow, BindValue, limitOf, stringList } from './repo-model.js';
import { addExactScope, addNullableScope, addStateFilter, addTextFilter, repositoryScopeFromParams, scopeFromParams, workspaceArtifactScope } from './repo-scope.js';
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
  const agentId = params.agentId;
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
  const agentId = params.agentId;
  if (agentId) {
    where.push('t.agent_id = ?');
    binds.push(agentId);
  }
  const sqlWhere = where.length > 0 ? `WHERE ${where.join(' AND ')}` : '';
  const rows = db.prepare(
    `SELECT l.file_path, l.run_id, t.agent_id, t.rationale AS reason,
            l.acquired_at, l.expires_at, t.task_id, t.workspace_path, t.artifact, t.status
       FROM awareness_locks l
       JOIN task_runs t ON t.run_id = l.run_id
       ${sqlWhere}
      ORDER BY datetime(l.acquired_at) DESC
      LIMIT ?`
  ).all(...binds, limitOf(params.limit)) as unknown as Array<Record<string, string | null>>;

  return rows.map(row => ({
    path: String(row['file_path']),
    agent: String(row['agent_id']),
    state: row['status'] === 'PENDING' ? 'pending_verification' : 'locked',
    reason: String(row['reason'] ?? ''),
    run_id: String(row['run_id']),
    expires_at: row['expires_at'] ?? null,
    task_id: row['task_id'] ?? null,
    workspace_path: row['workspace_path'] ?? null,
    artifact: row['artifact'] ?? null,
    acquired_at: String(row['acquired_at']),
  }));
}

export function agentRows(db: DatabaseSync, params: AwarenessQueryParams): AwarenessQueryRow[] {
  const scope = repositoryScopeFromParams(params);
  const registeredWhere: string[] = [];
  const registeredBinds: BindValue[] = [];
  if (scope.workspacePaths.length > 0) {
    registeredWhere.push(`(a.workspace_path IN (${scope.workspacePaths.map(() => '?').join(',')}) OR a.workspace_path = '')`);
    registeredBinds.push(...scope.workspacePaths);
  }
  if (scope.artifact) {
    registeredWhere.push('(a.artifact = ? OR a.artifact IS NULL)');
    registeredBinds.push(scope.artifact);
  }
  const registeredSqlWhere = registeredWhere.length > 0 ? `WHERE ${registeredWhere.join(' AND ')}` : '';

  const signalWhere: string[] = [];
  const signalBinds: BindValue[] = [];
  addExactScope(signalWhere, signalBinds, scope, 's');
  const signalSqlWhere = signalWhere.length > 0 ? `WHERE ${signalWhere.join(' AND ')}` : '';
  const outerWhere: string[] = [];
  const binds: BindValue[] = [...registeredBinds, ...signalBinds];
  const query = params.query?.trim();
  if (query) {
    outerWhere.push(`INSTR(LOWER(
      COALESCE(c.agent_id, '') || ' ' || COALESCE(c.agent_name, '') || ' ' ||
      COALESCE(c.context, '') || ' ' || COALESCE(c.agent_vendor, '') || ' ' ||
      COALESCE(c.agent_host, '') || ' ' || COALESCE(c.provenance, '')
    ), LOWER(?)) > 0`);
    binds.push(query);
  }
  if (params.agentId) {
    outerWhere.push('c.agent_id = ?');
    binds.push(params.agentId);
  }
  const outerSqlWhere = outerWhere.length > 0 ? `WHERE ${outerWhere.join(' AND ')}` : '';
  const offset = params.offset == null || !Number.isFinite(params.offset)
    ? 0
    : Math.max(0, Math.floor(params.offset));
  return db.prepare(`WITH
    registered_candidates AS (
      SELECT a.agent_id, a.agent_name, a.workspace_path, a.artifact, a.context, a.status,
        a.registered_at, a.last_seen_at,
        CASE WHEN json_type(a.metadata_json, '$.vendor') = 'text'
          THEN json_extract(a.metadata_json, '$.vendor') ELSE NULL END AS agent_vendor,
        CASE WHEN json_type(a.metadata_json, '$.host') = 'text'
          THEN json_extract(a.metadata_json, '$.host') ELSE NULL END AS agent_host,
        ROW_NUMBER() OVER (
          PARTITION BY a.agent_id
          ORDER BY a.last_seen_at DESC, a.workspace_path, a.agent_id
        ) AS identity_rank
      FROM awareness_agents a
      ${registeredSqlWhere}
    ),
    registered AS (
      SELECT agent_id, agent_name, workspace_path, artifact, context, status,
        registered_at, last_seen_at, agent_vendor, agent_host, 'registered' AS provenance
      FROM registered_candidates
      WHERE identity_rank = 1
    ),
    scoped_signals AS (
      SELECT s.signal_id, s.from_agent, s.to_agent, s.workspace_path, s.artifact, s.created_at
      FROM signals s
      ${signalSqlWhere}
    ),
    observed_candidates AS (
      SELECT TRIM(s.from_agent) AS agent_id, s.workspace_path, s.artifact, s.created_at, s.signal_id
      FROM scoped_signals s
      WHERE LENGTH(TRIM(s.from_agent)) BETWEEN 1 AND 128
      UNION ALL
      SELECT TRIM(CAST(recipient.value AS TEXT)) AS agent_id,
        s.workspace_path, s.artifact, s.created_at, s.signal_id
      FROM scoped_signals s
      JOIN json_each(CASE
        WHEN json_valid(s.to_agent) AND json_type(s.to_agent) IN ('array', 'text') THEN s.to_agent
        WHEN SUBSTR(LTRIM(COALESCE(s.to_agent, '')), 1, 1) NOT IN ('[', '"')
          THEN json_array(s.to_agent)
        ELSE json('[]')
      END) AS recipient
      WHERE recipient.type = 'text'
        AND LENGTH(TRIM(CAST(recipient.value AS TEXT))) BETWEEN 1 AND 128
    ),
    observed_ranked AS (
      SELECT agent_id, workspace_path, artifact, created_at,
        ROW_NUMBER() OVER (
          PARTITION BY agent_id
          ORDER BY created_at DESC, signal_id DESC
        ) AS identity_rank
      FROM observed_candidates
    ),
    observed AS (
      SELECT o.agent_id, '' AS agent_name, o.workspace_path, o.artifact,
        NULL AS context, NULL AS status, NULL AS registered_at, o.created_at AS last_seen_at,
        NULL AS agent_vendor, NULL AS agent_host, 'observed' AS provenance
      FROM observed_ranked o
      WHERE o.identity_rank = 1
        AND NOT EXISTS (SELECT 1 FROM registered r WHERE r.agent_id = o.agent_id)
    ),
    combined AS (
      SELECT * FROM registered
      UNION ALL
      SELECT * FROM observed
    )
    SELECT c.agent_id, c.agent_name, c.status, c.agent_vendor, c.agent_host,
      c.workspace_path, c.artifact, c.context, c.registered_at, c.last_seen_at, c.provenance
    FROM combined c
    ${outerSqlWhere}
    ORDER BY c.last_seen_at DESC, c.agent_id COLLATE BINARY ASC
    LIMIT ? OFFSET ?`
  ).all(...binds, limitOf(params.limit), offset) as unknown as AwarenessQueryRow[];
}

/** Shared signal recipient visibility for list, workboard, profile, and snapshots. */
export function addSignalVisibility(where: string[], binds: BindValue[], params: AwarenessQueryParams): void {
  const agentId = params.agentId ?? params.recipientAgentId;
  if (!agentId) return;
  where.push('(from_agent = ? OR to_agent = ? OR to_agent IS NULL)');
  binds.push(agentId, agentId);
}

export function signalRows(db: DatabaseSync, params: AwarenessQueryParams): AwarenessQueryRow[] {
  const scope = repositoryScopeFromParams(params);
  const where: string[] = [];
  const binds: BindValue[] = [];
  addExactScope(where, binds, scope);
  addTextFilter(where, binds, params.query, ['subject', 'body', 'kind', 'files_json', 'refs_json', 'from_agent', 'to_agent']);
  addStateFilter(where, binds, stringList(params.state), 'status', state => state.toLowerCase());
  addSignalVisibility(where, binds, params);
  const since = params.since?.trim();
  if (since) {
    where.push('created_at >= ?');
    binds.push(since);
  }
  const includeBodies = Boolean(params.includeBodies);
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

/**
 * Feedback addressed to the human developer who authored the agent's operating
 * instructions. Developer-review-tagged memories are the canonical source.
 */
export function developerReviewRows(db: DatabaseSync, params: AwarenessQueryParams): AwarenessQueryRow[] {
  const scope = scopeFromParams(params);
  const limit = limitOf(params.limit, 60, 500);

  const rows: AwarenessQueryRow[] = [];
  const memWhere = ["state = 'ACTIVE'", `tags_json LIKE '%"developer-review"%'`];
  const memBinds: BindValue[] = [];
  addNullableScope(memWhere, memBinds, scope);
  addTextFilter(memWhere, memBinds, params.query, ['task_context', 'observation']);
  const memRows = db.prepare(
    `SELECT memory_id, agent_id, task_context, observation, importance, created_at, updated_at
       FROM awareness_memories
      WHERE ${memWhere.join(' AND ')}
      ORDER BY importance DESC, datetime(created_at) DESC
      LIMIT ?`
  ).all(...memBinds, limit) as unknown as Array<Record<string, string | number | null>>;
  for (const row of memRows) {
    const observation = String(row['observation'] ?? '');
    rows.push({
      source: 'memory',
      id: String(row['memory_id']),
      memory_id: String(row['memory_id']),
      state: 'recorded',
      feedback: observation,
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
