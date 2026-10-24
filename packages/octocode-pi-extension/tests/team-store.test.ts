import fs from 'node:fs';
import path from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import type { ExtensionAPI, ExtensionContext } from '@earendil-works/pi-coding-agent';
import { describe, expect, it } from 'vitest';
import { AGENT_APPLICATION_ID, AgentDbError } from '../src/agentdb/schema.js';
import { trafficRow, widgetLines } from '../src/team/panel.js';
import { LEASE_MS } from '../src/team/roster.js';
import { pathKey, TEAM_WORKSPACE_ENV, TeamStore } from '../src/team/store.js';
import { stub } from './doubles.js';
import { theme } from './fake-pi.js';
import { repo, tmp } from './helpers.js';
import { member, sent, take } from './team-harness.js';

const open = (cwd: string, db: string) => TeamStore.open(cwd, db);

describe('team database', () => {
  it('lists live agents oldest first, hides dead or silent ones without mutating, and sweep drops only the dead with their leases', () => {
    const cwd = repo();
    const store = open(cwd, path.join(tmp(), 'team.sqlite'));
    store.save(member('main-aaaaaa', { joinedAt: 1 }));
    store.save(member('researcher-bbbbbb', { joinedAt: 2 }));
    store.save(member('gone-cccccc', { pid: 2 ** 22 + 12345 }));
    store.save(member('silent-dddddd', { updatedAt: Date.now() - 120_000 }));
    expect(store.lock('gone-cccccc', [{ path: 'a.ts', kind: 'file', reason: 'x' }]).ok).toBe(true);
    expect(store.list().map((m) => m.id)).toEqual(['main-aaaaaa', 'researcher-bbbbbb']);
    // A read changes nothing: the dead one's lease is still held until the sweep.
    expect(store.leases().map((lease) => lease.owner)).toEqual(['gone-cccccc']);
    store.sweep();
    const names = store.names();
    names.sort();
    expect(names).toEqual(['main-aaaaaa', 'researcher-bbbbbb', 'silent-dddddd']);
    expect(store.leases()).toEqual([]);
    // The silent one is hidden, not dropped: its process runs.
    expect(store.send('main-aaaaaa', ['silent-dddddd'], 'still there?', { replyRequired: false })).toMatchObject({ id: expect.any(Number), departed: [] });
    store.remove('main-aaaaaa');
    expect(store.list().map((m) => m.id)).toEqual(['researcher-bbbbbb']);
    store.close();
  });

  it('shares one database between workspaces without mixing them', () => {
    const file = path.join(tmp(), 'team.sqlite');
    const [one, two] = [repo(), repo()];
    const a = open(one, file);
    const b = open(two, file);
    a.save(member('main-aaaaaa'));
    expect(b.list()).toEqual([]);
    expect(open(one, file).list().map((m) => m.id)).toEqual(['main-aaaaaa']);
  });

  it('delivers once, tracks replies and only asks for the ones that need them', () => {
    const store = open(repo(), path.join(tmp(), 'team.sqlite'));
    store.save(member('main-aaaaaa'));
    store.save(member('researcher-bbbbbb'));
    const ask = sent(store.send('main-aaaaaa', ['researcher-bbbbbb'], 'check tests', { replyRequired: true }));
    const fyi = sent(store.send('main-aaaaaa', ['researcher-bbbbbb'], 'heads up', { replyRequired: false }));
    expect(store.list().find((m) => m.id === 'researcher-bbbbbb')?.pending).toBe(0);
    const got = take(store, 'researcher-bbbbbb');
    expect(got.map((m) => [m.id, m.text, m.replyRequired])).toEqual([[ask, 'check tests', true], [fyi, 'heads up', false]]);
    expect(take(store, 'researcher-bbbbbb')).toEqual([]);
    expect(store.list().find((m) => m.id === 'researcher-bbbbbb')?.pending).toBe(1);
    store.send('researcher-bbbbbb', ['main-aaaaaa'], 'tests fine', { replyRequired: false, replyTo: ask });
    expect(store.list().find((m) => m.id === 'researcher-bbbbbb')?.pending).toBe(0);
    expect(take(store, 'main-aaaaaa')[0]).toMatchObject({ from: 'researcher-bbbbbb', replyTo: ask });
  });

  it('dead-letters unread messages when the recipient leaves and refuses to send to departed agents', () => {
    const store = open(repo(), path.join(tmp(), 'team.sqlite'));
    store.save(member('a-0001'));
    store.save(member('b-0002'));
    const id = sent(store.send('main-aaaaaa', ['a-0001'], 'hi', { replyRequired: true }));
    store.remove('a-0001');
    expect(take(store, 'a-0001')).toEqual([]);
    expect(store.recent()[0]).toMatchObject({ id, state: 'dead-lettered', to: ['a-0001'] });
    expect(store.send('main-aaaaaa', ['a-0001'], 'again', { replyRequired: false })).toEqual({ id: undefined, departed: ['a-0001'] });
    const partial = store.send('main-aaaaaa', ['a-0001', 'b-0002'], 'both', { replyRequired: false });
    expect(partial.departed).toEqual(['a-0001']);
    expect(store.recent()[0]).toMatchObject({ id: partial.id, to: ['b-0002'], state: 'queued' });
  });

  it('renews live leases and drops ones that lapsed before renewal, so `owned` stops listing them', () => {
    const store = open(repo(), path.join(tmp(), 'team.sqlite'));
    const now = Date.now();
    store.lock('one', [{ path: 'src/a.ts', kind: 'file', reason: 'x' }, { path: 'docs/', kind: 'tree', reason: 'x' }], now);
    store.renew('one', now + 1_000);
    // Renewed from the heartbeat: both are live for a full lease again.
    expect(store.owned('one', now + 1_000 + LEASE_MS - 1).size).toBe(2);
    const later = now + 1_000 + LEASE_MS + 1;
    store.renew('one', later);
    // Dropped, not revived: not even a reader looking back at a moment they were live finds them.
    expect(store.owned('one', now + 2_000).size).toBe(0);
    expect(store.owned('one', later).size).toBe(0);
    expect(store.conflict('src/a.ts', 'two', 'file', later)).toBeUndefined();
  });
});

