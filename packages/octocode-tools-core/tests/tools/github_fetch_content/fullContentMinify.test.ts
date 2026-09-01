import { beforeEach, describe, expect, it, vi } from 'vitest';

// Capture the query the provider actually receives so we can assert which
// minify mode reaches the GitHub layer (where comment/blank-line stripping
// happens).
const getFileContent = vi.fn();

const fakeProvider = {
  type: 'github',
  capabilities: {},
  getFileContent: (...args: unknown[]) => getFileContent(...args),
};

vi.mock('../../../src/providers/factory.js', () => ({
  getProvider: () => fakeProvider,
}));

// materializeExactFile (triggered by fullContent + minify:none) would otherwise
// hit disk/network — stub it out.
vi.mock('../../../src/github/directoryFetch.js', () => ({
  fetchFileContentToDisk: vi.fn(async () => ({
    localPath: '/tmp/x',
    repoRoot: '/tmp/x',
    cached: false,
    branch: 'main',
    commitSha: '0123456789abcdef0123456789abcdef01234567',
  })),
  fetchDirectoryContents: vi.fn(),
}));

import { fetchMultipleGitHubFileContents } from '../../../src/tools/github_fetch_content/execution.js';
import {
  FileContentQueryLocalSchema,
  FileContentBulkQueryLocalSchema,
} from '../../../src/tools/github_fetch_content/scheme.js';

const FILE_WITH_COMMENTS = '// a comment\nconst x = 1;\n\nconst y = 2;\n';

function providerOk() {
  return {
    data: { path: 'src/a.ts', content: FILE_WITH_COMMENTS },
    status: 200,
    provider: 'github',
    rawResponseChars: FILE_WITH_COMMENTS.length,
  };
}

function minifyOf(): string | undefined {
  const arg = getFileContent.mock.calls[0]?.[0] as { minify?: string };
  return arg?.minify;
}

describe('ghGetFileContent — fullContent is verbatim (minify:none) by default', () => {
  beforeEach(() => {
    getFileContent.mockReset();
    getFileContent.mockResolvedValue(providerOk());
  });

  it('defaults fullContent reads to minify:none so comments are not stripped', async () => {
    const result = await fetchMultipleGitHubFileContents({
      queries: [
        {
          owner: 'o',
          repo: 'r',
          path: 'src/a.ts',
          branch: 'main',
          fullContent: true,
        },
      ],
    } as never);

    expect(getFileContent).toHaveBeenCalledTimes(1);
    expect(minifyOf()).toBe('none');
    expect(JSON.stringify(result.structuredContent)).toContain(
      '0123456789abcdef0123456789abcdef01234567'
    );
  });

  it('an explicit minify still wins over the fullContent default', async () => {
    await fetchMultipleGitHubFileContents({
      queries: [
        {
          owner: 'o',
          repo: 'r',
          path: 'src/a.ts',
          branch: 'main',
          fullContent: true,
          minify: 'standard',
        },
      ],
    } as never);

    expect(minifyOf()).toBe('standard');
  });

  it('non-fullContent reads keep the standard minify default', async () => {
    await fetchMultipleGitHubFileContents({
      queries: [{ owner: 'o', repo: 'r', path: 'src/a.ts', branch: 'main' }],
    } as never);

    expect(minifyOf()).toBe('standard');
  });

  // Regression guard for the real executor path: executeDirectTool parses the
  // inputSchema (applying any schema default) BEFORE execution runs. A schema
  // default on minify would inject 'standard' here and silently defeat the
  // fullContent→none resolution — the live bug the unit tests above missed.
  it('schema does not inject a minify default (single query)', () => {
    const parsed = FileContentQueryLocalSchema.parse({
      owner: 'o',
      repo: 'r',
      path: 'src/a.ts',
    });
    expect(parsed.minify).toBeUndefined();
  });

  it('resolves fullContent to minify:none even after inputSchema parsing', async () => {
    // Mirror the executor: parse the bulk inputSchema first, then execute.
    const parsed = FileContentBulkQueryLocalSchema.parse({
      queries: [
        {
          owner: 'o',
          repo: 'r',
          path: 'src/a.ts',
          branch: 'main',
          fullContent: true,
        },
      ],
    });
    await fetchMultipleGitHubFileContents(parsed as never);
    expect(minifyOf()).toBe('none');
  });

  it('aligns row metadata with a nested partial file and executable line continuation', async () => {
    getFileContent.mockResolvedValue({
      data: {
        path: 'src/a.ts',
        content: 'first ten lines',
        totalLines: 25,
        startLine: 1,
        endLine: 10,
        isPartial: true,
      },
      status: 200,
      provider: 'github',
      rawResponseChars: 100,
    });
    const result = await fetchMultipleGitHubFileContents({
      queries: [
        {
          owner: 'o',
          repo: 'r',
          path: 'src/a.ts',
          startLine: 1,
          endLine: 10,
          minify: 'none',
        },
      ],
    } as never);
    const row = (
      result.structuredContent as {
        results: Array<{
          data: { files: Array<Record<string, any>> };
          meta: { diagnostics?: { partial?: boolean; codes?: string[] } };
        }>;
      }
    ).results[0]!;
    expect(row.data.files[0]?.next?.continueLines).toBeDefined();
    expect(row.meta.diagnostics?.partial).toBe(true);
    expect(row.meta.diagnostics?.codes ?? []).not.toContain(
      'continuationMissing'
    );
  });
});
