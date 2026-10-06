import fs from 'node:fs';
import path from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { leaseIdleMs } from '../src/team/held.js';
import { MESSAGE_TYPE, Team, USER_SENDER } from '../src/team/session.js';
import { registerCollab } from '../src/team/tools.js';
import { leaseAge, resolveTarget, sessionTree, wakes } from '../src/team/routing.js';
import type { Member } from '../src/team/model.js';
import { LEASE_MS, TeamStore } from '../src/team/store.js';
import { trafficRow, widgetLines } from '../src/team/panel.js';
import { fakeCtx, fakePi, theme } from './fake-pi.js';
import { tmp } from './helpers.js';

const row = (id: string, extra: Partial<Member> = {}): Member => ({
  id, role: id.replace(/-[0-9a-f]{6}$/, ''), pid: process.pid, status: 'idle', joinedAt: 1_000, updatedAt: Date.now(), toolCalls: 0, input: 0, output: 0, cost: 0, ...extra,
});

const repo = () => {
  const dir = tmp();
  fs.mkdirSync(path.join(dir, '.git'));
  return dir;
};

const stores: TeamStore[] = [];
const open = (cwd: string, file: string) => {
  const store = TeamStore.open(cwd, file, {});
  stores.push(store);
  return store;
};

const teams: Array<{ team: Team; ctx: ReturnType<typeof fakeCtx> }> = [];
function agent(dbFile: string, cwd: string, env: Record<string, string> = {}, hasUI = true) {
  const fake = fakePi();
  const team = new Team(fake.pi, { OCTOCODE_AGENT_DB: dbFile, ...env });
  registerCollab(fake.pi, team);
  const ctx = fakeCtx({ cwd, hasUI });
  team.start(ctx);
  teams.push({ team, ctx });
  return { ...fake, team, ctx };
}

afterEach(() => {
  vi.useRealTimers();
  for (const { team } of teams.splice(0)) team.stop();
  for (const store of stores.splice(0)) store.close();
});

describe('message wake rule', () => {
  it('wakes the asker with the answer to its open question, never with a thank-you or a second answer', () => {
    const store = open(repo(), path.join(tmp(), 'team.sqlite'));
    store.save(row('a'));
    store.save(row('b'));
    const question = store.send('a', ['b'], 'which file?', { replyRequired: true }).id!;
    expect(wakes(store.pending('b')[0]!)).toBe(true);
    const answer = store.send('b', ['a'], 'src/x.ts', { replyRequired: false, replyTo: question }).id!;
    expect(store.pending('a').find((message) => message.id === answer)).toMatchObject({ wake: true });
    // A reply to an FYI (the answer asked for nothing) does not wake its recipient.
    const thanks = store.send('a', ['b'], 'thanks', { replyRequired: false, replyTo: answer }).id!;
    expect(store.pending('b').find((message) => message.id === thanks)).toMatchObject({ wake: false });
    // The question is answered: answering it again is an FYI.
    const again = store.send('b', ['a'], 'also y.ts', { replyRequired: false, replyTo: question }).id!;
    expect(store.pending('a').find((message) => message.id === again)).toMatchObject({ wake: false });
    expect(store.replyTarget(question)).toEqual({ from: 'a', replyRequired: true });
    expect(store.replyTarget(999_999)).toBeUndefined();
  });

  it('refuses a replyTo that names no message in the repository', async () => {
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const a = agent(db, cwd);
    const b = agent(db, cwd);
    a.team.join();
    b.team.join();
    expect(a.team.send(b.team.id!, 'hi', { replyTo: 424242 })).toEqual({ error: expect.stringContaining('No message #424242') });
  });
});

