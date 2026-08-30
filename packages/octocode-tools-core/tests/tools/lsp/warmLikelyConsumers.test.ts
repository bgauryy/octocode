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
    vi.clearAllMocks();
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
});
