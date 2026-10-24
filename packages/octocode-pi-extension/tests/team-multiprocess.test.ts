import { spawn, type ChildProcess } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { buildSync } from 'esbuild';
import { afterAll, afterEach, describe, expect, it, vi } from 'vitest';
import { processStartTime } from '../src/shared/process.js';
import type { Member } from '../src/team/model.js';
import { Team } from '../src/team/session.js';
import { AGENT_ID_ENV, TeamStore } from '../src/team/store.js';
import { fakeCtx, fakePi } from './fake-pi.js';
import { repo, tmp } from './helpers.js';

const posix = process.platform !== 'win32';
const member = (id: string, extra: Partial<Member> = {}): Member => ({
  id, role: 'worker', pid: process.pid, status: 'idle', joinedAt: 1_000, updatedAt: Date.now(), toolCalls: 0, input: 0, cacheRead: 0, output: 0, cost: 0, unpriced: false, ...extra,
});
/** A pid that belonged to a process that has exited. */
const deadPid = 2 ** 22 + 4242;
/** Another running process (this one's parent): a holder this process is not. */
const other = (): Pick<Member, 'pid' | 'started'> => ({ pid: process.ppid, ...(processStartTime(process.ppid) ? { started: processStartTime(process.ppid)! } : {}) });

const children: ChildProcess[] = [];
afterAll(() => fs.rmSync(path.join(import.meta.dirname, '..', 'node_modules', '.team-join-worker'), { recursive: true, force: true }));
afterEach(() => {
  vi.restoreAllMocks();
  for (const child of children.splice(0)) child.kill('SIGKILL');
});

describe('joining the team', () => {
  it.runIf(posix)('gives 20 processes joining at once 20 distinct rows when they all draw the same names', async () => {
    // Bundled next to the package's dependencies, which stay external and so resolve from there.
    const worker = path.join(import.meta.dirname, '..', 'node_modules', '.team-join-worker', 'worker.mjs');
    buildSync({ entryPoints: [path.join(import.meta.dirname, 'fixtures', 'team-join-worker.ts')], bundle: true, packages: 'external', platform: 'node', format: 'esm', outfile: worker, logLevel: 'silent' });
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    // The database exists before the race, as it does for the first of several real Pi processes.
    TeamStore.open(cwd, db).close();
    const joined = Array.from({ length: 20 }, () => {
      const child = spawn(process.execPath, [worker, cwd, db], { stdio: ['pipe', 'pipe', 'inherit'] });
      children.push(child);
      return new Promise<string>((resolve, reject) => {
        let out = '';
        child.stdout!.on('data', (chunk: Buffer) => {
          out += chunk.toString();
          const name = /joined (\S+)/.exec(out)?.[1];
          if (name) resolve(name);
        });
        child.once('exit', (code) => reject(new Error(`worker exited with ${code}: ${out}`)));
      });
    });
    const names = await Promise.all(joined);
    expect(new Set(names).size).toBe(20);
    const store = TeamStore.open(cwd, db);
    try {
      expect(store.list().map((each) => each.id).sort()).toEqual([...names].sort());
    } finally {
      store.close();
    }
  }, 30_000);

  it('never overwrites a name another running process holds, and draws again', () => {
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const store = TeamStore.open(cwd, db);
    try {
      expect(store.join(member('inky', { ...other() }))).toBe(true);
      // Another process, same name: refused, and the holder's row is untouched.
      expect(store.join(member('inky', { goal: 'intruder' }))).toBe(false);
      expect(store.list()[0]).toMatchObject({ id: 'inky', pid: process.ppid });
      expect(store.list()[0]!.goal).toBeUndefined();
      // The same process joining again refreshes its row.
      expect(store.join(member('mine'))).toBe(true);
      expect(store.join(member('mine', { goal: 'again' }))).toBe(true);
      expect(store.list().find((each) => each.id === 'mine')?.goal).toBe('again');
    } finally {
      store.close();
    }
  });

  it('takes over a name whose holder is gone, dead-lettering its unread mail', () => {
    const store = TeamStore.open(repo(), path.join(tmp(), 'team.sqlite'));
    try {
      store.save(member('sender'));
      store.save(member('inky', { pid: deadPid }));
      // The holder died after the mail was stored for it.
      const stored = store.send('sender', ['inky'], 'hello', { replyRequired: false });
      expect(stored.id).toBeUndefined();
      expect(store.join(member('inky'))).toBe(true);
      expect(store.list().find((each) => each.id === 'inky')?.pid).toBe(process.pid);
    } finally {
      store.close();
    }
  });

  it('draws another name when a Team finds its first draw taken, and refuses a chosen name that is held', () => {
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const store = TeamStore.open(cwd, db);
    expect(store.join(member('taken', { ...other() }))).toBe(true);
    const real = Team.prototype.newName;
    let first = true;
    vi.spyOn(Team.prototype, 'newName').mockImplementation(function (this: Team, inUse?: Iterable<string>) {
      if (first) {
        first = false;
        return 'taken';
      }
      return real.call(this, inUse);
    });
    const fake = fakePi();
    const team = new Team(fake.pi, { OCTOCODE_AGENT_DB: db });
    team.start(fakeCtx({ cwd }));
    const me = team.join();
    expect(me.id).not.toBe('taken');
    expect(store.list().map((each) => each.id).sort()).toEqual([me.id, 'taken'].sort());
    expect(store.list().find((each) => each.id === 'taken')?.pid).toBe(process.ppid);

    const child = new Team(fakePi().pi, { OCTOCODE_AGENT_DB: db, [AGENT_ID_ENV]: 'taken' });
    // A subagent's name is its parent's choice: it joins at start, and a name a running process holds is an error.
    expect(() => child.start(fakeCtx({ cwd }))).toThrow(/held by another running agent/);
    store.close();
  });

  it('does not remove or overwrite a row that names another process when this one leaves or beats', () => {
    const store = TeamStore.open(repo(), path.join(tmp(), 'team.sqlite'));
    try {
      store.save(member('inky', { ...other() }));
      const mine = member('inky');
      expect(store.save(mine)).toBe(false);
      expect(store.remove('inky', Date.now(), mine)).toBe(false);
      expect(store.list()[0]).toMatchObject({ id: 'inky', pid: process.ppid });
      expect(store.remove('inky', Date.now(), { pid: process.ppid, started: other().started })).toBe(true);
      expect(store.list()).toEqual([]);
    } finally {
      store.close();
    }
  });
});

