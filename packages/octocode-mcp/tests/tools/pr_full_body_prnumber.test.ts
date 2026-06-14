import { describe, it, expect } from 'vitest';
import { mapPullRequestProviderResultData } from '../../../octocode-tools-core/src/tools/providerMappers.js';

const longBody = 'x'.repeat(1200);

const data = {
  items: [
    {
      number: 1964,
      title: 'feat: optional deps',
      body: longBody,
      url: 'https://github.com/o/r/pull/1964',
      state: 'open',
    },
  ],
  totalCount: 1,
} as never;

describe('PR body: never truncated (lossless)', () => {
  it('search returns the complete body with no truncation marker', () => {
    const { resultData } = mapPullRequestProviderResultData(data);
    const body = (resultData.pull_requests as Array<{ body: string }>)[0]!.body;
    expect(body).toBe(longBody);
    expect(body).not.toMatch(/chars total|use prNumber|\[truncated/i);
  });

  it('prNumber lookup also returns the complete body', () => {
    const { resultData } = mapPullRequestProviderResultData(data, {
      includeFileChanges: true,
    });
    const body = (resultData.pull_requests as Array<{ body: string }>)[0]!.body;
    expect(body).toBe(longBody);
  });
});
