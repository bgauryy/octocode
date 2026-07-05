/**
 * refinements.ts — Refinement (repo-fix queue) operations.
 */

import { randomUUID } from 'node:crypto';
import type { DatabaseSync } from 'node:sqlite';
import { utcNow, parseJsonList } from './helpers.js';
import { fillScope } from './git.js';
import type {
  InsertRefinementParams, InsertRefinementResult,
  GetRefinementsParams, GetRefinementsResult,
  RefinementRow,
} from './types.js';

/**
 * Insert a new refinement record.
 * Returns { refinementId, refinement } — does NOT emit JSON.
 */
export function insertRefinement(
  db: DatabaseSync,
  params: InsertRefinementParams,
): InsertRefinementResult {
  const {
    agentId = 'agent',
    reasoning,
    remember,
    quality = 'good',
    state = 'open',
    workspacePath,
    repo: repoArg,
    ref: refArg,
    files = [],
    cwd,
  } = params;

  const refinementId = 'ref_' + randomUUID().replace(/-/g, '');
  const now = utcNow();
  const scope = fillScope(
    { workspace_path: workspacePath ?? null, repo: repoArg ?? null, ref: refArg ?? null },
    cwd ?? process.cwd()
  );

  db.prepare(`
    INSERT INTO refinements (
      refinement_id, agent_id, workspace_path, repo, ref,
      files_json, reasoning, remember, quality, state, created_at, updated_at
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
  `).run(
    refinementId, agentId,
    scope.workspace_path ?? process.cwd(),
    scope.repo ?? null,
    scope.ref ?? null,
    JSON.stringify(files),
    reasoning, remember, quality, state, now, now
  );

  return {
    refinementId,
    refinement: {
      refinement_id: refinementId,
      agent_id: agentId,
      workspace_path: scope.workspace_path ?? process.cwd(),
      repo: scope.repo,
      ref: scope.ref,
      files,
      reasoning,
      remember,
      quality,
      state,
      created_at: now,
      updated_at: now,
    },
  };
}

/**
 * Query open/ongoing refinements for a workspace/repo.
 */
export function getRefinements(
  db: DatabaseSync,
  params: GetRefinementsParams = {},
): GetRefinementsResult {
  const {
    workspacePath,
    repo: repoArg,
    quality,
    states: statesRaw,
    limit: limitRaw = 10,
    cwd,
  } = params;

  const limit = Math.min(50, Math.max(1, Number(limitRaw) || 10));
  const states = statesRaw ?? ['open', 'ongoing'];

  const scope = fillScope(
    { workspace_path: workspacePath ?? null, repo: repoArg ?? null },
    cwd ?? process.cwd()
  );

  const queryParams: (string | number)[] = [...states];
  const stateFilter = `state IN (${states.map(() => '?').join(',')})`;
  let sql = `SELECT * FROM refinements WHERE ${stateFilter}`;

  if (quality) {
    sql += ' AND quality = ?';
    queryParams.push(quality);
  }

  if (scope.repo) {
    sql += ' AND (repo = ? OR repo IS NULL)';
    queryParams.push(scope.repo);
  } else if (scope.workspace_path) {
    sql += ' AND (workspace_path = ? OR workspace_path IS NULL)';
    queryParams.push(scope.workspace_path);
  }

  sql += ` ORDER BY CASE state WHEN 'ongoing' THEN 0 ELSE 1 END, updated_at DESC LIMIT ?`;
  queryParams.push(limit);

  const rows = db.prepare(sql).all(...queryParams) as unknown as RefinementRow[];
  const refinements = rows.map(r => ({
    refinement_id: r.refinement_id,
    agent_id: r.agent_id,
    workspace_path: r.workspace_path,
    repo: r.repo,
    ref: r.ref,
    files: parseJsonList(r.files_json),
    reasoning: r.reasoning,
    remember: r.remember,
    quality: r.quality as 'good' | 'bad',
    state: r.state as 'open' | 'ongoing' | 'done',
    created_at: r.created_at,
    updated_at: r.updated_at,
  }));

  return { count: refinements.length, refinements };
}

// ─── deleteRefinement ───────────────────────────────────────────────────────────────

export interface DeleteRefinementResult {
  deleted: number;
  dry_run?: true;
  would_delete?: number;
  refinement_ids: string[];
}

export function deleteRefinement(
  db: DatabaseSync,
  params: { refinementIds: string[]; workspacePath?: string; dryRun?: boolean },
): DeleteRefinementResult {
  const { refinementIds, workspacePath, dryRun = false } = params;

  if (refinementIds.length === 0) {
    return { deleted: 0, refinement_ids: [] };
  }

  const ph = refinementIds.map(() => '?').join(',');
  const where: string[] = [`refinement_id IN (${ph})`];
  const binds: (string | number)[] = [...refinementIds];

  if (workspacePath) {
    where.push('(workspace_path = ? OR workspace_path IS NULL)');
    binds.push(workspacePath);
  }

  const rows = db.prepare(
    `SELECT refinement_id FROM refinements WHERE ${where.join(' AND ')}`
  ).all(...binds) as unknown as Array<{ refinement_id: string }>;
  const ids = rows.map(r => r.refinement_id);

  if (dryRun) {
    return { deleted: 0, dry_run: true, would_delete: ids.length, refinement_ids: ids };
  }

  if (ids.length > 0) {
    const delPh = ids.map(() => '?').join(',');
    db.prepare(`DELETE FROM refinements WHERE refinement_id IN (${delPh})`).run(...ids);
  }

  return { deleted: ids.length, refinement_ids: ids };
}
