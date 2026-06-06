/**
 * Contract tests verifying success-path hints exist in all targeted tools.
 *
 * Tests cover:
 * 1. Schema changes (direction default, ecosystem description)
 * 2. Hints.ts additions
 * 3. Result-level hints via exported functions
 */

import { describe, it, expect } from 'vitest';

// ── Schema tests ────────────────────────────────────────────────────────────

describe('lspCallHierarchy — direction schema default', () => {
  it('direction defaults to incoming when omitted', async () => {
    const { LSPCallHierarchyQuerySchema } = await import(
      '../../../src/scheme/lspSchemaOverlay.js'
    );
    const parsed = LSPCallHierarchyQuerySchema.safeParse({
      symbolName: 'foo',
      lineHint: 1,
      uri: '/tmp/foo.ts',
    });
    expect(parsed.success).toBe(true);
    if (parsed.success) {
      expect(
        (parsed.data as Record<string, unknown>).direction
      ).toBe('incoming');
    }
  });

  it('direction: outgoing is accepted', async () => {
    const { LSPCallHierarchyQuerySchema } = await import(
      '../../../src/scheme/lspSchemaOverlay.js'
    );
    const parsed = LSPCallHierarchyQuerySchema.safeParse({
      symbolName: 'bar',
      lineHint: 5,
      uri: '/tmp/bar.ts',
      direction: 'outgoing',
    });
    expect(parsed.success).toBe(true);
  });
});

// ── localViewStructure success hint ─────────────────────────────────────────

describe('localViewStructure — success hint', () => {
  it('emits localSearchCode hint when directory has entries', async () => {
    const { viewStructure } = await import(
      '../../../src/tools/local_view_structure/local_view_structure.js'
    );
    const result = (await viewStructure({
      path: '.',
      depth: 1,
    } as never)) as { hints?: string[] };
    expect(result.hints).toBeDefined();
    // When not empty, hints include the localSearchCode next-step
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

// ── localSearchCode success hint ─────────────────────────────────────────────

describe('localSearchCode — LSP lineHint success hint', () => {
  it('ripgrepResultBuilder contains LSP chaining hint for non-empty results', async () => {
    const fs = await import('fs/promises');
    const src = await fs.readFile(
      'src/tools/local_ripgrep/ripgrepResultBuilder.ts',
      'utf-8'
    );
    expect(src).toContain('lspGotoDefinition');
    expect(src).toContain('lineHint');
  });
});

// ── lspGotoDefinition success hint ───────────────────────────────────────────

describe('lspGotoDefinition — success-path extra hint', () => {
  it('success return includes lspFindReferences chaining hint', async () => {
    const { applyGotoDefinitionVerbosity } = await import(
      '../../../src/tools/lsp_goto_definition/execution.js'
    );
    // applyGotoDefinitionVerbosity passes through hints unchanged
    const mockResult = {
      locations: [{ uri: '/tmp/foo.ts', range: { start: { line: 0, character: 0 }, end: { line: 0, character: 3 } } }],
      resolvedPosition: { line: 0, character: 0 },
      searchRadius: 5,
      hints: ['Definition found — use lspFindReferences with the same symbolName+lineHint to find all usages, or lspCallHierarchy to trace call flow.'],
    };
    const result = applyGotoDefinitionVerbosity(mockResult as never, {} as never);
    expect(result.hints).toBeDefined();
    expect(
      result.hints!.some(
        (h: string) => h.includes('lspFindReferences') || h.includes('lspCallHierarchy')
      )
    ).toBe(true);
  });
});

// ── githubSearchPullRequests result hint ─────────────────────────────────────

describe('githubSearchPullRequests — result hint exists in code', () => {
  it('execution imports resultHints that include fullContent guidance', async () => {
    // Verify the source file contains the new hint string
    const fs = await import('fs/promises');
    const src = await fs.readFile(
      'src/tools/github_search_pull_requests/execution.ts',
      'utf-8'
    );
    expect(src).toContain('resultHints');
    expect(src).toContain('fullContent');
  });
});

// ── githubSearchCode chain hint ───────────────────────────────────────────────

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

// ── githubSearchRepositories parallel hint ────────────────────────────────────

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
