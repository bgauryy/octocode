import { describe, expect, it } from 'vitest';

import { shapePullRequestForContent } from '../../../src/tools/github_search_pull_requests/contentResponse.js';
import { shapeFileSurfaces } from '../../../src/tools/github_search_pull_requests/contentResponse/fileSurfaces.js';
import { normalizePullRequestContentRequest } from '../../../src/tools/github_search_pull_requests/contentRequest.js';

// A diff whose ADDED lines include a full-line comment, a blank line, a
// block-comment line, and a code line with a trailing inline comment — every
// one of these was removed/rewritten by the old `stripPatchComments` minifier.
// The tool must return the patch VERBATIM.
const PATCH = [
  '@@ -1,3 +1,8 @@',
  ' const a = 1;',
  '+// added top-level comment',
  '+',
  '+/* block comment */',
  '+const b = 2; // trailing inline comment',
  '+const c = 3;',
  ' const d = 4;',
  '-const removed = 0;',
].join('\n');

const PR = {
  number: 7,
  title: 'verbatim diff',
  state: 'open',
  fileChanges: [
    {
      path: 'src/index.ts',
      status: 'modified',
      additions: 5,
      deletions: 1,
      patch: PATCH,
    },
  ],
};

describe('ghSearchPullRequests — diffs are returned verbatim (no minification)', () => {
  it('shapeFileSurfaces returns the patch byte-for-byte, comments and blank lines intact', () => {
    const request = normalizePullRequestContentRequest({
      content: { changedFiles: true, patches: { mode: 'all' } },
    } as never);

    const surfaces = shapeFileSurfaces(
      PR as never,
      { owner: 'octo', repo: 'engine' },
      request
    ) as { changedFiles: Array<{ patch: string }> };

    expect(surfaces.changedFiles).toHaveLength(1);
    expect(surfaces.changedFiles[0].patch).toBe(PATCH);
  });

  it('keeps added comment/blank/inline-comment lines that the old minifier stripped', () => {
    const request = normalizePullRequestContentRequest({
      content: { changedFiles: true, patches: { mode: 'all' } },
    } as never);

    const patch = (
      shapeFileSurfaces(
        PR as never,
        { owner: 'octo', repo: 'engine' },
        request
      ) as { changedFiles: Array<{ patch: string }> }
    ).changedFiles[0].patch;

    expect(patch).toContain('+// added top-level comment');
    expect(patch).toContain('+/* block comment */');
    expect(patch).toContain('+const b = 2; // trailing inline comment');
    expect(patch.split('\n')).toContain('+'); // the added blank line survives
  });

  it('shapePullRequestForContent leaves the diff verbatim even when body minify is on', () => {
    const request = normalizePullRequestContentRequest({
      content: { changedFiles: true, patches: { mode: 'all' } },
    } as never);

    // shouldMinify=true drives BODY prose normalization only; it must not touch diffs.
    const shaped = shapePullRequestForContent(
      PR as never,
      { owner: 'octo', repo: 'engine' },
      request,
      true,
      true
    ) as { changedFiles?: Array<{ patch: string }> };

    expect(shaped.changedFiles?.[0]?.patch).toBe(PATCH);
  });
});
