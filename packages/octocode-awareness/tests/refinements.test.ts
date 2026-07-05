import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { initDb } from '../src/db.js';
import { insertRefinement, getRefinements } from '../src/refinements.js';

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

describe('insertRefinement', () => {
  it('returns a refinementId prefixed ref_', () => {
    const db = freshDb();
    const { refinementId } = insertRefinement(db, {
      reasoning: 'fix this', remember: 'thing to fix',
    });
    expect(refinementId).toMatch(/^ref_/);
  });

  it('stores reasoning and remember fields', () => {
    const db = freshDb();
    const { refinementId, refinement } = insertRefinement(db, {
      agentId: 'tester',
      reasoning: 'the router is missing error handling',
      remember: 'add try/catch in /api/users',
      quality: 'bad',
      state: 'open',
    });
    expect(refinement.reasoning).toBe('the router is missing error handling');
    expect(refinement.remember).toBe('add try/catch in /api/users');
    expect(refinement.quality).toBe('bad');
    expect(refinement.state).toBe('open');
    expect(refinement.agent_id).toBe('tester');

    const row = db.prepare('SELECT * FROM refinements WHERE refinement_id = ?')
      .get(refinementId) as Record<string, unknown>;
    expect(row['reasoning']).toBe('the router is missing error handling');
  });

  it('defaults quality=good and state=open', () => {
    const db = freshDb();
    const { refinement } = insertRefinement(db, { reasoning: 'r', remember: 'rem' });
    expect(refinement.quality).toBe('good');
    expect(refinement.state).toBe('open');
  });

  it('stores files array', () => {
    const db = freshDb();
    const { refinementId } = insertRefinement(db, {
      reasoning: 'r', remember: 'rem',
      files: ['/a.ts', '/b.ts'],
    });
    const row = db.prepare('SELECT files_json FROM refinements WHERE refinement_id = ?')
      .get(refinementId) as { files_json: string };
    expect(JSON.parse(row.files_json)).toEqual(['/a.ts', '/b.ts']);
  });
});

describe('getRefinements', () => {
  it('returns open refinements by default', () => {
    const db = freshDb();
    insertRefinement(db, { reasoning: 'r', remember: 'rem', state: 'open' });
    insertRefinement(db, { reasoning: 'r', remember: 'rem', state: 'done' });
    const { refinements, count } = getRefinements(db);
    expect(count).toBe(1);
    expect(refinements[0]!.state).toBe('open');
  });

  it('respects state filter', () => {
    const db = freshDb();
    insertRefinement(db, { reasoning: 'r', remember: 'rem', state: 'ongoing' });
    insertRefinement(db, { reasoning: 'r', remember: 'rem', state: 'done' });
    const { refinements } = getRefinements(db, { states: ['done'] });
    expect(refinements).toHaveLength(1);
    expect(refinements[0]!.state).toBe('done');
  });

  it('respects quality filter', () => {
    const db = freshDb();
    insertRefinement(db, { reasoning: 'r', remember: 'good handoff', quality: 'good', state: 'open' });
    insertRefinement(db, { reasoning: 'r', remember: 'bad handoff', quality: 'bad', state: 'open' });
    const { refinements } = getRefinements(db, { quality: 'bad' });
    expect(refinements).toHaveLength(1);
    expect(refinements[0]!.quality).toBe('bad');
    expect(refinements[0]!.remember).toBe('bad handoff');
  });

  it('hides session handoffs by default and includes them on opt-in', () => {
    const db = freshDb();
    insertRefinement(db, { reasoning: 'r', remember: 'real repo fix', quality: 'bad', state: 'open' });
    insertRefinement(db, { reasoning: 'r', remember: 'Review session handoff for agent', quality: 'handoff', state: 'open' });

    const defaultView = getRefinements(db);
    expect(defaultView.refinements).toHaveLength(1);
    expect(defaultView.refinements[0]!.remember).toBe('real repo fix');

    const withHandoffs = getRefinements(db, { includeHandoffs: true });
    expect(withHandoffs.refinements.map(r => r.quality).sort()).toEqual(['bad', 'handoff']);
  });

  it('respects limit', () => {
    const db = freshDb();
    for (let i = 0; i < 5; i++) {
      insertRefinement(db, { reasoning: 'r', remember: 'rem' });
    }
    const { refinements } = getRefinements(db, { limit: 2 });
    expect(refinements).toHaveLength(2);
  });

  it('returns files as an array', () => {
    const db = freshDb();
    insertRefinement(db, { reasoning: 'r', remember: 'rem', files: ['/a.ts'] });
    const { refinements } = getRefinements(db);
    expect(refinements[0]!.files).toEqual(['/a.ts']);
  });

  it('ongoing refinements sort before open', () => {
    const db = freshDb();
    insertRefinement(db, { reasoning: 'r', remember: 'open one', state: 'open' });
    insertRefinement(db, { reasoning: 'r', remember: 'ongoing one', state: 'ongoing' });
    const { refinements } = getRefinements(db, { states: ['open', 'ongoing'] });
    expect(refinements[0]!.state).toBe('ongoing');
  });
});
