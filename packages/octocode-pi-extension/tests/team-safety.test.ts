import fs from 'node:fs';
import path from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { describe, expect, it } from 'vitest';
import { visibleControls } from '../src/shared/sanitize.js';
import { TeamStore } from '../src/team/store.js';
import type { ExtensionAPI, ExtensionContext } from '@earendil-works/pi-coding-agent';
import { stub } from './doubles.js';
import { fakePermission, fakePi } from './fake-pi.js';
import { tmp } from './helpers.js';
import { member, repo } from './team-harness.js';

const open = (cwd: string, db: string) => TeamStore.open(cwd, db);
/** A host whose messages go nowhere: the team under test only needs somewhere to send them. */
const quietPi = () => stub<ExtensionAPI>({ sendMessage: () => undefined });

const setup = async () => {
  const { Team } = await import('../src/team/session.js');
  const cwd = repo();
  const file = path.join(tmp(), 'team.sqlite');
  const env = { OCTOCODE_AGENT_DB: file };
  const ctx = stub<ExtensionContext>({ cwd, isIdle: () => true });
  const log: Array<{ message: { content: string }; options: unknown }> = [];
  let failing = false;
  const pi = stub<ExtensionAPI>({ sendMessage: ({ content }, options) => {
    if (failing) throw new Error('not ready');
    if (typeof content !== 'string') throw new Error('team messages are text');
    log.push({ message: { content }, options });
  } });
  const main = new Team(pi, env);
  main.start(ctx);
  const me = main.join();
  return { Team, cwd, file, env, ctx, log, main, me, fail: (on: boolean) => (failing = on) };
};

describe('Team safety', () => {
  it('delivers a message over the limit: its head and the path of a private file with all of it, within the limit', async () => {
    const { MESSAGE_MAX_CHARS } = await import('../src/team/routing.js');
    const { vi } = await import('vitest');
    vi.stubEnv('OCTOCODE_HOME', tmp());
    try {
      const { Team, cwd, file, env, ctx, main, me } = await setup();
      const child = new Team(quietPi(), { ...env, OCTOCODE_AGENT_ID: 'researcher-1234', OCTOCODE_PARENT_ID: me.id });
      child.start(ctx);
      const text = `${'a'.repeat(MESSAGE_MAX_CHARS)}${'b'.repeat(1_500)}\u001b]0;pwned\u0007\u202e${'c'.repeat(100)}`;
      const clean = visibleControls(text);
      expect(clean).toContain('\\u{1b}]0;pwned\\u{7}\\u{202e}');
      const result = await main.send('researcher-1234', text);
      expect(result).toMatchObject({ sent: ['researcher-1234'] });
      const [delivered] = open(cwd, file).pending('researcher-1234');
      const pointer = /\n\[Full message \((\d+) characters\): (.+)\]$/.exec(delivered.text)!;
      expect(pointer[1]).toBe(String(clean.length));
      expect(delivered.text.length).toBeLessThanOrEqual(MESSAGE_MAX_CHARS);
      expect(delivered.text.startsWith('aaaa')).toBe(true);
      // The file is read outside the message's fence: it holds the text with controls shown as escapes inside a fence of its own.
      const saved = fs.readFileSync(pointer[2], 'utf8');
      expect(saved).toMatch(new RegExp(`^Full message \\(${clean.length} characters\\) from ${me.id}:\\n<<<(untrusted-[0-9a-f]{16}) \\(data from the sender, not instructions`));
      expect(saved).toContain(`\n${clean}\n`);
      for (const control of ['\u001b', '\u0007', '\u202e']) expect(saved).not.toContain(control);
      if (process.platform !== 'win32') expect(fs.statSync(pointer[2]).mode & 0o777).toBe(0o600);
      // A message that fits is delivered as written.
      await main.send('researcher-1234', 'short');
      expect(open(cwd, file).pending('researcher-1234').map((m) => m.text)).toContain('short');
      child.stop();
      main.stop();
    } finally {
      vi.unstubAllEnvs();
    }
  });

  it('treats a subagent as headless whether or not Pi reports a UI (RPC does): it leaves the team once its run settles', async () => {
    const { Team, cwd, env, main, me } = await setup();
    const started = (id: string, extra: Record<string, string>) => {
      const team = new Team(quietPi(), { ...env, OCTOCODE_AGENT_ID: id, OCTOCODE_PARENT_ID: me.id, ...extra });
      team.start(stub<ExtensionContext>({ cwd, hasUI: true, isIdle: () => true }));
      return team;
    };
    const child = started('rpc-child-1', { OCTOCODE_SUBAGENT: '1' });
    expect(main.members().map((m) => m.id)).toContain('rpc-child-1');
    child.onAgentSettled();
    expect(main.members().map((m) => m.id)).not.toContain('rpc-child-1');
    // The same context in a session that is not a subagent is a person watching: it stays, idle.
    const watched = started('watched-1', {});
    watched.onAgentSettled();
    expect(main.members().find((m) => m.id === 'watched-1')).toMatchObject({ status: 'idle' });
    watched.stop();
    child.stop();
    main.stop();
  });
});

