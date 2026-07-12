import { existsSync } from 'node:fs';
import { isAbsolute, relative, resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { AwarenessQueryParams, AwarenessQueryRow, BindValue, limitOf, utcNow } from './repo-model.js';
import { addExactScope, addNullableScope, addTextFilter, localPathFromReference, scopeFromParams, stripLocationSuffix, withScope, workspaceArtifactScope } from './repo-scope.js';
import { memoryRows, runRows, taskRows } from './repo-plans.js';
import { agentRows, developerReviewRows, lockRows, refinementRows, signalRows } from './repo-coordination.js';
import { summarize } from './repo-formats.js';

export function trackFile(
  map: Map<string, AwarenessQueryRow>,
  filePath: string,
  source: string,
  date: string | null | undefined,
  workspacePath: string | null,
): void {
  const resolved = localPathFromReference(filePath, workspacePath);
  const clean = resolved ?? stripLocationSuffix(filePath.startsWith('file:') ? filePath.slice('file:'.length) : filePath);
  if (!clean) return;
  const fileExists = existsSync(clean);
  const row = map.get(clean) ?? {
    file_path: clean,
    file_exists: fileExists,
    missing_file: !fileExists,
    memories: 0,
    gotchas: 0,
    tasks: 0,
    runs: 0,
    locks: 0,
    refinements: 0,
    signals: 0,
    edits: 0,
    last_seen_at: null,
  };
  row['file_exists'] = Boolean(row['file_exists']) || fileExists;
  row['missing_file'] = !Boolean(row['file_exists']);
  const current = Number(row[source] ?? 0);
  row[source] = current + 1;
  if (date && (!row['last_seen_at'] || String(date) > String(row['last_seen_at']))) row['last_seen_at'] = date;
  map.set(clean, row);
}

export function fileRows(db: DatabaseSync, params: AwarenessQueryParams): AwarenessQueryRow[] {
  const scope = scopeFromParams(params);
  const limit = limitOf(params.limit, 80, 500);
  const files = new Map<string, AwarenessQueryRow>();
  const requestedFile = params.file?.trim()
    ? (localPathFromReference(params.file, scope.workspacePath) ?? resolve(scope.workspacePath ?? process.cwd(), params.file))
    : null;

  const memoryWhere = ["m.state = 'ACTIVE'", "r.reference LIKE 'file:%'"];
  const memoryBinds: BindValue[] = [];
  addNullableScope(memoryWhere, memoryBinds, scope, 'm');
  addTextFilter(memoryWhere, memoryBinds, params.query, ['r.reference', 'm.task_context', 'm.observation']);
  const memoryRefs = db.prepare(
    `SELECT r.reference, m.label, m.created_at
       FROM memory_refs r
       JOIN memories m ON m.memory_id = r.memory_id
      WHERE ${memoryWhere.join(' AND ')}
      ORDER BY datetime(m.created_at) DESC
      LIMIT ?`
  ).all(...memoryBinds, 1000) as unknown as Array<{ reference: string; label: string; created_at: string }>;
  for (const ref of memoryRefs) {
    trackFile(files, ref.reference, 'memories', ref.created_at, scope.workspacePath);
    if (ref.label === 'GOTCHA') trackFile(files, ref.reference, 'gotchas', ref.created_at, scope.workspacePath);
  }

  for (const row of taskRows(db, withScope(params, { limit: 500 }))) {
    for (const file of row['paths'] as string[]) trackFile(files, file, 'tasks', String(row['created_at']), scope.workspacePath);
  }
  for (const row of runRows(db, withScope(params, { limit: 500 }))) {
    for (const file of row['files'] as string[]) trackFile(files, file, 'runs', String(row['created_at']), scope.workspacePath);
  }
  for (const row of lockRows(db, withScope(params, { limit: 500 }))) {
    trackFile(files, String(row['file_path']), 'locks', String(row['acquired_at']), scope.workspacePath);
  }
  for (const row of refinementRows(db, withScope(params, { limit: 500 }))) {
    for (const file of row['files'] as string[]) trackFile(files, file, 'refinements', String(row['updated_at']), scope.workspacePath);
  }
  for (const row of signalRows(db, withScope(params, { limit: 500 }))) {
    for (const file of row['files'] as string[]) trackFile(files, file, 'signals', String(row['created_at']), scope.workspacePath);
  }

  const editWhere: string[] = [];
  const editBinds: BindValue[] = [];
  addExactScope(editWhere, editBinds, workspaceArtifactScope(scope));
  addTextFilter(editWhere, editBinds, params.query, ['file_path', 'old_file_path', 'operation', 'agent_id']);
  const editSqlWhere = editWhere.length > 0 ? `WHERE ${editWhere.join(' AND ')}` : '';
  const edits = db.prepare(
    `SELECT file_path, old_file_path, operation, agent_id, created_at
       FROM edit_log
       ${editSqlWhere}
      ORDER BY datetime(created_at) DESC
      LIMIT ?`
  ).all(...editBinds, 1000) as unknown as Array<{ file_path: string; old_file_path: string | null; created_at: string }>;
  for (const edit of edits) {
    trackFile(files, edit.file_path, 'edits', edit.created_at, scope.workspacePath);
    if (edit.old_file_path) trackFile(files, edit.old_file_path, 'edits', edit.created_at, scope.workspacePath);
  }

  return [...files.values()]
    .filter(row => !requestedFile || row['file_path'] === requestedFile)
    .sort((a, b) => {
      const scoreA = (a['missing_file'] ? 20 : 0) + Number(a['locks'] ?? 0) * 10 + Number(a['gotchas'] ?? 0) * 6 + Number(a['memories'] ?? 0) * 4 + Number(a['tasks'] ?? 0) * 3 + Number(a['runs'] ?? 0) + Number(a['edits'] ?? 0);
      const scoreB = (b['missing_file'] ? 20 : 0) + Number(b['locks'] ?? 0) * 10 + Number(b['gotchas'] ?? 0) * 6 + Number(b['memories'] ?? 0) * 4 + Number(b['tasks'] ?? 0) * 3 + Number(b['runs'] ?? 0) + Number(b['edits'] ?? 0);
      return scoreB - scoreA || String(b['last_seen_at'] ?? '').localeCompare(String(a['last_seen_at'] ?? ''));
    })
    .slice(0, limit);
}

export function countWhere(db: DatabaseSync, table: string, where: string[], binds: BindValue[]): number {
  const sqlWhere = where.length > 0 ? `WHERE ${where.join(' AND ')}` : '';
  const row = db.prepare(`SELECT COUNT(*) AS count FROM ${table} ${sqlWhere}`).get(...binds) as { count: number };
  return row.count;
}

export function repoProfileRows(db: DatabaseSync, params: AwarenessQueryParams): AwarenessQueryRow[] {
  const scope = scopeFromParams(params);
  const memWhere = ["state = 'ACTIVE'"];
  const memBinds: BindValue[] = [];
  addNullableScope(memWhere, memBinds, scope);

  const taskWhere: string[] = [];
  const taskBinds: BindValue[] = [];
  addExactScope(taskWhere, taskBinds, workspaceArtifactScope(scope), 'p');

  const planWhere: string[] = [];
  const planBinds: BindValue[] = [];
  addExactScope(planWhere, planBinds, workspaceArtifactScope(scope));

  const runWhere: string[] = [];
  const runBinds: BindValue[] = [];
  addExactScope(runWhere, runBinds, workspaceArtifactScope(scope));

  const lockWhere: string[] = [];
  const lockBinds: BindValue[] = [];
  addExactScope(lockWhere, lockBinds, workspaceArtifactScope(scope), 't');
  lockWhere.push("t.status = 'ACTIVE'", '(l.expires_at IS NULL OR l.expires_at > ?)');
  lockBinds.push(utcNow());

  const allRefinementWhere = ["state IN ('open','ongoing')"];
  const refinementBinds: BindValue[] = [];
  addExactScope(allRefinementWhere, refinementBinds, scope);
  const actionableRefinementWhere = [...allRefinementWhere, "quality NOT IN ('handoff','instructions')"];

  const signalWhere = ["status = 'open'"];
  const signalBinds: BindValue[] = [];
  addExactScope(signalWhere, signalBinds, scope);

  const trackedFiles = fileRows(db, withScope(params, { limit: 500 }));
  return [
    { metric: 'active_memories', count: countWhere(db, 'memories', memWhere, memBinds) },
    { metric: 'gotchas', count: memoryRows(db, withScope(params, { view: 'gotchas', limit: 500 }), { gotchas: true }).length },
    { metric: 'lessons', count: memoryRows(db, withScope(params, { view: 'lessons', limit: 500 }), { lessons: true }).length },
    { metric: 'plans', count: countWhere(db, 'plans', planWhere, planBinds) },
    { metric: 'tasks', count: countWhere(db, 'tasks t JOIN plans p ON p.plan_id = t.plan_id', taskWhere, taskBinds) },
    { metric: 'runs', count: countWhere(db, 'task_runs', runWhere, runBinds) },
    { metric: 'active_locks', count: countWhere(db, 'locks l JOIN task_runs t ON t.run_id = l.run_id', lockWhere, lockBinds) },
    { metric: 'actionable_refinements', count: countWhere(db, 'refinements', actionableRefinementWhere, refinementBinds) },
    { metric: 'all_open_refinements', count: countWhere(db, 'refinements', allRefinementWhere, refinementBinds) },
    { metric: 'open_signals', count: countWhere(db, 'signals', signalWhere, signalBinds) },
    { metric: 'known_agents', count: agentRows(db, withScope(params, { limit: 500 })).length },
    { metric: 'tracked_files', count: trackedFiles.length },
    { metric: 'missing_file_refs', count: trackedFiles.filter(row => row['missing_file']).length },
    { metric: 'developer_review', count: developerReviewRows(db, withScope(params, { limit: 500 })).length },
  ];
}

export function activityRows(db: DatabaseSync, params: AwarenessQueryParams): AwarenessQueryRow[] {
  const limit = limitOf(params.limit);
  const rows: AwarenessQueryRow[] = [];
  for (const row of memoryRows(db, withScope(params, { limit }))) {
    rows.push({
      kind: 'memory',
      id: String(row['memory_id']),
      title: `${row['label']}: ${summarize(String(row['task_context']), 80)}`,
      detail: summarize(String(row['observation']), 180),
      agent_id: String(row['agent_id']),
      created_at: String(row['created_at']),
    });
  }
  for (const row of taskRows(db, withScope(params, { limit }))) {
    rows.push({
      kind: 'task',
      id: String(row['task_id']),
      title: `${row['status']}: ${summarize(String(row['title']), 100)}`,
      detail: summarize(String(row['reasoning']), 180),
      agent_id: String(row['claimed_by'] ?? row['created_by']),
      created_at: String(row['created_at']),
    });
  }
  for (const row of runRows(db, withScope(params, { limit }))) {
    rows.push({
      kind: 'run',
      id: String(row['run_id']),
      title: `${row['status']}: ${summarize(String(row['rationale']), 100)}`,
      detail: summarize(String(row['test_plan']), 180),
      agent_id: String(row['agent_id']),
      created_at: String(row['created_at']),
    });
  }
  for (const row of signalRows(db, withScope(params, { limit }))) {
    rows.push({
      kind: 'signal',
      id: String(row['signal_id']),
      title: `${row['kind']}: ${summarize(String(row['subject']), 100)}`,
      detail: summarize(String(row['body'] ?? ''), 180),
      agent_id: String(row['from_agent']),
      created_at: String(row['created_at']),
    });
  }
  for (const row of refinementRows(db, withScope(params, { limit }))) {
    rows.push({
      kind: 'refinement',
      id: String(row['refinement_id']),
      title: `${row['state']}: ${summarize(String(row['remember']), 100)}`,
      detail: summarize(String(row['reasoning']), 180),
      agent_id: String(row['agent_id']),
      created_at: String(row['updated_at']),
    });
  }
  return rows
    .sort((a, b) => String(b['created_at']).localeCompare(String(a['created_at'])))
    .slice(0, limit);
}

export function rowFiles(row: AwarenessQueryRow): string[] {
  const raw = row['files'] ?? row['paths'];
  return Array.isArray(raw) ? raw.map(String) : [];
}

export function displayPath(filePath: string, workspacePath: string | null): string {
  if (!workspacePath) return filePath.replace(/\\/g, '/');
  const rel = relative(workspacePath, filePath);
  return rel && rel !== '..' && !rel.startsWith(`..${process.platform === 'win32' ? '\\' : '/'}`) && !isAbsolute(rel)
    ? rel.replace(/\\/g, '/')
    : filePath.replace(/\\/g, '/');
}

export function filesUnderWorkRows(db: DatabaseSync, params: AwarenessQueryParams): AwarenessQueryRow[] {
  const scope = scopeFromParams(params);
  const limit = limitOf(params.limit, 80, 500);
  const rowLimit = Math.min(2000, Math.max(limit, limit * 4));
  const where = ["tr.status = 'ACTIVE'", 'rf.ended_at IS NULL', 'rf.expires_at > ?'];
  const binds: BindValue[] = [utcNow()];
  addExactScope(where, binds, workspaceArtifactScope(scope), 'tr');
  addTextFilter(where, binds, params.query, ['rf.file_path', 'tr.agent_id', 'tr.rationale', 'rf.reason_override', 't.title', 'p.name']);
  const agentId = params.agentId ?? params.agent_id;
  if (agentId) { where.push('tr.agent_id = ?'); binds.push(agentId); }
  if (params.file?.trim()) {
    const root = scope.workspacePath ?? process.cwd();
    const filePath = isAbsolute(params.file) ? resolve(params.file) : resolve(root, params.file);
    where.push('rf.file_path = ?');
    binds.push(filePath);
  }
  const rows = db.prepare(
    `SELECT rf.file_path, rf.run_id, tr.agent_id, tr.task_id,
            COALESCE(rf.reason_override, t.reasoning, tr.rationale) AS reason,
            t.plan_id, p.name AS plan_name, tr.workspace_path,
            CASE WHEN l.lock_id IS NULL THEN 0 ELSE 1 END AS locked,
            l.expires_at AS lock_expires_at
       FROM run_files rf
       JOIN task_runs tr ON tr.run_id = rf.run_id
       LEFT JOIN tasks t ON t.task_id = tr.task_id
       LEFT JOIN plans p ON p.plan_id = t.plan_id
       LEFT JOIN locks l ON l.run_id = rf.run_id AND l.file_path = rf.file_path
         AND (l.expires_at IS NULL OR l.expires_at > ?)
      WHERE ${where.join(' AND ')}
      ORDER BY rf.file_path, datetime(rf.heartbeat_at) DESC, tr.agent_id, rf.run_id
      LIMIT ?`
  ).all(utcNow(), ...binds, rowLimit) as unknown as Array<Record<string, string | number | null>>;

  const grouped = new Map<string, Array<Record<string, string | number | null>>>();
  for (const row of rows) {
    const path = String(row['file_path']);
    const peers = grouped.get(path) ?? [];
    peers.push(row);
    grouped.set(path, peers);
  }

  return [...grouped.entries()].slice(0, limit).map(([filePath, peers]) => {
    const shown = peers.slice(0, 3);
    const lock = peers.find(peer => Number(peer['locked']) === 1);
    const workspacePath = String(peers[0]?.['workspace_path'] ?? scope.workspacePath ?? '') || null;
    return {
      item_type: 'file',
      id: filePath,
      path: displayPath(filePath, workspacePath),
      peer_count: peers.length,
      agents: shown.map(peer => String(peer['agent_id'])),
      run_ids: shown.map(peer => String(peer['run_id'])),
      task_ids: shown.flatMap(peer => peer['task_id'] == null ? [] : [String(peer['task_id'])]),
      plan_ids: shown.flatMap(peer => peer['plan_id'] == null ? [] : [String(peer['plan_id'])]),
      plans: shown.flatMap(peer => peer['plan_name'] == null ? [] : [String(peer['plan_name'])]),
      reasons: shown.map(peer => summarize(String(peer['reason'] ?? ''), 80)),
      omitted_peer_count: Math.max(0, peers.length - shown.length),
      locked: Boolean(lock),
      lock_agent_id: lock == null ? null : String(lock['agent_id']),
      lock_expires_at: lock?.['lock_expires_at'] ?? null,
    };
  });
}

export function pushLimited(
  columns: Record<string, AwarenessQueryRow[]>,
  counts: Record<string, number>,
  column: string,
  row: AwarenessQueryRow,
  limit: number,
): void {
  counts[column] = (counts[column] ?? 0) + 1;
  const rows = columns[column] ?? [];
  if (rows.length < limit) {
    rows.push({ column, ...row });
    columns[column] = rows;
  }
}
