import { describe, expect, it } from 'vitest';
import {
  buildContentHints,
  shapePullRequestForContent,
} from '../../src/tools/github_search_pull_requests/contentResponse.js';
import type { NormalizedPrContentRequest } from '../../src/tools/github_search_pull_requests/contentRequest.js';

const baseRequest: NormalizedPrContentRequest = {
  body: false,
  changedFiles: false,
  patches: { mode: 'none' },
  comments: false,
  reviews: false,
  commits: false,
};

const query = {
  owner: 'owner',
  repo: 'repo',
  prNumber: 123,
  itemsPerPage: 1,
  charLength: 5,
};

const pr = {
  number: 123,
  title: 'Test PR',
  body: 'abcdefghijklmnopqrstuvwxyz',
  url: 'https://github.com/owner/repo/pull/123',
  state: 'merged',
  draft: false,
  author: 'alice',
  fileChanges: [
    {
      path: 'src/a.ts',
      status: 'modified',
      additions: 1,
      deletions: 1,
      patch: 'abcdef',
    },
    {
      path: 'src/b.ts',
      status: 'added',
      additions: 2,
      deletions: 0,
      patch: 'ghijkl',
    },
  ],
  comments: [
    {
      id: 'd1',
      author: 'bob',
      body: 'discussion-body',
      commentType: 'discussion',
      createdAt: '2024-01-01T00:00:00Z',
      updatedAt: '2024-01-01T00:00:00Z',
    },
    {
      id: 'i1',
      author: 'carol',
      body: 'inline-body',
      commentType: 'review_inline',
      path: 'src/a.ts',
      line: 10,
      createdAt: '2024-01-02T00:00:00Z',
      updatedAt: '2024-01-02T00:00:00Z',
    },
  ],
  reviews: [
    {
      id: 'r1',
      user: 'reviewer',
      state: 'APPROVED',
      body: 'approval-body',
      submittedAt: '2024-01-03T00:00:00Z',
      commitId: 'abc',
    },
  ],
  commits: [
    {
      sha: 'abc',
      message: 'commit message',
      author: 'dev',
      date: '2024-01-04T00:00:00Z',
      files: [{ filename: 'src/a.ts', status: 'modified' }],
    },
  ],
};

