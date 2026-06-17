import { describe, expect, it } from 'vitest';
import { normalizePullRequestContentRequest } from '../../../octocode-tools-core/src/tools/github_search_pull_requests/contentRequest.js';

describe('ghHistoryResearch content request normalization', () => {
  it('keeps request lean by default', () => {
    const request = normalizePullRequestContentRequest({});
    expect(request).toMatchObject({
      body: false,
      changedFiles: false,
      patches: { mode: 'none' },
      comments: false,
      commits: false,
    });
    expect('metadata' in request).toBe(false);
  });

  it('maps explicit selected patch ranges', () => {
    const request = normalizePullRequestContentRequest({
      content: {
        patches: {
          mode: 'selected',
          files: ['src/a.ts'],
          ranges: [{ file: 'src/a.ts', additions: [10] }],
        },
      },
    });
    expect(request.changedFiles).toBe(true);
    expect(request.patches).toEqual({
      mode: 'selected',
      files: ['src/a.ts'],
      ranges: [{ file: 'src/a.ts', additions: [10] }],
    });
  });

  it('maps reviewMode full to all content surfaces', () => {
    const request = normalizePullRequestContentRequest({ reviewMode: 'full' });
    expect(request.body).toBe(true);
    expect(request.changedFiles).toBe(true);
    expect(request.patches.mode).toBe('all');
    expect(request.comments).toMatchObject({
      discussion: true,
      reviewInline: true,
      includeBots: false,
    });
    expect(request.commits).toEqual({ list: true, includeFiles: false });
  });

  it('supports explicit comments and selected files', () => {
    const request = normalizePullRequestContentRequest({
      content: {
        patches: { mode: 'selected', files: ['src/a.ts'] },
        comments: { reviewInline: true, discussion: false, file: 'src/a.ts' },
      },
    });
    expect(request.patches).toEqual({ mode: 'selected', files: ['src/a.ts'] });
    expect(request.comments).toMatchObject({
      reviewInline: true,
      discussion: false,
      file: 'src/a.ts',
    });
  });

  it('maps commit selectors with and without files', () => {
    expect(
      normalizePullRequestContentRequest({
        content: { commits: { list: true, includeFiles: true } },
      }).commits
    ).toEqual({ list: true, includeFiles: true });
    expect(
      normalizePullRequestContentRequest({
        content: { commits: { list: false, includeFiles: true } },
      }).commits
    ).toEqual({ list: false, includeFiles: true });
  });
});
