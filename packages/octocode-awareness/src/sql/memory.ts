// src/sql/memory.ts — SQL constants for memories, memory_refs, memories_fts tables

// ─── memories: INSERT ─────────────────────────────────────────────────────────

// Insert a new memory record with all columns
export const MEMORY_INSERT = `
  INSERT INTO memories (
    memory_id, agent_id, task_context, observation, importance,
    label, tags_json, workspace_path, artifact, repo, ref,
    file_tree_fingerprint, novelty_score, created_at, updated_at,
    last_accessed_at, access_count, failure_signature, valid_from, valid_to, decay_half_life_days
  ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?)
`;

// ─── memories: SELECT ─────────────────────────────────────────────────────────

// Select all columns for a single memory by primary key
export const MEMORY_SELECT_BY_ID = `
  SELECT * FROM memories WHERE memory_id = ?
`;

// FTS-assisted search: joins memories_fts, scores with BM25 column weights (task_context=10, observation=7, tags=2)
// Dynamic: caller appends AND-joined condition fragment for importance/state/label/tag filters
// Params: ftsQuery, ...filterParams, limit
export const MEMORY_FTS_SELECT = `
  SELECT m.*, ABS(bm25(memories_fts, 0, 10, 7, 2)) AS _bm25
  FROM memories m
  JOIN memories_fts ON memories_fts.memory_id = m.memory_id
  WHERE memories_fts MATCH ?
    AND {WHERE}
  ORDER BY _bm25 DESC
  LIMIT ?
`;

// Fallback search when FTS is unavailable; returns _bm25=0 for uniform score treatment
// Dynamic: {WHERE} is replaced with AND-joined filter conditions; params: ...filterParams, limit
export const MEMORY_FALLBACK_SELECT = `
  SELECT m.*, 0 AS _bm25
  FROM memories m
  WHERE {WHERE}
  ORDER BY m.importance DESC, m.created_at DESC
  LIMIT ?
`;

// Load specific memories by id list, filtered by importance and state (for reference-filter bypass)
// Dynamic: {ID_PLACEHOLDERS} = comma-joined ?s for ids; {STATE_PLACEHOLDERS} = comma-joined ?s for states
// Params: ...ids, minImportance, ...states
export const MEMORY_SELECT_BY_IDS_FILTERED = `
  SELECT m.*, 0 AS _bm25 FROM memories m
  WHERE m.memory_id IN ({ID_PLACEHOLDERS})
    AND m.importance >= ?
    AND m.state IN ({STATE_PLACEHOLDERS})
`;

// Collect memory_ids matching a dynamic WHERE clause (used by forgetMemory selector groups)
// Dynamic: {WHERE} is an OR-combined selector expression built at runtime
// Params: ...selectorBindParams
export const MEMORY_SELECT_IDS_WHERE = `
  SELECT memory_id FROM memories WHERE {WHERE}
`;

// ─── memories: UPDATE ─────────────────────────────────────────────────────────

// Increment access_count and update last_accessed_at for a set of recalled memories
// Dynamic: {PLACEHOLDERS} = comma-joined ?s for memory_ids; Params: now, ...memoryIds
export const MEMORY_BUMP_ACCESS = `
  UPDATE memories
  SET access_count = COALESCE(access_count, 0) + 1, last_accessed_at = ?
  WHERE memory_id IN ({PLACEHOLDERS})
`;

// Update last_accessed_at and access_count by memory_id (used by helpers)
// Params: last_accessed_at, access_count, memory_id
export const MEMORY_UPDATE_ACCESS = `
  UPDATE memories
  SET last_accessed_at = ?, access_count = ?
  WHERE memory_id = ?
`;

// Mark a memory as SUPERSEDED, recording which memory replaced it and clamping valid_to
// Params: superseded_by, updated_at, valid_from (for COALESCE), expired_at, memory_id, new_memory_id
export const MEMORY_SUPERSEDE_UPDATE = `
  UPDATE memories
  SET state = 'SUPERSEDED', superseded_by = ?, updated_at = ?,
      valid_to = COALESCE(valid_to, ?), expired_at = ?
  WHERE memory_id = ? AND memory_id <> ?
`;

