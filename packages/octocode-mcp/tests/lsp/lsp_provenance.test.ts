/**
 * T1.8 — Regression: all 3 LSP tools must stamp `lspMode`
 * ('semantic' | 'fallback') on every successful result so the agent can
 * decide whether to trust the result as authoritative.
 *
 * Pinned by source inspection — protects against accidental removal of
 * the provenance tag.
 */
import { readFile } from 'fs/promises';
import { describe, expect, it } from 'vitest';

const TOOL_SOURCES = [
  'src/tools/lsp_find_references/lsp_find_references.ts',
  'src/tools/lsp_goto_definition/execution.ts',
  'src/tools/lsp_call_hierarchy/callHierarchy.ts',
];

describe('T1.8 — Every LSP tool stamps lspMode for provenance', () => {
  for (const relative of TOOL_SOURCES) {
    it(`${relative} stamps both 'semantic' and 'fallback'`, async () => {
      const source = await readFile(`${process.cwd()}/${relative}`, 'utf-8');
      expect(source).toMatch(/lspMode\s*:\s*['"]semantic['"]/);
      expect(source).toMatch(/lspMode\s*:\s*['"]fallback['"]/);
    });
  }
});
