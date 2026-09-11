import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { realpathSync } from 'node:fs';
import { join } from 'node:path';
import { initDb } from '../src/db-init.js';
import { insertMemory } from '../src/memory-write.js';
import { getMemory } from '../src/memory-recall.js';
import { pruneStale } from '../src/maintenance-stale.js';
import { preFlightIntent } from '../src/intents-preflight.js';
import { fillScope } from '../src/git.js';

/**
 * Tests locking documented behavior to the runtime — every case here was a
 * documented feature the runtime silently ignored (2026-07-07 review).
 */

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

describe('getMemory explain', () => {
  it('attaches score_components whose weighted sum equals the score', async () => {
    const db = freshDb();
    (await insertMemory(db, { taskContext: 'auth router', observation: 'tenant order matters', importance: 8 }));
    const { memories } = (await getMemory(db, { query: 'auth router', limit: 1, explain: true }));
    const c = memories[0]!.score_components!;
    expect(c.final).toBeCloseTo(memories[0]!.score!, 10);
    const recomputed =
      c.weights.importance * c.importance + c.weights.recency * c.recency +
      c.weights.access * c.access + c.weights.lexical * c.relevance;
    expect(c.final).toBeCloseTo(recomputed, 10);
  });
});

describe('pruneStale — documented filters', () => {
  // Claim all files FIRST, then age the rows: preFlightIntent auto-prunes
  // expired locks on each call, so interleaved claim-then-age loses fixtures.
  function claimAll(db: DatabaseSync, specs: Array<{ agent: string; file: string }>): string[] {
    return specs.map(({ agent, file }) => {
      const claim = preFlightIntent(db, {
        agentId: agent, workspacePath: '/tmp/ws', rationale: 'r', testPlan: 't', targetFiles: [file],
      });
      if (!claim.ok) throw new Error('claim failed');
      return claim.run.run_id;
    });
  }
  function age(db: DatabaseSync, runId: string, minutesOld: number, expired: boolean) {
    const acquired = new Date(Date.now() - minutesOld * 60000).toISOString().replace(/\.\d{3}Z$/, 'Z');
    const expires = expired
      ? new Date(Date.now() - 60000).toISOString().replace(/\.\d{3}Z$/, 'Z')
      : new Date(Date.now() + 3600000).toISOString().replace(/\.\d{3}Z$/, 'Z');
    db.prepare('UPDATE awareness_locks SET acquired_at = ?, expires_at = ? WHERE run_id = ?')
      .run(acquired, expires, runId);
  }

  it('older-than-minutes also prunes old live locks; dry-run matches the real prune', () => {
    const db = freshDb();
    const [oldLive, expired, fresh] = claimAll(db, [
      { agent: 'a', file: '/tmp/ws/old-live.ts' },
      { agent: 'a', file: '/tmp/ws/expired.ts' },
      { agent: 'a', file: '/tmp/ws/fresh.ts' },
    ]);
    age(db, oldLive!, 60, false);
    age(db, expired!, 60, true);
    age(db, fresh!, 1, false);

    const preview = pruneStale(db, { older_than_minutes: 20, dry_run: true });
    expect(preview.would_prune).toBe(2);
    const real = pruneStale(db, { older_than_minutes: 20 });
    expect(real.pruned_locks).toBe(2);
  });

  it('expired-only ignores old live locks; agent/file filters narrow selection', () => {
    const db = freshDb();
    const [oldLive, expiredA, expiredB] = claimAll(db, [
      { agent: 'a', file: '/tmp/ws/old-live.ts' },
      { agent: 'a', file: '/tmp/ws/expired-a.ts' },
      { agent: 'b', file: '/tmp/ws/expired-b.ts' },
    ]);
    age(db, oldLive!, 60, false);
    age(db, expiredA!, 60, true);
    age(db, expiredB!, 60, true);

    expect(pruneStale(db, { older_than_minutes: 20, expired_only: true, dry_run: true }).would_prune).toBe(2);
    expect(pruneStale(db, { expired_only: true, agent_id: 'b', dry_run: true }).would_prune).toBe(1);
    const byFile = pruneStale(db, { expired_only: true, target_file: ['/tmp/ws/expired-a.ts'] });
    expect(byFile.pruned_locks).toBe(1);
  });
});

describe('fillScope — workspace-first git detection', () => {
  it('does not tag a non-git workspace with the cwd repo', () => {
    // cwd is inside this monorepo (a git repo); the workspace is not a repo.
    const scope = fillScope({ workspace_path: '/tmp/definitely-not-a-git-repo-xyz' }, process.cwd());
    expect(scope.repo).toBeNull();
    expect(scope.ref).toBeNull();
    expect(scope.workspace_path).toBe(join(realpathSync('/tmp'), 'definitely-not-a-git-repo-xyz'));
  });
});