// Persist a dense embedding BLOB and its model identifier onto an existing memory row
// Params: embedding (BLOB), embedding_model, updated_at, memory_id
export const MEMORY_EMBEDDING_UPDATE = `
  UPDATE memories SET embedding = ?, embedding_model = ?, updated_at = ?
  WHERE memory_id = ?
`;

// ─── memories: DELETE ─────────────────────────────────────────────────────────

// Hard-delete memories by id list
// Dynamic: {PLACEHOLDERS} = comma-joined ?s; Params: ...ids
export const MEMORY_DELETE_BY_IDS = `
  DELETE FROM memories WHERE memory_id IN ({PLACEHOLDERS})
`;

// ─── memories: embedding search ───────────────────────────────────────────────

// Fetch stored embeddings for in-process cosine ranking; capped at 2000 to bound heap usage
// Dynamic: {WHERE} = AND-joined conditions for state/model filter; Params: ...binds
export const MEMORY_EMBEDDING_SELECT = `
  SELECT memory_id, embedding, embedding_model FROM memories
  WHERE {WHERE}
  ORDER BY COALESCE(last_accessed_at, created_at) DESC
  LIMIT 2000
`;

// ─── memories: weakness mining ────────────────────────────────────────────────

// Cluster active memories by failure_signature; score = count × avg_importance
// Dynamic: {WHERE} = AND-joined workspace/agent conditions; Params: ...filterBinds, minCount, fetchLimit
export const MEMORY_MINE_WEAKNESS_SELECT = `
  SELECT failure_signature,
         count(*) AS freq,
         avg(importance) AS avg_imp,
         count(*) * avg(importance) AS score,
         group_concat(memory_id, ',') AS ids,
         group_concat(DISTINCT label) AS labels
  FROM memories
  WHERE {WHERE}
  GROUP BY failure_signature
  HAVING freq >= ?
  ORDER BY score DESC
  LIMIT ?
`;

// Fetch the highest-importance observation per failure_signature for cluster representatives
// Dynamic: {PLACEHOLDERS} = comma-joined ?s for raw signatures; Params: ...rawSigs
export const MEMORY_MINE_WEAKNESS_REPS_SELECT = `
  SELECT failure_signature, observation, max(importance)
  FROM memories
  WHERE failure_signature IN ({PLACEHOLDERS}) AND state = 'ACTIVE'
  GROUP BY failure_signature
`;

// Count total distinct failure signatures and total flagged memories in the active store
export const MEMORY_MINE_WEAKNESS_TOTALS = `
  SELECT count(DISTINCT failure_signature) AS sigs, count(*) AS mems
  FROM memories WHERE failure_signature IS NOT NULL AND state = 'ACTIVE'
`;

// ─── memories_fts: INSERT / DELETE ────────────────────────────────────────────

// Insert the searchable text columns into the FTS5 virtual table
// Params: memory_id, task_context, observation, tags (space-joined terms string)
export const MEMORY_FTS_INSERT = `
  INSERT INTO memories_fts(memory_id, task_context, observation, tags) VALUES (?, ?, ?, ?)
`;

// Remove FTS rows for a set of deleted memories
// Dynamic: {PLACEHOLDERS} = comma-joined ?s; Params: ...ids
export const MEMORY_FTS_DELETE_BY_IDS = `
  DELETE FROM memories_fts WHERE memory_id IN ({PLACEHOLDERS})
`;

// ─── memory_refs: SELECT / DELETE ─────────────────────────────────────────────

// Look up all memory_ids that carry a specific reference value
// Params: reference
export const MEMORY_REFS_SELECT_BY_REF = `
  SELECT memory_id FROM memory_refs WHERE reference = ?
`;

// Remove reference index rows for a set of deleted memories
// Dynamic: {PLACEHOLDERS} = comma-joined ?s; Params: ...ids
export const MEMORY_REFS_DELETE_BY_IDS = `
  DELETE FROM memory_refs WHERE memory_id IN ({PLACEHOLDERS})
`;
