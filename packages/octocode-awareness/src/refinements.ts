/**
 * refinements.ts — Refinement (repo-fix queue) operations.
 */

import { randomUUID } from 'node:crypto';
import type { DatabaseSync } from 'node:sqlite';
import { normalizeArtifact, utcNow, parseJsonList } from './helpers.js';
import { fillScope } from './git.js';
import { REFINEMENTS_INSERT, REFINEMENTS_DELETE } from './sql/refinements.js';
import type {
  InsertRefinementParams, InsertRefinementResult,
  GetRefinementsParams, GetRefinementsResult,
  RefinementRow, RefinementQuality,
} from './types.js';

export const REFINEMENT_QUALITIES: readonly RefinementQuality[] = ['good', 'bad', 'handoff', 'instructions'];

/** Validate before INSERT/UPDATE so callers get an allow-list message instead of a raw SQL CHECK failure. */
function assertRefinementQuality(quality: string): void {
  if (!REFINEMENT_QUALITIES.includes(quality as RefinementQuality)) {
    throw new Error(`invalid refinement quality "${quality}"; allowed: ${REFINEMENT_QUALITIES.join(', ')}`);
  }
}

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
    artifact,
    repo: repoArg,
    ref: refArg,
    files = [],
    cwd,
  } = params;

  if (state === 'done') {
    throw new Error('terminal refinement creation is not allowed; create open/ongoing, then close it with an actor and check receipt');
  }
  assertRefinementQuality(quality);

  const refinementId = 'ref_' + randomUUID().replace(/-/g, '');
  const now = utcNow();
  const scope = fillScope(
    { workspace_path: workspacePath ?? null, artifact: normalizeArtifact(artifact), repo: repoArg ?? null, ref: refArg ?? null },
    cwd ?? process.cwd()
  );
  const workspace = scope.workspace_path ?? process.cwd();
  const normalizedFiles = [...new Set(files)];
  const existing = db.prepare(`SELECT * FROM refinements
    WHERE workspace_path IS ? AND artifact IS ? AND repo IS ? AND ref IS ?
      AND quality = ? AND remember = ? AND files_json = ?
      AND state IN ('open', 'ongoing')
    ORDER BY updated_at DESC LIMIT 1`)
    .get(workspace, scope.artifact, scope.repo ?? null, scope.ref ?? null,
      quality, remember, JSON.stringify(normalizedFiles)) as unknown as RefinementRow | undefined;
  if (existing) {
    return {
      refinementId: existing.refinement_id,
      refinement: {
        refinement_id: existing.refinement_id,
        agent_id: existing.agent_id,
        workspace_path: existing.workspace_path,
        artifact: existing.artifact ?? null,
        repo: existing.repo,
        ref: existing.ref,
        files: parseJsonList(existing.files_json),
        reasoning: existing.reasoning,
        remember: existing.remember,
        quality: existing.quality as RefinementQuality,
        state: existing.state as 'open' | 'ongoing' | 'done',
        created_at: existing.created_at,
        updated_at: existing.updated_at,
      },
    };
  }

  db.prepare(REFINEMENTS_INSERT).run(
    refinementId, agentId,
    workspace,
    scope.artifact,
    scope.repo ?? null,
    scope.ref ?? null,
    JSON.stringify(normalizedFiles),
    reasoning, remember, quality, state, now, now
  );

  return {
    refinementId,
    refinement: {
      refinement_id: refinementId,
      agent_id: agentId,
      workspace_path: workspace,
      artifact: scope.artifact,
      repo: scope.repo,
      ref: scope.ref,
      files: normalizedFiles,
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
    artifact,
    repo: repoArg,
    ref: refArg,
    quality,
    includeHandoffs = false,
    states: statesRaw,
    limit: limitRaw = 10,
    cwd,
  } = params;

  const limit = Math.min(50, Math.max(1, Number(limitRaw) || 10));
  const states = statesRaw ?? ['open', 'ongoing'];

  const scope = fillScope(
    { workspace_path: workspacePath ?? null, artifact: normalizeArtifact(artifact), repo: repoArg ?? null, ref: refArg ?? null },
    cwd ?? process.cwd()
  );

  const queryParams: (string | number)[] = [...states];
  const stateFilter = `state IN (${states.map(() => '?').join(',')})`;
  let sql = `SELECT * FROM refinements WHERE ${stateFilter}`;

  if (quality) {
    sql += ' AND quality = ?';
    queryParams.push(quality);
  } else if (!includeHandoffs) {
    // Default coding queue excludes handoffs (surfaced via --include-handoffs)
    // and instructions-feedback (surfaced via `reflect developer-review`), which
    // are addressed to the human operator, not the next coding agent.
    sql += " AND quality NOT IN ('handoff', 'instructions')";
  }

  if (scope.workspace_path) {
    sql += ' AND (workspace_path = ? OR workspace_path IS NULL)';
    queryParams.push(scope.workspace_path);
  }
  if (scope.artifact) {
    sql += ' AND (artifact = ? OR artifact IS NULL)';
    queryParams.push(scope.artifact);
  }
  if (scope.repo) {
    sql += ' AND (repo = ? OR repo IS NULL)';
    queryParams.push(scope.repo);
  }
  if (scope.ref) {
    sql += ' AND (ref = ? OR ref IS NULL)';
    queryParams.push(scope.ref);
  }

  sql += ` ORDER BY CASE state WHEN 'ongoing' THEN 0 ELSE 1 END, updated_at DESC LIMIT ?`;
  queryParams.push(limit);

  // When the default queue hides handoff/instructions rows, report how many were
  // hidden so callers know to look via --include-handoffs / `reflect developer-review`.
  let handoffCount: number | undefined;
  let instructionsCount: number | undefined;
  if (!quality && !includeHandoffs) {
    const countParams: (string | number)[] = [...states];
    let scopeSql = '';
    if (scope.workspace_path) { scopeSql += ' AND (workspace_path = ? OR workspace_path IS NULL)'; countParams.push(scope.workspace_path); }
    if (scope.artifact) { scopeSql += ' AND (artifact = ? OR artifact IS NULL)'; countParams.push(scope.artifact); }
    if (scope.repo) { scopeSql += ' AND (repo = ? OR repo IS NULL)'; countParams.push(scope.repo); }
    if (scope.ref) { scopeSql += ' AND (ref = ? OR ref IS NULL)'; countParams.push(scope.ref); }
    const rows = db.prepare(
      `SELECT quality, COUNT(*) AS c FROM refinements
        WHERE ${stateFilter} AND quality IN ('handoff', 'instructions') ${scopeSql}
        GROUP BY quality`
    ).all(...countParams) as unknown as Array<{ quality: string; c: number }>;
    const byQuality = new Map(rows.map(r => [r.quality, Number(r.c)]));
    handoffCount = byQuality.get('handoff') ?? 0;
    instructionsCount = byQuality.get('instructions') ?? 0;
  }

  const rows = db.prepare(sql).all(...queryParams) as unknown as RefinementRow[];
  const refinements = rows.map(r => ({
    refinement_id: r.refinement_id,
    agent_id: r.agent_id,
    workspace_path: r.workspace_path,
    artifact: r.artifact ?? null,
    repo: r.repo,
    ref: r.ref,
    files: parseJsonList(r.files_json),
    reasoning: r.reasoning,
    remember: r.remember,
    quality: r.quality as RefinementQuality,
    state: r.state as 'open' | 'ongoing' | 'done',
    created_at: r.created_at,
    updated_at: r.updated_at,
  }));

  return {
    count: refinements.length,
    refinements,
    ...(handoffCount !== undefined ? { handoff_count: handoffCount } : {}),
    ...(instructionsCount !== undefined ? { instructions_count: instructionsCount } : {}),
  };
}