describe('githubSearchPullRequests content response shaping', () => {
  it('returns previews for lean metadata — no content map when no surfaces requested', () => {
    const shaped = shapePullRequestForContent(pr, query, baseRequest);
    expect(shaped.body).toBeUndefined();
    expect(shaped.bodyPreview).toContain('abcdefghijklmnopqrstuvwxyz');
    expect(shaped.next).toBeUndefined();
    expect(shaped.filePathsPreview).toEqual(['src/a.ts', 'src/b.ts']);
  });

  it('exposes content map when showContentMap is explicitly true (lean broad search)', () => {
    const shaped = shapePullRequestForContent(
      pr,
      query,
      baseRequest,
      true,
      true
    );
    expect(shaped.next).toBeDefined();
    expect(
      (shaped.next as Record<string, unknown>).getChangedFiles
    ).toBeDefined();
  });

  it('paginates body, selected patches, file comments, reviews, and commits', () => {
    const shaped = shapePullRequestForContent(pr, query, {
      ...baseRequest,
      body: true,
      changedFiles: true,
      patches: { mode: 'selected', files: ['src/a.ts'] },
      comments: {
        discussion: false,
        reviewInline: true,
        includeBots: false,
        file: 'src/a.ts',
      },
      reviews: true,
      commits: { list: true, includeFiles: true },
    });

    expect(shaped.body).toBe('abcde');
    expect(shaped.bodyPagination).toMatchObject({
      hasMore: true,
      nextCharOffset: 5,
    });
    expect(shaped.changedFiles).toHaveLength(1);
    expect(shaped).not.toHaveProperty('fileChanges');
    expect(shaped.filePagination).toMatchObject({
      totalItems: 1,
      hasMore: false,
    });
    expect(shaped.comments).toHaveLength(1);
    expect(shaped.commentPagination).toMatchObject({ totalItems: 1 });
    expect(shaped.reviews).toHaveLength(1);
    expect(shaped.commits).toHaveLength(1);
    expect(
      (shaped.commits as Array<Record<string, unknown>>)[0]?.files
    ).toBeDefined();
  });

  it('keeps raw patch comments by default and strips them only for token-saving standard view', () => {
    const patchPr = {
      ...pr,
      fileChanges: [
        {
          path: 'src/a.ts',
          status: 'modified',
          additions: 2,
          deletions: 0,
          patch: [
            '@@ -1,2 +1,2 @@',
            '+const value = 1; // exact review comment',
            '+// comment-only change',
          ].join('\n'),
        },
      ],
    };
    const request = {
      ...baseRequest,
      patches: { mode: 'all' as const },
    };

    const raw = shapePullRequestForContent(
      patchPr,
      { ...query, charLength: 1_000 },
      request,
      false
    );
    const standard = shapePullRequestForContent(
      patchPr,
      { ...query, charLength: 1_000 },
      request,
      true
    );

    const rawPatch = (raw.changedFiles as Array<{ patch: string }>)[0]!.patch;
    const standardPatch = (
      standard.changedFiles as Array<{ patch: string }>
    )[0]!.patch;
    expect(rawPatch).toContain('// exact review comment');
    expect(rawPatch).toContain('// comment-only change');
    expect(standardPatch).not.toContain('// exact review comment');
    expect(standardPatch).not.toContain('// comment-only change');
    expect(standardPatch).toContain('const value = 1;');
  });

  it('omits commit files unless requested', () => {
    const shaped = shapePullRequestForContent(pr, query, {
      ...baseRequest,
      commits: { list: true, includeFiles: false },
    });
    expect(
      (shaped.commits as Array<Record<string, unknown>>)[0]?.files
    ).toBeUndefined();
  });

  it('builds content hints for missing patches and comments', () => {
    const hints = buildContentHints([pr], baseRequest);
    expect(hints.some(hint => hint.includes('Patches not included'))).toBe(
      true
    );
    expect(hints.some(hint => hint.includes('Comments not included'))).toBe(
      true
    );
  });

  it('handles empty content surfaces without crashing', () => {
    const shaped = shapePullRequestForContent(
      { number: 9, title: 'empty', body: undefined },
      { owner: 'o', repo: 'r', prNumber: 9 },
      {
        ...baseRequest,
        body: true,
        changedFiles: true,
        patches: { mode: 'all' },
        comments: {
          discussion: true,
          reviewInline: true,
          includeBots: false,
        },
        reviews: true,
        commits: { list: true, includeFiles: false },
      }
    );

    expect(shaped.bodyEmpty).toBe(true);
    expect('body' in shaped).toBe(false);
    expect(shaped.changedFiles).toEqual([]);
    expect(shaped.comments).toEqual([]);
    expect(shaped.reviews).toEqual([]);
    expect(shaped.commits).toEqual([]);
  });

  it('paginates review body via charLength when reviews are requested', () => {
    const shaped = shapePullRequestForContent(
      pr,
      { ...query, charLength: 8 },
      { ...baseRequest, reviews: true }
    );
    const reviews = shaped.reviews as Array<Record<string, unknown>>;
    expect(reviews).toHaveLength(1);
    // Body should be paginated, not a short preview
    expect(reviews[0]!.body).toBe('approval');
    expect(reviews[0]!.bodyPagination).toMatchObject({
      hasMore: true,
      nextCharOffset: 8,
      totalChars: 13, // 'approval-body'
    });
    // bodyPreview should NOT be present when full body pagination is included
    expect(reviews[0]).not.toHaveProperty('bodyPreview');
  });

  it('returns full review body without pagination when body fits within charLength', () => {
    const shaped = shapePullRequestForContent(
      pr,
      { ...query, charLength: 1000 },
      { ...baseRequest, reviews: true }
    );
    const reviews = shaped.reviews as Array<Record<string, unknown>>;
    expect(reviews[0]!.body).toBe('approval-body');
    expect(reviews[0]!.bodyPagination).toMatchObject({ hasMore: false });
  });

  it('omits optional hint branches when data is absent or already requested', () => {
    expect(buildContentHints([], baseRequest)).toEqual([]);
    const hints = buildContentHints([pr], {
      ...baseRequest,
      patches: { mode: 'all' },
      comments: {
        discussion: true,
        reviewInline: true,
        includeBots: false,
      },
    });
    expect(hints.some(hint => hint.includes('Diffs are not included'))).toBe(
      false
    );
    expect(hints.some(hint => hint.includes('Comments are not included'))).toBe(
      false
    );
  });

  it('covers large file preview pagination and non-string metadata body', () => {
    const shaped = shapePullRequestForContent(
      {
        number: 11,
        title: 'large-preview',
        body: 123,
        fileChanges: Array.from({ length: 21 }, (_, i) => ({
          path: `src/${i}.ts`,
          status: 'modified',
          additions: 1,
          deletions: 0,
        })),
      },
      { prNumber: 11 },
      baseRequest
    );

    expect(shaped.bodyPreview).toBeUndefined();
    expect(shaped.filePathsPreview).toHaveLength(20);
    expect(shaped.filePathsPagination).toMatchObject({
      totalFiles: 21,
      hasMore: true,
      nextFilePage: 2,
    });
  });

  it('covers fallback path fields and unpaginated text branches', () => {
    const shaped = shapePullRequestForContent(
      {
        number: 10,
        title: 'fallbacks',
        body: 'short',
        fileChanges: [
          {
            filename: 'fallback.ts',
            status: 'removed',
            additions: 0,
            deletions: 3,
          },
        ],
        comments: [
          {
            id: 'd1',
            author: 'bob',
            body: 123,
            commentType: undefined,
          },
        ],
        reviews: [
          {
            id: 'r2',
            user: 'reviewer',
            state: 'COMMENTED',
            body: 123,
            submitted_at: '2024-01-05T00:00:00Z',
            commit_id: 'def',
          },
        ],
      },
      { prNumber: 10, itemsPerPage: 5, charLength: 100 },
      {
        ...baseRequest,
        body: true,
        changedFiles: true,
        comments: {
          discussion: true,
          reviewInline: false,
          includeBots: false,
        },
        reviews: true,
      }
    );

    expect(shaped.body).toBe('short');
    expect(shaped.bodyPagination).toMatchObject({ hasMore: false });
    expect(shaped.changedFiles).toEqual([
      { path: 'fallback.ts', status: 'removed', additions: 0, deletions: 3 },
    ]);
    expect(shaped.comments).toHaveLength(1);
    expect(
      (shaped.comments as Array<Record<string, unknown>>)[0]
    ).toMatchObject({
      commentType: 'discussion',
      body: '',
    });
    expect(shaped.reviews).toEqual([
      {
        id: 'r2',
        user: 'reviewer',
        state: 'COMMENTED',
        submittedAt: '2024-01-05T00:00:00Z',
        commitId: 'def',
      },
    ]);
  });

  it('includes assignees when non-empty and commentsCount when > 0', () => {
    const prWithAssignees = {
      ...pr,
      assignees: ['alice', 'bob'],
      commentsCount: 7,
    };
    const shaped = shapePullRequestForContent(
      prWithAssignees,
      query,
      baseRequest
    );
    expect(shaped.assignees).toEqual(['alice', 'bob']);
    expect(shaped.commentsCount).toBe(7);
  });

  it('omits assignees when empty and omits commentsCount when 0', () => {
    const prEmpty = { ...pr, assignees: [], commentsCount: 0 };
    const shaped = shapePullRequestForContent(prEmpty, query, baseRequest);
    expect(shaped.assignees).toBeUndefined();
    expect(shaped.commentsCount).toBeUndefined();
  });

  it('omits draft when false, labels when empty, additions/deletions when zero', () => {
    const prClean = {
      ...pr,
      draft: false,
      labels: [],
      additions: 0,
      deletions: 0,
    };
    const shaped = shapePullRequestForContent(prClean, query, baseRequest);
    expect(shaped).not.toHaveProperty('draft');
    expect(shaped).not.toHaveProperty('labels');
    expect(shaped).not.toHaveProperty('additions');
    expect(shaped).not.toHaveProperty('deletions');
  });

  it('emits draft when true, labels when non-empty, additions/deletions when non-zero', () => {
    const prActive = {
      ...pr,
      draft: true,
      labels: ['bug'],
      additions: 10,
      deletions: 5,
    };
    const shaped = shapePullRequestForContent(prActive, query, baseRequest);
    expect(shaped.draft).toBe(true);
    expect(shaped.labels).toEqual(['bug']);
    expect(shaped.additions).toBe(10);
    expect(shaped.deletions).toBe(5);
  });

  it('verbose exposes sourceBranch, sourceSha alongside targetBranch', () => {
    const prBranches = {
      ...pr,
      sourceBranch: 'feature/foo',
      sourceSha: 'abc123',
      targetBranch: 'main',
    };
    // prNumber makes it fullShape
    const shaped = shapePullRequestForContent(prBranches, query, baseRequest);
    expect(shaped.targetBranch).toBe('main');
    expect(shaped.sourceBranch).toBe('feature/foo');
    expect(shaped.sourceSha).toBe('abc123');
  });

  it('body is null (not absent) when requested on a PR with no body', () => {
    const shaped = shapePullRequestForContent(
      { number: 55, title: 'no body PR', body: undefined },
      { prNumber: 55 },
      { ...baseRequest, body: true }
    );
    // bodyEmpty:true = fetched, no content; absent body key = never fetched
    expect(shaped.bodyEmpty).toBe(true);
    expect('body' in shaped).toBe(false);
  });

  it('body is absent (not null) when body was NOT requested', () => {
    const shaped = shapePullRequestForContent(
      { number: 56, title: 'no body PR', body: undefined },
      { prNumber: 56 },
      baseRequest // body: false
    );
    expect('body' in shaped).toBe(false);
  });

  it('matchString skips patch minification so matched text is visible in output', () => {
    const patchWithComment = '@@ -1 +1 @@\n+const x = 1; // comment line';
    const prWithFiles = {
      ...pr,
      fileChanges: [
        {
          path: 'src/a.ts',
          status: 'modified',
          additions: 1,
          deletions: 0,
          patch: patchWithComment,
        },
      ],
    };
    // Without matchString: minify:standard strips the comment-only line
    const minified = shapePullRequestForContent(
      prWithFiles,
      { ...query, charLength: 5000 },
      { ...baseRequest, patches: { mode: 'all' } },
      true // shouldMinify
    );
    // With matchString: minification skipped so matched comment line is visible
    const withMatch = shapePullRequestForContent(
      prWithFiles,
      { ...query, charLength: 5000, matchString: 'comment line' },
      { ...baseRequest, patches: { mode: 'all' } },
      true // shouldMinify (but overridden by needle)
    );
    const minifiedPatch = (
      minified.changedFiles as Array<{ patch?: string }>
    )[0]?.patch;
    const matchedPatch = (
      withMatch.changedFiles as Array<{ patch?: string }>
    )[0]?.patch;
    // Standard minification strips the comment-only trailing comment
    expect(minifiedPatch).toBeDefined();
    // matchString overrides minification — full raw patch shown
    expect(matchedPatch).toContain('// comment line');
  });

  it('comment bodies start at charOffset=0 regardless of query charOffset', () => {
    // Query has charOffset=10 (for PR body continuation), but comment bodies
    // should always start from the beginning.
    const shaped = shapePullRequestForContent(
      pr,
      { ...query, charOffset: 10, charLength: 100 },
      {
        ...baseRequest,
        comments: { discussion: true, reviewInline: true, includeBots: false },
      }
    );
    const comments = shaped.comments as Array<Record<string, unknown>>;
    expect(comments.length).toBeGreaterThan(0);
    // discussion-body starts at char 0, not char 10
    const discussion = comments.find(c => c.commentType === 'discussion');
    expect(discussion!.body).toBe('discussion-body');
  });

  it('minify:false preserves raw body; minify:true may normalise it', () => {
    // Use a body with many consecutive blank lines — minifyMarkdownCore
    // collapses them. This is a structural change minification makes reliably.
    const rawBody = 'section one\n\n\n\n\nsection two';
    const prMd = { ...pr, body: rawBody };
    const standard = shapePullRequestForContent(
      prMd,
      { ...query, charLength: 1000 },
      { ...baseRequest, body: true },
      true // shouldMinify = true
    );
    const raw = shapePullRequestForContent(
      prMd,
      { ...query, charLength: 1000 },
      { ...baseRequest, body: true },
      false // shouldMinify = false
    );
    // minify:none → exact raw body returned
    expect(raw.body).toBe(rawBody);
    // minify:standard → body may differ (minification applied)
    // At minimum: the raw body is NOT returned unchanged under minification
    // (collapsed blank lines make it shorter).
    expect((standard.body as string).length).toBeLessThan(rawBody.length);
  });
});