describe('team database', () => {
  it('reserves files and trees all-or-nothing on component boundaries, case-insensitively', () => {
    const cwd = repo();
    const store = open(cwd, path.join(tmp(), 'team.sqlite'));
    for (const id of ['one', 'two', 'three']) store.save(member(id));
    const ask = (paths: Array<[string, 'file' | 'tree']>, owner: string) => store.lock(owner, paths.map(([p, kind]) => ({ path: p, kind, reason: 'work' })));
    expect(ask([['src/a.ts', 'file'], ['docs', 'tree']], 'one').ok).toBe(true);
    // Same file in another case, a file under the tree, and the tree's parent tree all conflict.
    expect(ask([['SRC/A.ts', 'file']], 'two').ok).toBe(false);
    expect(ask([['docs/x/y.md', 'file']], 'two').ok).toBe(false);
    expect(ask([['.', 'tree']], 'two').ok).toBe(false);
    // Siblings that only share a string prefix are independent.
    expect(ask([['src/a.tsx', 'file'], ['documents', 'tree']], 'two').ok).toBe(true);
    // One conflicting path rejects the whole set: nothing partial is left behind.
    const partial = ask([['free.ts', 'file'], ['src/a.ts', 'file']], 'three');
    expect(partial.ok).toBe(false);
    expect(store.leases().some((lease) => lease.owner === 'three')).toBe(false);
    // The owner may re-lock its own paths; unlocking frees them for others.
    expect(ask([['src/a.ts', 'file']], 'one').ok).toBe(true);
    expect(store.conflict(path.join(cwd, 'docs', 'x.md'), 'two')?.owner).toBe('one');
    expect(store.unlock('one', ['src/a.ts'])).toBe(1);
    expect(ask([['src/a.ts', 'file']], 'two').ok).toBe(true);
    expect(store.unlock('one')).toBe(1);
  });

  it('builds comparison keys per component', () => {
    expect(pathKey('/w', '/w/Src/A.ts')).toBe('/src/a.ts');
    expect(pathKey('/w', 'src/./a.ts')).toBe('/src/a.ts');
    expect(pathKey('/w', '.')).toBe('');
  });
});

