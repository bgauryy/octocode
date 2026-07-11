/**
 * notifications.ts — Agent-to-agent workspace messaging.
 *
 * Mirrors Python awareness.py's notify / notify-get / notify-resolve / notify-prune.
 * Uses the `signals` + `signal_reads` tables in the schema.
 */
import { randomUUID } from 'node:crypto';
import type { DatabaseSync } from 'node:sqlite';
import { normalizeArtifact, normalizeNotificationKind, utcNow, parseJsonList } from './helpers.js';
import { fillScope } from './git.js';
import { SIGNALS_SELECT_THREAD_ID, SIGNALS_INSERT } from './sql/index.js';
import type { InsertNotificationParams, InsertNotificationResult, NotificationRecord, NotificationKind, NotificationStatus } from './types.js';

// ─── Internal row type ────────────────────────────────────────────────────────

export interface NotificationRow {
  signal_id: string;
  workspace_path: string;
  artifact: string | null;
  repo: string | null;
  ref: string | null;
  from_agent: string;
  to_agent: string | null;
  kind: string;
  subject: string;
  body: string | null;
  files_json: string;
  refs_json: string;
  thread_id: string;
  reply_to: string | null;
  importance: number;
  status: string;
  created_at: string;
}

export function rowToNotification(r: NotificationRow): NotificationRecord {
  return {
    signal_id: r.signal_id,
    workspace_path: r.workspace_path,
    artifact: r.artifact,
    repo: r.repo,
    ref: r.ref,
    from_agent: r.from_agent,
    to_agent: r.to_agent,
    kind: r.kind as NotificationKind,
    subject: r.subject,
    body: r.body,
    // ARCH-7: Use shared parseJsonList helper instead of duplicated inline IIFEs
    files: parseJsonList(r.files_json),
    refs: parseJsonList(r.refs_json),
    thread_id: r.thread_id,
    reply_to: r.reply_to,
    importance: r.importance,
    status: r.status as NotificationStatus,
    created_at: r.created_at,
  };
}

// ─── insertNotification ────────────────────────────────────────────────────────

export function insertNotification(
  db: DatabaseSync,
  params: InsertNotificationParams,
): InsertNotificationResult {
  const {
    agentId,
    toAgent = null,
    kind,
    subject,
    body = null,
    files = [],
    refIds = [],
    inReplyTo = null,
    importance = 5,
    cwd,
  } = params;

  const normalizedKind = normalizeNotificationKind(kind);
  if (!Number.isInteger(importance) || importance < 1 || importance > 10) {
    throw new Error(`importance must be an integer between 1 and 10, got ${String(importance)}`);
  }

  const scope = fillScope(
    { workspace_path: params.workspacePath ?? null, artifact: normalizeArtifact(params.artifact), repo: params.repo ?? null, ref: params.ref ?? null },
    cwd ?? process.cwd(),
  );

  const signalId = 'ntf_' + randomUUID().replace(/-/g, '');
  const createdAt = utcNow();
  const wsPath = scope.workspace_path ?? process.cwd();

  // Thread: inherit from parent or start new
  let threadId: string;
  if (inReplyTo) {
    const parent = db.prepare(SIGNALS_SELECT_THREAD_ID).get(inReplyTo) as { thread_id: string } | undefined;
    if (!parent) {
      throw new Error(`insertNotification: parent signal ${inReplyTo} not found (deleted?). Omit inReplyTo to start a new thread.`);
    }
    if (!canReadOrJoinThread(db, parent.thread_id, agentId)) {
      throw new Error(`insertNotification: agent ${agentId} is not a participant in thread ${parent.thread_id}`);
    }
    threadId = parent.thread_id;
  } else {
    threadId = signalId;
  }

  db.prepare(SIGNALS_INSERT).run(
    signalId, wsPath, scope.artifact, scope.repo, scope.ref,
    agentId, toAgent, normalizedKind, subject, body,
    JSON.stringify(files), JSON.stringify(refIds),
    threadId, inReplyTo, importance, createdAt,
  );

  return { signal_id: signalId, thread_id: threadId, workspace_path: wsPath, artifact: scope.artifact };
}

export function appendSignalScope(
  where: string[],
  binds: (string | number)[],
  scope: { workspace_path: string | null; artifact: string | null; repo: string | null; ref: string | null },
  alias = 'n',
): void {
  const prefix = alias ? `${alias}.` : '';
  if (scope.workspace_path) {
    where.push(`(${prefix}workspace_path = ? OR ${prefix}workspace_path IS NULL)`);
    binds.push(scope.workspace_path);
  }
  if (scope.artifact) {
    where.push(`(${prefix}artifact = ? OR ${prefix}artifact IS NULL)`);
    binds.push(scope.artifact);
  }
  if (scope.repo) {
    where.push(`(${prefix}repo = ? OR ${prefix}repo IS NULL)`);
    binds.push(scope.repo);
  }
  if (scope.ref) {
    where.push(`(${prefix}ref = ? OR ${prefix}ref IS NULL)`);
    binds.push(scope.ref);
  }
}

export function isBroadcastThread(db: DatabaseSync, threadId: string): boolean {
  return db.prepare(`SELECT 1 FROM signals
    WHERE thread_id = ? AND reply_to IS NULL AND to_agent IS NULL
    LIMIT 1`).get(threadId) != null;
}

export function isThreadParticipant(db: DatabaseSync, threadId: string, agentId: string): boolean {
  const addressed = db.prepare(`SELECT 1 FROM signals
    WHERE thread_id = ? AND (from_agent = ? OR to_agent = ?)
    LIMIT 1`).get(threadId, agentId, agentId) != null;
  if (addressed) return true;
  if (!isBroadcastThread(db, threadId)) return false;
  return db.prepare(`SELECT 1 FROM signal_reads read
    JOIN signals signal ON signal.signal_id = read.signal_id
    WHERE signal.thread_id = ? AND read.agent_id = ?
    LIMIT 1`).get(threadId, agentId) != null;
}

export function canReadOrJoinThread(db: DatabaseSync, threadId: string, agentId: string): boolean {
  return isBroadcastThread(db, threadId) || isThreadParticipant(db, threadId, agentId);
}

export function inferReplyTargets(db: DatabaseSync, inReplyTo: string, agentId: string): string[] {
  const parent = db.prepare('SELECT thread_id FROM signals WHERE signal_id = ?')
    .get(inReplyTo) as { thread_id: string } | undefined;
  if (!parent) {
    throw new Error(`insertNotification: parent signal ${inReplyTo} not found (deleted?). Omit inReplyTo to start a new thread.`);
  }
  const rows = db.prepare('SELECT from_agent, to_agent FROM signals WHERE thread_id = ?')
    .all(parent.thread_id) as unknown as Array<{ from_agent: string; to_agent: string | null }>;
  const participants = new Set<string>();
  for (const row of rows) {
    participants.add(row.from_agent);
    if (row.to_agent) participants.add(row.to_agent);
  }
  participants.delete(agentId);
  if (participants.size === 0) {
    throw new Error('agent_signal reply has no inferred recipient; pass --to-agent');
  }
  return [...participants].sort();
}
