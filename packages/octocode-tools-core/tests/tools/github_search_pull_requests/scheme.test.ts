import { describe, expect, it } from 'vitest';

import {
  GitHubPullRequestSearchBulkQueryLocalSchema,
  GitHubPullRequestSearchQueryLocalSchema,
} from '../../../src/tools/github_search_pull_requests/scheme.js';

describe('ghHistoryResearch schema', () => {
  const baseQuery = { owner: 'octo', repo: 'repo', prNumber: 1 };

  it('rejects selected patch mode without files or ranges', () => {
    const result = GitHubPullRequestSearchQueryLocalSchema.safeParse({
      ...baseQuery,
      content: { patches: { mode: 'selected' } },
    });

    expect(result.success).toBe(false);
    if (!result.success) {
      expect(
        result.error.issues.map(issue => issue.message).join('\n')
      ).toMatch(/mode="selected" requires non-empty files or ranges/);
    }
  });

  it('rejects patch file selectors without selected mode', () => {
    const result = GitHubPullRequestSearchQueryLocalSchema.safeParse({
      ...baseQuery,
      content: { patches: { files: ['src/index.ts'] } },
    });

    expect(result.success).toBe(false);
    if (!result.success) {
      expect(
        result.error.issues.map(issue => issue.message).join('\n')
      ).toMatch(/require mode="selected"/);
    }
  });

  it('accepts selected patch mode with file selectors', () => {
    const result = GitHubPullRequestSearchQueryLocalSchema.safeParse({
      ...baseQuery,
      content: {
        patches: { mode: 'selected', files: ['src/index.ts'] },
      },
    });

    expect(result.success).toBe(true);
  });

  it('keeps bulk parsing relaxed so execution can report per-query errors', () => {
    const result = GitHubPullRequestSearchBulkQueryLocalSchema.safeParse({
      queries: [
        { ...baseQuery, content: { patches: { mode: 'selected' } } },
        { ...baseQuery, prNumber: 2 },
      ],
    });

    expect(result.success).toBe(true);
  });
});
