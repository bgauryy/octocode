// src/sql/signals.ts — SQL constants for signals and signal_reads tables

// ─── signals: lookup ──────────────────────────────────────────────────────────

export const SIGNALS_SELECT_PARENT = 'SELECT thread_id, workspace_path FROM signals WHERE signal_id = ?';

// ─── signals: insert ──────────────────────────────────────────────────────────

export const SIGNALS_INSERT = `INSERT INTO signals
   (signal_id, workspace_path, artifact, repo, ref, from_agent, to_agent, kind, subject, body,
    files_json, refs_json, thread_id, reply_to, importance, status, created_at)
   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'open', ?)`;

// ─── signals: select (inbox / thread) ────────────────────────────────────────

/** Base SELECT columns; append JOIN fragment and WHERE/ORDER/LIMIT at call site. */
export const SIGNALS_SELECT_BASE = 'SELECT n.* FROM signals n';

/**
 * LEFT JOIN fragment for unread filtering.
 * Binds: (agent_id) — must be prepended before WHERE binds.
 */
export const SIGNALS_SELECT_LEFT_JOIN_READS = 'LEFT JOIN signal_reads nr ON nr.signal_id = n.signal_id AND nr.agent_id = ?';

/** Appended to ORDER BY block for all inbox/thread queries. */
export const SIGNALS_SELECT_ORDER_LIMIT = 'ORDER BY n.created_at DESC LIMIT ?';

// Note: signal resolve/ack updates (by-ids, by-thread, thread-open-for-agent)
// are written inline in src/notifications-inbox.ts, not shared here — keep it that way
// so there is one source of truth per query.

// ─── signals: delete ─────────────────────────────────────────────────────────

/**
 * Delete signals by IDs.
 * Caller must interpolate IN-list placeholders: `IN (${ph})`.
 * Binds: (...signal_ids)
 */
export const SIGNALS_DELETE_BY_IDS = (ph: string): string => `DELETE FROM signals WHERE signal_id IN (${ph})`;

// ─── signal_reads: insert ─────────────────────────────────────────────────────

/**
 * Mark a signal as read by an agent (idempotent).
 * Binds: (signal_id, agent_id, read_at)
 */
export const SIGNAL_READS_INSERT_IGNORE = 'INSERT OR IGNORE INTO signal_reads(signal_id, agent_id, read_at) VALUES (?, ?, ?)';