describe('broadcast scope', () => {
  it('reaches the sender\'s session tree only, never another session in the same repository', () => {
    const members = [
      row('main-aaaaaa'), row('researcher-bbbbbb', { parentId: 'main-aaaaaa' }), row('reviewer-cccccc', { parentId: 'researcher-bbbbbb' }),
      row('main-zzzzzz'), row('general-yyyyyy', { parentId: 'main-zzzzzz' }),
    ];
    expect([...sessionTree(members, 'reviewer-cccccc')].sort()).toEqual(['main-aaaaaa', 'researcher-bbbbbb', 'reviewer-cccccc']);
    const ids = (result: Member[] | string) => (typeof result === 'string' ? result : result.map((member) => member.id).sort());
    expect(ids(resolveTarget(members, 'reviewer-cccccc', 'all'))).toEqual(['main-aaaaaa', 'researcher-bbbbbb']);
    // The user's broadcast is scoped to the session it was typed in.
    expect(ids(resolveTarget(members, USER_SENDER, 'all', undefined, 'main-zzzzzz'))).toEqual(['general-yyyyyy', 'main-zzzzzz']);
    expect(ids(resolveTarget([row('main-aaaaaa'), row('main-zzzzzz')], 'main-aaaaaa', 'all'))).toMatch(/No other agents are in your session tree/);
  });

  it('sends /agents tell all only to this session and its subagents', async () => {
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const main = agent(db, cwd);
    const mine = main.team.join().id;
    const child = agent(db, cwd, { OCTOCODE_AGENT_ID: 'general-111111', OCTOCODE_PARENT_ID: mine });
    const stranger = agent(db, cwd);
    child.team.join();
    stranger.team.join();
    const result = main.team.send('all', 'stop and report', { from: USER_SENDER });
    expect('sent' in result && result.sent.sort()).toEqual(['general-111111', mine].sort());
  });
});

describe('dead letters', () => {
  it('tells the sender once, without waking it, that a recipient left before reading', async () => {
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const a = agent(db, cwd);
    const b = agent(db, cwd);
    a.team.join();
    b.team.join();
    const sent = a.team.send(b.team.id!, 'please check auth.ts\nmore detail', { replyRequired: false });
    expect('id' in sent).toBe(true);
    const id = (sent as { id: number }).id;
    b.team.leave();
    await vi.waitFor(() => expect(a.sent.some((entry) => String(entry.message.content).startsWith(`Message #${id} to ${b.team.id ?? ''}`))).toBe(true), { timeout: 8_000 });
    const notice = a.sent.find((entry) => String(entry.message.content).includes(`Message #${id}`))!;
    expect(notice.message).toMatchObject({ customType: MESSAGE_TYPE, content: expect.stringContaining('was not read') });
    expect(notice.message.content).toContain('"please check auth.ts"');
    expect(notice.options).toEqual({ deliverAs: 'steer' });
    // Once only.
    const store = open(cwd, db);
    expect(store.deadLetters(a.team.id!)).toEqual([]);
  });

  it('marks dead letters notified in the store', () => {
    const store = open(repo(), path.join(tmp(), 'team.sqlite'));
    store.save(row('a'));
    store.save(row('b'));
    store.save(row('c'));
    const id = store.send('a', ['b', 'c'], 'hello', { replyRequired: false }).id!;
    store.remove('b');
    store.remove('c');
    const letters = store.deadLetters('a');
    expect(letters).toEqual([{ id, recipients: ['b', 'c'], text: 'hello' }]);
    store.markNotified(letters);
    expect(store.deadLetters('a')).toEqual([]);
  });
});

describe('headless subagent exit', () => {
  it('leaves the team when its run settles, so later messages are refused instead of acked unread', async () => {
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const parent = agent(db, cwd);
    const parentId = parent.team.join().id;
    const child = agent(db, cwd, { OCTOCODE_AGENT_ID: 'general-222222', OCTOCODE_PARENT_ID: parentId }, false);
    child.team.join();
    child.team.lock(['src/a.ts'], 'work');
    await child.emit('agent_settled', {}, child.ctx);
    expect(parent.team.members().map((member) => member.id)).not.toContain('general-222222');
    expect(parent.team.reservation(path.join(cwd, 'src/a.ts'), 'src/a.ts')).toBeUndefined();
    expect(parent.team.send('general-222222', 'one more')).toEqual({ error: expect.any(String) });
  });

  it('stays joined when a session with a UI goes idle', async () => {
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const main = agent(db, cwd);
    const id = main.team.join().id;
    await main.emit('agent_settled', {}, main.ctx);
    expect(main.team.members().map((member) => member.id)).toContain(id);
  });
});

