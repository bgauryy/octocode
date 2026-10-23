import path from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { LEASE_IDLE_MS } from '../src/team/held.js';
import { parentProcessGone } from '../src/team/parent.js';
import { LEASE_MS } from '../src/team/roster.js';
import { leaseAge } from '../src/team/routing.js';
import { Team } from '../src/team/session.js';
import { fakeCtx, fakePi } from './fake-pi.js';
import { tmp } from './helpers.js';
import { agent, closeTeams, fakeClock, member, openStore, repo, teamMail, trackTeam } from './team-harness.js';

afterEach(closeTeams);

describe('headless subagent exit', () => {
  it('stops a subagent whose parent process is gone through Pi\'s own SIGTERM shutdown', () => {
    vi.useFakeTimers();
    const kill = vi.spyOn(process, 'kill').mockImplementation((pid) => {
      if (pid === process.pid) return true;
      throw Object.assign(new Error('no such process'), { code: 'ESRCH' });
    });
    try {
      const db = path.join(tmp(), 'team.sqlite');
      const live = agent(db, repo(), { OCTOCODE_AGENT_ID: 'live-000001', OCTOCODE_PARENT_PID: String(process.pid) }, { hasUI: false });
      vi.advanceTimersByTime(1_500);
      expect(kill).not.toHaveBeenCalledWith(process.pid, 'SIGTERM');
      live.team.stop();
      agent(db, repo(), { OCTOCODE_AGENT_ID: 'orphan-000001', OCTOCODE_PARENT_PID: String(2 ** 22 + 12345) }, { hasUI: false });
      vi.advanceTimersByTime(1_500);
      expect(kill).toHaveBeenCalledWith(process.pid, 'SIGTERM');
    } finally {
      kill.mockRestore();
    }
  });

  it('stops a subagent whose parent died even when the parent pid is alive again (reused)', () => {
    vi.useFakeTimers();
    const kill = vi.spyOn(process, 'kill').mockImplementation(() => true);
    const parentPid = process.ppid;
    const ppid = vi.spyOn(process, 'ppid', 'get').mockReturnValue(parentPid);
    try {
      agent(path.join(tmp(), 'team.sqlite'), repo(), { OCTOCODE_AGENT_ID: 'child-000001', OCTOCODE_PARENT_PID: String(parentPid) }, { hasUI: false });
      vi.advanceTimersByTime(1_500);
      expect(kill).not.toHaveBeenCalledWith(process.pid, 'SIGTERM');
      // Reparented: the parent exited, whatever now owns its pid.
      ppid.mockReturnValue(1);
      vi.advanceTimersByTime(1_500);
      expect(kill).toHaveBeenCalledWith(process.pid, 'SIGTERM');
    } finally {
      ppid.mockRestore();
      kill.mockRestore();
    }
  });

  it('stops a subagent whose parent is gone even when it never joined the team', () => {
    vi.useFakeTimers();
    const kill = vi.spyOn(process, 'kill').mockImplementation((pid) => {
      if (pid === process.pid) return true;
      throw Object.assign(new Error('no such process'), { code: 'ESRCH' });
    });
    try {
      // No agent id: the child does not join, and the team database is never opened.
      const fake = fakePi();
      const team = new Team(fake.pi, { OCTOCODE_AGENT_DB: path.join(tmp(), 'missing', 'team.sqlite'), OCTOCODE_PARENT_PID: String(2 ** 22 + 12345) });
      const ctx = fakeCtx({ cwd: repo(), hasUI: false });
      team.start(ctx);
      trackTeam(team, ctx);
      expect(team.id).toBeUndefined();
      vi.advanceTimersByTime(1_500);
      expect(kill).toHaveBeenCalledWith(process.pid, 'SIGTERM');
    } finally {
      kill.mockRestore();
    }
  });

  it('tells a parent that reused its pid from the parent by its start time', () => {
    const ppid = vi.spyOn(process, 'ppid', 'get').mockReturnValue(1);
    try {
      // Through a launcher (the parent is not our ppid): the recorded start time decides.
      expect(parentProcessGone(`${process.pid} Thu Jan  1 00:00:00 1970`)).toBe(process.platform !== 'win32');
      expect(parentProcessGone(String(process.pid))).toBe(false);
      expect(parentProcessGone('garbage')).toBe(false);
    } finally {
      ppid.mockRestore();
    }
  });
});

