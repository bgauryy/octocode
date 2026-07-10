import { describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { initDb } from '../src/db.js';
import { agentSignal, getNotifications } from '../src/notifications.js';

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

describe('signal thread privacy', () => {
  it('does not let an outsider read, join, or resolve a targeted thread', () => {
    const db = freshDb();
    const published = agentSignal(db, {
      action: 'publish',
      agentId: 'sender',
      toAgents: ['recipient'],
      kind: 'question',
      subject: 'private question',
      body: 'PRIVATE_BODY',
      workspacePath: '/repo',
    });
    if (published.action !== 'publish') throw new Error('publish failed');

    const outsiderRead = getNotifications(db, {
      agentId: 'outsider',
      threadId: published.thread_id,
      workspacePath: '/repo',
      unreadOnly: false,
      markRead: true,
    });
    expect(outsiderRead.signals).toEqual([]);
    expect(db.prepare('SELECT COUNT(*) AS count FROM signal_reads WHERE agent_id = ?')
      .get('outsider')).toEqual({ count: 0 });

    expect(() => agentSignal(db, {
      action: 'reply',
      agentId: 'outsider',
      toAgents: ['sender'],
      subject: 'intrusion',
      inReplyTo: published.signal_id,
      workspacePath: '/repo',
    })).toThrow(/not a participant/);

    const outsiderResolve = agentSignal(db, {
      action: 'resolve',
      agentId: 'outsider',
      threadId: published.thread_id,
      workspacePath: '/repo',
    });
    expect(outsiderResolve).toMatchObject({ action: 'resolve', resolved: 0, signal_ids: [] });

    const recipientRead = getNotifications(db, {
      agentId: 'recipient',
      threadId: published.thread_id,
      workspacePath: '/repo',
      unreadOnly: false,
    });
    expect(recipientRead.signals.map((signal) => signal.body)).toEqual(['PRIVATE_BODY']);
  });

  it('keeps broadcast threads joinable but requires participation before resolve', () => {
    const db = freshDb();
    const published = agentSignal(db, {
      action: 'publish',
      agentId: 'sender',
      kind: 'fyi',
      subject: 'public note',
      body: 'PUBLIC_BODY',
      workspacePath: '/repo',
    });
    if (published.action !== 'publish') throw new Error('publish failed');

    const beforeRead = agentSignal(db, {
      action: 'resolve',
      agentId: 'reader',
      threadId: published.thread_id,
      workspacePath: '/repo',
    });
    expect(beforeRead).toMatchObject({ action: 'resolve', resolved: 0 });

    const joined = getNotifications(db, {
      agentId: 'reader',
      threadId: published.thread_id,
      workspacePath: '/repo',
      unreadOnly: false,
      markRead: true,
    });
    expect(joined.signals).toHaveLength(1);

    const reply = agentSignal(db, {
      action: 'reply',
      agentId: 'reader',
      subject: 'joined',
      inReplyTo: published.signal_id,
      workspacePath: '/repo',
    });
    expect(reply).toMatchObject({ action: 'reply', thread_id: published.thread_id });

    const resolved = agentSignal(db, {
      action: 'resolve',
      agentId: 'reader',
      threadId: published.thread_id,
      workspacePath: '/repo',
    });
    expect(resolved).toMatchObject({ action: 'resolve', resolved: 2 });
  });
});
