import { describe, it, expect, vi } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { initDb } from '../src/db.js';
import { reflect } from '../src/reflect.js';

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

describe('reflect', () => {
  it('returns learning_memory_id prefixed mem_', () => {
    const db = freshDb();
    const result = reflect(db, { task: 'unit test reflect', outcome: 'worked' });
    expect(result.learning_memory_id).toMatch(/^mem_/);
  });

  it('inserts a memory — no stdout emission', () => {
    const db = freshDb();
    const writeSpy = vi.spyOn(process.stdout, 'write');
    reflect(db, { task: 'silent test', outcome: 'worked' });
    expect(writeSpy).not.toHaveBeenCalled();
    writeSpy.mockRestore();
  });

  it('creates a repo_fix_refinement when fix_repo is set', () => {
    const db = freshDb();
    const result = reflect(db, {
      task: 'fix something', outcome: 'failed',
      fixRepo: 'add input validation to /api/users',
    });
    expect(result.repo_fix_refinement_id).toMatch(/^ref_/);
    const row = db.prepare('SELECT * FROM refinements WHERE refinement_id = ?')
      .get(result.repo_fix_refinement_id!) as Record<string, unknown>;
    expect(row['quality']).toBe('bad');
    expect(row['state']).toBe('open');
  });

  it('does NOT create a refinement when fix_repo is absent', () => {
    const db = freshDb();
    const result = reflect(db, { task: 'simple task', outcome: 'worked' });
    expect(result.repo_fix_refinement_id).toBeNull();
  });

  it('defaults invalid outcome to partial', () => {
    const db = freshDb();
    const result = reflect(db, { task: 't', outcome: 'INVALID' as 'worked' });
    expect(result.outcome).toBe('partial');
  });

  it('outcome worked is stored correctly', () => {
    const db = freshDb();
    const result = reflect(db, { task: 't', outcome: 'worked' });
    expect(result.outcome).toBe('worked');
  });

  it('uses lesson as the observation when provided', () => {
    const db = freshDb();
    const result = reflect(db, {
      task: 'auth refactor', outcome: 'worked',
      lesson: 'always verify JWT expiry',
    });
    const mem = db.prepare('SELECT observation FROM memories WHERE memory_id = ?')
      .get(result.learning_memory_id) as { observation: string };
    expect(mem.observation).toContain('always verify JWT expiry');
  });

  it('includes worked/didntWork in narrative when no lesson', () => {
    const db = freshDb();
    const result = reflect(db, {
      task: 'routing', outcome: 'partial',
      worked: 'basic routes pass', didntWork: 'nested routes fail',
    });
    const mem = db.prepare('SELECT observation FROM memories WHERE memory_id = ?')
      .get(result.learning_memory_id) as { observation: string };
    expect(mem.observation).toContain('basic routes pass');
    expect(mem.observation).toContain('nested routes fail');
  });

  it('harness_fix is true when fix_harness is set', () => {
    const db = freshDb();
    const result = reflect(db, {
      task: 't', outcome: 'failed', fixHarness: 'add retry logic',
    });
    expect(result.harness_fix).toBe(true);
  });

  it('harness_fix is false when fix_harness is absent', () => {
    const db = freshDb();
    const result = reflect(db, { task: 't', outcome: 'worked' });
    expect(result.harness_fix).toBe(false);
  });

  it('stores reflection label and failure signature', () => {
    const db = freshDb();
    const result = reflect(db, {
      task: 't', outcome: 'failed', failureSignature: 'mechanism:test|cause:unit',
    });
    const mem = db.prepare('SELECT label, failure_signature FROM memories WHERE memory_id = ?')
      .get(result.learning_memory_id) as { label: string; failure_signature: string };
    expect(mem.label).toBe('EXPERIENCE');
    expect(mem.failure_signature).toBe('mechanism:test|cause:unit');
  });

  it('stores reflection and harness tags', () => {
    const db = freshDb();
    const result = reflect(db, {
      task: 't', outcome: 'failed', fixHarness: 'fix something',
    });
    const mem = db.prepare('SELECT tags_json FROM memories WHERE memory_id = ?')
      .get(result.learning_memory_id) as { tags_json: string };
    const tags: string[] = JSON.parse(mem.tags_json);
    expect(tags).toContain('reflection');
    expect(tags).toContain('failed');
    expect(tags).toContain('harness');
  });

  it('eval_failure_count is always 0', () => {
    const db = freshDb();
    const result = reflect(db, { task: 't', outcome: 'worked' });
    expect(result.eval_failure_count).toBe(0);
    expect(result.eval_failure_ids).toEqual([]);
  });

  it('returns similar reflection ids for repeated reflections', () => {
    const db = freshDb();
    const first = reflect(db, { task: 'repeat task', outcome: 'worked', lesson: 'same durable lesson about repeated verification' });
    const second = reflect(db, { task: 'repeat task', outcome: 'worked', lesson: 'same durable lesson about repeated verification' });
    expect(second.similar_memory_ids).toContain(first.learning_memory_id);
    expect(second.novelty_score).toBeLessThan(0.75);
  });

  it('next message is non-empty', () => {
    const db = freshDb();
    const result = reflect(db, { task: 't', outcome: 'worked' });
    expect(result.next.length).toBeGreaterThan(10);
  });

  it('custom importance overrides the default', () => {
    const db = freshDb();
    const result = reflect(db, { task: 't', outcome: 'worked', importance: 9 });
    const mem = db.prepare('SELECT importance FROM memories WHERE memory_id = ?')
      .get(result.learning_memory_id) as { importance: number };
    expect(mem.importance).toBe(9);
  });

  it('stores scope references, primary file, expiry, and refinement files', () => {
    const db = freshDb();
    const result = reflect(db, {
      task: 'scoped reflection',
      outcome: 'partial',
      lesson: 'folder-scoped lessons should be discoverable',
      file: 'src/index.ts',
      files: ['src/tools/memory.ts'],
      folders: ['docs'],
      references: ['file:AGENTS.md'],
      validTo: '2099-01-01T00:00:00Z',
      fixRepo: 'update scoped docs',
    });
    // References are stored in memory_refs.
    const refs = (db.prepare(
      'SELECT reference FROM memory_refs WHERE memory_id = ? ORDER BY ordinal'
    ).all(result.learning_memory_id) as Array<{ reference: string }>).map(r => r.reference);
    expect(refs).toEqual(expect.arrayContaining([
      'file:AGENTS.md',
      expect.stringMatching(/^file:.*src\/index\.ts$/),
      expect.stringMatching(/^file:.*src\/tools\/memory\.ts$/),
      expect.stringMatching(/^dir:.*docs$/),
    ]));
    // valid_to still lives on the memories row
    const mem = db.prepare('SELECT valid_to FROM memories WHERE memory_id = ?')
      .get(result.learning_memory_id) as { valid_to: string };
    expect(mem.valid_to).toBe('2099-01-01T00:00:00Z');

    const ref = db.prepare('SELECT files_json FROM refinements WHERE refinement_id = ?')
      .get(result.repo_fix_refinement_id!) as { files_json: string };
    const refinementFiles: string[] = JSON.parse(ref.files_json);
    // Paths are normalized with file:/dir: prefixes + absolute resolution (same as scopeReferences)
    expect(refinementFiles).toHaveLength(2);
    expect(refinementFiles[0]).toMatch(/^file:.*src\/tools\/memory\.ts$/);
    expect(refinementFiles[1]).toMatch(/^dir:.*docs$/);
  });
});