describe('headless subagent exit', () => {
  it('keeps a child whose launcher exited while its parent still runs', () => {
    vi.useFakeTimers();
    const kill = vi.spyOn(process, 'kill').mockImplementation(() => true);
    const ppid = vi.spyOn(process, 'ppid', 'get').mockReturnValue(process.ppid);
    try {
      // The parent pid is not our ppid at load: something (a launcher) sat in between.
      agent(path.join(tmp(), 'team.sqlite'), repo(), { OCTOCODE_AGENT_ID: 'child-000002', OCTOCODE_PARENT_PID: String(process.pid) }, { hasUI: false });
      ppid.mockReturnValue(1);
      vi.advanceTimersByTime(1_500);
      expect(kill).not.toHaveBeenCalledWith(process.pid, 'SIGTERM');
    } finally {
      ppid.mockRestore();
      kill.mockRestore();
    }
  });

  it('takes no message while idle, so mail that arrives after its last turn is a dead letter, not acked unread', async () => {
    fakeClock();
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const parent = agent(db, cwd);
    const parentId = parent.team.join().id;
    const child = agent(db, cwd, { OCTOCODE_AGENT_ID: 'general-333333', OCTOCODE_PARENT_ID: parentId }, { hasUI: false, idle: true });
    child.team.join();
    const result = await parent.team.send('general-333333', 'one more thing', { replyRequired: false });
    if ('error' in result) throw new Error(result.error);
    const { id } = result;
    await vi.advanceTimersByTimeAsync(2_000);
    expect(teamMail(child.sent)).toEqual([]);
    await child.emit('agent_settled', {}, child.ctx);
    await vi.waitFor(() => expect(parent.sent.some((entry) => String(entry.message.content).startsWith(`Message #${id} to general-333333`))).toBe(true), { timeout: 8_000 });
  }, 10_000);

  it('takes messages while its run is active', async () => {
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const parent = agent(db, cwd);
    const parentId = parent.team.join().id;
    const child = agent(db, cwd, { OCTOCODE_AGENT_ID: 'general-444444', OCTOCODE_PARENT_ID: parentId }, { hasUI: false, idle: false });
    child.team.join();
    (await parent.team.send('general-444444', 'use the v2 API'));
    await vi.waitFor(() => expect(teamMail(child.sent)).toHaveLength(1), { timeout: 3_000 });
  });

  it('leaves the team when its run settles, so later messages are refused instead of acked unread', async () => {
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const parent = agent(db, cwd);
    const parentId = parent.team.join().id;
    const child = agent(db, cwd, { OCTOCODE_AGENT_ID: 'general-222222', OCTOCODE_PARENT_ID: parentId }, { hasUI: false });
    child.team.join();
    child.team.lock(['src/a.ts'], 'work');
    await child.emit('agent_settled', {}, child.ctx);
    expect(parent.team.members().map((m) => m.id)).not.toContain('general-222222');
    expect(parent.team.reservation(path.join(cwd, 'src/a.ts'), 'src/a.ts')).toBeUndefined();
    expect((await parent.team.send('general-222222', 'one more'))).toEqual({ error: expect.any(String) });
  });

  it('stays joined when a session with a UI goes idle', async () => {
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const main = agent(db, cwd);
    const id = main.team.join().id;
    await main.emit('agent_settled', {}, main.ctx);
    expect(main.team.members().map((m) => m.id)).toContain(id);
  });
});

