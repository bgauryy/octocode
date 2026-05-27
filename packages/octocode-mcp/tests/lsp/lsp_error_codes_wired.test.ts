/**
 * T2.1b — Every LSP tool error path must also surface a structured
 * `errorCode` from LSP_ERROR_CODES. The legacy `errorType` stays for
 * the dynamic-hint engine; `errorCode` is the stable wire-level value
 * agents pattern-match on.
 *
 * Source-pinning test (cheap, no IO) — protects against regressions
 * where a new branch is added without the structured code.
 */
import { readFile } from 'fs/promises';
import { describe, expect, it } from 'vitest';

const TARGETS: Array<{ file: string; minOccurrences: number }> = [
  {
    file: 'src/tools/lsp_find_references/lsp_find_references.ts',
    minOccurrences: 1,
  },
  { file: 'src/tools/lsp_goto_definition/execution.ts', minOccurrences: 1 },
  { file: 'src/tools/lsp_call_hierarchy/callHierarchy.ts', minOccurrences: 1 },
  {
    file: 'src/tools/lsp_call_hierarchy/callHierarchyLsp.ts',
    minOccurrences: 1,
  },
];

describe('T2.1b — LSP_ERROR_CODES is wired into every LSP tool', () => {
  for (const target of TARGETS) {
    it(`${target.file} imports LSP_ERROR_CODES and uses it on the symbol-not-found path`, async () => {
      const source = await readFile(`${process.cwd()}/${target.file}`, 'utf-8');
      expect(source).toMatch(/LSP_ERROR_CODES/);
      expect(source).toMatch(/LSP_ERROR_CODES\.SYMBOL_NOT_FOUND/);
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
