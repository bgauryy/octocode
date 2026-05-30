/**
 * T1.8 — Regression: the 3 LSP tools stamp `lspMode: 'fallback'` ONLY when
 * a text-fallback path produced the result. Semantic results omit the
 * field entirely (absent ≡ semantic) — that's the new lean contract.
 *
 * Pinned by source inspection — protects against accidental emission of
 * `lspMode: 'semantic'` (which would be redundant noise) and ensures the
 * fallback marker survives future refactors.
 */
import { readFile } from 'fs/promises';
import { describe, expect, it } from 'vitest';

const TOOL_SOURCES = [
  'src/tools/lsp_find_references/lsp_find_references.ts',
  'src/tools/lsp_goto_definition/execution.ts',
  'src/tools/lsp_call_hierarchy/callHierarchy.ts',
];

describe('T1.8 — Every LSP tool stamps lspMode=fallback for downgraded provenance', () => {
  for (const relative of TOOL_SOURCES) {
    it(`${relative} stamps 'fallback' (and omits 'semantic')`, async () => {
      const source = await readFile(`${process.cwd()}/${relative}`, 'utf-8');
      expect(source).toMatch(/lspMode\s*:\s*['"]fallback['"]/);
      // 'semantic' should not appear as an emitted value (absent ≡ semantic).
      // It may appear in type annotations / comments, but never as a literal
      // value assigned to `lspMode`.
      expect(source).not.toMatch(/lspMode\s*:\s*['"]semantic['"]/);
    });
  }
});
