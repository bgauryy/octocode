import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { realpathSync } from 'node:fs';
import { join } from 'node:path';
import { initDb } from '../src/db.js';
import { insertMemory, getMemory } from '../src/memory.js';
import { insertRefinement, updateRefinement, getRefinements } from '../src/refinements.js';
import { reflect } from '../src/reflect.js';
import { pruneStale } from '../src/maintenance.js';
import { preFlightIntent } from '../src/intents.js';
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

describe('reflect — judgment_note / duo / eval_failures', () => {
  it('folds judgmentNote into the narrative observation', () => {
    const db = freshDb();
    const r = reflect(db, {
      agentId: 'a', task: 'shipped feature', outcome: 'worked',
      judgmentNote: 'checked E2E output; timing untested',
    });
    const { memories } = getMemory(db, { query: 'shipped feature', limit: 5 });
    const mem = memories.find((m) => m.memory_id === r.learning_memory_id);
    expect(mem?.observation).toContain('judgment: checked E2E output; timing untested');
  });

  it('duo emits an advisory reflection_duo packet and stores nothing extra', () => {
    const db = freshDb();
    const r = reflect(db, { agentId: 'a', task: 'ambiguous refactor', outcome: 'partial', duo: true });
    expect(r.reflection_duo?.advisory).toBe(true);
    expect(r.reflection_duo?.roles.map((x) => x.role)).toEqual(['supporter', 'skeptic']);
    // Only the single learning memory exists — the packet is not stored.
    const count = (db.prepare('SELECT COUNT(*) c FROM memories').get() as { c: number }).c;
    expect(count).toBe(1);
  });

  it('evalFailures become eval-tagged memories without double-counting the summary', () => {
    const db = freshDb();
    const r = reflect(db, {
      agentId: 'a', task: 'eval run', outcome: 'failed',
      evalFailures: [
        { id: 'q1', dimension: 'correctness', failure_signature: 'mechanism:x|cause:y', suggested_lesson: 'always check z' },
        { id: 'q2', suggested_lesson: 'second lesson' },
      ],
    });
    expect(r.eval_failure_count).toBe(2);
    expect(r.eval_failure_ids).toHaveLength(2);

    const rows = db.prepare(
      "SELECT memory_id, failure_signature, observation, tags_json FROM memories"
    ).all() as Array<{ memory_id: string; failure_signature: string | null; observation: string; tags_json: string }>;
    // Structured eval rows are the failure events. The summary deliberately
    // carries no signature so one failed eval cannot form a count=2 cluster.
    const main = rows.find((x) => x.memory_id === r.learning_memory_id);
    expect(main?.failure_signature).toBeNull();
    const evalMems = rows.filter((x) => r.eval_failure_ids.includes(x.memory_id));
    expect(evalMems).toHaveLength(2);
    expect(evalMems.map((memory) => memory.failure_signature))
      .toContain('mechanism:x|cause:y');
    for (const m of evalMems) expect(JSON.parse(m.tags_json)).toContain('eval');
  });
});

describe('refinement update lifecycle', () => {
  it('updateRefinement requires and preserves an auditable closure receipt (open → done)', () => {
    const db = freshDb();
    const { refinementId } = insertRefinement(db, {
      agentId: 'a', reasoning: 'why', remember: 'do the thing',
      quality: 'bad', state: 'open', workspacePath: '/tmp/ws',
    });
    expect(() => updateRefinement(db, { refinementId, state: 'done' }))
      .toThrow(/actor.*check receipt/i);
    const upd = updateRefinement(db, {
      refinementId,
      state: 'done',
      actorAgentId: 'reviewer',
      checkReceipt: 'focused refinement lifecycle test passed',
    });
    expect(upd.updated).toBe(true);
    expect(upd.refinement?.state).toBe('done');
    expect(upd.refinement?.remember).toBe('do the thing'); // untouched
    expect(upd.refinement?.reasoning).toContain('Closure receipt');
    expect(upd.refinement?.reasoning).toContain('reviewer');
    expect(upd.refinement?.reasoning).toContain('focused refinement lifecycle test passed');
    const { count } = getRefinements(db, { workspacePath: '/tmp/ws' }); // defaults open+ongoing
    expect(count).toBe(0);
  });

  it('updateRefinement on a missing id reports updated:false', () => {
    const db = freshDb();
    expect(updateRefinement(db, { refinementId: 'ref_missing', state: 'done' }).updated).toBe(false);
  });
});

describe('getMemory explain', () => {
  it('attaches score_components whose weighted sum equals the score', () => {
    const db = freshDb();
    insertMemory(db, { taskContext: 'auth router', observation: 'tenant order matters', importance: 8 });
    const { memories } = getMemory(db, { query: 'auth router', limit: 1, explain: true });
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
    db.prepare('UPDATE locks SET acquired_at = ?, expires_at = ? WHERE run_id = ?')
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