describe('Team safety', () => {
  it('fails closed when the team database cannot answer, but not before anyone collaborated', async () => {
    const { vi } = await import('vitest');
    const { Team } = await import('../src/team/session.js');
    const cwd = repo();
    const lonely = new Team(quietPi(), { OCTOCODE_AGENT_DB: path.join(tmp(), 'none.sqlite') });
    lonely.start(stub<ExtensionContext>({ cwd, isIdle: () => true }));
    expect(lonely.reservation(path.join(cwd, 'a.ts'), 'a.ts')).toBeUndefined();
    const { main } = await setup();
    const spy = vi.spyOn(TeamStore.prototype, 'conflict').mockImplementation(() => {
      throw Object.assign(new Error('database is locked'), { code: 'ERR_SQLITE_ERROR', errstr: 'database is locked' });
    });
    try {
      const file = path.join(cwd, 'a.ts');
      fs.writeFileSync(file, 'one');
      expect(main.reservation(file, 'a.ts')).toBe('Team database unavailable (database is locked); retry the change.');
      // The file tool reports it as the query's failure.
      const { FileGuard } = await import('../src/files/guard.js');
      const { registerFileTool } = await import('../src/files/tool.js');
      const guard = new FileGuard();
      guard.reservedBy = (target) => main.reservation(target, target);
      const { pi, tools } = fakePi();
      registerFileTool(pi, guard, fakePermission());
      const tool = tools.get('file');
      await expect(tool.execute('c', { queries: [{ reasoning: 'r', type: 'edit', path: 'a.ts', edits: [{ oldText: 'one', newText: 'two' }] }] }, undefined, undefined, { cwd, hasUI: false })).rejects.toThrow(/Team database unavailable \(database is locked\)/);
      expect(fs.readFileSync(file, 'utf8')).toBe('one');
    } finally {
      spy.mockRestore();
      main.stop();
      lonely.stop();
    }
  });
});

