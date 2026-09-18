import { beforeEach, expect, it, vi } from 'vitest';
import { FileContentQueryLocalSchema } from '@octocodeai/octocode-core/schema';
import { fetchGitHubFileContentAPI } from '../../src/github/fileContent.js';
import { transformFileContentResult } from '../../src/providers/github/githubContent.js';
import {
  mapFileContentProviderResult,
  mapFileContentToolQuery,
} from '../../src/tools/providerMappers/fileContent.js';
import { readFileEntry } from '../../src/tools/github_fetch_content/finalizer/entryParsers.js';
import { clearAllCache } from '../../src/utils/http/cache/management.js';

const fixture = vi.hoisted(() => ({
  head: 'a'.repeat(40),
  getCommit: vi.fn(),
  getContent: vi.fn(),
  listCommits: vi.fn(async () => ({ data: [] })),
}));
vi.mock('../../src/github/client.js', () => ({
  getOctokit: async () => ({ rest: { repos: fixture } }),
  resolveCacheAuthFingerprint: async () => 'snapshot-fixture',
  resolveDefaultBranch: async () => 'main',
}));

beforeEach(() => {
  clearAllCache();
  vi.clearAllMocks();
  fixture.head = 'a'.repeat(40);
  fixture.getCommit.mockImplementation(async () => ({
    data: { sha: fixture.head },
  }));
  fixture.getContent.mockImplementation(async ({ ref }) => {
    const revision = /^[a-f0-9]{40}$/.test(ref ?? '') ? ref : fixture.head;
    return {
      data: {
        type: 'file',
        size: 13,
        content: Buffer.from(
          revision === 'a'.repeat(40) ? 'Hello World!\n' : 'Changed text\n'
        ).toString('base64'),
      },
      headers: {},
    };
  });
});

async function read(input: Record<string, unknown>) {
  const query = {
    ...FileContentQueryLocalSchema.parse(input),
    minify: 'none' as const,
  };
  const response = await fetchGitHubFileContentAPI(query);
  if (!('data' in response) || !response.data)
    throw new Error(JSON.stringify(response));
  const provider = transformFileContentResult(
    response.data,
    mapFileContentToolQuery(query)
  );
  return readFileEntry(mapFileContentProviderResult(provider, query), query);
}

it.each(['main', undefined])(
  'keeps all byte pages at one commit when %s moves between processes',
  async branch => {
    let query: Record<string, unknown> = {
      owner: 'o',
      repo: 'r',
      path: 'README',
      branch,
      chunkType: 'bytes',
      limit: 5,
    };
    const pages: string[] = [];
    for (let index = 0; index < 5; index++) {
      const page = await read(query);
      pages.push(page.content);
      if (!page.next?.continue) break;
      expect(page.next.continue.query.branch).toBe('a'.repeat(40));
      expect(
        FileContentQueryLocalSchema.safeParse(page.next.continue.query).success
      ).toBe(true);
      fixture.head = 'b'.repeat(40);
      clearAllCache();
      query = page.next.continue.query;
    }
    expect(pages.join('')).toBe('Hello World!\n');
    expect(fixture.getCommit).toHaveBeenCalledTimes(1);
    expect(fixture.listCommits).toHaveBeenCalledWith(
      expect.objectContaining({ sha: 'a'.repeat(40) })
    );
  }
);

it('reads an already pinned SHA without a ref-resolution request', async () => {
  const page = await read({
    owner: 'o',
    repo: 'r',
    path: 'README',
    branch: 'a'.repeat(40),
  });
  expect(page.content).toBe('Hello World!\n');
  expect(fixture.getCommit).not.toHaveBeenCalled();
});
