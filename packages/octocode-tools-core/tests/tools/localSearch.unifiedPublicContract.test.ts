import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

import { executeDirectTool } from '../../src/tools/directToolCatalog.exec.js';
import { findDirectToolDefinition } from '../../src/tools/directToolCatalog/toolCatalogDefinitions.js';
import { executeFindFiles } from '../../src/tools/local_find_files/execution.js';
import { executeRipgrepSearch } from '../../src/tools/local_ripgrep/execution.js';
import { executeViewStructure } from '../../src/tools/local_view_structure/execution.js';

type ToolResult = Awaited<ReturnType<typeof executeDirectTool>>;

function firstData(result: ToolResult): Record<string, unknown> {
  const structured = result.structuredContent as
    { results?: Array<{ data?: Record<string, unknown> }> } | undefined;
  return structured?.results?.[0]?.data ?? {};
}

function normalizeInternalContinuations(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(normalizeInternalContinuations);
  if (!value || typeof value !== 'object') return value;
  const record = Object.fromEntries(
    Object.entries(value).map(([key, item]) => [
      key,
      key === 'searchTime' ? '<elapsed>' : normalizeInternalContinuations(item),
    ])
  );
  const operation = {
    'local.text': 'text',
    'local.files': 'files',
    'local.tree': 'tree',
  }[record.tool as string];
  if (operation) {
    record.tool = 'localSearch';
    record.query = { operation, ...(record.query as object) };
  }
  return record;
}

function withoutOperation(data: Record<string, unknown>) {
  const { operation: _operation, ...rest } = data;
  return rest;
}

function assertLocalSearchContinuationsValid(value: unknown): void {
  if (Array.isArray(value)) {
    value.forEach(assertLocalSearchContinuationsValid);
    return;
  }
  if (!value || typeof value !== 'object') return;
  const record = value as Record<string, unknown>;
  if (record.tool === 'localSearch') {
    const definition = findDirectToolDefinition('localSearch');
    expect(
      definition?.schema.safeParse(record.query).success,
      JSON.stringify(record.query)
    ).toBe(true);
  }
  Object.values(record).forEach(assertLocalSearchContinuationsValid);
}

