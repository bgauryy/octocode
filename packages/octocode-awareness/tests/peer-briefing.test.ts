import { DatabaseSync } from 'node:sqlite';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { initDb } from '../src/db-init.js';
import { agentSignal } from '../src/notifications-signals.js';
import { getNotifications } from '../src/notifications-inbox.js';
import { peerBriefing } from '../src/peer-briefing.js';

const databases: DatabaseSync[] = [];
afterEach(() => { vi.restoreAllMocks(); for (const db of databases.splice(0)) db.close(); });
function fixture() {
  const db = new DatabaseSync(':memory:'); initDb(db); databases.push(db);
  const params = { agentId: 'owner', workspacePath: '/repo', sessionId: 'session' };
  const send = (subject: string, body = 'Check the parser contract.') => agentSignal(db, {
    action: 'publish', agentId: 'peer', workspacePath: '/repo', cwd: '/repo',
    toAgents: ['owner'], kind: 'question', subject, body,
  });
  return { db, params, send };
}
const packet = (context: string) => JSON.parse(context.slice(context.indexOf('\n') + 1));

describe('peer-only hook briefing', () => {
  it('delivers identities, thread IDs and bodies once without memory queries or acknowledgement', () => {
    const { db, params, send } = fixture();
    send('Parser ownership');
    const prepare = vi.spyOn(db, 'prepare');
    const first = peerBriefing(db, params)!;
    expect(first).toContain('Check the parser contract.');
    expect(packet(first).signals[0]).toMatchObject({ from_agent: 'peer', to_agent: 'owner', kind: 'question' });
    expect(packet(first).signals[0].thread_id).toBeTruthy();
    expect(peerBriefing(db, params)).toBeNull();
    expect(prepare.mock.calls.some(([sql]) => /awareness_memories|refinements|task_runs/.test(sql))).toBe(false);
    expect(getNotifications(db, { agentId: 'owner', workspacePath: '/repo', cwd: '/repo' }).count).toBe(1);
    expect(peerBriefing(db, { ...params, sessionId: 'new-session' })).not.toBeNull();
  });

  it('keeps message-page and clipped-body continuations executable and lossless', () => {
    const { db, params, send } = fixture();
    for (let i = 0; i < 7; i++) send(`Question ${i}`, 'x'.repeat(900));
    const first = packet(peerBriefing(db, params)!);
    expect(first.partialReasons).toEqual(['limit', 'message_length']);
    const listParams = first.next.list.params as Record<string, unknown>;
    const second = getNotifications(db, { agentId: 'owner', workspacePath: '/repo', cwd: '/repo',
      cursor: String(listParams.cursor), limit: 5 });
    expect(new Set([...first.signals.map((s: { signal_id: string }) => s.signal_id), ...second.signals.map(s => s.signal_id)]).size).toBe(7);
    expect(second.partial).toBe(false);
    const ids = first.next.read.params.signal_id as string[];
    const full = getNotifications(db, { agentId: 'owner', workspacePath: '/repo', cwd: '/repo', signalIds: ids, unreadOnly: false });
    expect(full.signals).toHaveLength(5);
    expect(full.signals.every(s => s.body?.length === 900)).toBe(true);
  });

  it('stays silent for empty or unrelated inboxes', () => {
    const { db, params, send } = fixture();
    expect(peerBriefing(db, params)).toBeNull();
    send('Private question');
    expect(peerBriefing(db, { ...params, agentId: 'outsider' })).toBeNull();
    expect(peerBriefing(db, { ...params, workspacePath: '/another-repo' })).toBeNull();
  });
});
