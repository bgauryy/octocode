import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { initDb } from '../src/db.js';
import { insertNotification, getNotifications, resolveNotification, pruneNotifications, agentSignal } from '../src/notifications.js';

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

describe('notifications', () => {
  it('enforces importance bounds in the domain layer', () => {
    const db = freshDb();
    for (const importance of [0, 1.5, 11]) {
      expect(() => insertNotification(db, {
        agentId: 'agent-a', kind: 'fyi', subject: 'invalid', importance,
      })).toThrow(/importance.*integer.*1.*10/i);
    }
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

    const ack = agentSignal(db, {
      action: 'ack',
      agentId: 'agent-b',
      signalIds: [published.signal_id],
      workspacePath: '/repo',
    });
    expect(ack.action).toBe('ack');
    if (ack.action !== 'ack') throw new Error('ack failed');
    expect(ack.acknowledged).toBe(1);
    const afterAck = agentSignal(db, { action: 'list', agentId: 'agent-b', workspacePath: '/repo' });
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
});
