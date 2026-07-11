import type { DatabaseSync } from 'node:sqlite';
import { normalizeArtifact, utcNow } from './helpers.js';
import { fillScope } from './git.js';
import { SIGNALS_DELETE_BY_IDS, SIGNAL_READS_INSERT_IGNORE, SIGNAL_READS_DELETE_BY_SIGNAL_IDS } from './sql/index.js';
import type { PruneNotificationsParams, PruneNotificationsResult, NotificationRecord, AgentSignalParams, AgentSignalResult, AgentSignalRecord } from './types.js';
import { appendSignalScope, inferReplyTargets, insertNotification, isThreadParticipant } from './notifications-core.js';
import { getNotifications, resolveNotification } from './notifications-inbox.js';

// ─── pruneNotifications ────────────────────────────────────────────────────────

export function signalRecord(n: NotificationRecord): AgentSignalRecord {
  return { ...n, to_agents: n.to_agent ? [n.to_agent] : [] };
}

export function requireSignalText(value: string | null | undefined, field: string): string {
  if (typeof value !== 'string' || value.trim().length === 0) {
    throw new Error(`agent_signal ${field} is required`);
  }
  return value;
}

/**
 * Explicitly named ids must exist: a typo'd --signal-id otherwise ack/resolves
 * zero rows and reports ok, so the caller believes the signal was handled.
 */
export function assertSignalsExist(db: DatabaseSync, signalIds: string[]): void {
  if (signalIds.length === 0) return;
  const unique = [...new Set(signalIds)];
  const rows = db.prepare(
    `SELECT signal_id FROM signals WHERE signal_id IN (${unique.map(() => '?').join(',')})`,
  ).all(...unique) as unknown as Array<{ signal_id: string }>;
  const found = new Set(rows.map((r) => r.signal_id));
  const missing = unique.filter((id) => !found.has(id));
  if (missing.length > 0) {
    throw new Error(`signal(s) not found: ${missing.join(', ')}`);
  }
}

export function acknowledgeNotifications(
  db: DatabaseSync,
  agentId: string,
  signalIds: string[] = [],
  threadId: string | null = null,
  params: { workspacePath?: string | null; artifact?: string | null; cwd?: string } = {},
): { acknowledged: number; signal_ids: string[] } {
  assertSignalsExist(db, signalIds);
  const where: string[] = ["status = 'open'", '(to_agent IS NULL OR to_agent = ?)', 'from_agent <> ?'];
  const binds: (string | number)[] = [agentId, agentId];
  if (signalIds.length > 0) {
    where.push(`signal_id IN (${signalIds.map(() => '?').join(',')})`);
    binds.push(...signalIds);
  }
  if (threadId) {
    where.push('thread_id = ?');
    binds.push(threadId);
  }
  const scope = fillScope(
    { workspace_path: params.workspacePath ?? null, artifact: normalizeArtifact(params.artifact), repo: null, ref: null },
    params.cwd ?? process.cwd(),
  );
  if (signalIds.length === 0) {
    appendSignalScope(where, binds, scope, '');
  }
  const rows = db.prepare(`SELECT signal_id FROM signals WHERE ${where.join(' AND ')}`)
    .all(...binds) as unknown as Array<{ signal_id: string }>;
  const ids = rows.map((r) => r.signal_id);
  const uniqueIds = [...new Set(ids)];
  if (uniqueIds.length === 0) return { acknowledged: 0, signal_ids: [] };

  const now = utcNow();
  const insertRead = db.prepare(SIGNAL_READS_INSERT_IGNORE);
  let acknowledged = 0;
  for (const id of uniqueIds) {
    const result = insertRead.run(id, agentId, now) as { changes: number };
    acknowledged += result.changes;
  }
  return { acknowledged, signal_ids: uniqueIds };
}

