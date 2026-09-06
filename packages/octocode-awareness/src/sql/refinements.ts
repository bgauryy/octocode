// src/sql/refinements.ts — SQL constants for refinements table

export const REFINEMENTS_INSERT = `INSERT INTO refinements (
     refinement_id, agent_id, workspace_path, artifact, repo, ref,
     files_json, reasoning, remember, quality, state, created_at, updated_at
   ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`;

/** Delete refinements by id. Caller builds the IN (?,…) placeholder list dynamically. */
export const REFINEMENTS_DELETE = `DELETE FROM refinements WHERE refinement_id IN `;

// ─── Count / digest fragments ──────────────────────────────────────────────────