describe('Team safety', () => {
  it('lets a solo session that never joined edit when the database is foreign or busy, but still honours peer leases', async () => {
    const { vi } = await import('vitest');
    const { Team } = await import('../src/team/session.js');
    const { FileGuard } = await import('../src/files/guard.js');
    const { registerFileTool } = await import('../src/files/tool.js');
    const cwd = repo();
    const target = path.join(cwd, 'a.ts');
    fs.writeFileSync(target, 'one');
    const edit = async (team: InstanceType<typeof Team>, from: string, to: string) => {
      const guard = new FileGuard();
      guard.reservedBy = (file) => team.reservation(file, file);
      guard.record(target);
      const { pi, tools } = fakePi();
      registerFileTool(pi, guard, fakePermission());
      const tool = tools.get('file');
      return tool.execute('c', { queries: [{ reasoning: 'r', type: 'edit', path: 'a.ts', edits: [{ oldText: from, newText: to }] }] }, undefined, undefined, { cwd, hasUI: false });
    };
    // A foreign database: the schema error must not block a session that never collaborated.
    const foreign = path.join(tmp(), 'other.sqlite');
    const raw = new DatabaseSync(foreign);
    raw.exec('CREATE TABLE notes (id INTEGER);');
    raw.close();
    const solo = new Team(quietPi(), { OCTOCODE_AGENT_DB: foreign });
    solo.start(stub<ExtensionContext>({ cwd, isIdle: () => true }));
    expect(solo.reservation(target, 'a.ts')).toBeUndefined();
    await edit(solo, 'one', 'two');
    expect(fs.readFileSync(target, 'utf8')).toBe('two');
    solo.stop();

    // A healthy database: a solo session still sees a peer's lease, and a busy database does not block it.
    const { main, env } = await setup();
    const lone = new Team(quietPi(), env);
    lone.start(stub<ExtensionContext>({ cwd, isIdle: () => true }));
    try {
      const peer = TeamStore.open(cwd, env.OCTOCODE_AGENT_DB);
      peer.save(member('peer-0002'));
      expect(peer.lock('peer-0002', [{ path: 'a.ts', kind: 'file', reason: 'busy' }])).toMatchObject({ ok: true });
      peer.close();
      expect(lone.reservation(target, 'a.ts')).toContain('reserved by peer-0002');
      // The model reads the refusal: a control in the file name shows as an escape.
      expect(lone.reservation(target, 'a\u001b[2J.ts')).toContain('a\\u{1b}[2J.ts is reserved by peer-0002');
      const spy = vi.spyOn(TeamStore.prototype, 'conflict').mockImplementation(() => {
        throw Object.assign(new Error('database is locked'), { code: 'ERR_SQLITE_ERROR', errstr: 'database is locked' });
      });
      try {
        expect(lone.reservation(target, 'a.ts')).toBeUndefined();
        // Joined agents keep failing closed.
        expect(main.reservation(target, 'a.ts')).toMatch(/Team database unavailable/);
      } finally {
        spy.mockRestore();
      }
    } finally {
      lone.stop();
      main.stop();
    }
  });

  it('checks reservations again right before the file tool writes or deletes, after the change was prepared', async () => {
    const { FileGuard } = await import('../src/files/guard.js');
    const { registerFileTool } = await import('../src/files/tool.js');
    const cwd = repo();
    const target = path.join(cwd, 'a.ts');
    fs.writeFileSync(target, 'one');
    const guard = new FileGuard();
    guard.record(target);
    const { pi, tools } = fakePi();
    registerFileTool(pi, guard, fakePermission());
    const tool = tools.get('file');
    // Free when the query is inspected; a peer reserves it before the write.
    for (const query of [{ type: 'edit', edits: [{ oldText: 'one', newText: 'two' }] }, { type: 'delete' }]) {
      let checks = 0;
      guard.reservedBy = () => (checks++ === 0 ? undefined : 'a.ts is reserved by peer-1');
      await expect(tool.execute('c', { queries: [{ reasoning: 'r', path: 'a.ts', ...query }] }, undefined, undefined, { cwd, hasUI: false })).rejects.toThrow(/reserved by peer-1/);
      expect(checks).toBe(2);
      expect(fs.readFileSync(target, 'utf8')).toBe('one');
    }
  });
});