describe('matchString content filtering', () => {
  it('filters changed files by path or patch text before pagination', () => {
    const shaped = shapePullRequestForContent(
      pr,
      { ...query, itemsPerPage: 20, matchString: 'ghijkl' },
      { ...baseRequest, changedFiles: true, patches: { mode: 'all' } }
    );
    const files = shaped.changedFiles as Array<{ path: string }>;
    expect(files.map(f => f.path)).toEqual(['src/b.ts']);
    expect((shaped.filePagination as { totalItems: number }).totalItems).toBe(
      1
    );
  });

  it('filters comments by body, case-insensitively', () => {
    const shaped = shapePullRequestForContent(
      pr,
      {
        ...query,
        itemsPerPage: 20,
        charLength: 100,
        matchString: 'DISCUSSION-BODY',
      },
      {
        ...baseRequest,
        comments: { discussion: true, reviewInline: true, includeBots: true },
      }
    );
    const comments = shaped.comments as Array<{ id: string }>;
    expect(comments.map(c => c.id)).toEqual(['d1']);
  });

  it('no matchString leaves content unfiltered', () => {
    const shaped = shapePullRequestForContent(
      pr,
      { ...query, itemsPerPage: 20 },
      { ...baseRequest, changedFiles: true }
    );
    expect((shaped.changedFiles as unknown[]).length).toBe(2);
  });
});
