import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { execSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb } from '../src/db-init.js';
import { insertMemory } from '../src/memory-write.js';
import { getMemory } from '../src/memory-recall.js';

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

function tempGitRepo(prefix: string): string {
  const root = mkdtempSync(join(tmpdir(), prefix));
  execSync('git init -q', { cwd: root });
  execSync('git config user.email t@t.test', { cwd: root });
  execSync('git config user.name t', { cwd: root });
  writeFileSync(join(root, 'README.md'), 'seed');
  execSync('git add -A && git commit -q -m seed', { cwd: root });
  return root;
}

describe('getMemory allWorkspaces (cross-workspace recall)', () => {
  it('--all-workspaces ignores workspace_path scoping and returns memories from every workspace', () => {
    const db = freshDb();
    insertMemory(db, {
      agentId: 'a', taskContext: 'alpha task', observation: 'unique cross-workspace marker alpha-7q3kp',
      importance: 7, label: 'GOTCHA', workspacePath: '/ws/alpha',
    });
    insertMemory(db, {
      agentId: 'a', taskContext: 'beta task', observation: 'unique cross-workspace marker beta-7q3kp',
      importance: 7, label: 'GOTCHA', workspacePath: '/ws/beta',
    });
    insertMemory(db, {
      agentId: 'a', taskContext: 'gamma task', observation: 'unique cross-workspace marker gamma-7q3kp',
      importance: 7, label: 'GOTCHA', workspacePath: '/ws/gamma',
    });

    // Default recall scoped to alpha: alpha row only (no truly-global rows here).
    const scoped = getMemory(db, {
      query: 'cross-workspace marker 7q3kp', workspacePath: '/ws/alpha', limit: 20,
    });
    expect(scoped.count).toBe(1);
    expect(scoped.memories.every(m => m.workspace_path === '/ws/alpha')).toBe(true);
    expect(scoped.all_workspaces).toBe(false);

    // allWorkspaces returns all three regardless of the workspacePath passed in.
    const all = getMemory(db, {
      query: 'cross-workspace marker 7q3kp', workspacePath: '/ws/alpha', allWorkspaces: true, limit: 20,
    });
    expect(all.count).toBe(3);
    expect(all.all_workspaces).toBe(true);
    expect(new Set(all.memories.map(m => m.workspace_path)))
      .toEqual(new Set(['/ws/alpha', '/ws/beta', '/ws/gamma']));

    // globalOnly is the orthogonal mode: strictly all-NULL provenance -> none here.
    expect(getMemory(db, { query: 'cross-workspace marker 7q3kp', globalOnly: true, limit: 20 }).count)
      .toBe(0);
  });

  it('--all-workspaces composes with an explicit artifact filter (skips only workspace_path)', () => {
    const db = freshDb();
    insertMemory(db, {
      agentId: 'a', taskContext: 't', observation: 'compose marker ws-a9k shared',
      importance: 7, label: 'GOTCHA', workspacePath: '/ws/x', artifact: 'pkgA',
    });
    insertMemory(db, {
      agentId: 'a', taskContext: 't', observation: 'compose marker ws-a9k shared',
      importance: 7, label: 'GOTCHA', workspacePath: '/ws/y', artifact: 'pkgA',
    });
    insertMemory(db, {
      agentId: 'a', taskContext: 't', observation: 'compose marker ws-a9k shared',
      importance: 7, label: 'GOTCHA', workspacePath: '/ws/z', artifact: 'pkgB',
    });

    // allWorkspaces overrides the explicit --workspace /ws/x, but artifact=pkgA still applies.
    const res = getMemory(db, {
      query: 'compose marker ws-a9k shared', workspacePath: '/ws/x',
      allWorkspaces: true, artifact: 'pkgA', limit: 20,
    });
    expect(res.count).toBe(2);
    expect(res.memories.every(m => m.artifact === 'pkgA')).toBe(true);
    expect(new Set(res.memories.map(m => m.workspace_path))).toEqual(new Set(['/ws/x', '/ws/y']));
  });

  it('--all-workspaces skips repo/ref inferred from cwd git, finding a memory in a different git repo', () => {
    const db = freshDb();
    const repoA = tempGitRepo('oc-allws-a-');
    const repoB = tempGitRepo('oc-allws-b-');
    try {
      // Recorded in repoA: fillScope stores workspace_path=repoA root, repo=repoA, ref=repoA branch.
      insertMemory(db, {
        agentId: 'a', taskContext: 'cross-repo', observation: 'unique crossrepo marker k9p2 all-workspaces',
        importance: 8, label: 'GOTCHA', cwd: repoA,
      });

      // Default recall from repoB is scoped to repoB -> must NOT see repoA's memory.
      const scoped = getMemory(db, {
        query: 'unique crossrepo marker k9p2 all-workspaces', cwd: repoB, limit: 20,
      });
      expect(scoped.count).toBe(0);

      // all-workspaces from repoB must skip cwd-inferred repo/ref and find repoA's memory.
      const all = getMemory(db, {
        query: 'unique crossrepo marker k9p2 all-workspaces', cwd: repoB, allWorkspaces: true, limit: 20,
      });
      expect(all.count).toBe(1);
      expect(all.all_workspaces).toBe(true);
    } finally {
      rmSync(repoA, { recursive: true, force: true });
      rmSync(repoB, { recursive: true, force: true });
    }
  });

  it('applied_filters reports all_workspaces when explain is set', () => {
    const db = freshDb();
    insertMemory(db, {
      agentId: 'a', taskContext: 't', observation: 'audit marker qz1 explain',
      importance: 7, label: 'GOTCHA', workspacePath: '/ws/audit',
    });
    const res = getMemory(db, {
      query: 'audit marker qz1 explain', allWorkspaces: true, explain: true, limit: 10,
    });
    expect(res.all_workspaces).toBe(true);
    expect(res.applied_filters?.all_workspaces).toBe(true);
  });
});
