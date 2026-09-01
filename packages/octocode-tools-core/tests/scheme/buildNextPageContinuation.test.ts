/**
 * Regression test for the continuation meta-leak fix.
 *
 * buildNextPageContinuation must strip auto-filled per-call metadata
 * (goal / reasoning) from the replayable
 * continuation query, so an agent running `next` does not resend stale meta
 * from the originating call. Affects local.files, local.tree, and
 * any list-style local tool that paginates via this helper.
 */
import { describe, expect, it } from 'vitest';

import { buildNextPageContinuation } from '../../src/scheme/pagination.js';

describe('buildNextPageContinuation', () => {
  it('strips auto-filled per-call metadata from the continuation query', () => {
    const cont = buildNextPageContinuation('local.files', {
      goal: 'Execute local.files via octocode',
      reasoning: 'Executed via octocode tool command',
      pattern: '**/*.ts',
      page: 2,
    });

    expect(cont.tool).toBe('local.files');
    expect(cont.confidence).toBe('exact');
    // Real query params survive.
    expect(cont.query).toMatchObject({ pattern: '**/*.ts', page: 2 });
    // Auto-filled meta is gone.
    expect(cont.query).not.toHaveProperty('goal');
    expect(cont.query).not.toHaveProperty('reasoning');
  });

  it('does not mutate the caller-supplied query object', () => {
    const original = { goal: 'g', page: 3 };
    buildNextPageContinuation('local.tree', original);
    expect(original).toHaveProperty('goal', 'g');
  });

  it('returns the query unchanged when there is no meta to strip', () => {
    const q = { pattern: '*.md', page: 2 };
    const cont = buildNextPageContinuation('local.files', q);
    expect(cont.query).toEqual(q);
  });
});