describe('who counts as gone', () => {
  const rows = (store: TeamStore) => (store as unknown as { db: { prepare(sql: string): { all(): unknown[] } } }).db.prepare('SELECT id FROM agents ORDER BY id').all();

  it.runIf(posix)('keeps the mail of a process stalled for 90 s and resumed (SIGSTOP, SIGCONT)', () => {
    const child = spawn('sleep', ['60'], { stdio: 'ignore' });
    children.push(child);
    const store = TeamStore.open(repo(), path.join(tmp(), 'team.sqlite'));
    try {
      const pid = child.pid!;
      const started = processStartTime(pid);
      const joinedAt = Date.now();
      const later = joinedAt + 90_000;
      store.save(member('sender', { updatedAt: later }));
      store.save(member('stalled', { pid, ...(started ? { started } : {}), updatedAt: joinedAt }));
      process.kill(pid, 'SIGSTOP');
      // Silent for 90 s: hidden from the list, but its process runs, so mail is kept and nothing is dropped.
      expect(store.list(later).map((each) => each.id)).toEqual(['sender']);
      const sent = store.send('sender', ['stalled'], 'are you there?', { replyRequired: true }, later);
      expect(sent.id).toBeGreaterThan(0);
      store.sweep(later);
      expect(store.names().sort()).toEqual(['sender', 'stalled']);
      process.kill(pid, 'SIGCONT');
      // It resumes and beats: visible again with its mail.
      store.save(member('stalled', { pid, ...(started ? { started } : {}), updatedAt: later }));
      expect(store.list(later).map((each) => each.id).sort()).toEqual(['sender', 'stalled']);
      expect(store.pending('stalled').map((each) => each.text)).toEqual(['are you there?']);
    } finally {
      store.close();
    }
  });

  it('lists without writing: a gone or silent member stays in the database until the sweep', () => {
    const store = TeamStore.open(repo(), path.join(tmp(), 'team.sqlite'));
    try {
      store.save(member('alive'));
      store.save(member('gone', { pid: deadPid }));
      store.save(member('silent', { updatedAt: Date.now() - 120_000 }));
      const before = rows(store);
      expect(store.list().map((each) => each.id)).toEqual(['alive']);
      expect(rows(store)).toEqual(before);
      store.sweep();
      expect(rows(store)).toEqual([{ id: 'alive' }, { id: 'silent' }]);
    } finally {
      store.close();
    }
  });

  it('dead-letters the unread mail of a gone member at the sweep, and of one that leaves', () => {
    const store = TeamStore.open(repo(), path.join(tmp(), 'team.sqlite'));
    try {
      store.save(member('sender'));
      store.save(member('gone', { pid: process.pid }));
      const id = store.send('sender', ['gone'], 'unread', { replyRequired: true }).id!;
      // The recipient's process dies after the mail was stored.
      (store as unknown as { db: { prepare(sql: string): { run(...args: unknown[]): unknown } } }).db.prepare('UPDATE agents SET pid = ? WHERE id = ?').run(deadPid, 'gone');
      expect(store.recent()[0]).toMatchObject({ id, state: 'queued' });
      store.sweep();
      expect(store.names()).not.toContain('gone');
      expect(store.recent()[0]).toMatchObject({ id, state: 'dead-lettered' });
    } finally {
      store.close();
    }
  });

  it('treats a row whose pid now names a later process as gone', () => {
    const store = TeamStore.open(repo(), path.join(tmp(), 'team.sqlite'));
    try {
      // The parent process runs, but it did not start when this row says: the recorded process is gone.
      store.save(member('reused', { pid: process.ppid, started: '1' }));
      store.save(member('real', { ...other() }));
      store.sweep();
      expect(store.names().sort()).toEqual(['real']);
      expect(store.list().map((each) => each.id)).toEqual(['real']);
    } finally {
      store.close();
    }
  });

  it('does not retire a member that joined again under the name between the judgment and the removal', () => {
    const store = TeamStore.open(repo(), path.join(tmp(), 'team.sqlite'));
    try {
      store.save(member('inky', { pid: deadPid }));
      store.join(member('inky', { goal: 'new owner' }));
      store.sweep();
      expect(store.names()).toEqual(['inky']);
      expect(store.list()[0]).toMatchObject({ id: 'inky', pid: process.pid });
    } finally {
      store.close();
    }
  });
});
