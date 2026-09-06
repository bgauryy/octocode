import { beforeEach, expect, it, vi } from 'vitest';
import { RequestError } from 'octokit';
const mocks = vi.hoisted(() => ({ content: vi.fn(), defaultBranch: vi.fn() }));
vi.mock('../../src/github/client.js', () => ({
  getOctokit: async () => ({ rest: { repos: { getContent: mocks.content } } }),
  resolveDefaultBranch: mocks.defaultBranch,
}));
vi.mock('../../src/github/fileContentRaw/pathSuggestions.js', () => ({
  findPathSuggestions: async () => [],
  buildPathSuggestionHints: () => [],
}));
import { fetchRawGitHubFileContent } from '../../src/github/fileContentRaw/fetch.js';

beforeEach(() => vi.resetAllMocks());
it.each(['main', 'master', 'release'])(
  'never substitutes another branch after an explicit %s miss',
  async branch => {
    mocks.defaultBranch.mockResolvedValue('development');
    mocks.content.mockImplementation(async ({ ref }) => {
      if (ref === branch)
        throw new RequestError('Not Found', 404, {
          request: {
            method: 'GET',
            url: 'https://api.github.com/repos/o/r/contents/a.ts',
            headers: {},
          },
        });
      return {
        data: {
          type: 'file',
          content: Buffer.from('wrong branch').toString('base64'),
          size: 12,
        },
        headers: {},
      };
    });
    const result = await fetchRawGitHubFileContent({
      owner: 'o',
      repo: 'r',
      path: 'a.ts',
      branch,
    });
    expect(result).toMatchObject({ status: 404 });
    expect('data' in result).toBe(false);
    expect(mocks.content).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(result)).not.toMatch(/Ask user|Branch .* not found/);
  }
);
