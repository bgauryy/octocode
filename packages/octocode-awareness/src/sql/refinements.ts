// src/sql/refinements.ts — SQL constants for refinements table

/** Full column list for SELECT projections. */
const COLS =
  'refinement_id, agent_id, workspace_path, artifact, repo, ref, ' +
  'files_json, reasoning, remember, quality, state, created_at, updated_at';

export const REFINEMENTS_INSERT =
  `INSERT INTO refinements (
     refinement_id, agent_id, workspace_path, artifact, repo, ref,
     files_json, reasoning, remember, quality, state, created_at, updated_at
   ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`;

/** Select all open/ongoing refinements (repo-fix queue), excluding session handoffs by default.
 *  Caller appends optional scope clauses (repo / workspace_path) + LIMIT before executing. */
export const REFINEMENTS_SELECT_OPEN =
  `SELECT ${COLS} FROM refinements
   WHERE state IN ('open','ongoing') AND quality NOT IN ('handoff','instructions')
   ORDER BY CASE state WHEN 'ongoing' THEN 0 ELSE 1 END, updated_at DESC`;

/** Select refinements scoped to a workspace path (also matches unscoped rows).
 *  Caller supplies workspace_path as the bind parameter. */
export const REFINEMENTS_SELECT_BY_WORKSPACE =
  `SELECT ${COLS} FROM refinements
   WHERE (workspace_path = ? OR workspace_path IS NULL)
   ORDER BY CASE state WHEN 'ongoing' THEN 0 ELSE 1 END, updated_at DESC`;

/** Advance a refinement through its lifecycle (open → ongoing → done).
 *  Binds: state, updated_at, refinement_id. */
export const REFINEMENTS_UPDATE_STATE =
  `UPDATE refinements SET state = ?, updated_at = ? WHERE refinement_id = ?`;

/** Delete refinements by id. Caller builds the IN (?,…) placeholder list dynamically. */
export const REFINEMENTS_DELETE =
  `DELETE FROM refinements WHERE refinement_id IN `;

// ─── Count / digest fragments ──────────────────────────────────────────────────

/** Count open/ongoing non-handoff refinements for a workspace scope.
 *  Caller appends AND (repo = ? OR …) / AND (workspace_path = ? OR …) as needed. */
export const REFINEMENTS_COUNT_OPEN =
  `SELECT COUNT(*) AS c FROM refinements
   WHERE state IN ('open','ongoing') AND quality NOT IN ('handoff','instructions')`;

/** Hard-delete old session handoffs and completed refinements (used by digest).
 *  Binds: handoff_updated_at_cutoff, done_updated_at_cutoff. */
export const REFINEMENTS_DELETE_STALE =
  `DELETE FROM refinements
   WHERE (quality = 'handoff' AND state = 'done' AND updated_at < ?)
      OR (quality IN ('good','bad') AND state = 'done' AND updated_at < ?)`;

/** Dry-run counterpart of REFINEMENTS_DELETE_STALE — counts without deleting.
 *  Binds: handoff_updated_at_cutoff, done_updated_at_cutoff. */
export const REFINEMENTS_COUNT_STALE =
  `SELECT COUNT(*) AS c FROM refinements
   WHERE (quality = 'handoff' AND state = 'done' AND updated_at < ?)
      OR (quality IN ('good','bad') AND state = 'done' AND updated_at < ?)`;
