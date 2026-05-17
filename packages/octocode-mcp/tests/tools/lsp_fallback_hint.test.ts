/**
 * LSP fallback-hint regression tests.
 *
 * Ensures each LSP tool emits an explicit "LSP unavailable" hint when
 * `isLanguageServerAvailable` returns false. Without this signal, agents
 * mistake the text-based fallback for real semantic results and report
 * "LSP isn't resolving symbols for this project (likely no TS server indexed)".
 *
 * @module tests/tools/lsp_fallback_hint.test
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

// fs/promises — readFile / stat are used by each tool before the LSP path
vi.mock('fs/promises', () => ({
  readFile: vi.fn(),
  stat: vi.fn(),
}));

vi.mock('child_process', () => ({
  exec: vi.fn(),
  spawn: vi.fn(),
}));

vi.mock('util', () => ({
  promisify: (fn: Function) => fn,
}));

vi.mock('../../src/lsp/resolver.js', () => {
  class MockSymbolResolutionError extends Error {
    searchRadius: number;
    constructor(message: string, searchRadius: number) {
      super(message);
      this.name = 'SymbolResolutionError';
      this.searchRadius = searchRadius;
    }
  }
  return {
    // must use regular function — invoked via `new SymbolResolver(...)`
    SymbolResolver: vi.fn().mockImplementation(function () {
      return {
        resolvePositionFromContent: vi.fn().mockReturnValue({
          position: { line: 3, character: 7 },
          foundAtLine: 4,
        }),
        extractContext: vi.fn().mockReturnValue({
          content: 'const testSymbol = 1;',
          startLine: 3,
          endLine: 5,
        }),
      };
    }),
    SymbolResolutionError: MockSymbolResolutionError,
  };
});

vi.mock('../../src/lsp/manager.js', async importOriginal => {
  const actual = await importOriginal<object>();
  return {
    ...actual,
    acquirePooledClient: vi.fn().mockResolvedValue(null),
    isLanguageServerAvailable: vi.fn().mockResolvedValue(false),
  };
});

// Force the pattern-matching fallback for find references to an empty result
// (keeps the test independent of ripgrep being on PATH).
vi.mock(
  '../../src/tools/lsp_find_references/lspReferencesPatterns.js',
  async () => ({
    findReferencesWithPatternMatching: vi.fn().mockResolvedValue({
      status: 'empty',
      hints: [],
    }),
    escapeForRegex: (v: string) => v,
  })
);

vi.mock(
  '../../src/tools/lsp_call_hierarchy/callHierarchyPatterns.js',
  async () => ({
    callHierarchyWithPatternMatching: vi.fn().mockResolvedValue({
      status: 'empty',
      hints: [],
    }),
    parseRipgrepJsonOutput: vi.fn(),
    extractFunctionBody: vi.fn(),
  })
);

import * as fs from 'fs/promises';
import * as managerModule from '../../src/lsp/manager.js';

import { findReferences } from '../../src/tools/lsp_find_references/lsp_find_references.js';
import { processCallHierarchy } from '../../src/tools/lsp_call_hierarchy/callHierarchy.js';

const SAMPLE = `
export function testSymbol(): void {
  return;
}
`.trim();

const LSP_UNAVAILABLE_RE =
  /LSP unavailable|text-based fallback|install typescript-language-server/i;

describe('LSP fallback hint — surfaced when isLanguageServerAvailable=false', () => {
  const testPath = `${process.cwd()}/src/testfile.ts`;

  beforeEach(() => {
    process.env.WORKSPACE_ROOT = process.cwd();
    vi.mocked(fs.readFile).mockResolvedValue(SAMPLE);
    vi.mocked(fs.stat).mockResolvedValue({
      isFile: () => true,
    } as unknown as Awaited<ReturnType<typeof fs.stat>>);
    vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(false);
    vi.mocked(managerModule.acquirePooledClient).mockResolvedValue(null);
  });

  afterEach(() => {
    delete process.env.WORKSPACE_ROOT;
    vi.clearAllMocks();
  });

  it('lspFindReferences emits an LSP-unavailable hint', async () => {
    const result = await findReferences({
      uri: testPath,
      symbolName: 'testSymbol',
      lineHint: 4,
      mainResearchGoal: 'g',
      researchGoal: 'g',
      reasoning: 'r',
    } as unknown as Parameters<typeof findReferences>[0]);

    const hints = (result.hints ?? []).filter(Boolean) as string[];
    expect(
      hints.some(h => LSP_UNAVAILABLE_RE.test(h)),
      `expected an LSP-unavailable hint, got: ${JSON.stringify(hints)}`
    ).toBe(true);
    expect(
      result.lspMode,
      'lspFindReferences must mark fallback path with lspMode="fallback" so agents can branch programmatically without parsing hint strings'
    ).toBe('fallback');
  });

  it('lspCallHierarchy emits an LSP-unavailable hint', async () => {
    const result = await processCallHierarchy({
      uri: testPath,
      symbolName: 'testSymbol',
      lineHint: 4,
      direction: 'incoming',
      mainResearchGoal: 'g',
      researchGoal: 'g',
      reasoning: 'r',
    } as unknown as Parameters<typeof processCallHierarchy>[0]);

    const hints = (result.hints ?? []).filter(Boolean) as string[];
    expect(
      hints.some(h => LSP_UNAVAILABLE_RE.test(h)),
      `expected an LSP-unavailable hint, got: ${JSON.stringify(hints)}`
    ).toBe(true);
    expect(
      result.lspMode,
      'lspCallHierarchy must mark fallback path with lspMode="fallback"'
    ).toBe('fallback');
  });

  // Regression: char-budget pagination must not drop the LSP-unavailable hint.
  // applyCallHierarchyOutputLimit previously rebuilt hints from pagedData.hints
  // (a JSON slice that may be empty on page 2+). Aligning with goto-definition's
  // pattern — re-spreading result.hints into combinedHints — keeps the install
  // hint visible on every page so agents never silently treat a fallback chain
  // as a real semantic call graph.
  it('lspCallHierarchy preserves the LSP-unavailable hint across char-budget pagination', async () => {
    const longCalls = Array.from({ length: 12 }, (_, i) => ({
      from: {
        name: `caller_${i}`,
        kind: 'function' as const,
        uri: testPath,
        range: {
          start: { line: i, character: 0 },
          end: { line: i, character: 50 },
        },
        content: `function caller_${i}() { testSymbol(); }`.repeat(8),
      },
      fromRanges: [
        {
          start: { line: i, character: 10 },
          end: { line: i, character: 20 },
        },
      ],
    }));

    const callHierarchyPatterns =
      await import('../../src/tools/lsp_call_hierarchy/callHierarchyPatterns.js');
    vi.mocked(
      callHierarchyPatterns.callHierarchyWithPatternMatching
    ).mockResolvedValueOnce({
      status: 'hasResults',
      item: {
        name: 'testSymbol',
        kind: 'function',
        uri: testPath,
        range: {
          start: { line: 0, character: 0 },
          end: { line: 2, character: 1 },
        },
      },
      incomingCalls: longCalls,
      direction: 'incoming',
      depth: 1,
      hints: [],
    });

    const pageOne = await processCallHierarchy({
      uri: testPath,
      symbolName: 'testSymbol',
      lineHint: 4,
      direction: 'incoming',
      charLength: 200,
      mainResearchGoal: 'g',
      researchGoal: 'g',
      reasoning: 'r',
    } as unknown as Parameters<typeof processCallHierarchy>[0]);

    const totalChars = (
      pageOne as unknown as {
        outputPagination?: { totalChars?: number; charLength?: number };
      }
    ).outputPagination?.totalChars;
    expect(totalChars, 'expected pagination to trigger').toBeGreaterThan(200);

    vi.mocked(
      callHierarchyPatterns.callHierarchyWithPatternMatching
    ).mockResolvedValueOnce({
      status: 'hasResults',
      item: {
        name: 'testSymbol',
        kind: 'function',
        uri: testPath,
        range: {
          start: { line: 0, character: 0 },
          end: { line: 2, character: 1 },
        },
      },
      incomingCalls: longCalls,
      direction: 'incoming',
      depth: 1,
      hints: [],
    });

    const pageTwo = await processCallHierarchy({
      uri: testPath,
      symbolName: 'testSymbol',
      lineHint: 4,
      direction: 'incoming',
      charOffset: 200,
      charLength: 200,
      mainResearchGoal: 'g',
      researchGoal: 'g',
      reasoning: 'r',
    } as unknown as Parameters<typeof processCallHierarchy>[0]);

    const pageTwoHints = (pageTwo.hints ?? []).filter(Boolean) as string[];
    expect(
      pageTwoHints.some(h => LSP_UNAVAILABLE_RE.test(h)),
      `page 2 must still carry the LSP-unavailable hint, got: ${JSON.stringify(pageTwoHints)}`
    ).toBe(true);
    // lspMode is the structured signal that survives independently of hint
    // string parsing; agents can route on it without regex matching.
    expect(
      pageTwo.lspMode,
      'page 2 must still carry lspMode="fallback" — structured field survives char-budget pagination'
    ).toBe('fallback');
  });
});

describe('LSP goto-definition fallback hint', () => {
  const testPath = `${process.cwd()}/src/testfile.ts`;

  beforeEach(() => {
    process.env.WORKSPACE_ROOT = process.cwd();
    vi.mocked(fs.readFile).mockResolvedValue(SAMPLE);
    vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(false);
    vi.mocked(managerModule.acquirePooledClient).mockResolvedValue(null);
  });

  afterEach(() => {
    delete process.env.WORKSPACE_ROOT;
    vi.clearAllMocks();
  });

  it('createFallbackResult path emits the LSP-unavailable hint', async () => {
    // gotoDefinition is module-private; drive it via the registered handler.
    const { registerLSPGotoDefinitionTool } =
      await import('../../src/tools/lsp_goto_definition/lsp_goto_definition.js');
    const calls: unknown[] = [];
    const mockServer = {
      registerTool: vi.fn((_n, _c, handler) => {
        calls.push(handler);
        return handler;
      }),
    };
    registerLSPGotoDefinitionTool(mockServer as never);
    const handler = calls[0] as (args: {
      queries: unknown[];
    }) => Promise<{ content: { text: string }[] }>;

    const result = await handler({
      queries: [
        {
          uri: testPath,
          symbolName: 'testSymbol',
          lineHint: 4,
          mainResearchGoal: 'g',
          researchGoal: 'g',
          reasoning: 'r',
        },
      ],
    });

    const text = result.content?.[0]?.text ?? '';
    expect(
      LSP_UNAVAILABLE_RE.test(text),
      `expected LSP-unavailable hint in serialized output, got: ${text.slice(0, 400)}`
    ).toBe(true);
    // Bulk responses serialise as YAML, so accept either YAML (lspMode: "fallback")
    // or JSON ("lspMode": "fallback"). Quoting is optional in YAML.
    expect(
      /lspMode["']?\s*:\s*["']?fallback["']?/.test(text),
      `expected lspMode="fallback" in serialized output, got: ${text.slice(0, 400)}`
    ).toBe(true);
  });
});

// Mirror tests for the LSP-success path — confirms lspMode is set both ways
// (not just on fallback). Without this, a regression that hard-codes
// lspMode='fallback' would still pass the fallback tests above.
vi.mock('../../src/tools/lsp_find_references/lspReferencesCore.js', () => ({
  findReferencesWithLSP: vi.fn(),
}));
vi.mock('../../src/tools/lsp_call_hierarchy/callHierarchyLsp.js', () => ({
  callHierarchyWithLSP: vi.fn(),
}));

describe('LSP mode field — semantic when LSP returns results', () => {
  const testPath = `${process.cwd()}/src/testfile.ts`;

  beforeEach(() => {
    process.env.WORKSPACE_ROOT = process.cwd();
    vi.mocked(fs.readFile).mockResolvedValue(SAMPLE);
    vi.mocked(fs.stat).mockResolvedValue({
      isFile: () => true,
    } as unknown as Awaited<ReturnType<typeof fs.stat>>);
    vi.mocked(managerModule.isLanguageServerAvailable).mockResolvedValue(true);
  });

  afterEach(() => {
    delete process.env.WORKSPACE_ROOT;
    vi.clearAllMocks();
  });

  it('lspFindReferences tags result with lspMode="semantic" when LSP returns locations', async () => {
    const lspCore =
      await import('../../src/tools/lsp_find_references/lspReferencesCore.js');
    vi.mocked(lspCore.findReferencesWithLSP).mockResolvedValueOnce({
      status: 'hasResults',
      locations: [
        {
          uri: testPath,
          range: {
            start: { line: 0, character: 7 },
            end: { line: 0, character: 17 },
          },
          content: 'export function testSymbol(): void {',
        },
      ],
      hints: [],
    });

    const result = await findReferences({
      uri: testPath,
      symbolName: 'testSymbol',
      lineHint: 1,
      mainResearchGoal: 'g',
      researchGoal: 'g',
      reasoning: 'r',
    } as unknown as Parameters<typeof findReferences>[0]);

    expect(
      result.lspMode,
      'LSP-only branch (pattern returned empty) must be tagged semantic'
    ).toBe('semantic');
    const hints = (result.hints ?? []).filter(Boolean) as string[];
    expect(
      hints.some(h => LSP_UNAVAILABLE_RE.test(h)),
      'semantic results must NOT carry the LSP-unavailable hint'
    ).toBe(false);
  });

  it('lspCallHierarchy tags result with lspMode="semantic" when LSP returns a call graph', async () => {
    const callLsp =
      await import('../../src/tools/lsp_call_hierarchy/callHierarchyLsp.js');
    vi.mocked(callLsp.callHierarchyWithLSP).mockResolvedValueOnce({
      status: 'hasResults',
      item: {
        name: 'testSymbol',
        kind: 'function',
        uri: testPath,
        range: {
          start: { line: 0, character: 0 },
          end: { line: 2, character: 1 },
        },
      },
      incomingCalls: [],
      direction: 'incoming',
      depth: 1,
      hints: [],
    });

    const result = await processCallHierarchy({
      uri: testPath,
      symbolName: 'testSymbol',
      lineHint: 1,
      direction: 'incoming',
      mainResearchGoal: 'g',
      researchGoal: 'g',
      reasoning: 'r',
    } as unknown as Parameters<typeof processCallHierarchy>[0]);

    expect(result.lspMode, 'LSP success path must be tagged semantic').toBe(
      'semantic'
    );
    const hints = (result.hints ?? []).filter(Boolean) as string[];
    expect(
      hints.some(h => LSP_UNAVAILABLE_RE.test(h)),
      'semantic results must NOT carry the LSP-unavailable hint'
    ).toBe(false);
  });
});
