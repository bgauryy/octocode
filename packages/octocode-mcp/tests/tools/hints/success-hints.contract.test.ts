import { describe, it, expect } from 'vitest';

describe('lspGetSemanticContent — call-flow schema', () => {
  it('accepts callers and callees as explicit semantic content types', async () => {
    const { LspGetSemanticContentQuerySchema } =
      await import('../../../src/tools/lsp/semantic_content/scheme.js');

    for (const type of ['callers', 'callees'] as const) {
      const parsed = LspGetSemanticContentQuerySchema.safeParse({
        type,
        symbolName: 'foo',
        lineHint: 1,
        uri: '/tmp/foo.ts',
      });
      expect(parsed.success).toBe(true);
    }
  });
});

describe('localViewStructure — success hint', () => {
  it('emits localSearchCode hint when directory has entries', async () => {
    const { viewStructure } =
      await import('../../../src/tools/local_view_structure/local_view_structure.js');
    const result = (await viewStructure({
      path: '.',
      depth: 1,
    } as never)) as { hints?: string[] };
    expect(result.hints).toBeDefined();
    if (result.hints && !('status' in result)) {
      expect(
        result.hints.some(
          (h: string) =>
            h.includes('localSearchCode') || h.includes('localGetFileContent')
        )
      ).toBe(true);
    }
  });
});

describe('localSearchCode — LSP lineHint success hint', () => {
  it('ripgrepResultBuilder contains LSP chaining hint for non-empty results', async () => {
    const fs = await import('fs/promises');
    const src = await fs.readFile(
      'src/tools/local_ripgrep/ripgrepResultBuilder.ts',
      'utf-8'
    );
    expect(src).toContain('lspGetSemanticContent');
    expect(src).toContain('lineHint');
  });
});

describe('lspGetSemanticContent — success-path extra hint', () => {
  it('definition success hint points to current semantic content types', async () => {
    const { semanticHints } =
      await import('../../../src/tools/lsp/semantic_content/hints.js');
    const result = semanticHints('definition', true);
    const joined = result.join(' ');

    expect(joined).toContain('type="references"');
    expect(joined).toContain('type="callers"');
    expect(joined).toContain('type="callees"');
  });
});

describe('githubSearchPullRequests — result hint exists in code', () => {
  it('execution imports resultHints that include reviewMode guidance', async () => {
    const fs = await import('fs/promises');
    const src = await fs.readFile(
      'src/tools/github_search_pull_requests/execution.ts',
      'utf-8'
    );
    expect(src).toContain('resultHints');
    expect(src).toContain('reviewMode');
  });
});

describe('githubSearchCode — chain hint', () => {
  it('execution contains githubGetFileContent escalation hint', async () => {
    const fs = await import('fs/promises');
    const src = await fs.readFile(
      'src/tools/github_search_code/execution.ts',
      'utf-8'
    );
    expect(src).toContain('githubGetFileContent');
    expect(src).toContain('extraHints');
  });
});

describe('githubSearchRepositories — parallel explore hint', () => {
  it('execution contains parallel githubViewRepoStructure hint', async () => {
    const fs = await import('fs/promises');
    const src = await fs.readFile(
      'src/tools/github_search_repos/execution.ts',
      'utf-8'
    );
    expect(src).toContain('parallel');
    expect(src).toContain('githubViewRepoStructure');
  });
});