export function agentSignal(db: DatabaseSync, params: AgentSignalParams): AgentSignalResult {
  switch (params.action) {
    case 'publish':
    case 'reply': {
      const toAgents = params.toAgents?.length
        ? params.toAgents
        : params.action === 'reply'
          ? inferReplyTargets(db, requireSignalText(params.inReplyTo, 'inReplyTo'), params.agentId)
          : [null];
      const results = toAgents.map((toAgent) => insertNotification(db, {
        agentId: params.agentId,
        workspacePath: params.workspacePath,
        artifact: params.artifact,
        repo: params.repo,
        ref: params.ref,
        toAgent,
        kind: params.action === 'reply' ? 'reply' : params.kind ?? 'fyi',
        subject: requireSignalText(params.subject, 'subject'),
        body: params.body ?? null,
        files: params.files ?? [],
        refIds: params.refs ?? [],
        inReplyTo: params.inReplyTo ?? null,
        importance: params.importance ?? 5,
        cwd: params.cwd,
      }));
      return {
        action: params.action,
        signal_id: results[0]!.signal_id,
        signal_ids: results.map((r) => r.signal_id),
        thread_id: results[0]!.thread_id,
        workspace_path: results[0]!.workspace_path,
        artifact: results[0]!.artifact,
      };
    }
    case 'list': {
      const result = getNotifications(db, {
        agentId: params.agentId,
        workspacePath: params.workspacePath,
        artifact: params.artifact,
        repo: params.repo,
        ref: params.ref,
        kinds: params.kinds ?? [],
        signalIds: params.signalIds ?? [],
        threadId: params.threadId ?? null,
        unreadOnly: params.unreadOnly ?? true,
        markRead: params.markRead ?? false,
        limit: params.limit ?? 20,
        cwd: params.cwd,
      });
      return {
        action: 'list',
        count: result.count,
        signals: result.signals.map(signalRecord),
        unread_only: result.unread_only,
      };
    }
    case 'resolve': {
      const result = resolveNotification(db, {
        agentId: params.agentId,
        notificationIds: params.signalIds ?? [],
        threadId: params.threadId ?? null,
        workspacePath: params.workspacePath,
        artifact: params.artifact,
        cwd: params.cwd,
      });
      return { action: 'resolve', ...result };
    }
    case 'ack': {
      return {
        action: 'ack',
        ...acknowledgeNotifications(db, params.agentId, params.signalIds ?? [], params.threadId ?? null, {
          workspacePath: params.workspacePath,
          artifact: params.artifact,
          cwd: params.cwd,
        }),
      };
    }
  }
}

export function pruneNotifications(
  db: DatabaseSync,
  params: PruneNotificationsParams,
): PruneNotificationsResult {
  const { agentId, notificationIds = [], resolvedOnly = false, olderThanDays, dryRun = false, cwd } = params;

  if (!resolvedOnly) throw new Error('signal prune only deletes resolved messages');
  if (olderThanDays == null || !Number.isFinite(olderThanDays) || olderThanDays < 1) {
    throw new Error('signal prune requires --older-than-days >= 1');
  }

  const scope = fillScope(
    { workspace_path: params.workspacePath ?? null, artifact: normalizeArtifact(params.artifact), repo: null, ref: null },
    cwd ?? process.cwd(),
  );

  const where: string[] = ["status = 'resolved'", 'created_at < ?'];
  const binds: (string | number)[] = [];
  binds.push(new Date(Date.now() - Math.floor(olderThanDays) * 86400000).toISOString());

  if (notificationIds.length > 0) {
    where.push(`signal_id IN (${notificationIds.map(() => '?').join(',')})`);
    binds.push(...notificationIds);
  }
  appendSignalScope(where, binds, scope, '');

  const whereClause = where.join(' AND ');
  const rows = db.prepare(`SELECT signal_id, thread_id FROM signals WHERE ${whereClause}`)
    .all(...binds) as unknown as Array<{ signal_id: string; thread_id: string }>;
  const ids = rows
    .filter((row) => isThreadParticipant(db, row.thread_id, agentId))
    .map((row) => row.signal_id);

  if (dryRun) {
    return { deleted: 0, dry_run: true, would_delete: ids.length, signal_ids: ids };
  }

  if (ids.length > 0) {
    const ph = ids.map(() => '?').join(',');
    db.prepare(SIGNALS_DELETE_BY_IDS(ph)).run(...ids);
    db.prepare(SIGNAL_READS_DELETE_BY_SIGNAL_IDS(ph)).run(...ids);
  }

  return { deleted: ids.length, signal_ids: ids };
}
