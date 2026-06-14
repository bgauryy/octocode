import { describe, it, expect } from 'vitest';

describe('lspGetSemanticContent — call-flow schema', () => {
  it('accepts callers and callees as explicit semantic content types', async () => {
    const { LspGetSemanticContentQuerySchema } =
      await import('../../../../octocode-tools-core/src/tools/lsp/semantic_content/scheme.js');

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
      await import('../../../../octocode-tools-core/src/tools/local_view_structure/local_view_structure.js');
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
      '../octocode-tools-core/src/tools/local_ripgrep/ripgrepResultBuilder.ts',
      'utf-8'
    );
    expect(src).toContain('lspGetSemanticContent');
    expect(src).toContain('lineHint');
  });
});

describe('lspGetSemanticContent — success-path handoff hints', () => {
  it('successful results return concise next-step hints', async () => {
    const { semanticHints } =
      await import('../../../../octocode-tools-core/src/tools/lsp/semantic_content/hints.js');

    for (const type of [
      'definition',
      'references',
      'callers',
      'callees',
      'callHierarchy',
      'hover',
      'documentSymbols',
      'typeDefinition',
      'implementation',
    ] as const) {
      expect(semanticHints(type, true).length).toBeGreaterThan(0);
    }
  });

  it('incomplete results still return targeted recovery guidance', async () => {
    const { semanticHints } =
      await import('../../../../octocode-tools-core/src/tools/lsp/semantic_content/hints.js');
    const result = semanticHints('definition', false);

    // Type-specific recovery only — the vague generic preamble was removed.
    expect(result.length).toBeGreaterThan(0);
    expect(result.join(' ')).toContain('rerun localSearchCode');
  });
});

describe('githubSearchPullRequests — result hint exists in code', () => {
  it('execution imports resultHints that include reviewMode guidance', async () => {
    const fs = await import('fs/promises');
    const src = await fs.readFile(
      '../octocode-tools-core/src/tools/github_search_pull_requests/execution.ts',
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
      '../octocode-tools-core/src/tools/github_search_code/execution.ts',
      'utf-8'
    );
    expect(src).toContain('githubGetFileContent');
    expect(src).toContain('extraHints');
  });
});

describe('lspGetSemanticContent — success handoff hints', () => {
  it('semanticHints returns concrete next steps on successful definition', async () => {
    const { semanticHints } =
      await import('../../../../octocode-tools-core/src/tools/lsp/semantic_content/hints.js');
    const hints = semanticHints('definition', true);
    expect(hints.join('\n')).toContain('localGetFileContent');
    expect(hints.join('\n')).toContain('references');
  });

  it('semanticHints returns lineHint guidance on successful documentSymbols', async () => {
    const { semanticHints } =
      await import('../../../../octocode-tools-core/src/tools/lsp/semantic_content/hints.js');
    const hints = semanticHints('documentSymbols', true);
    expect(hints.join('\n')).toContain('lineHint');
  });
});

describe('githubSearchRepositories — parallel explore hint', () => {
  it('execution contains parallel githubViewRepoStructure hint', async () => {
    const fs = await import('fs/promises');
    const src = await fs.readFile(
      '../octocode-tools-core/src/tools/github_search_repos/execution.ts',
      'utf-8'
    );
    expect(src).toContain('parallel');
    expect(src).toContain('githubViewRepoStructure');
  });
});
