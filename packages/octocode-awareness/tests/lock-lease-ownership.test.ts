import { DatabaseSync } from 'node:sqlite';
import { execFileSync } from 'node:child_process';
import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { createAwarenessClient } from '../src/client.js';
import { openAwarenessStore } from '../src/coordination/open.js';

async function fixture(prefix: string) {
  const workspace = await mkdtemp(join(tmpdir(), prefix));
  return { workspace, database: join(workspace, 'awareness.sqlite3') };
}

function git(cwd: string, args: string[]) {
  execFileSync('git', args, { cwd, stdio: 'ignore' });
}

describe('lock lease ownership', () => {
  it('canonicalizes equivalent target paths and preserves owner isolation', async () => {
    const { workspace, database } = await fixture('awareness-lock-path-');
    const owner = openAwarenessStore({ workspace, dbPath: database });
    try {
      owner.acquireLock({ filePath: './src/../src/shared.ts', agentId: 'owner', reason: 'audit', testPlan: 'audit' });
      expect(() => owner.acquireLock({ filePath: 'src/shared.ts', agentId: 'other', reason: 'audit', testPlan: 'audit' }))
        .toThrow(/conflict/);

      const result = await createAwarenessClient({ workspace, database, agentId: 'other' }).execute({
        operation: 'work.protect', params: { action: 'release', run_id: owner.listLocks()[0]!.runId },
      });
      expect(result.exitCode).toBe(1);
      expect(owner.listLocks()).toHaveLength(1);
    } finally {
      owner.close();
      await rm(workspace, { recursive: true, force: true });
    }
  });

  it('does not revive an expired work lease through work touch', async () => {
    const { workspace, database } = await fixture('awareness-lock-expiry-');
    const store = openAwarenessStore({ workspace, dbPath: database });
    try {
      const work = store.startWork({ filePath: 'expired.ts', agentId: 'owner', reason: 'audit', testPlan: 'audit', ttlSeconds: 60 });
      const db = new DatabaseSync(database);
      try {
        db.prepare('UPDATE run_files SET expires_at = ? WHERE run_id = ?').run('2000-01-01T00:00:00Z', work.runId);
      } finally {
        db.close();
      }

      const result = await createAwarenessClient({ workspace, database, agentId: 'owner' }).execute({
        operation: 'work.update', params: { transition: 'touch', run_id: work.runId, file: ['expired.ts'] },
      });
      expect(result.exitCode).toBe(1);
    } finally {
      store.close();
      await rm(workspace, { recursive: true, force: true });
    }
  });

  it('does not broaden a targeted release to every file in the run', async () => {
    const { workspace, database } = await fixture('awareness-lock-release-');
    const store = openAwarenessStore({ workspace, dbPath: database });
    try {
      const first = store.acquireLock({ filePath: 'a.ts', agentId: 'owner', reason: 'audit', testPlan: 'audit' });
      const client = createAwarenessClient({ workspace, database, agentId: 'owner' });
      const second = await client.execute({
        operation: 'work.protect', params: {
          action: 'acquire',
          run_id: first.runId,
          target_file: ['b.ts'],
          rationale: 'audit',
          test_plan: 'audit',
        },
      });
      expect(second.exitCode).toBe(0);
      expect(store.listLocks()).toHaveLength(2);

      const release = await client.execute({
        operation: 'work.protect', params: { action: 'release', run_id: first.runId, target_file: ['missing.ts'] },
      });
      expect(release.exitCode).toBe(1);
      expect(store.listLocks()).toHaveLength(2);
    } finally {
      store.close();
      await rm(workspace, { recursive: true, force: true });
    }
  });

  it('keeps locks independent across linked Git worktrees sharing one ledger', async () => {
    const repo = await mkdtemp(join(tmpdir(), 'awareness-lock-worktree-'));
    const linked = `${repo}-linked`;
    const database = join(repo, 'awareness.sqlite3');
    let mainStore: ReturnType<typeof openAwarenessStore> | undefined;
    let linkedStore: ReturnType<typeof openAwarenessStore> | undefined;
    try {
      await mkdir(join(repo, 'src'));
      await writeFile(join(repo, 'src/shared.ts'), 'export const value = 1;');
      git(repo, ['init', '-b', 'main']);
      git(repo, ['config', 'user.email', 'audit@example.invalid']);
      git(repo, ['config', 'user.name', 'Awareness Audit']);
      git(repo, ['add', '.']);
      git(repo, ['commit', '-m', 'fixture']);
      git(repo, ['worktree', 'add', linked, '-b', 'audit-linked']);

      mainStore = openAwarenessStore({ workspace: repo, dbPath: database });
      linkedStore = openAwarenessStore({ workspace: linked, dbPath: database });
      mainStore.acquireLock({ filePath: 'src/shared.ts', agentId: 'main', reason: 'audit', testPlan: 'audit' });
      expect(() => linkedStore!.acquireLock({ filePath: 'src/shared.ts', agentId: 'linked', reason: 'audit', testPlan: 'audit' }))
        .not.toThrow();
      expect(mainStore.listLocks()).toHaveLength(1);
      expect(linkedStore.listLocks()).toHaveLength(1);
    } finally {
      linkedStore?.close();
      mainStore?.close();
      try { git(repo, ['worktree', 'remove', '--force', linked]); } catch { /* fixture cleanup */ }
      await rm(linked, { recursive: true, force: true });
      await rm(repo, { recursive: true, force: true });
    }
  });
});