describe('localSearch unified public contract', () => {
  let fixtureRoot = '';

  beforeAll(async () => {
    const fixtureParent = join(process.cwd(), '.octocode', 'tmp');
    await mkdir(fixtureParent, { recursive: true });
    fixtureRoot = await mkdtemp(join(fixtureParent, 'local-search-contract-'));
    await mkdir(join(fixtureRoot, 'nested'));
    await writeFile(
      join(fixtureRoot, 'alpha.ts'),
      `export const contractNeedle = 1;\n${'paginationNeedle\n'.repeat(12)}`,
      'utf8'
    );
    await writeFile(join(fixtureRoot, 'nested', 'empty.txt'), '', 'utf8');
    await writeFile(
      join(fixtureRoot, 'nested', 'beta.ts'),
      'export const secondContractNeedle = 2;\n',
      'utf8'
    );
  });

  afterAll(async () => {
    await rm(fixtureRoot, { recursive: true, force: true });
  });

  it('publishes one discriminated schema with text, structural, files, and tree variants', () => {
    const definition = findDirectToolDefinition('localSearch');
    expect(
      definition,
      'localSearch must replace the three legacy tools'
    ).toBeDefined();

    const accepted = [
      {
        operation: 'text',
        path: fixtureRoot,
        searchText: 'contractNeedle',
        resultView: 'discovery',
      },
      {
        operation: 'structural',
        path: fixtureRoot,
        pattern: 'const $NAME = $VALUE',
        langType: 'typescript',
        resultView: 'countMatches',
      },
      {
        operation: 'files',
        path: fixtureRoot,
        names: ['*.ts'],
      },
      {
        operation: 'tree',
        path: fixtureRoot,
        maxDepth: 2,
      },
    ];

    for (const query of accepted) {
      expect(
        definition!.schema.safeParse(query).success,
        JSON.stringify(query)
      ).toBe(true);
    }
  });

  it.each([
    [
      'text rejects the legacy mode selector',
      {
        operation: 'text',
        path: '/repo',
        searchText: 'needle',
        mode: 'discovery',
      },
    ],
    [
      'structural rejects the legacy output selector',
      {
        operation: 'structural',
        path: '/repo',
        pattern: 'call($ARG)',
        output: 'countMatches',
      },
    ],
    [
      'text rejects file-discovery fields',
      {
        operation: 'text',
        path: '/repo',
        searchText: 'needle',
        names: ['*.ts'],
      },
    ],
    [
      'structural rejects text-search fields',
      {
        operation: 'structural',
        path: '/repo',
        pattern: 'call($ARG)',
        searchText: 'call',
      },
    ],
    [
      'files rejects tree-only fields',
      {
        operation: 'files',
        path: '/repo',
        names: ['*.ts'],
        recursive: true,
      },
    ],
    [
      'tree rejects file-metadata fields',
      {
        operation: 'tree',
        path: '/repo',
        size: { greater: '1KB' },
      },
    ],
    [
      'tree rejects the legacy recursive alias',
      {
        operation: 'tree',
        path: '/repo',
        recursive: true,
      },
    ],
  ])('%s', (_label, query) => {
    const definition = findDirectToolDefinition('localSearch');
    expect(definition).toBeDefined();
    expect(definition!.schema.safeParse(query).success).toBe(false);
  });

  it('keeps structural pattern and rule mutually exclusive', () => {
    const definition = findDirectToolDefinition('localSearch');
    expect(definition).toBeDefined();
    expect(
      definition!.schema.safeParse({
        operation: 'structural',
        path: '/repo',
        pattern: 'call($ARG)',
        rule: 'rule:\n  kind: call_expression',
      }).success
    ).toBe(false);
  });

  it.each(['countLines', 'matchOnly'] as const)(
    'does not advertise unsupported structural resultView %s',
    resultView => {
      const definition = findDirectToolDefinition('localSearch');
      expect(definition).toBeDefined();
      expect(
        definition!.schema.safeParse({
          operation: 'structural',
          path: '/repo',
          pattern: 'call($ARG)',
          resultView,
        }).success
      ).toBe(false);
    }
  );

  it('reports a missing search root instead of returning an empty success', async () => {
    const missingPath = join(fixtureRoot, 'does-not-exist');
    const result = await executeDirectTool('localSearch', {
      queries: [
        {
          operation: 'text',
          path: missingPath,
          searchText: 'needle',
        },
      ],
    });
    const row = (
      result.structuredContent as {
        results?: Array<{
          status?: string;
          data?: { errorCode?: string };
        }>;
      }
    ).results?.[0];

    expect(row?.status).toBe('error');
    expect(row?.data?.errorCode).toBe('fileAccessFailed');
  });

  it('reports public field names without duplicate legacy validation errors', () => {
    const definition = findDirectToolDefinition('localSearch');
    const parsed = definition!.schema.safeParse({
      operation: 'files',
      path: '/repo',
      pageSize: 51,
    });

    expect(parsed.success).toBe(false);
    if (parsed.success) return;
    expect(parsed.error.issues.map(issue => issue.path.join('.'))).toEqual([
      'pageSize',
    ]);
  });

  it('dispatches text to the existing text-search behavior', async () => {
    const legacy = await executeRipgrepSearch({
      queries: [
        {
          path: fixtureRoot,
          searchText: 'contractNeedle',
          mode: 'discovery',
        },
      ],
    });
    const unified = await executeDirectTool('localSearch', {
      queries: [
        {
          operation: 'text',
          path: fixtureRoot,
          searchText: 'contractNeedle',
          resultView: 'discovery',
        },
      ],
    });
    expect(unified.isError, JSON.stringify(unified)).not.toBe(true);

    const expected = firstData(legacy);
    const actual = firstData(unified);
    expect(normalizeInternalContinuations(withoutOperation(actual))).toEqual(
      normalizeInternalContinuations(expected)
    );
    assertLocalSearchContinuationsValid(actual);
  });

  it('emits text pagination continuations that satisfy the public schema', async () => {
    const response = await executeDirectTool('localSearch', {
      queries: [
        {
          operation: 'text',
          path: fixtureRoot,
          searchText: 'paginationNeedle',
          maxMatchesPerFile: 2,
        },
      ],
    });

    expect(response.isError, JSON.stringify(response)).not.toBe(true);
    assertLocalSearchContinuationsValid(firstData(response));
  });

  it('dispatches structural to the existing structural-search behavior', async () => {
    const legacy = await executeRipgrepSearch({
      queries: [
        {
          path: fixtureRoot,
          mode: 'structural',
          pattern: 'const $NAME = $VALUE',
          langType: 'typescript',
        },
      ],
    });
    const unified = await executeDirectTool('localSearch', {
      queries: [
        {
          operation: 'structural',
          path: fixtureRoot,
          pattern: 'const $NAME = $VALUE',
          langType: 'typescript',
        },
      ],
    });
    expect(unified.isError, JSON.stringify(unified)).not.toBe(true);

    const expected = firstData(legacy);
    const actual = firstData(unified);
    expect(normalizeInternalContinuations(withoutOperation(actual))).toEqual(
      normalizeInternalContinuations(expected)
    );
    assertLocalSearchContinuationsValid(actual);
  });

  it('emits and executes schema-valid structural page continuations', async () => {
    const first = await executeDirectTool('localSearch', {
      queries: [
        {
          operation: 'structural',
          path: fixtureRoot,
          pattern: 'const $NAME = $VALUE',
          langType: 'typescript',
          resultView: 'countMatches',
          pageSize: 1,
        },
      ],
    });

    expect(first.isError, JSON.stringify(first)).not.toBe(true);
    const firstPage = firstData(first) as {
      pagination?: { hasMore?: boolean };
      next?: { nextPage?: { query?: Record<string, unknown> } };
    };
    expect(firstPage.pagination?.hasMore).toBe(true);
    assertLocalSearchContinuationsValid(firstPage);
    const continuation = firstPage.next?.nextPage?.query;
    expect(continuation).toBeDefined();

    const second = await executeDirectTool('localSearch', {
      queries: [continuation],
    });
    expect(second.isError, JSON.stringify(second)).not.toBe(true);
    expect(
      (firstData(second) as { pagination?: { currentPage?: number } })
        .pagination?.currentPage
    ).toBe(2);
  });

  it('dispatches files to the existing metadata-search behavior', async () => {
    const legacy = await executeFindFiles({
      queries: [{ path: fixtureRoot, names: ['*.ts'], entryType: 'f' }],
    });
    const unified = await executeDirectTool('localSearch', {
      queries: [
        {
          operation: 'files',
          path: fixtureRoot,
          names: ['*.ts'],
          entryType: 'f',
        },
      ],
    });

    expect(
      normalizeInternalContinuations(withoutOperation(firstData(unified)))
    ).toEqual(normalizeInternalContinuations(firstData(legacy)));
    assertLocalSearchContinuationsValid(firstData(unified));
  });

  it('dispatches tree to the existing tree-orientation behavior', async () => {
    const legacy = await executeViewStructure({
      queries: [{ path: fixtureRoot, maxDepth: 2, detail: 'basic' }],
    });
    const unified = await executeDirectTool('localSearch', {
      queries: [
        {
          operation: 'tree',
          path: fixtureRoot,
          maxDepth: 2,
          detail: 'basic',
        },
      ],
    });

    const expected = firstData(legacy);
    const actual = firstData(unified);
    expect(normalizeInternalContinuations(withoutOperation(actual))).toEqual(
      normalizeInternalContinuations(expected)
    );
    assertLocalSearchContinuationsValid(actual);
  });
});
