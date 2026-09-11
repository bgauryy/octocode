import { describe, expect, it } from 'vitest';

import { getMultipleGitHubHistoryItems } from '../../src/tools/github_search_pull_requests/historyExecutions.js';
import { executeAstSearch } from '../../src/tools/ast_search/execution.js';
import { executeLocalSearch } from '../../src/tools/local_search/execution.js';

type ResultRow = {
  readonly status: unknown;
  readonly data: unknown;
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function getRows(result: {
  readonly structuredContent?: unknown;
}): ResultRow[] {
  const structuredContent = result.structuredContent;
  if (
    !isRecord(structuredContent) ||
    !Array.isArray(structuredContent.results)
  ) {
    return [];
  }

  return structuredContent.results
    .filter(isRecord)
    .map(row => ({ status: row.status, data: row.data }));
}

function getError(row: ResultRow | undefined): string {
  if (!row || !isRecord(row.data) || typeof row.data.error !== 'string') {
    return '';
  }

  return row.data.error;
}

describe('tool execution schema validation', () => {
  it('returns a per-query error for a mis-gated localSearch query', async () => {
    const result = await executeLocalSearch({
      queries: [
        {
          searchText: 'token',
          path: '/repo',
          unique: 'list',
        },
      ],
    });

    const rows = getRows(result);
    expect(result.isError).toBe(true);
    expect(rows[0]?.status).toBe('error');
    expect(getError(rows[0])).toContain('require resultView:\\"matchOnly\\"');
  });

  it('returns a per-query error for inverted AST files depth', async () => {
    const result = await executeAstSearch({
      queries: [
        {
          operation: 'files',
          path: '/repo',
          minDepth: 4,
          maxDepth: 2,
        },
      ],
    });

    const rows = getRows(result);
    expect(result.isError).toBe(true);
    expect(rows[0]?.status).toBe('error');
    expect(getError(rows[0])).toContain(
      'minDepth must be less than or equal to maxDepth'
    );
  });

  it('returns a per-query error for unusable selected PR patch requests', async () => {
    const result = await getMultipleGitHubHistoryItems({
      queries: [
        {
          operation: 'pullRequest',
          owner: 'octo',
          repo: 'repo',
          number: 1,
          content: { patches: { mode: 'selected' } },
        },
      ],
    });

    const rows = getRows(result);
    expect(result.isError).toBe(true);
    expect(rows[0]?.status).toBe('error');
    expect(getError(rows[0])).toContain('Invalid input');
  });
});
