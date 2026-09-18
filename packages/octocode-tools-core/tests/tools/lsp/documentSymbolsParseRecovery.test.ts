import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { contextUtils } from '../../../src/utils/contextUtils.js';
import { executeLspSearch } from '../../../src/tools/lsp/semantic_content/execution.js';

describe('documentSymbols native parse recovery', () => {
  let directory: string | undefined;
  afterEach(async () => {
    vi.restoreAllMocks();
    if (directory) await rm(directory, { recursive: true, force: true });
  });

  async function read(source: string, format: 'structured' | 'compact') {
    directory = await mkdtemp(join(process.cwd(), '.tmp-symbol-recovery-'));
    const uri = join(directory, 'source.js');
    await writeFile(uri, source);
    const result = await executeLspSearch({
      queries: [{ operation: 'documentSymbols', uri, format }],
    } as Parameters<typeof executeLspSearch>[0]);
    return (
      result.structuredContent as {
        results: Array<{
          status?: string;
          data: {
            lsp: { source?: string };
            incompleteResults?: boolean;
            terminalLimit?: boolean;
            partialReasons?: string[];
            payload: {
              symbols: unknown[];
              diagnostics?: Array<{ code: string; message: string }>;
            };
          };
        }>;
      }
    ).results[0]!;
  }

  it.each(['structured', 'compact'] as const)(
    'preserves native recovery evidence in %s output',
    async format => {
      const source = 'export function good() {}\nconst broken = ;';
      expect(
        contextUtils.extractJsSymbols(source, '/tmp/source.js')
      ).toBeNull();
      const facts = JSON.parse(
        contextUtils.extractGraphFacts(source, '/tmp/source.js')!
      );
      expect(facts.diagnostics).toContain(
        'tree-sitter recovered from parse errors; graph facts may be partial'
      );

      const row = await read(source, format);
      expect(row.status).not.toBe('error');
      expect(row.data.lsp.source).toBe('native-graph-facts');
      expect(row.data.payload.symbols.length).toBeGreaterThan(0);
      expect(row.data.incompleteResults).toBe(true);
      expect(row.data.terminalLimit).toBe(true);
      expect(row.data.partialReasons).toContain('parseRecovery');
      expect(row.data.payload.diagnostics).toContainEqual({
        code: 'parseRecovery',
        message:
          'tree-sitter recovered from parse errors; graph facts may be partial',
      });
    }
  );

  it('keeps valid fallback declarations complete', async () => {
    vi.spyOn(contextUtils, 'extractJsSymbols').mockReturnValue(null);
    const row = await read('export function good() {}\n', 'structured');
    expect(row.data.lsp.source).toBe('native-graph-facts');
    expect(row.data.payload.symbols.length).toBeGreaterThan(0);
    expect(row.data.incompleteResults).not.toBe(true);
    expect(row.data.terminalLimit).not.toBe(true);
    expect(row.data.partialReasons ?? []).not.toContain('parseRecovery');
    expect(row.data.payload.diagnostics).toBeUndefined();
  });

  it('retains parse recovery when no declarations could be recovered', async () => {
    const row = await read('const broken = ;', 'structured');
    expect(row.status).not.toBe('error');
    expect(row.data.lsp.source).toBe('native-graph-facts');
    expect(row.data.payload.symbols ?? []).toEqual([]);
    expect(row.data.incompleteResults).toBe(true);
    expect(row.data.partialReasons).toContain('parseRecovery');
  });
});
