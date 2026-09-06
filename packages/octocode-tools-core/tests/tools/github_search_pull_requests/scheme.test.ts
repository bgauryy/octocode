import { describe, expect, it } from 'vitest';

import {
  GitHubPullRequestSearchBulkQueryLocalSchema,
  GitHubPullRequestSearchQueryLocalSchema,
} from '../../../src/tools/github_search_pull_requests/scheme.js';
import { formatDirectToolSchemaText } from '../../../src/tools/directToolCatalog/toolCatalogFormatters.js';

describe('internal pull-request history schema', () => {
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

  it('emits input JSON schema so defaulted fields are not required from agents', () => {
    const schema = JSON.parse(formatDirectToolSchemaText('ghGetHistoryItem'));
    const querySchema = schema.properties.queries.items;
    const branches = querySchema.anyOf ?? querySchema.oneOf ?? [querySchema];

    const pullRequest = branches.find(
      (branch: { properties?: { operation?: { const?: string } } }) =>
        branch.properties?.operation?.const === 'pullRequest'
    );
    expect(pullRequest).toBeDefined();
    expect(pullRequest.required ?? []).not.toContain('pageSize');
    expect(pullRequest.required ?? []).not.toContain('minify');
    expect(pullRequest.properties.pageSize).toMatchObject({ minimum: 1 });
    expect(pullRequest.properties.minify.enum).toEqual(['none', 'standard']);
  });
});
