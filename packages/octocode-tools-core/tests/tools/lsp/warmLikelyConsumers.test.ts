import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  searchContentRipgrep: vi.fn(),
  runTypedLexicalSearch: vi.fn(),
}));

vi.mock('../../../src/tools/local_ripgrep/searchContentRipgrep.js', () => ({
  searchContentRipgrep: mocks.searchContentRipgrep,
}));

vi.mock('../../../src/tools/local_search/typedLexicalService.js', () => ({
  runTypedLexicalSearch: mocks.runTypedLexicalSearch,
}));

const { warmLikelyConsumers } =
  await import('../../../src/tools/lsp/semantic_content/semanticAnchored.js');

describe('warmLikelyConsumers', () => {
  beforeEach(() => {
    vi.resetAllMocks();
  });

  it('reports unavailable name-based warmup without searching an empty pattern', async () => {
    const openDocumentFromDisk = vi.fn();
    const result = await warmLikelyConsumers(
      { openDocumentFromDisk } as never,
      {
        absolutePath: '/repo/source.ts',
        resolvedSymbol: { position: { line: 0, character: 1 } },
      } as never,
      '/repo'
    );
    expect(result).toMatchObject({
      possiblyTruncated: true,
      incompleteReasons: ['anchorName'],
    });
    expect(mocks.runTypedLexicalSearch).not.toHaveBeenCalled();
    expect(openDocumentFromDisk).not.toHaveBeenCalled();
  });

  it('warms beyond the old 12-file cap and reports a possible truncation signal', async () => {
    const workspaceRoot = '/repo';
    const files = Array.from({ length: 100 }, (_, index) => ({
      path: `/repo/src/consumer-${index}.ts`,
    }));
    mocks.runTypedLexicalSearch.mockResolvedValue({ files });
    const openDocumentFromDisk = vi.fn().mockResolvedValue('opened');

    const result = await warmLikelyConsumers(
      { openDocumentFromDisk } as never,
      {
        absolutePath: '/repo/src/source.ts',
        uri: 'file:///repo/src/source.ts',
        content: 'export function executeBulkOperation() {}',
        resolvedSymbol: {
          name: 'executeBulkOperation',
          uri: 'file:///repo/src/source.ts',
          foundAtLine: 1,
          position: { line: 0, character: 16 },
          range: {
            start: { line: 0, character: 16 },
            end: { line: 0, character: 36 },
          },
        },
      } as never,
      workspaceRoot
    );

    expect(openDocumentFromDisk).toHaveBeenCalledTimes(100);
    expect(result.warmedFiles).toBe(100);
    expect(result.possiblyTruncated).toBe(true);
  });

  it('skips files that exceed the UTF-8 byte cap', async () => {
    mocks.runTypedLexicalSearch.mockResolvedValue({
      files: [{ path: '/repo/src/consumer.ts' }],
    });
    const openDocumentFromDisk = vi
      .fn()
      .mockRejectedValue(
        new Error('File is too large for LSP document open: consumer.ts')
      );

    const result = await warmLikelyConsumers(
      { openDocumentFromDisk } as never,
      {
        absolutePath: '/repo/src/source.ts',
        resolvedSymbol: { name: 'executeBulkOperation' },
      } as never,
      '/repo'
    );

    expect(openDocumentFromDisk).toHaveBeenCalledTimes(1);
    expect(result.skippedLarge).toBe(1);
    expect(result.warmedFiles).toBe(0);
    expect(result.possiblyTruncated).toBe(true);
  });

  it('follows bounded search pages and reports the complete candidate count', async () => {
    const files = Array.from({ length: 34 }, (_, i) => ({
      path: `/repo/${i}.ts`,
    }));
    mocks.runTypedLexicalSearch
      .mockResolvedValueOnce({
        files: files.slice(0, 20),
        pagination: { totalFiles: 34, hasMore: true, nextPage: 2 },
      })
      .mockResolvedValueOnce({
        files: files.slice(20),
        pagination: { totalFiles: 34, hasMore: false },
      });
    const openDocumentFromDisk = vi.fn().mockResolvedValue('opened');
    const result = await warmLikelyConsumers(
      { openDocumentFromDisk } as never,
      {
        absolutePath: '/repo/source.ts',
        resolvedSymbol: { name: 'target' },
      } as never,
      '/repo'
    );
    expect(mocks.runTypedLexicalSearch).toHaveBeenNthCalledWith(
      1,
      expect.objectContaining({
        pageSize: 100,
        page: 1,
        regex: 'literal',
        resultView: 'files',
      })
    );
    expect(mocks.runTypedLexicalSearch).toHaveBeenNthCalledWith(
      2,
      expect.objectContaining({ pageSize: 100, page: 2 })
    );
    expect(openDocumentFromDisk).toHaveBeenCalledTimes(34);
    expect(result).toMatchObject({
      candidates: 34,
      warmedFiles: 34,
      possiblyTruncated: false,
    });
  });

  it('stops at the warmup bound while preserving the true candidate count', async () => {
    mocks.runTypedLexicalSearch.mockResolvedValue({
      files: Array.from({ length: 100 }, (_, i) => ({ path: `/repo/${i}.ts` })),
      pagination: { totalFiles: 134, hasMore: true, nextPage: 2 },
    });
    const openDocumentFromDisk = vi.fn().mockResolvedValue('opened');
    const result = await warmLikelyConsumers(
      { openDocumentFromDisk } as never,
      {
        absolutePath: '/repo/source.ts',
        resolvedSymbol: { name: 'target' },
      } as never,
      '/repo'
    );
    expect(mocks.runTypedLexicalSearch).toHaveBeenCalledTimes(1);
    expect(openDocumentFromDisk).toHaveBeenCalledTimes(100);
    expect(result).toMatchObject({
      candidates: 134,
      warmedFiles: 100,
      possiblyTruncated: true,
    });
  });

  it('does not report a complete warmup after search or open failures', async () => {
    const client = {
      openDocumentFromDisk: vi.fn().mockRejectedValue(new Error('open failed')),
    };
    const anchor = {
      absolutePath: '/repo/source.ts',
      resolvedSymbol: { name: 'target' },
    } as never;
    mocks.runTypedLexicalSearch.mockRejectedValueOnce(
      new Error('search failed')
    );
    expect(
      (await warmLikelyConsumers(client as never, anchor, '/repo'))
        .possiblyTruncated
    ).toBe(true);
    mocks.runTypedLexicalSearch.mockResolvedValueOnce({
      files: [{ path: '/repo/consumer.ts' }],
      pagination: { totalFiles: 1, hasMore: false },
    });
    expect(
      (await warmLikelyConsumers(client as never, anchor, '/repo'))
        .possiblyTruncated
    ).toBe(true);
  });

  it('does not loop forever when a continuation repeats its page', async () => {
    mocks.runTypedLexicalSearch.mockResolvedValue({
      files: [{ path: '/repo/consumer.ts' }],
      pagination: { totalFiles: 2, hasMore: true, nextPage: 1 },
    });
    const result = await warmLikelyConsumers(
      { openDocumentFromDisk: vi.fn().mockResolvedValue('opened') } as never,
      {
        absolutePath: '/repo/source.ts',
        resolvedSymbol: { name: 'target' },
      } as never,
      '/repo'
    );
    expect(mocks.runTypedLexicalSearch).toHaveBeenCalledTimes(1);
    expect(result.possiblyTruncated).toBe(true);
  });

  it('reports a complete scan when exactly the file bound has no continuation', async () => {
    mocks.runTypedLexicalSearch.mockResolvedValue({
      files: Array.from({ length: 100 }, (_, i) => ({ path: `/repo/${i}.ts` })),
      pagination: { totalFiles: 100, hasMore: false },
    });
    const result = await warmLikelyConsumers(
      { openDocumentFromDisk: vi.fn().mockResolvedValue('opened') } as never,
      {
        absolutePath: '/repo/source.ts',
        resolvedSymbol: { name: 'target' },
      } as never,
      '/repo'
    );
    expect(result).toMatchObject({
      candidates: 100,
      warmedFiles: 100,
      possiblyTruncated: false,
    });
  });

  it('preserves a returned search error as incomplete coverage', async () => {
    mocks.runTypedLexicalSearch.mockResolvedValue({
      status: 'error',
      error: 'failed',
    });
    const result = await warmLikelyConsumers(
      { openDocumentFromDisk: vi.fn().mockResolvedValue('opened') } as never,
      {
        absolutePath: '/repo/source.ts',
        resolvedSymbol: { name: 'target' },
      } as never,
      '/repo'
    );
    expect(result).toMatchObject({
      possiblyTruncated: true,
      incompleteReasons: ['search'],
    });
  });

  it.each([
    { stats: { errorCount: 1, capped: false } },
    { terminalLimit: true, partialReasons: ['nativeSearchError'] },
  ])('preserves incomplete native search coverage: %j', async coverage => {
    mocks.runTypedLexicalSearch.mockResolvedValue({
      files: [{ path: '/repo/consumer.ts' }],
      pagination: { totalFiles: 1, hasMore: false },
      ...coverage,
    });
    const openDocumentFromDisk = vi.fn().mockResolvedValue('opened');
    const result = await warmLikelyConsumers(
      { openDocumentFromDisk } as never,
      {
        absolutePath: '/repo/source.ts',
        resolvedSymbol: { name: 'target' },
      } as never,
      '/repo'
    );
    expect(openDocumentFromDisk).toHaveBeenCalledTimes(1);
    expect(result).toMatchObject({
      warmedFiles: 1,
      possiblyTruncated: true,
      incompleteReasons: ['search'],
    });
  });

  it('delegates bounded disk reuse to the pooled LSP client', async () => {
    mocks.runTypedLexicalSearch.mockResolvedValue({
      files: [{ path: '/repo/consumer.ts' }],
      pagination: { totalFiles: 1, hasMore: false },
    });
    const openDocumentFromDisk = vi.fn().mockResolvedValue('unchanged');
    const openDocument = vi.fn(() => {
      throw new Error('warmup must not reread and resend through openDocument');
    });

    const result = await warmLikelyConsumers(
      { openDocument, openDocumentFromDisk } as never,
      {
        absolutePath: '/repo/source.ts',
        resolvedSymbol: { name: 'target' },
      } as never,
      '/repo'
    );

    expect(openDocumentFromDisk).toHaveBeenCalledWith(
      '/repo/consumer.ts',
      512 * 1024
    );
    expect(openDocument).not.toHaveBeenCalled();
    expect(result.warmedFiles).toBe(1);
  });
});
