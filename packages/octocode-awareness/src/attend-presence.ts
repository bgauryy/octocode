import type { DatabaseSync } from 'node:sqlite';
import { resolve } from 'node:path';
import { normalizeWorkspacePath, repositoryWorkspacePaths } from './git.js';
import { assertKnownOptions } from './helpers.js';
import { attendAwareness } from './attend-query.js';
import type { AttendParams } from './attend-model.js';
import { workspaceChanges } from './workspace-changes.js';

export interface AttendWorkspaceParams extends AttendParams {
  details?: boolean;
  changes?: boolean;
  offset?: number;
}

/** The default lobby reads presence only. Detailed coordination is requested explicitly. */
export function attendWorkspace(db: DatabaseSync, params: AttendWorkspaceParams = {}) {
  assertKnownOptions(params, ['details', 'changes', 'offset', 'revision', 'runtimeObservation', 'agentId', 'workspacePath',
    'artifact', 'repo', 'ref', 'query', 'file', 'limit', 'compact', 'includeBodies', 'explainOrgan', 'cwd'], 'attendWorkspace');
  const { details, changes, offset = 0, ...inspection } = params;
  const files = Array.isArray(params.file) ? params.file : params.file ? [params.file] : [];
  if (changes) {
    if (details || params.query?.trim() || files.length || params.artifact || params.repo || params.ref || params.explainOrgan) {
      throw new Error('attend: changes cannot be combined with coordination detail filters');
    }
    return workspaceChanges(db, { workspacePath: params.workspacePath, cwd: params.cwd,
      agentId: params.agentId, limit: params.limit, offset, revision: params.revision, includeBodies: params.includeBodies });
  }
  if (details || params.query?.trim() || files.length || params.explainOrgan || params.revision || params.artifact || params.repo || params.ref || params.includeBodies) {
    if (offset !== 0) throw new Error('attend: offset is only available for the default peer briefing');
    return attendAwareness(db, inspection);
  }
  const limit = params.limit ?? 10;
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 50) throw new Error('attend: limit must be an integer from 1 to 50');
  if (!Number.isSafeInteger(offset) || offset < 0) throw new Error('attend: offset must be a non-negative integer');
  const cwd = resolve(params.cwd ?? process.cwd());
  const workspace = normalizeWorkspacePath(params.workspacePath ?? cwd, cwd) ?? resolve(params.workspacePath ?? cwd);
  const workspaces = repositoryWorkspacePaths(workspace);
  const rows = db.prepare(`SELECT agent_id, agent_name, role, status, last_seen_at, workspace_path,
    CASE WHEN json_type(metadata_json, '$.vendor') = 'text' THEN json_extract(metadata_json, '$.vendor') ELSE NULL END AS agent_vendor,
    CASE WHEN json_type(metadata_json, '$.host') = 'text' THEN json_extract(metadata_json, '$.host') ELSE NULL END AS agent_host
    FROM awareness_agents WHERE workspace_path IN (SELECT value FROM json_each(?)) AND status != 'LEFT'
    ORDER BY agent_id, workspace_path LIMIT ? OFFSET ?`).all(JSON.stringify(workspaces), limit + 1, offset);
  const partial = rows.length > limit;
  const peers = rows.slice(0, limit);
  return {
    ok: true, mode: 'presence' as const, workspace_path: workspace, self_id: params.agentId ?? null,
    repository_workspaces: workspaces,
    peers, count: peers.length, offset, partial, partialReasons: partial ? ['limit'] : [],
    ...(partial ? { next: { list: { operation: 'context.orient', params: {
      limit: Math.min(3, limit), offset: offset + peers.length,
    } } } } : {}),
    note: 'Registered workspace peers; last_seen_at is activity evidence, not proof of a live session. Hooks deliver messages. Inspect other capabilities only when needed.',
  };
}