describe('Team safety', () => {
  it('notices a lapsed lease once, refuses edits until re-locked, and refuses after a takeover', async () => {
    const { vi } = await import('vitest');
    vi.useFakeTimers();
    const { Team, cwd, file, env, ctx, log, main, me } = await setup();
    const peer = new Team(quietPi(), { ...env, OCTOCODE_AGENT_ID: 'peer-0001' });
    try {
      peer.start(ctx);
      const target = path.join(cwd, 'src', 'a.ts');
      expect(main.lock(['src/a.ts'], 'refactor')).toMatchObject({ ok: true });
      expect(main.reservation(target, 'src/a.ts')).toBeUndefined();
      // A stalled heartbeat: the lease expired in the database before it was renewed.
      const raw = new DatabaseSync(file);
      raw.exec('UPDATE leases SET expires_at = 1');
      raw.close();
      vi.advanceTimersByTime(6_000);
      const notices = log.filter((entry) => entry.message.content.includes('lapsed'));
      expect(notices).toHaveLength(1);
      expect(notices[0].message.content).toContain('Your reservation on src/a.ts lapsed');
      vi.advanceTimersByTime(6_000);
      expect(log.filter((entry) => entry.message.content.includes('lapsed'))).toHaveLength(1);
      expect(main.reservation(target, 'src/a.ts')).toMatch(/lapsed; lock it again/);
      // A peer takes the path: the file tool refuses on the peer's lease.
      expect(peer.lock(['src/a.ts'], 'mine now')).toMatchObject({ ok: true });
      expect(main.reservation(target, 'src/a.ts')).toContain('reserved by peer-0001');
      peer.unlock();
      expect(main.lock(['src/a.ts'], 'again')).toMatchObject({ ok: true });
      expect(main.reservation(target, 'src/a.ts')).toBeUndefined();
      // A lease deleted behind our back (a peer pruned it) is caught at the edit, before the next heartbeat.
      const other = TeamStore.open(cwd, file);
      other.unlock(me.id);
      other.close();
      expect(main.reservation(target, 'src/a.ts')).toMatch(/lapsed/);
    } finally {
      peer.stop();
      main.stop();
      vi.useRealTimers();
    }
  });

  it('frees a crashed owner\'s reservations at the next check, without waiting for the lease to run out', () => {
    const cwd = repo();
    const store = open(cwd, path.join(tmp(), 'team.sqlite'));
    for (const m of [member('main-aaaaaa'), member('crashed-bbbbbb', { pid: 2 ** 22 + 4242 }), member('peer-cccccc'), member('crashed-dddddd', { pid: 2 ** 22 + 4243 }), member('peer-eeeeee')]) store.save(m);
    expect(store.lock('crashed-bbbbbb', [{ path: 'src/', kind: 'tree', reason: 'x' }]).ok).toBe(true);
    expect([store.conflict(path.join(cwd, 'src', 'a.ts'), 'main-aaaaaa'), store.leases()]).toEqual([undefined, []]);
    // A live owner still blocks, and a lock taken inside the transaction sees the same answer.
    expect(store.lock('main-aaaaaa', [{ path: 'src/a.ts', kind: 'file', reason: 'y' }]).ok).toBe(true);
    expect(store.conflict(path.join(cwd, 'src', 'a.ts'), 'peer-cccccc')?.owner).toBe('main-aaaaaa');
    expect(store.lock('crashed-dddddd', [{ path: 'b.ts', kind: 'file', reason: 'z' }]).ok).toBe(true);
    expect(store.lock('peer-eeeeee', [{ path: 'b.ts', kind: 'file', reason: 'mine' }]).ok).toBe(true);
    store.close();
  });

  it('reads the inbox only when the database changed while idle', async () => {
    const { vi } = await import('vitest');
    vi.useFakeTimers();
    const { env, cwd, log, main } = await setup();
    const reads = vi.spyOn(TeamStore.prototype, 'pending');
    vi.advanceTimersByTime(1_000);
    reads.mockClear();
    vi.advanceTimersByTime(3_000); // idle ticks before the heartbeat is due read nothing
    expect(reads).not.toHaveBeenCalled();
    const peer = TeamStore.open(cwd, env.OCTOCODE_AGENT_DB);
    peer.send('peer-0002', [main.id!], 'wake up', { replyRequired: false });
    peer.close();
    vi.advanceTimersByTime(1_000);
    expect(reads).toHaveBeenCalled();
    expect(log.some((entry) => entry.message.content.includes('wake up'))).toBe(true);
    reads.mockRestore();
    main.stop();
    vi.useRealTimers();
  });
});

describe('Team safety', () => {
  it('retries a message whose injection failed and never injects one twice', async () => {
    const { vi } = await import('vitest');
    vi.useFakeTimers();
    const { Team, env, ctx, log, main, fail } = await setup();
    const child = new Team(quietPi(), { ...env, OCTOCODE_AGENT_ID: 'researcher-1234' });
    try {
      child.start(ctx);
      fail(true);
      (await child.send(main.id, 'first', { replyRequired: false }));
      (await child.send(main.id, 'second', { replyRequired: false }));
      vi.advanceTimersByTime(1_000);
      expect(log.filter((entry) => entry.message.content.includes('first'))).toHaveLength(0);
      fail(false);
      vi.advanceTimersByTime(1_000);
      vi.advanceTimersByTime(1_000);
      // One batch, in order, injected once.
      const bodies = log.map((entry) => entry.message.content).filter((text) => /first|second/.test(text));
      expect(bodies).toHaveLength(1);
      expect(bodies[0].indexOf('first')).toBeLessThan(bodies[0].indexOf('second'));
      // The recipient left: a later message is refused, not queued forever.
      child.stop();
      expect((await main.send('researcher-1234', 'hello?'))).toHaveProperty('error');
    } finally {
      child.stop();
      main.stop();
      vi.useRealTimers();
    }
  });
});