describe('reservation lifetime', () => {
  it('lets a lease lapse unless its owner renews it', () => {
    const store = openStore(repo(), path.join(tmp(), 'team.sqlite'));
    const now = Date.now();
    // Its owner keeps heartbeating through the whole test: only the lease's own expiry is under test.
    store.save(member('one', { updatedAt: now + 2 * LEASE_MS }));
    store.lock('one', [{ path: 'a.ts', kind: 'file', reason: 'x' }], now);
    expect(store.conflict('a.ts', 'two', 'file', now + LEASE_MS - 1)).toBeDefined();
    expect(store.conflict('a.ts', 'two', 'file', now + LEASE_MS + 1)).toBeUndefined();
    store.renew('one', now + LEASE_MS - 1);
    expect(store.conflict('a.ts', 'two', 'file', now + LEASE_MS + 1)).toBeDefined();
  });

  it('stops renewing an idle session\'s reservations once it has been idle past the limit', () => {
    vi.useFakeTimers({ toFake: ['Date'] });
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const owner = agent(db, cwd);
    const peer = agent(db, cwd);
    owner.team.lock(['src/a.ts'], 'auth');
    peer.team.join();
    expect(peer.team.reservation(path.join(cwd, 'src/a.ts'), 'src/a.ts')).toMatch(/reserved by .* \(auth; locked \d+s ago\)/);
    // Idle past the limit: heartbeats continue (the row stays fresh) but the lease is not renewed and runs out.
    const until = Date.now() + LEASE_IDLE_MS + 2 * LEASE_MS;
    while (Date.now() < until) {
      vi.setSystemTime(Date.now() + 40_000);
      owner.team.join(); // a heartbeat-equivalent flush
      peer.team.join();
    }
    expect(peer.team.reservation(path.join(cwd, 'src/a.ts'), 'src/a.ts')).toBeUndefined();
    expect(owner.sent.some((entry) => /lapsed after \d+m idle/.test(String(entry.message.content)))).toBe(true);
  });

  it('formats the lock age', () => {
    expect(leaseAge({ id: 1, path: 'a', kind: 'file', owner: 'o', reason: '', acquiredAt: 0, expiresAt: 0 })).toBe('locked');
    expect(leaseAge({ id: 1, path: 'a', kind: 'file', owner: 'o', reason: '', acquiredAt: 1_000, expiresAt: 0 }, 1_000 + 12 * 60_000)).toMatch(/^locked 12m/);
  });

  it('ignores a lease whose owner has no agent row (it left)', () => {
    const store = openStore(repo(), path.join(tmp(), 'team.sqlite'));
    store.save(member('two'));
    expect(store.lock('ghost', [{ path: 'src/a.ts', kind: 'file', reason: '' }]).ok).toBe(true);
    expect(store.lock('two', [{ path: 'src/a.ts', kind: 'file', reason: '' }]).ok).toBe(true);
  });
});

describe('delivery edge cases', () => {
  it('leaves the team before ending a subagent whose parent is gone, so its row does not wait for a sweep', () => {
    vi.useFakeTimers();
    const kill = vi.spyOn(process, 'kill').mockImplementation((pid) => {
      if (pid === process.pid) return true;
      throw Object.assign(new Error('no such process'), { code: 'ESRCH' });
    });
    try {
      const db = path.join(tmp(), 'team.sqlite');
      const cwd = repo();
      const child = agent(db, cwd, { OCTOCODE_AGENT_ID: 'child-000003', OCTOCODE_PARENT_PID: String(2 ** 22 + 54321) }, { hasUI: false });
      expect(child.team.id).toBe('child-000003');
      vi.advanceTimersByTime(1_500);
      expect(kill).toHaveBeenCalledWith(process.pid, 'SIGTERM');
      expect(child.team.id).toBeUndefined();
      expect(openStore(cwd, db).list().map((m) => m.id)).not.toContain('child-000003');
    } finally {
      kill.mockRestore();
    }
  });

  it('compares a parent\'s start time only when asked, so a poll does not run ps every second', () => {
    const ppid = vi.spyOn(process, 'ppid', 'get').mockReturnValue(1);
    try {
      expect(parentProcessGone(`${process.pid} Thu Jan  1 00:00:00 1970`, false)).toBe(false);
    } finally {
      ppid.mockRestore();
    }
  });
});
