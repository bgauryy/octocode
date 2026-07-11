/**
 * sessions.ts — Session CRUD operations against the sessions table.
 * Requires Node >=22.13.0 (unflagged node:sqlite built-in).
 */

import type { DatabaseSync } from 'node:sqlite';
import { randomUUID } from 'node:crypto';
import { normalizeArtifact, utcNow } from './helpers.js';
import { normalizeWorkspacePath } from './git.js';
import type { InsertSessionParams, EndSessionParams, SessionRow } from './types.js';
import {
  SESSIONS_INSERT,
  SESSIONS_SELECT_BY_ID,
  SESSIONS_SELECT_ACTIVE,
  SESSIONS_LIST_SELECT,
  SESSIONS_LIST_ORDER,
  SESSIONS_LIST_CLAUSE_AGENT_ID,
  SESSIONS_LIST_CLAUSE_WORKSPACE_PATH,
  SESSIONS_LIST_CLAUSE_ARTIFACT,
  SESSIONS_LIST_CLAUSE_ACTIVE,
} from './sql/sessions.js';

// ─── Scope ────────────────────────────────────────────────────────────────────

/**
 * Normalize to the same git-root + symlink-canonicalized scope key used by
 * memory/lock/signal, so session rows meet those tables under the same
 * workspace_path instead of a raw, possibly symlinked/pre-git-init string.
 */
function scopedWorkspacePath(workspacePath?: string | null): string | null {
  return workspacePath ? normalizeWorkspacePath(workspacePath, workspacePath) : null;
}

/**
 * Resolve a caller-supplied session id for a run without allowing an existing
 * row to silently drift across agents or scopes. Callers already hold the run
 * creation transaction, so the lookup/insert is serialized with that run.
 */
export function ensureRunSession(
  db: DatabaseSync,
  params: { sessionId: string; agentId: string; workspacePath: string; artifact?: string | null },
): SessionRow {
  const sessionId = params.sessionId.trim();
  if (!sessionId) throw new Error('session id is required');
  const workspacePath = scopedWorkspacePath(params.workspacePath);
  const artifact = normalizeArtifact(params.artifact);
  const existing = getSession(db, sessionId);

  if (existing) {
    if (existing.agent_id !== params.agentId) {
      throw new Error(`session ${sessionId} belongs to agent ${existing.agent_id}`);
    }
    if (existing.workspace_path !== workspacePath) {
      throw new Error(`session ${sessionId} belongs to workspace ${existing.workspace_path ?? '(none)'}`);
    }
    if (existing.artifact !== artifact) {
      throw new Error(`session ${sessionId} belongs to artifact ${existing.artifact ?? '(none)'}`);
    }
    if (existing.ended_at != null) {
      throw new Error(`session ${sessionId} has already ended`);
    }
    return existing;
  }

  const now = utcNow();
  db.prepare(SESSIONS_INSERT).run(
    sessionId,
    params.agentId,
    workspacePath,
    artifact,
    null,
    null,
    now,
  );
  return getSession(db, sessionId)!;
}

// ─── Insert ───────────────────────────────────────────────────────────────────

/** Create a new session. Returns the full SessionRow. */
export function insertSession(db: DatabaseSync, params: InsertSessionParams): SessionRow {
  const sessionId = 'sess_' + randomUUID();
  const now = utcNow();
  const artifact = normalizeArtifact(params.artifact);
  const workspacePath = scopedWorkspacePath(params.workspacePath);

  db.prepare(SESSIONS_INSERT).run(
    sessionId,
    params.agentId,
    workspacePath,
    artifact,
    params.repo ?? null,
    params.ref ?? null,
    now,
  );

  return {
    session_id: sessionId,
    agent_id: params.agentId,
    workspace_path: workspacePath,
    artifact,
    repo: params.repo ?? null,
    ref: params.ref ?? null,
    started_at: now,
    ended_at: null,
    summary: null,
  };
}

// ─── End ──────────────────────────────────────────────────────────────────────

/** End an active session (sets ended_at + optional summary). Returns the updated row, or null if not found. */
export function endSession(db: DatabaseSync, params: EndSessionParams): SessionRow | null {
  const now = utcNow();
  const where = ['session_id = ?', 'agent_id = ?', 'ended_at IS NULL'];
  const binds: Array<string | null> = [params.sessionId, params.agentId];
  if (params.workspacePath !== undefined) {
    where.push('workspace_path IS ?');
    binds.push(scopedWorkspacePath(params.workspacePath));
  }
  if (params.artifact !== undefined) {
    where.push('artifact IS ?');
    binds.push(normalizeArtifact(params.artifact));
  }
  const result = db.prepare(
    `UPDATE sessions SET ended_at = ?, summary = ? WHERE ${where.join(' AND ')} RETURNING *`,
  ).get(now, params.summary ?? null, ...binds) as SessionRow | undefined;
  return result ?? null;
}

// ─── Get ──────────────────────────────────────────────────────────────────────

/** Get a session by ID. Returns null if not found. */
export function getSession(db: DatabaseSync, sessionId: string): SessionRow | null {
  const row = db.prepare(SESSIONS_SELECT_BY_ID).get(sessionId) as SessionRow | undefined;
  return row ?? null;
}

// ─── List ─────────────────────────────────────────────────────────────────────

/** List sessions filtered by agent or workspace. */
export function listSessions(
  db: DatabaseSync,
  params: { agentId?: string; workspacePath?: string; artifact?: string | null; limit?: number; active?: boolean } = {}
): SessionRow[] {
  const clauses: string[] = [];
  const args: (string | number | null)[] = [];

  if (params.agentId !== undefined) {
    clauses.push(SESSIONS_LIST_CLAUSE_AGENT_ID);
    args.push(params.agentId);
  }

  if (params.workspacePath !== undefined) {
    clauses.push(SESSIONS_LIST_CLAUSE_WORKSPACE_PATH);
    args.push(scopedWorkspacePath(params.workspacePath));
  }

  const artifact = normalizeArtifact(params.artifact);
  if (artifact !== null) {
    clauses.push(SESSIONS_LIST_CLAUSE_ARTIFACT);
    args.push(artifact);
  }

  if (params.active === true) {
    clauses.push(SESSIONS_LIST_CLAUSE_ACTIVE);
  }

  const where = clauses.length > 0 ? `WHERE ${clauses.join(' AND ')}` : '';
  const requestedLimit = params.limit == null || !Number.isFinite(params.limit)
    ? 100
    : Math.floor(params.limit);
  const limit = Math.min(100, Math.max(1, requestedLimit));

  return db.prepare(
    `${SESSIONS_LIST_SELECT} ${where} ${SESSIONS_LIST_ORDER} LIMIT ?`
  ).all(...args, limit) as unknown as SessionRow[];
}

// ─── Get or create ────────────────────────────────────────────────────────────

/**
 * Get or create a session for an agent+workspace.
 * Returns existing active session_id if one exists, else creates new.
 */
export function getOrCreateSession(
  db: DatabaseSync,
  params: InsertSessionParams
): string {
  const existing = db.prepare(SESSIONS_SELECT_ACTIVE).get(
    params.agentId,
    scopedWorkspacePath(params.workspacePath),
    normalizeArtifact(params.artifact),
    normalizeArtifact(params.artifact),
  ) as { session_id: string } | undefined;

  if (existing) return existing.session_id;

  return insertSession(db, params).session_id;
}
