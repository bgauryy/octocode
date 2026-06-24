/**
 * P4 — pullRequests `matchString` is an OQL-layer *content* filter over fetched
 * PR bodies/comments/reviews (never a search-index claim): keep only matching
 * PRs, spotlight where they matched, and emit honest diagnostics (partialResult
 * when some drop, zeroMatches when none match).
 */
import { describe, expect, it } from 'vitest';
import {
  filterPullRequestsByMatch,
  filterCommitsByMatch,
  type PullRequestMatch,
} from '../../src/oql/adapters/researchTargets.js';
import type { AdapterResult } from '../../src/oql/adapters/local.js';
import type { OqlRecordResultRow } from '../../src/oql/types.js';

function prRow(
  id: string,
  data: Record<string, unknown>
): OqlRecordResultRow {
  return { kind: 'record', recordType: 'pullRequest', id, data };
}

function commitRow(
  id: string,
  data: Record<string, unknown>
): OqlRecordResultRow {
  return { kind: 'record', recordType: 'commit', id, data };
}

function base(rows: OqlRecordResultRow[]): AdapterResult {
  return {
    results: rows,
    diagnostics: [],
    provenance: [{ backend: 'ghHistoryResearch' }],
  };
}

describe('filterPullRequestsByMatch (P4)', () => {
  const fixture = () =>
    base([
      prRow('1', { number: 1, title: 'Fix auth', body: 'refactors the OAuth token flow' }),
      prRow('2', { number: 2, title: 'Docs', body: 'updates the README only' }),
      prRow('3', {
        number: 3,
        title: 'Cache',
        body: 'no match here',
        comments: [{ body: 'we should revisit the OAuth token cache' }],
      }),
    ]);

  it('keeps only PRs whose body contains the needle and spotlights the hit', () => {
    const out = filterPullRequestsByMatch(fixture(), {
      needle: 'oauth token',
      scope: 'body',
    } satisfies PullRequestMatch);
    expect(out.results.map(r => r.id)).toEqual(['1']);
    const m = (out.results[0] as OqlRecordResultRow).data.match as {
      matchString: string;
      scope: string;
      spotlight: string;
    };
    expect(m.matchString).toBe('oauth token');
    expect(m.scope).toBe('body');
    expect(m.spotlight.toLowerCase()).toContain('oauth token');
    // partialResult, not zeroMatches, since 1 of 3 matched.
    expect(out.diagnostics.some(d => d.code === 'partialResult')).toBe(true);
    expect(out.diagnostics.some(d => d.code === 'zeroMatches')).toBe(false);
  });

  it('matches inside comments under scope:"comments"', () => {
    const out = filterPullRequestsByMatch(fixture(), {
      needle: 'oauth token',
      scope: 'comments',
    });
    expect(out.results.map(r => r.id)).toEqual(['3']);
  });

  it('emits zeroMatches (info, non-blocking) when nothing matches', () => {
    const out = filterPullRequestsByMatch(fixture(), {
      needle: 'graphql subscription',
      scope: 'all',
    });
    expect(out.results).toHaveLength(0);
    const zero = out.diagnostics.find(d => d.code === 'zeroMatches');
    expect(zero).toBeDefined();
    expect(zero!.blocksAnswer).toBe(false);
    expect(zero!.severity).toBe('info');
  });

  it('is case-insensitive', () => {
    const out = filterPullRequestsByMatch(fixture(), {
      needle: 'OAUTH',
      scope: 'body',
    });
    expect(out.results.map(r => r.id)).toEqual(['1']);
  });
});

describe('filterCommitsByMatch (H1 — commits matchString is a content filter)', () => {
  const fixture = () =>
    base([
      commitRow('a', { sha: 'a', message: 'fix: harden OAuth token refresh' }),
      commitRow('b', { sha: 'b', message: 'docs: update README' }),
      commitRow('c', { sha: 'c', message: 'chore: bump deps' }),
    ]);

  it('keeps only commits whose message contains the needle, spotlights, and emits partialResult', () => {
    const out = filterCommitsByMatch(fixture(), 'oauth token');
    expect(out.results.map(r => r.id)).toEqual(['a']);
    const m = (out.results[0] as OqlRecordResultRow).data.match as {
      matchString: string;
      scope: string;
      spotlight: string;
    };
    expect(m.scope).toBe('message');
    expect(m.spotlight.toLowerCase()).toContain('oauth token');
    expect(out.diagnostics.some(d => d.code === 'partialResult')).toBe(true);
  });

  it('emits zeroMatches (info, non-blocking) when no commit message matches', () => {
    const out = filterCommitsByMatch(fixture(), 'graphql subscription');
    expect(out.results).toHaveLength(0);
    const zero = out.diagnostics.find(d => d.code === 'zeroMatches');
    expect(zero).toBeDefined();
    expect(zero!.blocksAnswer).toBe(false);
    expect(zero!.severity).toBe('info');
  });
});
