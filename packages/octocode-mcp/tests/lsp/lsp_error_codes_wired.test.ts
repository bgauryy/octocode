import { readFile } from 'fs/promises';
import { describe, expect, it } from 'vitest';

const TARGETS: Array<{ file: string; minOccurrences: number }> = [
  {
    file: 'src/tools/lsp/shared/resolveSymbolAnchor.ts',
    minOccurrences: 2,
  },
];

describe('T2.1b — LSP_ERROR_CODES is wired into every LSP tool', () => {
  for (const target of TARGETS) {
    it(`${target.file} imports LSP_ERROR_CODES and uses it on normalized LSP error paths`, async () => {
      const source = await readFile(`${process.cwd()}/${target.file}`, 'utf-8');
      expect(source).toMatch(/LSP_ERROR_CODES/);
      expect(source).toMatch(/LSP_ERROR_CODES\.SYMBOL_NOT_FOUND/);
      expect(
        [...source.matchAll(/LSP_ERROR_CODES\./g)].length
      ).toBeGreaterThanOrEqual(target.minOccurrences);
    });
  }

  it('the canonical taxonomy exports the codes used by tools', async () => {
    const { LSP_ERROR_CODES } = await import('../../src/lsp/lspErrorCodes.js');
    expect(LSP_ERROR_CODES.SYMBOL_NOT_FOUND).toBe('SYMBOL_NOT_FOUND');
    expect(LSP_ERROR_CODES.LSP_TIMEOUT).toBe('LSP_TIMEOUT');
    expect(LSP_ERROR_CODES.LSP_CAPABILITY_UNSUPPORTED).toBe(
      'LSP_CAPABILITY_UNSUPPORTED'
    );
  });
});
