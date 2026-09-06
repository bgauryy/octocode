import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  searchContentRipgrep: vi.fn(),
  readFile: vi.fn(),
}));

vi.mock('../../../src/tools/local_ripgrep/searchContentRipgrep.js', () => ({
  searchContentRipgrep: mocks.searchContentRipgrep,
}));

vi.mock('node:fs/promises', () => ({
  readFile: mocks.readFile,
}));

const { warmLikelyConsumers } =
  await import('../../../src/tools/lsp/semantic_content/semanticAnchored.js');

describe('warmLikelyConsumers', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mocks.readFile.mockResolvedValue('export const use = 1;');
  });

  it('warms beyond the old 12-file cap and reports a possible truncation signal', async () => {
    const workspaceRoot = '/repo';
    const files = Array.from({ length: 100 }, (_, index) => ({
      path: `/repo/src/consumer-${index}.ts`,
    }));
    mocks.searchContentRipgrep.mockResolvedValue({ files });
    const openDocument = vi.fn();

    const result = await warmLikelyConsumers(
      { openDocument } as never,
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

    expect(openDocument).toHaveBeenCalledTimes(100);
    expect(result.warmedFiles).toBe(100);
    expect(result.possiblyTruncated).toBe(true);
  });

  it('skips files that exceed the UTF-8 byte cap', async () => {
    mocks.searchContentRipgrep.mockResolvedValue({
      files: [{ path: '/repo/src/consumer.ts' }],
    });
    mocks.readFile.mockResolvedValue('€'.repeat(174_763));
    const openDocument = vi.fn();

    const result = await warmLikelyConsumers(
      { openDocument } as never,
      {
        absolutePath: '/repo/src/source.ts',
        resolvedSymbol: { name: 'executeBulkOperation' },
      } as never,
      '/repo'
    );

    expect(openDocument).not.toHaveBeenCalled();
    expect(result.skippedLarge).toBe(1);
    expect(result.warmedFiles).toBe(0);
    expect(result.possiblyTruncated).toBe(true);
  });

  it('follows bounded search pages and reports the complete candidate count', async () => {
    const files = Array.from({ length: 34 }, (_, i) => ({
      path: `/repo/${i}.ts`,
    }));
    mocks.searchContentRipgrep
      .mockResolvedValueOnce({
        files: files.slice(0, 20),
        pagination: { totalFiles: 34, hasMore: true, nextPage: 2 },
      })
      .mockResolvedValueOnce({
        files: files.slice(20),
        pagination: { totalFiles: 34, hasMore: false },
      });
    const openDocument = vi.fn();
    const result = await warmLikelyConsumers(
      { openDocument } as never,
      {
        absolutePath: '/repo/source.ts',
        resolvedSymbol: { name: 'target' },
      } as never,
      '/repo'
    );
    expect(mocks.searchContentRipgrep).toHaveBeenNthCalledWith(
      1,
      expect.objectContaining({ itemsPerPage: 100, page: 1 })
    );
    expect(mocks.searchContentRipgrep).toHaveBeenNthCalledWith(
      2,
      expect.objectContaining({ itemsPerPage: 100, page: 2 })
    );
    expect(openDocument).toHaveBeenCalledTimes(34);
    expect(result).toMatchObject({
      candidates: 34,
      warmedFiles: 34,
      possiblyTruncated: false,
    });
  });

  it('stops at the warmup bound while preserving the true candidate count', async () => {
    mocks.searchContentRipgrep.mockResolvedValue({
      files: Array.from({ length: 100 }, (_, i) => ({ path: `/repo/${i}.ts` })),
      pagination: { totalFiles: 134, hasMore: true, nextPage: 2 },
    });
    const openDocument = vi.fn();
    const result = await warmLikelyConsumers(
      { openDocument } as never,
      {
        absolutePath: '/repo/source.ts',
        resolvedSymbol: { name: 'target' },
      } as never,
      '/repo'
    );
    expect(mocks.searchContentRipgrep).toHaveBeenCalledTimes(1);
    expect(openDocument).toHaveBeenCalledTimes(100);
    expect(result).toMatchObject({
      candidates: 134,
      warmedFiles: 100,
      possiblyTruncated: true,
    });
  });

  it('does not report a complete warmup after search or open failures', async () => {
    const client = {
      openDocument: vi.fn().mockRejectedValue(new Error('open failed')),
    };
    const anchor = {
      absolutePath: '/repo/source.ts',
      resolvedSymbol: { name: 'target' },
    } as never;
    mocks.searchContentRipgrep.mockRejectedValueOnce(
      new Error('search failed')
    );
    expect(
      (await warmLikelyConsumers(client as never, anchor, '/repo'))
        .possiblyTruncated
    ).toBe(true);
    mocks.searchContentRipgrep.mockResolvedValueOnce({
      files: [{ path: '/repo/consumer.ts' }],
      pagination: { totalFiles: 1, hasMore: false },
    });
    expect(
      (await warmLikelyConsumers(client as never, anchor, '/repo'))
        .possiblyTruncated
    ).toBe(true);
  });

  it('does not loop forever when a continuation repeats its page', async () => {
    mocks.searchContentRipgrep.mockResolvedValue({
      files: [{ path: '/repo/consumer.ts' }],
      pagination: { totalFiles: 2, hasMore: true, nextPage: 1 },
    });
    const result = await warmLikelyConsumers(
      { openDocument: vi.fn() } as never,
      {
        absolutePath: '/repo/source.ts',
        resolvedSymbol: { name: 'target' },
      } as never,
      '/repo'
    );
    expect(mocks.searchContentRipgrep).toHaveBeenCalledTimes(1);
    expect(result.possiblyTruncated).toBe(true);
  });

  it('reports a complete scan when exactly the file bound has no continuation', async () => {
    mocks.searchContentRipgrep.mockResolvedValue({
      files: Array.from({ length: 100 }, (_, i) => ({ path: `/repo/${i}.ts` })),
      pagination: { totalFiles: 100, hasMore: false },
    });
    const result = await warmLikelyConsumers(
      { openDocument: vi.fn() } as never,
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
    mocks.searchContentRipgrep.mockResolvedValue({
      status: 'error',
      error: 'failed',
    });
    const result = await warmLikelyConsumers(
      { openDocument: vi.fn() } as never,
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
});
