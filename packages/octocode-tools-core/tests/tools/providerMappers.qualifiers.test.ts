import { describe, expect, it } from 'vitest';

import {
  mapFileContentToolQuery,
  mapPullRequestToolQuery,
} from '../../src/tools/providerMappers.js';
import { buildPullRequestSearchQuery } from '../../src/github/queryBuilders.js';

type PRArg = Parameters<typeof mapPullRequestToolQuery>[0];
type FileArg = Parameters<typeof mapFileContentToolQuery>[0];

describe('mapPullRequestToolQuery — PR search qualifiers reach the provider query', () => {
  // The reduced ghHistoryResearch surface keeps `checks` and `review` (plus the
  // reviewer qualifiers); milestone/language/locked/visibility/team-mentions/
  // project were removed end-to-end (schema strips them; mapper/builder ignore).
  it('forwards the retained checks/review qualifiers', () => {
    const out = mapPullRequestToolQuery({
      owner: 'facebook',
      repo: 'react',
      checks: 'success',
      review: 'approved',
    } as PRArg) as Record<string, unknown>;

    expect(out.checks).toBe('success');
    expect(out.review).toBe('approved');
  });

  it('leaves the qualifiers undefined when not supplied', () => {
    const out = mapPullRequestToolQuery({
      owner: 'facebook',
      repo: 'react',
    } as PRArg) as Record<string, unknown>;

    expect(out.checks).toBeUndefined();
    expect(out.review).toBeUndefined();
  });
});

describe('buildPullRequestSearchQuery — qualifiers render into GitHub search syntax', () => {
  // End-of-chain proof: the retained params produce the correct GitHub search
  // qualifiers. milestone/language/locked/visibility/team/project were removed.
  it('emits review/checks qualifiers', () => {
    const q = buildPullRequestSearchQuery({
      owner: 'facebook',
      repo: 'react',
      checks: 'success',
      review: 'approved',
    });

    expect(q).toContain('review:approved');
    expect(q).toContain('status:success');
  });
});

describe('mapFileContentToolQuery — forceRefresh reaches the provider query', () => {
  // Regression: forceRefresh was dropped on the primary single-file read path,
  // so the documented cache-bypass never fired for normal file fetches.
  it('forwards forceRefresh:true so the GitHub cache can be bypassed', () => {
    const out = mapFileContentToolQuery({
      owner: 'facebook',
      repo: 'react',
      path: 'packages/react/index.js',
      forceRefresh: true,
    } as FileArg) as Record<string, unknown>;

    expect(out.forceRefresh).toBe(true);
  });

  it('is falsy when forceRefresh is not requested', () => {
    const out = mapFileContentToolQuery({
      owner: 'facebook',
      repo: 'react',
      path: 'packages/react/index.js',
    } as FileArg) as Record<string, unknown>;

    expect(out.forceRefresh).toBeFalsy();
  });
});