describe('reservation lifetime', () => {
  it('lets a lease lapse unless its owner renews it', () => {
    const store = open(repo(), path.join(tmp(), 'team.sqlite'));
    const now = Date.now();
    // Its owner keeps heartbeating through the whole test: only the lease's own expiry is under test.
    store.save(row('one', { updatedAt: now + 2 * LEASE_MS }));
    store.lock('one', [{ path: 'a.ts', kind: 'file', reason: 'x' }], now);
    expect(store.conflict('a.ts', 'two', 'file', now + LEASE_MS - 1)).toBeDefined();
    expect(store.conflict('a.ts', 'two', 'file', now + LEASE_MS + 1)).toBeUndefined();
    store.renew('one', now + LEASE_MS - 1);
    expect(store.conflict('a.ts', 'two', 'file', now + LEASE_MS + 1)).toBeDefined();
  });

  it('defaults to 30 idle minutes, configurable, 0 meaning the whole session', () => {
    expect(leaseIdleMs({})).toBe(30 * 60_000);
    expect(leaseIdleMs({ OCTOCODE_LEASE_IDLE_MINUTES: '5' })).toBe(5 * 60_000);
    expect(leaseIdleMs({ OCTOCODE_LEASE_IDLE_MINUTES: '0' })).toBe(Number.POSITIVE_INFINITY);
  });

  it('stops renewing an idle session\'s reservations once it has been idle past the limit', () => {
    vi.useFakeTimers({ toFake: ['Date'] });
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const owner = agent(db, cwd, { OCTOCODE_LEASE_IDLE_MINUTES: '1' });
    const peer = agent(db, cwd);
    owner.team.lock(['src/a.ts'], 'auth');
    peer.team.join();
    expect(peer.team.reservation(path.join(cwd, 'src/a.ts'), 'src/a.ts')).toMatch(/reserved by .* \(auth; locked \d+s ago\)/);
    // Idle for 2 minutes: heartbeats continue (the row stays fresh) but the lease is not renewed and runs out.
    for (let minute = 0; minute < 4; minute++) {
      vi.setSystemTime(Date.now() + 40_000);
      owner.team.join(); // a heartbeat-equivalent flush
      peer.team.join();
    }
    expect(peer.team.reservation(path.join(cwd, 'src/a.ts'), 'src/a.ts')).toBeUndefined();
    expect(owner.sent.some((entry) => /lapsed after \dm idle/.test(String(entry.message.content)))).toBe(true);
  });

  it('formats the lock age', () => {
    expect(leaseAge({ id: 1, path: 'a', kind: 'file', owner: 'o', reason: '', acquiredAt: 0, expiresAt: 0 })).toBe('locked');
    expect(leaseAge({ id: 1, path: 'a', kind: 'file', owner: 'o', reason: '', acquiredAt: 1_000, expiresAt: 0 }, 1_000 + 12 * 60_000)).toMatch(/^locked 12m/);
  });

  it('ignores a lease whose owner has no agent row (it left)', () => {
    const store = open(repo(), path.join(tmp(), 'team.sqlite'));
    store.save(row('two'));
    expect(store.lock('ghost', [{ path: 'src/a.ts', kind: 'file', reason: '' }]).ok).toBe(true);
    expect(store.lock('two', [{ path: 'src/a.ts', kind: 'file', reason: '' }]).ok).toBe(true);
  });
});

describe('panel sanitizing', () => {
  it('strips terminal escapes from other processes\' task, activity and message text', () => {
    const evil = '\u001b]52;c;cHduZWQ=\u0007\u001b[2Jhi';
    const now = Date.now();
    const lines = widgetLines([row('main-aaaaaa', { status: 'working' }), row('general-bbbbbb', { parentId: 'main-aaaaaa', status: 'working', task: evil, activity: evil })], 'main-aaaaaa', now, theme, 120).join('\n');
    expect(lines).toContain('general-bbbbbb');
    expect(lines).not.toMatch(/\u001b\]52|\u001b\[2J|\u0007/);
    const traffic = trafficRow({ id: 1, from: 'a', to: ['b'], text: evil, at: now, state: 'queued' }, now, theme, 120);
    expect(traffic).not.toMatch(/\u001b\]52|\u001b\[2J|\u0007/);
    expect(traffic).toContain('hi');
  });
});