// ─── updateRefinement ─────────────────────────────────────────────────────────

export interface UpdateRefinementResult {
  updated: boolean;
  refinement: InsertRefinementResult['refinement'] | null;
}

/**
 * Partial update of an existing refinement — only changes passed fields.
 * This is how the open → ongoing → done lifecycle advances.
 */
export function updateRefinement(
  db: DatabaseSync,
  params: {
    refinementId: string;
    state?: 'open' | 'ongoing' | 'done';
    quality?: RefinementQuality;
    reasoning?: string;
    remember?: string;
    files?: string[];
    /** Actor applying a terminal transition. Required with checkReceipt for state=done. */
    actorAgentId?: string;
    /** Human-readable verification evidence for a terminal transition. */
    checkReceipt?: string;
  },
): UpdateRefinementResult {
  const { refinementId, state, quality, reasoning, remember, files, actorAgentId, checkReceipt } = params;
  if (quality !== undefined) assertRefinementQuality(quality);

  const existing = db.prepare('SELECT * FROM refinements WHERE refinement_id = ?')
    .get(refinementId) as unknown as RefinementRow | undefined;
  if (!existing) return { updated: false, refinement: null };

  let effectiveReasoning = reasoning;
  if (state === 'done') {
    const actor = actorAgentId?.trim() ?? '';
    const receipt = checkReceipt?.trim() ?? '';
    if (!actor || !receipt) {
      throw new Error('terminal refinement closure requires a non-empty actor and check receipt');
    }
    const baseReasoning = reasoning ?? existing.reasoning;
    effectiveReasoning = `${baseReasoning}\n\nClosure receipt (${utcNow()}) by ${actor}: ${receipt}`;
  }

  const sets: string[] = [];
  const binds: string[] = [];
  if (state !== undefined) { sets.push('state = ?'); binds.push(state); }
  if (quality !== undefined) { sets.push('quality = ?'); binds.push(quality); }
  if (effectiveReasoning !== undefined) { sets.push('reasoning = ?'); binds.push(effectiveReasoning); }
  if (remember !== undefined) { sets.push('remember = ?'); binds.push(remember); }
  if (files !== undefined) { sets.push('files_json = ?'); binds.push(JSON.stringify(files)); }
  if (sets.length === 0) throw new Error('updateRefinement: no fields to update');

  sets.push('updated_at = ?');
  binds.push(utcNow());

  db.prepare(
    `UPDATE refinements SET ${sets.join(', ')} WHERE refinement_id = ?`
  ).run(...binds, refinementId);

  const row = db.prepare('SELECT * FROM refinements WHERE refinement_id = ?')
    .get(refinementId) as unknown as RefinementRow;
  return {
    updated: true,
    refinement: {
      refinement_id: row.refinement_id,
      agent_id: row.agent_id,
      workspace_path: row.workspace_path,
      artifact: row.artifact ?? null,
      repo: row.repo,
      ref: row.ref,
      files: parseJsonList(row.files_json),
      reasoning: row.reasoning,
      remember: row.remember,
      quality: row.quality as RefinementQuality,
      state: row.state as 'open' | 'ongoing' | 'done',
      created_at: row.created_at,
      updated_at: row.updated_at,
    },
  };
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
  params: { refinementIds: string[]; workspacePath?: string; artifact?: string | null; dryRun?: boolean },
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
  const artifact = normalizeArtifact(params.artifact);
  if (artifact) {
    where.push('(artifact = ? OR artifact IS NULL)');
    binds.push(artifact);
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
    db.prepare(`${REFINEMENTS_DELETE}(${delPh})`).run(...ids);
  }

  return { deleted: ids.length, refinement_ids: ids };
}
