import { execFileSync, spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { subagentProcessEnv } from '../src/subagents/process.js';
import { createWorktree, describePrune, finishWorktree, isolationReport, mergeAgent, pendingRefs, pruneWorktrees, recordWorktreePid, worktreesDir } from '../src/subagents/worktree.js';
import { tmp } from './helpers.js';

const git = (cwd: string, ...args: string[]) => execFileSync('git', args, { cwd, encoding: 'utf8' }).trim();

function repo(): string {
  const dir = fs.realpathSync(tmp('octocode-wt-repo-'));
  git(dir, 'init', '-q', '-b', 'main');
  git(dir, 'config', 'user.name', 'Test');
  git(dir, 'config', 'user.email', 'test@example.com');
  fs.mkdirSync(path.join(dir, 'src'));
  fs.writeFileSync(path.join(dir, 'src', 'a.txt'), 'one\ntwo\n');
  git(dir, 'add', '-A');
  git(dir, 'commit', '-q', '-m', 'init');
  return dir;
}

describe('isolated subagent worktrees', () => {
  it('lands the change on refs/octocode/pi/<id>, not the main tree, removes the worktree and merges with /agents merge', async () => {
    const dir = repo();
    const home = tmp();
    // A pre-commit hook that would fail proves hooks are disabled for the private commit and the merge.
    fs.writeFileSync(path.join(dir, '.git', 'hooks', 'pre-commit'), '#!/bin/sh\nexit 1\n', { mode: 0o755 });
    const worktree = await createWorktree(path.join(dir, 'src'), 'impl-1', home);
    expect(worktree.warning).toBeUndefined();
    expect(worktree.path.startsWith(worktreesDir(dir, home))).toBe(true);
    expect(worktree.cwd).toBe(path.join(worktree.path, 'src'));
    fs.writeFileSync(path.join(worktree.cwd, 'a.txt'), 'one\nTWO\n');
    fs.writeFileSync(path.join(worktree.cwd, 'new.txt'), 'new\n');

    const result = await finishWorktree(worktree);
    expect(result.ref).toBe('refs/octocode/pi/impl-1');
    expect(result.stat).toMatch(/2 files changed/);
    expect(isolationReport('impl-1', result)).toContain('/agents merge impl-1');
    expect(fs.readFileSync(path.join(dir, 'src', 'a.txt'), 'utf8')).toBe('one\ntwo\n');
    expect(fs.existsSync(path.join(dir, 'src', 'new.txt'))).toBe(false);
    expect(git(dir, 'show', 'refs/octocode/pi/impl-1:src/a.txt')).toBe('one\nTWO');
    expect(fs.existsSync(worktree.path)).toBe(false);
    expect(git(dir, 'worktree', 'list')).not.toContain(worktree.path);
    expect(await pendingRefs(dir)).toEqual(['impl-1']);

    const merged = await mergeAgent(dir, 'impl-1');
    expect(merged).toMatchObject({ ok: true });
    expect(fs.readFileSync(path.join(dir, 'src', 'a.txt'), 'utf8')).toBe('one\nTWO\n');
    expect(await pendingRefs(dir)).toEqual([]);
  });

  it('never overwrites an unmerged ref when an agent id is reused', async () => {
    const dir = repo();
    const home = tmp();
    for (const text of ['first\n', 'second\n']) {
      const worktree = await createWorktree(dir, 'dup-1', home);
      fs.writeFileSync(path.join(worktree.cwd, 'src', 'a.txt'), text);
      const result = await finishWorktree(worktree);
      if (text === 'second\n') {
        expect(result.ref).toBe('refs/octocode/pi/dup-1-2');
        expect(isolationReport('dup-1', result)).toContain('/agents merge dup-1-2');
      }
    }
    expect(git(dir, 'show', 'refs/octocode/pi/dup-1:src/a.txt')).toBe('first');
    expect(git(dir, 'show', 'refs/octocode/pi/dup-1-2:src/a.txt')).toBe('second');
  });

  it('reports no ref when nothing changed', async () => {
    const dir = repo();
    const worktree = await createWorktree(dir, 'idle-1', tmp());
    expect(await finishWorktree(worktree)).toEqual({});
    expect(fs.existsSync(worktree.path)).toBe(false);
  });

  it('aborts a conflicting merge, lists the paths and keeps the tree and ref', async () => {
    const dir = repo();
    const worktree = await createWorktree(dir, 'impl-2', tmp());
    fs.writeFileSync(path.join(worktree.path, 'src', 'a.txt'), 'one\nfrom agent\n');
    await finishWorktree(worktree);
    fs.writeFileSync(path.join(dir, 'src', 'a.txt'), 'one\nfrom user\n');
    git(dir, 'commit', '-q', '-am', 'user change');

    const merged = await mergeAgent(dir, 'impl-2');
    expect(merged.ok).toBe(false);
    expect(merged.text).toContain('conflicts in:\nsrc/a.txt');
    expect(merged.text).toContain('aborted');
    expect(git(dir, 'status', '--porcelain')).toBe('');
    expect(fs.readFileSync(path.join(dir, 'src', 'a.txt'), 'utf8')).toBe('one\nfrom user\n');
    expect(await pendingRefs(dir)).toEqual(['impl-2']);
  });

  it('refuses outside git, warns on a dirty tree and rejects unknown ids', async () => {
    await expect(createWorktree(tmp(), 'x-1', tmp())).rejects.toThrow(/needs a git repository/);
    const dir = repo();
    fs.writeFileSync(path.join(dir, 'src', 'a.txt'), 'dirty\n');
    const worktree = await createWorktree(dir, 'dirty-1', tmp());
    expect(worktree.warning).toMatch(/uncommitted changes/);
    expect(fs.readFileSync(path.join(worktree.path, 'src', 'a.txt'), 'utf8')).toBe('one\ntwo\n');
    await finishWorktree(worktree);
    expect(await mergeAgent(dir, 'nobody-1')).toMatchObject({ ok: false, text: expect.stringContaining('does not exist') });
    expect(await mergeAgent(dir, '../evil')).toMatchObject({ ok: false });
  });

  it('refuses an id whose kept worktree still exists, leaving its owner record intact', async () => {
    const dir = repo();
    const home = tmp();
    const kept = await createWorktree(dir, 'kept-1', home);
    const record = fs.readFileSync(`${kept.path}.json`, 'utf8');
    await expect(createWorktree(dir, 'kept-1', home)).rejects.toThrow(/already exists/);
    expect(fs.readFileSync(`${kept.path}.json`, 'utf8')).toBe(record);
    await finishWorktree(kept);
  });

  it('prunes worktrees whose owning process died, saving their changes to the ref', async () => {
    const dir = repo();
    const home = tmp();
    const worktree = await createWorktree(dir, 'orphan-1', home);
    const live = await createWorktree(dir, 'live-1', home);
    fs.writeFileSync(path.join(worktree.path, 'src', 'a.txt'), 'orphaned work\n');
    const dead = spawnSync(process.execPath, ['-e', '0']).pid;
    const meta = `${worktree.path}.json`;
    fs.writeFileSync(meta, JSON.stringify({ ...JSON.parse(fs.readFileSync(meta, 'utf8')), pids: [dead] }));

    expect(await pruneWorktrees(dir, home)).toEqual({ saved: [{ agentId: 'orphan-1', ref: 'refs/octocode/pi/orphan-1' }], kept: [] });
    expect(fs.existsSync(worktree.path)).toBe(false);
    expect(fs.existsSync(meta)).toBe(false);
    expect(git(dir, 'show', 'refs/octocode/pi/orphan-1:src/a.txt')).toBe('orphaned work');
    expect(fs.existsSync(live.path)).toBe(true);
    await finishWorktree(live);
  });

  it('leaves a worktree whose owner record is unreadable but fresh, and warns about it once stale', async () => {
    const dir = repo();
    const home = tmp();
    const worktree = await createWorktree(dir, 'midwrite-1', home);
    const meta = `${worktree.path}.json`;
    fs.writeFileSync(meta, '');
    expect(await pruneWorktrees(dir, home)).toEqual({ saved: [], kept: [] });
    expect(fs.existsSync(worktree.path)).toBe(true);
    const old = new Date(Date.now() - 120_000);
    fs.utimesSync(meta, old, old);
    // Once stale it is orphaned: its start commit is recovered from the main tree, so its edit is saved on one ref.
    fs.writeFileSync(path.join(worktree.path, 'new.txt'), 'kept work\n');
    expect(await pruneWorktrees(dir, home)).toEqual({ saved: [{ agentId: 'midwrite-1', ref: 'refs/octocode/pi/midwrite-1' }], kept: [] });
    expect(fs.existsSync(worktree.path)).toBe(false);
    expect(await pruneWorktrees(dir, home)).toEqual({ saved: [], kept: [] });
    expect(git(dir, 'for-each-ref', '--format=%(refname)', 'refs/octocode').trim()).toBe('refs/octocode/pi/midwrite-1');
  });

  it('removes an unchanged worktree with an unreadable stale record, writing no ref', async () => {
    const dir = repo();
    const home = tmp();
    const worktree = await createWorktree(dir, 'blank-1', home);
    fs.writeFileSync(`${worktree.path}.json`, '');
    const old = new Date(Date.now() - 120_000);
    fs.utimesSync(`${worktree.path}.json`, old, old);
    expect(await pruneWorktrees(dir, home)).toEqual({ saved: [{ agentId: 'blank-1' }], kept: [] });
    expect(git(dir, 'for-each-ref', 'refs/octocode').trim()).toBe('');
  });

  it('keeps a worktree while its recorded subagent pid is alive, even after the parent died', async () => {
    const dir = repo();
    const home = tmp();
    const worktree = await createWorktree(dir, 'child-1', home);
    const meta = `${worktree.path}.json`;
    const dead = spawnSync(process.execPath, ['-e', '0']).pid;
    fs.writeFileSync(meta, JSON.stringify({ ...JSON.parse(fs.readFileSync(meta, 'utf8')), pids: [dead] }));
    const child = spawn(process.execPath, ['-e', 'setTimeout(() => {}, 30000)'], { stdio: 'ignore' });
    try {
      recordWorktreePid(worktree, child.pid!);
      expect(JSON.parse(fs.readFileSync(meta, 'utf8')).pids).toEqual([dead, child.pid]);
      expect(await pruneWorktrees(dir, home)).toEqual({ saved: [], kept: [] });
      expect(fs.existsSync(worktree.path)).toBe(true);
    } finally {
      child.kill('SIGKILL');
      await new Promise((resolve) => child.once('exit', resolve));
    }
    expect((await pruneWorktrees(dir, home)).saved.map((entry) => entry.agentId)).toEqual(['child-1']);
    expect(fs.existsSync(worktree.path)).toBe(false);
  });

  it('keeps the worktree and its files when the changes cannot be committed, naming the kept path', async () => {
    const dir = repo();
    const home = tmp();
    const worktree = await createWorktree(dir, 'broken-1', home);
    fs.writeFileSync(path.join(worktree.path, 'src', 'a.txt'), 'precious\n');
    fs.writeFileSync(path.join(worktree.path, 'src', 'new.txt'), 'untracked work\n');
    // A held index lock makes `git add` fail.
    const lock = path.join(git(worktree.path, 'rev-parse', '--absolute-git-dir'), 'index.lock');
    fs.writeFileSync(lock, '');
    await expect(finishWorktree(worktree)).rejects.toThrow(`worktree is kept at ${worktree.path}`);
    expect(fs.readFileSync(path.join(worktree.path, 'src', 'a.txt'), 'utf8')).toBe('precious\n');
    expect(fs.readFileSync(path.join(worktree.path, 'src', 'new.txt'), 'utf8')).toBe('untracked work\n');
    expect(fs.existsSync(`${worktree.path}.json`)).toBe(true);
    expect(git(dir, 'for-each-ref', '--format=%(refname)', 'refs/octocode/pi/')).toBe('');

    fs.rmSync(lock);
    const result = await finishWorktree(worktree);
    expect(result.ref).toBe('refs/octocode/pi/broken-1');
    expect(git(dir, 'show', 'refs/octocode/pi/broken-1:src/new.txt')).toBe('untracked work');
    expect(fs.existsSync(worktree.path)).toBe(false);
  });

  it('keeps an orphan it cannot save and reports it, with the actual ref names of saved ones', async () => {
    const dir = repo();
    const home = tmp();
    // An earlier ref already holds lost-1: the orphan's save goes to lost-1-2.
    git(dir, 'update-ref', 'refs/octocode/pi/lost-1', 'HEAD');
    const saved = await createWorktree(dir, 'lost-1', home);
    const stuck = await createWorktree(dir, 'stuck-1', home);
    const dead = spawnSync(process.execPath, ['-e', '0']).pid;
    for (const worktree of [saved, stuck]) {
      fs.writeFileSync(path.join(worktree.path, 'src', 'a.txt'), `${worktree.agentId} work\n`);
      const meta = `${worktree.path}.json`;
      fs.writeFileSync(meta, JSON.stringify({ ...JSON.parse(fs.readFileSync(meta, 'utf8')), pids: [dead] }));
    }
    fs.writeFileSync(path.join(git(stuck.path, 'rev-parse', '--absolute-git-dir'), 'index.lock'), '');
    const report = await pruneWorktrees(dir, home);
    expect(report.kept).toEqual([{ agentId: 'stuck-1', error: expect.stringContaining(`worktree is kept at ${stuck.path}`) }]);
    expect(fs.readFileSync(path.join(stuck.path, 'src', 'a.txt'), 'utf8')).toBe('stuck-1 work\n');
    const ref = report.saved[0]!.ref!;
    expect(ref).toBe('refs/octocode/pi/lost-1-2');
    expect(git(dir, 'show', `${ref}:src/a.txt`)).toBe('lost-1 work');
    const note = describePrune(report)!;
    expect(note.level).toBe('warning');
    expect(note.text).toContain(ref);
    expect(note.text).toContain(stuck.path);
    expect(describePrune({ saved: [], kept: [] })).toBeUndefined();
    expect(describePrune({ saved: [{ agentId: 'x-1' }], kept: [] })).toEqual({ level: 'info', text: expect.stringContaining('x-1 (no changes)') });
  });

  it('keeps an isolated child on its parent team workspace', () => {
    expect(subagentProcessEnv(undefined, {}, { id: 'a-1', workspace: '/repo' })['OCTOCODE_TEAM_WORKSPACE']).toBe('/repo');
    expect(subagentProcessEnv(undefined, {}, { id: 'a-1' })).not.toHaveProperty('OCTOCODE_TEAM_WORKSPACE');
  });
});
