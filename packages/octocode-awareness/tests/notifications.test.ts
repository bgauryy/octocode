import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { initDb } from '../src/db-init.js';
import { insertNotification } from '../src/notifications-core.js';
import { getNotifications, resolveNotification } from '../src/notifications-inbox.js';
import { pruneNotifications, agentSignal } from '../src/notifications-signals.js';
import { MESSAGE_RETENTION_DAYS, pruneExpiredNotifications, signalExpiresAt } from '../src/message-lifecycle.js';

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

describe('notifications', () => {
  it('inbox includes handoffs (broadcast or self-addressed) but not own non-handoff broadcasts', () => {
    const db = freshDb();
    // broadcast handoff from self (sessionCapture pattern): visible when the same
    // identity returns AND to any other agent in the workspace
    const broadcastHandoff = insertNotification(db, {
      agentId: 'agent-a', kind: 'handoff',
      subject: 'resume pending runs', workspacePath: '/repo',
    });
    // self-addressed handoff: also visible
    const selfHandoff = insertNotification(db, {
      agentId: 'agent-a', toAgent: 'agent-a', kind: 'handoff',
      subject: 'self handoff', workspacePath: '/repo',
    });
    // own non-handoff broadcast — must stay excluded from own inbox
    insertNotification(db, {
      agentId: 'agent-a', kind: 'fyi', subject: 'own broadcast', workspacePath: '/repo',
    });
    const own = getNotifications(db, { agentId: 'agent-a', workspacePath: '/repo' });
    const ownIds = own.signals.map(n => n.signal_id);
    expect(ownIds).toContain(broadcastHandoff.signal_id);
    expect(ownIds).toContain(selfHandoff.signal_id);
    expect(own.signals.some(n => n.subject === 'own broadcast')).toBe(false);
    // a different agent in the workspace also sees the broadcast handoff
    const peer = getNotifications(db, { agentId: 'agent-b', workspacePath: '/repo' });
    expect(peer.signals.map(n => n.signal_id)).toContain(broadcastHandoff.signal_id);
  });

  it('enforces importance bounds in the domain layer', () => {
    const db = freshDb();
    for (const importance of [0, 1.5, 11]) {
      expect(() => insertNotification(db, {
        agentId: 'agent-a', kind: 'fyi', subject: 'invalid', importance,
      })).toThrow(/importance.*integer.*1.*10/i);
    }
  });

  it('commits the canonical signal and its attributed peer event together', () => {
    const db = freshDb();
    const signal = insertNotification(db, {
      agentId: 'agent-a', toAgent: 'agent-b', kind: 'fyi',
      subject: 'check this', body: 'src/a.ts changed', files: ['src/a.ts'], workspacePath: '/repo',
    });
    expect(db.prepare('SELECT signal_id FROM signals WHERE signal_id = ?').get(signal.signal_id))
      .toEqual({ signal_id: signal.signal_id });
    const event = db.prepare('SELECT event_type, aggregate_id, actor_json, provenance_json, payload_json FROM event_outbox WHERE event_id = ?')
      .get(`evt_${signal.signal_id}`) as { event_type: string; aggregate_id: string; actor_json: string; provenance_json: string; payload_json: string } | undefined;
    expect(event).toBeDefined();
    expect(event).toMatchObject({ event_type: 'peer.message', aggregate_id: signal.signal_id });
    expect(JSON.parse(event!.actor_json)).toEqual({ kind: 'agent', id: 'agent-a' });
    expect(JSON.parse(event!.provenance_json)).toEqual({ source: 'peer', trust: 'attributed-data' });
    expect(JSON.parse(event!.payload_json)).toMatchObject({ messageId: signal.signal_id, toAgentId: 'agent-b', files: ['src/a.ts'] });
  });

  it('uses a subject-only signal as nonempty canonical peer-event text', () => {
    const db = freshDb();
    const signal = insertNotification(db, {
      agentId: 'agent-a', toAgent: 'agent-b', kind: 'fyi',
      subject: 'review the merged receipt', workspacePath: '/repo',
    });
    const event = db.prepare('SELECT payload_json FROM event_outbox WHERE event_id = ?')
      .get(`evt_${signal.signal_id}`) as { payload_json: string } | undefined;
    expect(event).toBeDefined();
    expect(JSON.parse(event!.payload_json)).toMatchObject({
      messageId: signal.signal_id,
      text: 'review the merged receipt',
    });
  });

  it.each([false, true])('rolls back the signal when its outbox write fails (nested=%s)', (nested) => {
    const db = freshDb();
    if (nested) db.exec('BEGIN');
    db.exec(`CREATE TRIGGER reject_peer_event BEFORE INSERT ON event_outbox
      BEGIN SELECT RAISE(ABORT, 'outbox unavailable'); END`);
    expect(() => insertNotification(db, {
      agentId: 'agent-a', toAgent: 'agent-b', kind: 'fyi', subject: 'cannot deliver', workspacePath: '/repo',
    })).toThrow(/outbox unavailable/);
    expect(db.prepare('SELECT COUNT(*) AS count FROM signals').get()).toEqual({ count: 0 });
    expect(db.prepare('SELECT COUNT(*) AS count FROM event_outbox').get()).toEqual({ count: 0 });
    expect(db.isTransaction).toBe(nested);
    if (nested) db.exec('COMMIT');
  });

  it('default unread inbox excludes resolved notifications', () => {
    const db = freshDb();
    const first = insertNotification(db, {
      agentId: 'agent-a',
      toAgent: 'agent-b',
      kind: 'handoff',
      subject: 'done',
      workspacePath: '/repo',
    });
    insertNotification(db, {
      agentId: 'agent-a',
      toAgent: 'agent-b',
      kind: 'handoff',
      subject: 'still open',
      workspacePath: '/repo',
    });

    resolveNotification(db, { notificationIds: [first.signal_id] });

    const unread = getNotifications(db, { agentId: 'agent-b', workspacePath: '/repo' });
    expect(unread.signals).toHaveLength(1);
    expect(unread.signals[0]!.subject).toBe('still open');
    expect(unread.signals[0]!.status).toBe('open');

    const all = getNotifications(db, { agentId: 'agent-b', workspacePath: '/repo', unreadOnly: false });
    expect(all.signals.map(n => n.status)).toEqual(expect.arrayContaining(['open', 'resolved']));
  });

  it('prunes only old resolved messages in the participant workspace', () => {
    const db = freshDb();
    const notification = insertNotification(db, {
      agentId: 'agent-a',
      toAgent: 'agent-b',
      kind: 'request',
      subject: 'please verify',
      workspacePath: '/repo-a',
    });

    db.prepare("UPDATE signals SET status = 'resolved', resolved_at = ?, created_at = ? WHERE signal_id = ?")
      .run(new Date().toISOString(), '2020-01-01T00:00:00.000Z', notification.signal_id);

    const outsider = pruneNotifications(db, {
      agentId: 'agent-c',
      notificationIds: [notification.signal_id],
      resolvedOnly: true,
      olderThanDays: 1,
      workspacePath: '/repo-a',
      dryRun: true,
    });
    expect(outsider.would_delete).toBe(0);

    const wrongScope = pruneNotifications(db, {
      agentId: 'agent-b',
      notificationIds: [notification.signal_id],
      resolvedOnly: true,
      olderThanDays: 1,
      workspacePath: '/repo-b',
      dryRun: true,
    });
    expect(wrongScope.would_delete).toBe(0);

    const deleted = pruneNotifications(db, {
      agentId: 'agent-b',
      notificationIds: [notification.signal_id],
      resolvedOnly: true,
      olderThanDays: 1,
      workspacePath: '/repo-a',
    });
    expect(deleted.deleted).toBe(1);
  });

  it('agentSignal publishes, lists, replies, and resolves a thread', () => {
    const db = freshDb();
    const published = agentSignal(db, {
      action: 'publish',
      agentId: 'agent-a',
      toAgents: ['agent-b'],
      kind: 'question',
      subject: 'can you review?',
      body: 'please check this file',
      files: ['src/a.ts'],
      refs: ['intent_1'],
      workspacePath: '/repo',
      importance: 8,
    });
    expect(published.action).toBe('publish');
    if (published.action !== 'publish') throw new Error('publish failed');
    expect(published.signal_ids).toHaveLength(1);

    const inbox = agentSignal(db, {
      action: 'list',
      agentId: 'agent-b',
      workspacePath: '/repo',
    });
    expect(inbox.action).toBe('list');
    if (inbox.action !== 'list') throw new Error('list failed');
    expect(inbox.signals).toHaveLength(1);
    expect(inbox.signals[0]!.kind).toBe('question');
    expect(inbox.signals[0]!.to_agents).toEqual(['agent-b']);
    expect(inbox.actions).toEqual({
      ack: { operation: 'agent_signal', request: { action: 'ack', agent_id: 'agent-b', signal_id: [published.signal_id] } },
    });

    const exact = agentSignal(db, {
      action: 'list', agentId: 'agent-b', workspacePath: '/repo', signalIds: [published.signal_id], unreadOnly: false,
    });
    expect(exact.action).toBe('list');
    if (exact.action !== 'list') throw new Error('exact list failed');
    expect(exact.actions?.ack.request).toEqual({ action: 'ack', agent_id: 'agent-b', signal_id: [published.signal_id] });
    expect(exact.actions?.reply).toEqual([{ operation: 'agent_signal', request: {
      action: 'reply', agent_id: 'agent-b', in_reply_to: published.signal_id, subject: 'Reply to: can you review?',
    }, draft: true, note: 'Add a substantive body before executing this reply draft.' }]);
    expect(exact.actions?.ack.request).not.toHaveProperty('workspace_path');

    const long = agentSignal(db, {
      action: 'publish', agentId: 'agent-a', toAgents: ['agent-b'], kind: 'fyi',
      subject: 'x'.repeat(200), workspacePath: '/repo',
    });
    if (long.action !== 'publish') throw new Error('long publish failed');
    const longRead = agentSignal(db, {
      action: 'list', agentId: 'agent-b', workspacePath: '/repo', signalIds: [long.signal_id], unreadOnly: false,
    });
    expect(longRead.action).toBe('list');
    if (longRead.action !== 'list') throw new Error('long exact list failed');
    expect(longRead.actions?.reply?.[0]?.request).toMatchObject({ subject: expect.any(String) });
    expect(String(longRead.actions?.reply?.[0]?.request.subject)).toHaveLength(200);

    const ack = agentSignal(db, {
      action: 'ack',
      agentId: 'agent-b',
      signalIds: [published.signal_id],
      workspacePath: '/repo',
    });
    expect(ack.action).toBe('ack');
    if (ack.action !== 'ack') throw new Error('ack failed');
    expect(ack.acknowledged).toBe(1);
    const afterAck = agentSignal(db, { action: 'list', agentId: 'agent-b', workspacePath: '/repo', signalIds: [published.signal_id] });
    expect(afterAck.action).toBe('list');
    if (afterAck.action !== 'list') throw new Error('list failed');
    expect(afterAck.signals).toHaveLength(0);

    const reply = agentSignal(db, {
      action: 'reply',
      agentId: 'agent-b',
      toAgents: ['agent-a'],
      subject: 'reviewed',
      body: 'looks good',
      inReplyTo: published.signal_id,
      workspacePath: '/repo',
    });
    expect(reply.action).toBe('reply');
    if (reply.action !== 'reply') throw new Error('reply failed');
    expect(reply.thread_id).toBe(published.thread_id);

    const resolved = agentSignal(db, {
      action: 'resolve',
      agentId: 'agent-a',
      threadId: published.thread_id,
      workspacePath: '/repo',
    });
    expect(resolved.action).toBe('resolve');
    if (resolved.action !== 'resolve') throw new Error('resolve failed');
    expect(resolved.resolved).toBe(2);
  });

  it('requires resolver participation when an agent id is supplied', () => {
    const db = freshDb();
    const notification = insertNotification(db, {
      agentId: 'agent-a',
      toAgent: 'agent-b',
      kind: 'request',
      subject: 'please handle',
      workspacePath: '/repo',
    });

    const outsider = resolveNotification(db, {
      agentId: 'agent-c',
      notificationIds: [notification.signal_id],
      workspacePath: '/repo',
    });
    expect(outsider.resolved).toBe(0);

    const participant = resolveNotification(db, {
      agentId: 'agent-b',
      notificationIds: [notification.signal_id],
      workspacePath: '/repo',
    });
    expect(participant.resolved).toBe(1);
  });

  it('caps inbox output at the public schema maximum', () => {
    const db = freshDb();
    for (let i = 0; i < 205; i++) {
      insertNotification(db, {
        agentId: 'agent-a',
        toAgent: 'agent-b',
        kind: 'request',
        subject: `request ${i}`,
        workspacePath: '/repo',
      });
    }

    const result = getNotifications(db, { agentId: 'agent-b', workspacePath: '/repo', limit: 500 });
    expect(result.signals).toHaveLength(200);
  });

  it('derives an explicit expiry from the canonical per-kind retention policy', () => {
    const db = freshDb();
    for (const kind of Object.keys(MESSAGE_RETENTION_DAYS) as Array<keyof typeof MESSAGE_RETENTION_DAYS>) {
      const signal = insertNotification(db, {
        agentId: 'agent-a', toAgent: 'agent-b', kind, subject: `policy ${kind}`, workspacePath: '/repo',
      });
      const row = db.prepare('SELECT created_at, expires_at FROM signals WHERE signal_id = ?')
        .get(signal.signal_id) as { created_at: string; expires_at: string | null };
      expect(row.expires_at).toBe(signalExpiresAt(kind, row.created_at));
    }
  });
  it('transitions expired messages and preserves an expired parent while a reply remains live', () => {
    const db = freshDb();
    const parent = insertNotification(db, {
      agentId: 'agent-a', toAgent: 'agent-b', kind: 'question', subject: 'parent', workspacePath: '/repo',
    });
    const reply = insertNotification(db, {
      agentId: 'agent-b', toAgent: 'agent-a', kind: 'reply', subject: 'reply', body: 'still relevant',
      inReplyTo: parent.signal_id, workspacePath: '/repo',
    });
    db.prepare('UPDATE signals SET expires_at = ? WHERE signal_id = ?').run('2020-01-01T00:00:00.000Z', parent.signal_id);
    db.prepare('UPDATE signals SET expires_at = ? WHERE signal_id = ?').run('2999-01-01T00:00:00.000Z', reply.signal_id);

    const result = pruneExpiredNotifications(db, { now: '2021-01-01T00:00:00.000Z' });
    expect(result.transitioned).toBe(1);
    expect(result.deleted).toBe(0);
    expect(db.prepare('SELECT status, resolved_at FROM signals WHERE signal_id = ?').get(parent.signal_id))
      .toMatchObject({ status: 'resolved', resolved_at: '2020-01-01T00:00:00.000Z' });
    expect(db.prepare('SELECT signal_id FROM signals WHERE signal_id = ?').get(reply.signal_id)).toBeDefined();
  });
  it('supports a dry-run and then prunes an expired thread only after its descendants expire', () => {
    const db = freshDb();
    const parent = insertNotification(db, {
      agentId: 'agent-a', toAgent: 'agent-b', kind: 'handoff', subject: 'parent', workspacePath: '/repo',
    });
    const reply = insertNotification(db, {
      agentId: 'agent-b', toAgent: 'agent-a', kind: 'reply', subject: 'reply', inReplyTo: parent.signal_id, workspacePath: '/repo',
    });
    db.prepare('UPDATE signals SET expires_at = ?').run('2020-01-01T00:00:00.000Z');

    const preview = pruneExpiredNotifications(db, { now: '2021-01-01T00:00:00.000Z', dryRun: true });
    expect(preview).toMatchObject({ transitioned: 0, wouldTransition: 2, deleted: 0, wouldDelete: 2 });
    expect(db.prepare('SELECT COUNT(*) AS count FROM signals').get()).toEqual({ count: 2 });

    const applied = pruneExpiredNotifications(db, { now: '2021-01-01T00:00:00.000Z' });
    expect(applied).toMatchObject({ transitioned: 2, deleted: 2 });
    expect(db.prepare('SELECT COUNT(*) AS count FROM signals').get()).toEqual({ count: 0 });
    expect(db.prepare('SELECT COUNT(*) AS count FROM signal_reads WHERE signal_id IN (?, ?)').get(parent.signal_id, reply.signal_id))
      .toEqual({ count: 0 });
  });
  it('keeps expired messages through the grace window and scopes lifecycle work to one workspace', () => {
    const db = freshDb();
    const first = insertNotification(db, {
      agentId: 'agent-a', toAgent: 'agent-b', kind: 'fyi', subject: 'repo a', workspacePath: '/repo-a',
    });
    const second = insertNotification(db, {
      agentId: 'agent-a', toAgent: 'agent-b', kind: 'fyi', subject: 'repo b', workspacePath: '/repo-b',
    });
    db.prepare('UPDATE signals SET expires_at = ?').run('2020-01-01T00:00:00.000Z');

    const inGrace = pruneExpiredNotifications(db, { now: '2020-01-05T00:00:00.000Z', workspacePath: '/repo-a' });
    expect(inGrace).toEqual({ transitioned: 1, deleted: 0 });
    expect(db.prepare('SELECT status FROM signals WHERE signal_id = ?').get(first.signal_id)).toEqual({ status: 'resolved' });
    expect(db.prepare('SELECT status FROM signals WHERE signal_id = ?').get(second.signal_id)).toEqual({ status: 'open' });
    expect(getNotifications(db, { agentId: 'agent-b', workspacePath: '/repo-a', unreadOnly: false }).signals).toHaveLength(1);

    const afterGrace = pruneExpiredNotifications(db, {
      now: '2020-01-05T00:00:00.000Z', pruneBefore: '2020-01-02T00:00:00.000Z', workspacePath: '/repo-a',
    });
    expect(afterGrace).toEqual({ transitioned: 0, deleted: 1 });
    expect(db.prepare('SELECT COUNT(*) AS count FROM signals WHERE signal_id = ?').get(first.signal_id)).toEqual({ count: 0 });
    expect(db.prepare('SELECT COUNT(*) AS count FROM signals WHERE signal_id = ?').get(second.signal_id)).toEqual({ count: 1 });
  });
  it('bounds expiry pruning and returns a deterministic continuation', () => {
    const db = freshDb();
    for (let i = 0; i < 3; i++) {
      const signal = insertNotification(db, {
        agentId: 'agent-a', toAgent: 'agent-b', kind: 'fyi', subject: `expired ${i}`, workspacePath: '/repo',
      });
      db.prepare('UPDATE signals SET expires_at = ? WHERE signal_id = ?').run(`2020-01-0${i + 1}T00:00:00.000Z`, signal.signal_id);
    }

    const result = pruneExpiredNotifications(db, {
      now: '2020-02-01T00:00:00.000Z', pruneBefore: '2020-01-31T00:00:00.000Z', limit: 2, workspacePath: '/repo',
    });
    expect(result).toMatchObject({ transitioned: 2, deleted: 2, partial: true });
    expect(result.next).toEqual({ pruneExpired: {
      now: '2020-02-01T00:00:00.000Z', pruneBefore: '2020-01-31T00:00:00.000Z', workspacePath: '/repo', limit: 2,
    } });
    expect(db.prepare('SELECT COUNT(*) AS count FROM signals').get()).toEqual({ count: 1 });
    const next = pruneExpiredNotifications(db, result.next!.pruneExpired);
    expect(next).toEqual({ transitioned: 1, deleted: 1 });
    expect(db.prepare('SELECT COUNT(*) AS count FROM signals').get()).toEqual({ count: 0 });
  });

});
