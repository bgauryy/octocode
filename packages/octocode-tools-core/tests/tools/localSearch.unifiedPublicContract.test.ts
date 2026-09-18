import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { executeDirectTool } from '../helpers/executeDirectTool.js';
import { findDirectToolDefinition } from '@octocodeai/octocode-core/schema';
import { LocalSearchQuerySchema } from '@octocodeai/octocode-core/schema';

type ToolResult = Awaited<ReturnType<typeof executeDirectTool>>;
type Row = { status?: string; data: Record<string, unknown> };

function firstRow(result: ToolResult): Row {
  return (result.structuredContent as { results: Row[] }).results[0]!;
}

function assertContinuations(value: unknown): void {
  if (Array.isArray(value)) return value.forEach(assertContinuations);
  if (!value || typeof value !== 'object') return;
  const record = value as Record<string, unknown>;
  if (record.tool === 'localSearch') {
    expect(typeof record.query).toBe('object');
    expect(LocalSearchQuerySchema.safeParse(record.query).success).toBe(true);
    expect((record.query as Record<string, unknown>).operation).toBeUndefined();
  }
  Object.values(record).forEach(assertContinuations);
}

describe('localSearch lexical public contract', () => {
  let root = '';

  beforeAll(async () => {
    await mkdir(join(process.cwd(), '.octocode', 'tmp'), { recursive: true });
    root = await mkdtemp(
      join(process.cwd(), '.octocode', 'tmp', 'local-search-contract-')
    );
    await writeFile(
      join(root, 'alpha.ts'),
      `export const contractNeedle = 1;\n${'paginationNeedle\n'.repeat(12)}`
    );
    await writeFile(
      join(root, 'beta.ts'),
      'export const secondContractNeedle = 2;\n'
    );
  });

  afterAll(async () => rm(root, { recursive: true, force: true }));

  it('accepts operation-free lexical queries and rejects legacy/AST fields', () => {
    const reasoning = 'Validate the operation-free localSearch contract.';
    const definition = findDirectToolDefinition('localSearch');
    expect(definition).toBeDefined();
    const schema = definition!.inputSchema;
    expect(
      schema.safeParse({ queries: [{ reasoning, path: root, searchText: 'needle' }] })
        .success
    ).toBe(true);
    expect(
      schema.safeParse({
        queries: [
          { reasoning, path: root, searchText: 'needle', operation: 'text' },
        ],
      }).success
    ).toBe(false);
    expect(
      schema.safeParse({
        queries: [{ reasoning, path: root, pattern: 'const $X = $Y' }],
      })
        .success
    ).toBe(false);
    expect(
      schema.safeParse({
        queries: [
          { reasoning, path: root, searchText: 'needle', mode: 'structural' },
        ],
      }).success
    ).toBe(false);
  });

  it.each([
    'paginated',
    'discovery',
    'detailed',
    'content',
    'files',
    'filesWithout',
    'countLines',
    'countMatches',
    'matchOnly',
  ] as const)('supports lexical resultView:%s', async resultView => {
    const result = await executeDirectTool('localSearch', {
      queries: [
        {
          path: root,
          searchText: 'contractNeedle',
          regex: 'literal',
          resultView,
        },
      ],
    });
    expect(result.isError, JSON.stringify(result)).not.toBe(true);
    expect(firstRow(result).status).not.toBe('error');
    assertContinuations(firstRow(result).data);
  });

  it.each(['literal', 'rust', 'pcre2'] as const)(
    'executes regex:%s',
    async regex => {
      const result = await executeDirectTool('localSearch', {
        queries: [
          {
            path: root,
            searchText:
              regex === 'pcre2' ? 'contract(?:Needle)?' : 'contractNeedle',
            regex,
            resultView: 'countMatches',
          },
        ],
      });
      expect(result.isError, JSON.stringify(result)).not.toBe(true);
      expect(
        (firstRow(result).data.stats as { totalOccurrences?: number })
          .totalOccurrences
      ).toBeGreaterThan(0);
    }
  );

  it('returns schema-valid operation-free continuations', async () => {
    const result = await executeDirectTool('localSearch', {
      queries: [
        {
          path: root,
          searchText: 'paginationNeedle',
          resultView: 'paginated',
          pageSize: 1,
        },
      ],
    });
    expect(result.isError, JSON.stringify(result)).not.toBe(true);
    assertContinuations(firstRow(result).data);
  });

  it('reports a missing search root as an error row', async () => {
    const result = await executeDirectTool('localSearch', {
      queries: [{ path: join(root, 'missing'), searchText: 'needle' }],
    });
    expect(firstRow(result).status).toBe('error');
    expect(firstRow(result).data.errorCode).toBe('fileAccessFailed');
  });
});
