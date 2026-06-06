import { describe, it, expect, vi } from 'vitest';

import { createSuccessResult } from '../../../src/tools/utils.js';
import { createErrorResult } from '../../../src/utils/response/error.js';
import { STATIC_TOOL_NAMES } from '../../../src/tools/toolNames.js';
import { getHints } from '../../../src/hints/index.js';

describe('createSuccessResult — hasResults path', () => {
  it('does not inject per-tool registry hints on hasResults', () => {
    const result = createSuccessResult(
      {},
      { items: ['a', 'b'] },
      true,
      STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
      {
        hintContext: { hasOwnerRepo: true, owner: 'a', repo: 'b' },
      }
    );

    expect(result.status).toBeUndefined();
    expect((result as Record<string, unknown>).hints).toBeUndefined();
  });

  it('passes extraHints through on hasResults', () => {
    const result = createSuccessResult(
      {},
      { items: ['a'] },
      true,
      STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
      {
        extraHints: ['Page 1/3 (showing 1-10 of 25 matches). Next: page=2'],
      }
    );

    expect(result.hints).toEqual([
      'Page 1/3 (showing 1-10 of 25 matches). Next: page=2',
    ]);
  });

  it('does not invoke the registry on success (verified via spy)', () => {
    const reg = vi.fn(getHints);
    const result = createSuccessResult(
      {},
      { ok: true },
      true,
      STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
      {
        hintContext: {
          hasOwnerRepo: true,
          owner: 'facebook',
          repo: 'react',
          extension: 'ts',
        },
      }
    );

    expect((result as { hints?: string[] }).hints).toBeUndefined();
    void reg;
  });
});

describe('createSuccessResult — empty path', () => {
  it('emits per-tool empty hint when context names a filter', () => {
    const result = createSuccessResult(
      {},
      { items: [] },
      false,
      STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
      {
        hintContext: {
          hasOwnerRepo: true,
          owner: 'a',
          repo: 'b',
        },
      }
    );

    expect(result.status).toBe('empty');
    expect(result.hints?.[0]).toContain('a/b');
  });

  it('merges per-tool empty hint with extraHints (no duplication)', () => {
    const result = createSuccessResult(
      {},
      { items: [] },
      false,
      STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
      {
        hintContext: { hasOwnerRepo: true, owner: 'a', repo: 'b' },
        extraHints: ['extra-from-executor'],
      }
    );

    expect(result.hints).toContain('extra-from-executor');
    expect(result.hints?.some(h => h.includes('a/b'))).toBe(true);
  });

  it('dedupes identical hints across registry + extra', () => {
    const result = createSuccessResult(
      {},
      { items: [] },
      false,
      STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
      {
        hintContext: { hasOwnerRepo: true, owner: 'a', repo: 'b' },
        extraHints: ['No matches in a/b.', 'No matches in a/b.'],
      }
    );

    const occurrences =
      result.hints?.filter(h => h === 'No matches in a/b.').length ?? 0;
    expect(occurrences).toBe(1);
  });

  it('filters empty strings', () => {
    const result = createSuccessResult(
      {},
      { items: [] },
      false,
      STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
      {
        extraHints: ['', '   ', 'real hint'],
      }
    );

    expect(result.hints).toEqual(['real hint']);
  });
});

describe('createErrorResult — per-tool error hints', () => {
  it('emits LSP error hint for symbol_not_found', () => {
    const result = createErrorResult(
      new Error('boom'),
      {},
      {
        toolName: STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION,
        hintContext: {
          errorType: 'symbol_not_found',
          symbolName: 'foo',
          lineHint: 12,
        },
      }
    );

    expect(result.status).toBe('error');
    expect(result.hints?.some(h => h.includes('foo') && h.includes('12'))).toBe(
      true
    );
  });

  it('emits clone permission hint', () => {
    const result = createErrorResult(
      new Error('denied'),
      {},
      {
        toolName: STATIC_TOOL_NAMES.GITHUB_CLONE_REPO,
        hintContext: { errorType: 'permission' },
      }
    );

    expect(result.hints?.some(h => h.includes('Token'))).toBe(true);
  });

  it('appends customHints to per-tool error hints', () => {
    const result = createErrorResult(
      new Error('denied'),
      {},
      {
        toolName: STATIC_TOOL_NAMES.GITHUB_CLONE_REPO,
        hintContext: { errorType: 'permission' },
        customHints: ['Extra: see ~/.octocode/'],
      }
    );

    expect(result.hints).toEqual(
      expect.arrayContaining([
        expect.stringContaining('Token'),
        'Extra: see ~/.octocode/',
      ])
    );
  });

  it('no error hint when errorType is unknown', () => {
    const result = createErrorResult(
      new Error('huh'),
      {},
      {
        toolName: STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION,
        hintContext: { errorType: 'mystery' as never },
      }
    );

    const lspNarration = (result.hints ?? []).filter(
      h => h.includes('Symbol') || h.includes('File not found')
    );
    expect(lspNarration).toEqual([]);
  });
});

describe('HintStatus narrowing', () => {
  it('getHints with empty status works', () => {
    const hints = getHints(STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE, 'empty', {
      hasOwnerRepo: true,
      owner: 'a',
      repo: 'b',
    });
    expect(hints[0]).toContain('a/b');
  });

  it('getHints with error status works', () => {
    const hints = getHints(STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY, 'error', {
      errorType: 'not_a_function',
    });
    expect(hints[0]).toContain('not a function');
  });

  it('returns empty array for unknown tool', () => {
    expect(getHints('nonExistentTool', 'empty')).toEqual([]);
  });
});
