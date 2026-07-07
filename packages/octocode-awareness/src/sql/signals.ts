// src/sql/signals.ts — SQL constants for signals and signal_reads tables

// ─── signals: lookup ──────────────────────────────────────────────────────────

export const SIGNALS_SELECT_THREAD_ID =
  'SELECT thread_id FROM signals WHERE signal_id = ?';

// ─── signals: insert ──────────────────────────────────────────────────────────

export const SIGNALS_INSERT =
  `INSERT INTO signals
   (signal_id, workspace_path, artifact, repo, ref, from_agent, to_agent, kind, subject, body,
    files_json, refs_json, thread_id, reply_to, importance, status, created_at)
   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'open', ?)`;

// ─── signals: select (inbox / thread) ────────────────────────────────────────

/** Base SELECT columns; append JOIN fragment and WHERE/ORDER/LIMIT at call site. */
export const SIGNALS_SELECT_BASE =
  'SELECT n.* FROM signals n';

/**
 * LEFT JOIN fragment for unread filtering.
 * Binds: (agent_id) — must be prepended before WHERE binds.
 */
export const SIGNALS_SELECT_LEFT_JOIN_READS =
  'LEFT JOIN signal_reads nr ON nr.signal_id = n.signal_id AND nr.agent_id = ?';

/** Appended to ORDER BY block for all inbox/thread queries. */
export const SIGNALS_SELECT_ORDER_LIMIT =
  'ORDER BY n.created_at DESC LIMIT ?';

// ─── signals: update ─────────────────────────────────────────────────────────

/**
 * Resolve signals by IDs.
 * Caller must interpolate IN-list placeholders: `IN (${ph})`.
 * Binds: (resolved_at, ...signal_ids)
 */
export const SIGNALS_UPDATE_RESOLVED_BY_IDS = (ph: string): string =>
  `UPDATE signals SET status = 'resolved', resolved_at = ? WHERE signal_id IN (${ph}) AND status = 'open' RETURNING signal_id`;

/**
 * Resolve all open signals in a thread.
 * Binds: (resolved_at, thread_id)
 */
export const SIGNALS_UPDATE_RESOLVED_BY_THREAD =
  `UPDATE signals SET status = 'resolved', resolved_at = ? WHERE thread_id = ? AND status = 'open' RETURNING signal_id`;

// ─── signals: select for ack ──────────────────────────────────────────────────

/**
 * Fetch open signal IDs in a thread that are addressed to or broadcast to an agent.
 * Binds: (thread_id, agent_id)
 */
export const SIGNALS_SELECT_THREAD_OPEN_FOR_AGENT =
  `SELECT signal_id FROM signals
   WHERE thread_id = ? AND status = 'open' AND (to_agent IS NULL OR to_agent = ?)`;

// ─── signals: delete ─────────────────────────────────────────────────────────

/**
 * Delete signals by IDs.
 * Caller must interpolate IN-list placeholders: `IN (${ph})`.
 * Binds: (...signal_ids)
 */
export const SIGNALS_DELETE_BY_IDS = (ph: string): string =>
  `DELETE FROM signals WHERE signal_id IN (${ph})`;

/**
 * Base SELECT used to preview/collect IDs before deletion (prune).
 * Caller appends a dynamic WHERE clause.
 */
export const SIGNALS_SELECT_IDS_FOR_PRUNE =
  'SELECT signal_id FROM signals WHERE';

// ─── signal_reads: insert ─────────────────────────────────────────────────────

/**
 * Mark a signal as read by an agent (idempotent).
 * Binds: (signal_id, agent_id, read_at)
 */
export const SIGNAL_READS_INSERT_IGNORE =
  'INSERT OR IGNORE INTO signal_reads(signal_id, agent_id, read_at) VALUES (?, ?, ?)';

// ─── signal_reads: delete ─────────────────────────────────────────────────────

/**
 * Delete read receipts for signals by IDs (used during prune).
 * Caller must interpolate IN-list placeholders: `IN (${ph})`.
 * Binds: (...signal_ids)
 */
export const SIGNAL_READS_DELETE_BY_SIGNAL_IDS = (ph: string): string =>
  `DELETE FROM signal_reads WHERE signal_id IN (${ph})`;
