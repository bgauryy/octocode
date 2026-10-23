import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it, vi } from 'vitest';
import { processStartTime } from '../src/shared/process.js';
import { subagentProcessEnv } from '../src/subagents/process.js';
import { createWorktree } from '../src/subagents/worktree.js';
import { git, repo, reportOf, setup, useScriptedPi } from './subagent-tool-harness.js';

useScriptedPi();

describe('subagent scratch sweep', () => {
  it('keeps old folders whose owning Pi process is alive (another session\'s background subagent) and removes the rest', async () => {
    const s = setup();
    const root = path.join(s.cwd, '.octocode', 'tmp', 'agents');
    const old = (Date.now() - 8 * 24 * 3_600_000) / 1000;
    const folder = (name: string, owner?: number) => {
      const dir = path.join(root, name);
      fs.mkdirSync(dir, { recursive: true });
      if (owner !== undefined) fs.writeFileSync(path.join(dir, '.owner'), String(owner));
      fs.utimesSync(dir, old, old);
      return dir;
    };
    const otherSession = folder('worker-live', process.ppid);
    // A recorded start time guards against pid reuse: the pid runs, but it is not the process that made the folder.
    const started = processStartTime(process.pid);
    const reused = folder('worker-reused', process.pid);
    fs.writeFileSync(path.join(reused, '.owner'), `${process.pid} Thu Jan  1 00:00:00 1970`);
    const sameProcess = folder('worker-same', process.pid);
    fs.writeFileSync(path.join(sameProcess, '.owner'), started ? `${process.pid} ${started}` : String(process.pid));
    const dead = folder('worker-dead', 9_999_999);
    const unowned = folder('worker-unowned');
    const fresh = path.join(root, 'worker-fresh');
    fs.mkdirSync(fresh);
    await s.fire('session_start', { type: 'session_start' }, s.ctx);
    await vi.waitFor(() => expect([dead, unowned, ...(started === undefined ? [] : [reused])].some((dir) => fs.existsSync(dir))).toBe(false));
    expect(fs.existsSync(otherSession)).toBe(true);
    expect(fs.existsSync(sameProcess)).toBe(true);
    expect(fs.existsSync(reused)).toBe(started === undefined);
    expect(fs.existsSync(fresh)).toBe(true);
    expect(fs.existsSync(dead)).toBe(false);
    expect(fs.existsSync(unowned)).toBe(false);
    await s.fire('session_shutdown', { type: 'session_shutdown' }, s.ctx);
  });
});

describe('isolated subagents and /agents merge', () => {
  it('lands the child\'s edits on its ref and merges them with /agents merge', async () => {
    const dir = repo();
    const s = setup(dir);
    const merge = { handler: (args: string, ctx: unknown) => s.commands.get('agents').handler(`merge ${args}`, ctx) };
    await merge.handler('', s.ctx);
    expect(s.ctx.ui.notes.at(-1)).toEqual({ message: 'Usage: /agents merge <id>. No isolated results to merge.', type: 'warning' });

    const result = await s.run({ task: 'edit a file', isolate: true });
    const id: string = result.details.id;
    expect(result.content[0].text).toContain(`/agents merge ${id}`);
    expect(fs.existsSync(path.join(dir, 'child.txt'))).toBe(false);
    await merge.handler('', s.ctx);
    expect(s.ctx.ui.notes.at(-1)!.message).toBe(`Usage: /agents merge <id>. Unmerged: ${id}`);

    await merge.handler(id, s.ctx);
    expect(s.ctx.ui.notes.at(-1)!.type).toBe('info');
    expect(fs.readFileSync(path.join(dir, 'child.txt'), 'utf8')).toBe('from the child\n');
    await merge.handler('bad id!', s.ctx);
    expect(s.ctx.ui.notes.at(-1)!.type).toBe('error');

    // Background isolated runs report the outcome in their message: child.txt is already merged, so nothing changed.
    await s.run({ task: 'edit in background', isolate: true, background: true });
    await vi.waitFor(() => expect(s.sent.at(-1)?.message.content).toMatch(/finished[\s\S]*changed no files/), { timeout: 15_000 });
    await expect(s.run({ task: 'fail isolated', isolate: true })).rejects.toThrow(/Subagent exited with code 3/);
    await s.emit('session_start', {}, s.ctx);
  }, 60_000);
});

describe('trust for isolated subagents', () => {
  it('hands the repository to an isolated child only when this session trusts its project config', async () => {
    const trusted = repo();
    const s = setup(trusted);
    const facts = reportOf((await s.run({ task: 'report', isolate: true })).content[0].text);
    expect(facts).toMatchObject({ workspace: trusted, trustRoot: trusted });
    expect(reportOf((await s.run({ task: 'report' })).content[0].text).trustRoot).toBeUndefined();

    const undecided = repo();
    fs.mkdirSync(path.join(undecided, '.claude'));
    fs.writeFileSync(path.join(undecided, '.claude', 'settings.json'), '{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"node"}]}]}}');
    git(undecided, 'add', '-A');
    git(undecided, 'commit', '-q', '-m', 'hooks');
    const u = setup(undecided);
    const child = reportOf((await u.run({ task: 'report', isolate: true })).content[0].text);
    expect(child).toMatchObject({ workspace: undecided });
    expect(child.trustRoot).toBeUndefined();
  }, 30_000);

  it('never passes an inherited trust root on, and sets one only when asked', () => {
    const inherited = subagentProcessEnv(undefined, { OCTOCODE_TRUST_ROOT: '/elsewhere' }, { id: 'general-1234', workspace: '/repo' });
    expect(inherited).toMatchObject({ OCTOCODE_TEAM_WORKSPACE: '/repo' });
    expect(inherited).not.toHaveProperty('OCTOCODE_TRUST_ROOT');
    expect(subagentProcessEnv(undefined, {}, { id: 'general-1234', workspace: '/repo', trustRoot: '/repo' })).toMatchObject({ OCTOCODE_TRUST_ROOT: '/repo' });
  });
});

describe('orphaned worktrees at session start', () => {
  it('saves what it can under the actual ref name and warns about a worktree it had to keep', async () => {
    const dir = repo();
    git(dir, 'update-ref', 'refs/octocode/pi/orphan-7', 'HEAD');
    const dead = spawnSync(process.execPath, ['-e', '0']).pid;
    const [saved, stuck] = [await createWorktree(dir, 'orphan-7'), await createWorktree(dir, 'stuck-7')];
    for (const worktree of [saved, stuck]) {
      fs.writeFileSync(path.join(worktree.path, `${worktree.agentId}.txt`), 'work\n');
      const meta = `${worktree.path}.json`;
      fs.writeFileSync(meta, JSON.stringify({ ...JSON.parse(fs.readFileSync(meta, 'utf8')), owners: [{ pid: dead }] }));
    }
    fs.writeFileSync(path.join(git(stuck.path, 'rev-parse', '--absolute-git-dir'), 'index.lock'), '');
    const s = setup(dir);
    await s.emit('session_start', {}, s.ctx);
    await vi.waitFor(() => expect(s.ctx.ui.notes.some((note) => note.type === 'warning')).toBe(true), { timeout: 10_000 });
    const note = s.ctx.ui.notes.find((entry) => entry.type === 'warning')!.message;
    expect(note).toContain('orphan-7 (changes on refs/octocode/pi/orphan-7-2)');
    expect(note).toContain(`Kept orphaned subagent worktree stuck-7`);
    expect(note).toContain(stuck.path);
    expect(fs.existsSync(path.join(stuck.path, 'stuck-7.txt'))).toBe(true);
  }, 30_000);
});
