/**
 * A `prNumber` lookup is a targeted single-PR fetch, so it should return the
 * FULL body — not the 500-char search preview. This also makes the truncation
 * hint ("use prNumber for full body") truthful: a plain search truncates; a
 * prNumber lookup gives the whole body.
 */
import { describe, it, expect } from 'vitest';
import { mapPullRequestProviderResultData } from '../../src/tools/providerMappers.js';

const longBody = 'x'.repeat(1200); // > MAX_PR_BODY_LENGTH (500)

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

describe('PR body: prNumber lookup returns full body', () => {
  it('search (no prNumber) truncates the body with a truthful hint', () => {
    const { resultData } = mapPullRequestProviderResultData(data);
    const body = (resultData.pull_requests as Array<{ body: string }>)[0].body;
    expect(body.length).toBeLessThan(longBody.length);
    expect(body).toMatch(/use prNumber for full body/);
  });

  it('prNumber lookup (fullBody) returns the complete body, no truncation hint', () => {
    const { resultData } = mapPullRequestProviderResultData(data, {
      fullBody: true,
    });
    const body = (resultData.pull_requests as Array<{ body: string }>)[0].body;
    expect(body).toBe(longBody);
    expect(body).not.toMatch(/chars total|use prNumber/);
  });
});