describe('message feed', () => {
  it('tracks each message from queued to answered and shows it in the panel', () => {
    const store = open(repo(), path.join(tmp(), 'team.sqlite'));
    store.save(member('main-aaaaaa'));
    store.save(member('researcher-bbbbbb'));
    const id = sent(store.send('main-aaaaaa', ['researcher-bbbbbb'], 'check the tests\nplease', { replyRequired: true }));
    expect(store.recent()[0]).toMatchObject({ id, from: 'main-aaaaaa', to: ['researcher-bbbbbb'], state: 'queued' });
    take(store, 'researcher-bbbbbb');
    expect(store.recent()[0].state).toBe('awaiting reply');
    store.send('researcher-bbbbbb', ['main-aaaaaa'], 'done', { replyRequired: false, replyTo: id });
    const [reply, original] = store.recent();
    expect(original.state).toBe('answered');
    expect(reply).toMatchObject({ replyTo: id, state: 'queued' });
    expect(store.recent(3, Date.now() + 11 * 60_000)).toEqual([]);
    const row = trafficRow(original, original.at + 5_000, theme, 200);
    expect(row).toContain('main-aaaaaa → researcher-bbbbbb');
    expect(row).toContain('answered');
    expect(row).toContain('"check the tests"');
    // The panel leaves out this session's own messages (they are in its transcript) and shows its subagents' traffic.
    const panel = [member('main-aaaaaa'), member('researcher-bbbbbb', { parentId: 'main-aaaaaa' })];
    expect(widgetLines(panel, 'main-aaaaaa', Date.now(), theme, 200, [original]).join('\n')).not.toContain('#' + id);
    expect(widgetLines(panel, 'other-session', Date.now(), theme, 200, [original])).toEqual([]);
    expect(widgetLines(panel, 'main-aaaaaa', Date.now(), theme, 200, [{ ...original, from: 'researcher-bbbbbb', to: ['tester-c'] }]).at(-1)).toContain('#' + id);
  });
});

const pragma = (file: string, name: string) => {
  const db = new DatabaseSync(file);
  try {
    return db.prepare(`PRAGMA ${name}`).get()?.[name];
  } finally {
    db.close();
  }
};

const exec = (file: string, sql: string) => {
  const db = new DatabaseSync(file);
  db.exec(sql);
  db.close();
};

describe('team database schema', () => {
  it('stamps a fresh database with the application id', () => {
    const file = path.join(tmp(), 'team.sqlite');
    open(repo(), file).close();
    expect(pragma(file, 'application_id')).toBe(AGENT_APPLICATION_ID);
  });

  it('opens an agent database whatever version number another build left on it, keeping its data', () => {
    const file = path.join(tmp(), 'team.sqlite');
    const cwd = repo();
    const other = open(cwd, file);
    other.save(member('a-0001'));
    other.close();
    exec(file, 'PRAGMA user_version=99;');
    open(cwd, file).close();
    const db = new DatabaseSync(file);
    expect(db.prepare('SELECT id FROM agents').all().map((row) => row['id'])).toEqual(['a-0001']);
    db.close();
  });

  it('refuses a foreign database and leaves it untouched', () => {
    const foreign = path.join(tmp(), 'other.sqlite');
    exec(foreign, 'CREATE TABLE notes (id INTEGER);');
    expect(() => open(repo(), foreign)).toThrow(AgentDbError);
    expect(pragma(foreign, 'application_id')).toBe(0);
    const stamped = path.join(tmp(), 'stamped.sqlite');
    exec(stamped, 'PRAGMA application_id=1234;');
    expect(() => open(repo(), stamped)).toThrow(/not an Octocode agent database/);
    expect(pragma(stamped, 'application_id')).toBe(1234);
  });

  it('shows why the team is off in the snapshot, without retrying the open', async () => {
    const { Team } = await import('../src/team/session.js');
    const file = path.join(tmp(), 'other.sqlite');
    exec(file, 'CREATE TABLE notes (id INTEGER);');
    const team = new Team(stub<ExtensionAPI>({ sendMessage: () => undefined }), { OCTOCODE_AGENT_DB: file });
    team.start(stub<ExtensionContext>({ cwd: repo(), isIdle: () => true }));
    expect(() => team.join()).toThrow(/not an Octocode agent database/);
    expect(team.snapshot()).toMatchObject({ members: [], error: expect.stringMatching(/Team database unavailable/) });
    expect(team.problem).toMatch(/not an Octocode agent database/);
    team.stop();
  });

  it('uses OCTOCODE_TEAM_WORKSPACE as the workspace when set', () => {
    const file = path.join(tmp(), 'team.sqlite');
    const parent = repo();
    const worktree = repo();
    expect(TeamStore.open(worktree, file, { [TEAM_WORKSPACE_ENV]: parent }).workspace).toBe(fs.realpathSync(parent));
    expect(TeamStore.open(worktree, file, {}).workspace).toBe(fs.realpathSync(worktree));
  });
});
