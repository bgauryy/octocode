import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { initDb } from '../src/db.js';
import { insertMemory, mineWeakness } from '../src/memory.js';

/**
 * mineWeakness clusters memories by failure_signature for the self-harness
 * loop (self-harness.md §2). Covers the three behaviors beyond a naive
 * GROUP BY: |surface:Z merging, Jaccard diversity filtering, and
 * count*avg-importance ranking — none of which had any test before this.
 */

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

function recordFailure(
  db: DatabaseSync,
  signature: string,
  importance = 5,
  observation = 'observed failure',
): void {
  insertMemory(db, {
    taskContext: `task for ${signature}`,
    observation,
    importance,
    label: 'GOTCHA',
    failureSignature: signature,
  });
}

describe('mineWeakness', () => {
  it('returns nothing below minCount', () => {
    const db = freshDb();
    recordFailure(db, 'mechanism:retry-loop|cause:timeout');
    const result = mineWeakness(db, { minCount: 2 });
    expect(result.clusters).toEqual([]);
    expect(result.total_signatures).toBe(1);
    expect(result.total_memories).toBe(1);
  });

  it('clusters repeated signatures and reports count/avg_importance', () => {
    const db = freshDb();
    recordFailure(db, 'mechanism:retry-loop|cause:timeout', 4);
    recordFailure(db, 'mechanism:retry-loop|cause:timeout', 6);
    recordFailure(db, 'mechanism:retry-loop|cause:timeout', 8);
    const result = mineWeakness(db, { minCount: 2 });
    expect(result.clusters).toHaveLength(1);
    const cluster = result.clusters[0]!;
    expect(cluster.count).toBe(3);
    expect(cluster.avg_importance).toBe(6);
    expect(cluster.memory_ids).toHaveLength(3);
  });

  it('merges |surface:Z variants into one base-signature cluster', () => {
    const db = freshDb();
    recordFailure(db, 'mechanism:unverified-conclusion|cause:missing-verify|surface:verify-gate', 5);
    recordFailure(db, 'mechanism:unverified-conclusion|cause:missing-verify|surface:verify-gate', 5);
    recordFailure(db, 'mechanism:unverified-conclusion|cause:missing-verify|surface:lock-conflict', 7);
    recordFailure(db, 'mechanism:unverified-conclusion|cause:missing-verify|surface:lock-conflict', 7);
    const result = mineWeakness(db, { minCount: 2 });
    expect(result.clusters).toHaveLength(1);
    const cluster = result.clusters[0]!;
    expect(cluster.base_signature).toBe('mechanism:unverified-conclusion|cause:missing-verify');
    expect(cluster.count).toBe(4);
    expect(cluster.surfaces).toEqual(['lock-conflict', 'verify-gate']);
    // total_signatures counts raw (pre-merge) signature values, not clusters.
    expect(result.total_signatures).toBe(2);
    expect(result.total_memories).toBe(4);
  });

  it('merges |surface:Z variants before applying minCount', () => {
    const db = freshDb();
    recordFailure(db, 'mechanism:flaky|cause:race|surface:verify-gate', 8);
    recordFailure(db, 'mechanism:flaky|cause:race|surface:lock-conflict', 8);
    const result = mineWeakness(db, { minCount: 2 });
    expect(result.clusters).toHaveLength(1);
    expect(result.clusters[0]).toMatchObject({
      base_signature: 'mechanism:flaky|cause:race',
      count: 2,
      surfaces: ['lock-conflict', 'verify-gate'],
    });
    expect(result.total_signatures).toBe(2);
    expect(result.total_memories).toBe(2);
  });

  it('suppresses near-duplicate mechanisms via the Jaccard diversity filter', () => {
    const db = freshDb();
    // sigTokens strips "mechanism"/"cause" and keeps {retry-loop, timeout, *-flake}.
    // Two of three tokens match (jaccard = 2/4 = 0.5) — at the suppression threshold.
    recordFailure(db, 'mechanism:retry-loop:timeout|cause:test-flake', 5);
    recordFailure(db, 'mechanism:retry-loop:timeout|cause:test-flake', 5);
    recordFailure(db, 'mechanism:retry-loop:timeout|cause:network-flake', 9);
    recordFailure(db, 'mechanism:retry-loop:timeout|cause:network-flake', 9);
    const result = mineWeakness(db, { minCount: 2, limit: 20 });
    expect(result.clusters).toHaveLength(1);
    // Higher score (count * avg_importance = 2*9=18 vs 2*5=10) wins the diversity slot.
    expect(result.clusters[0]!.base_signature).toBe('mechanism:retry-loop:timeout|cause:network-flake');
  });

  it('keeps genuinely distinct mechanisms separate', () => {
    const db = freshDb();
    recordFailure(db, 'mechanism:retry-loop|cause:timeout', 5);
    recordFailure(db, 'mechanism:retry-loop|cause:timeout', 5);
    recordFailure(db, 'mechanism:fts-miss|cause:empty-store', 5);
    recordFailure(db, 'mechanism:fts-miss|cause:empty-store', 5);
    const result = mineWeakness(db, { minCount: 2 });
    expect(result.clusters).toHaveLength(2);
    const sigs = result.clusters.map(c => c.base_signature).sort();
    expect(sigs).toEqual(['mechanism:fts-miss|cause:empty-store', 'mechanism:retry-loop|cause:timeout']);
  });

  it('ranks clusters by count * avg_importance descending', () => {
    const db = freshDb();
    recordFailure(db, 'mechanism:aaa|cause:xxx', 3);
    recordFailure(db, 'mechanism:aaa|cause:xxx', 3); // score 2*3=6
    recordFailure(db, 'mechanism:bbb|cause:yyy', 9);
    recordFailure(db, 'mechanism:bbb|cause:yyy', 9);
    recordFailure(db, 'mechanism:bbb|cause:yyy', 9); // score 3*9=27
    const result = mineWeakness(db, { minCount: 2 });
    expect(result.clusters.map(c => c.base_signature)).toEqual([
      'mechanism:bbb|cause:yyy',
      'mechanism:aaa|cause:xxx',
    ]);
  });

  it('excludes SUPERSEDED memories from clustering', () => {
    const db = freshDb();
    const { memoryId: first } = insertMemory(db, {
      taskContext: 'first', observation: 'old', importance: 5,
      label: 'GOTCHA', failureSignature: 'mechanism:stale|cause:old',
    });
    insertMemory(db, {
      taskContext: 'second', observation: 'new', importance: 5,
      label: 'GOTCHA', failureSignature: 'mechanism:stale|cause:old',
      supersedes: [first],
    });
    // Only one ACTIVE memory remains for this signature — below minCount 2.
    const result = mineWeakness(db, { minCount: 2 });
    expect(result.clusters).toEqual([]);
  });

  it('scopes to workspacePath while still including globally-scoped rows', () => {
    const db = freshDb();
    insertMemory(db, {
      taskContext: 'scoped', observation: 'a', importance: 5, label: 'GOTCHA',
      failureSignature: 'mechanism:scoped|cause:x', workspacePath: '/repo/a',
    });
    insertMemory(db, {
      taskContext: 'scoped', observation: 'b', importance: 5, label: 'GOTCHA',
      failureSignature: 'mechanism:scoped|cause:x', workspacePath: '/repo/a',
    });
    insertMemory(db, {
      taskContext: 'other-repo', observation: 'c', importance: 5, label: 'GOTCHA',
      failureSignature: 'mechanism:scoped|cause:x', workspacePath: '/repo/b',
    });
    insertMemory(db, {
      taskContext: 'other-repo', observation: 'd', importance: 5, label: 'GOTCHA',
      failureSignature: 'mechanism:scoped|cause:x', workspacePath: '/repo/b',
    });
    const result = mineWeakness(db, { minCount: 2, workspacePath: '/repo/a' });
    expect(result.clusters).toHaveLength(1);
    expect(result.clusters[0]!.count).toBe(2);
  });

  it('scopes totals and representatives to the same workspace as clusters', () => {
    const db = freshDb();
    insertMemory(db, {
      taskContext: 'scoped-a', observation: 'workspace-a representative', importance: 5, label: 'GOTCHA',
      failureSignature: 'mechanism:scoped-total|cause:x', workspacePath: '/repo/a',
    });
    insertMemory(db, {
      taskContext: 'scoped-a', observation: 'workspace-a second', importance: 5, label: 'GOTCHA',
      failureSignature: 'mechanism:scoped-total|cause:x', workspacePath: '/repo/a',
    });
    insertMemory(db, {
      taskContext: 'scoped-b', observation: 'workspace-b must not leak', importance: 10, label: 'GOTCHA',
      failureSignature: 'mechanism:scoped-total|cause:x', workspacePath: '/repo/b',
    });
    insertMemory(db, {
      taskContext: 'scoped-b', observation: 'workspace-b second', importance: 10, label: 'GOTCHA',
      failureSignature: 'mechanism:scoped-total|cause:x', workspacePath: '/repo/b',
    });

    const result = mineWeakness(db, { minCount: 2, workspacePath: '/repo/a' });
    expect(result.total_signatures).toBe(1);
    expect(result.total_memories).toBe(2);
    expect(result.clusters).toHaveLength(1);
    expect(result.clusters[0]!.count).toBe(2);
    expect(result.clusters[0]!.representative).toContain('workspace-a');
    expect(result.clusters[0]!.representative).not.toContain('workspace-b');
  });

  it('truncates the representative observation to 200 chars', () => {
    const db = freshDb();
    const long = 'x'.repeat(300);
    recordFailure(db, 'mechanism:long|cause:obs', 5, long);
    recordFailure(db, 'mechanism:long|cause:obs', 5, long);
    const result = mineWeakness(db, { minCount: 2 });
    expect(result.clusters[0]!.representative.length).toBe(200);
  });
});
