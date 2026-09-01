import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { executeLspGetSemantics } from '../../../src/tools/lsp/semantic_content/execution.js';

const tempDirs: string[] = [];

afterEach(async () => {
  await Promise.all(
    tempDirs.splice(0).map(dir => rm(dir, { recursive: true, force: true }))
  );
});

describe('lspGetSemantics tools-core smoke', () => {
  it('returns a row-level recovery error when workspaceSymbol lacks symbolName', async () => {
    const result = await executeLspGetSemantics({
      queries: [{ type: 'workspaceSymbol' }],
    } as never);

    const structured = result.structuredContent as {
      results?: Array<{
        status?: string;
        data?: { error?: string };
      }>;
    };
    const row = structured.results?.[0];
    expect(row?.status).toBe('error');
    expect(row?.data?.error).toContain('Set symbolName for workspaceSymbol.');
  });

  it('returns native documentSymbols for a local TypeScript file', async () => {
    const dir = await mkdtemp(join(process.cwd(), '.tmp-octocode-lsp-smoke-'));
    tempDirs.push(dir);
    const filePath = join(dir, 'fixture.ts');
    await writeFile(
      filePath,
      [
        'export function alpha() {',
        '  return 1;',
        '}',
        'export const beta = 2;',
      ].join('\n')
    );

    const result = await executeLspGetSemantics({
      queries: [
        {
          uri: filePath,
          type: 'documentSymbols',
          format: 'compact',
        },
      ],
    } as never);

    expect(result.isError).not.toBe(true);
    const structured = result.structuredContent as {
      results?: Array<{
        data?: {
          type?: string;
          payload?: { kind?: string };
          pagination?: { pageSize?: number; itemsPerPage?: number };
        };
      }>;
    };
    const row = structured?.results?.[0]?.data;
    expect(row?.type).toBe('documentSymbols');
    expect(row?.payload?.kind).toBe('documentSymbols');
    expect(row?.pagination?.pageSize).toBeGreaterThan(0);
    expect(row?.pagination?.itemsPerPage).toBeUndefined();
  });

  it('executes the emitted documentSymbols page continuation', async () => {
    const dir = await mkdtemp(join(process.cwd(), '.tmp-octocode-lsp-page-'));
    tempDirs.push(dir);
    const filePath = join(dir, 'fixture.ts');
    await writeFile(
      filePath,
      [
        'export const alpha = 1;',
        'export const beta = 2;',
        'export const gamma = 3;',
      ].join('\n')
    );

    const first = await executeLspGetSemantics({
      queries: [
        {
          uri: filePath,
          type: 'documentSymbols',
          page: 1,
          pageSize: 1,
          format: 'structured',
        },
      ],
    } as never);
    const firstData = (
      first.structuredContent as {
        results?: Array<{
          data?: {
            payload?: { symbols?: unknown[] };
            next?: {
              nextPage?: { query?: Record<string, unknown> };
            };
          };
        }>;
      }
    ).results?.[0]?.data;
    const continuation = firstData?.next?.nextPage?.query;
    expect(continuation).toMatchObject({
      uri: filePath,
      type: 'documentSymbols',
      page: 2,
      pageSize: 1,
    });

    const second = await executeLspGetSemantics({
      queries: [continuation],
    } as never);
    const secondData = (
      second.structuredContent as {
        results?: Array<{
          data?: {
            payload?: { symbols?: unknown[] };
            pagination?: { currentPage?: number };
          };
        }>;
      }
    ).results?.[0]?.data;

    expect(second.isError).not.toBe(true);
    expect(secondData?.pagination?.currentPage).toBe(2);
    expect(secondData?.payload?.symbols).not.toEqual(
      firstData?.payload?.symbols
    );
  });
});
