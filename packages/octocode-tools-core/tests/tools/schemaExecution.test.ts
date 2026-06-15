import { describe, expect, it } from 'vitest';

import { searchMultipleGitHubPullRequests } from '../../src/tools/github_search_pull_requests/execution.js';
import { executeFindFiles } from '../../src/tools/local_find_files/execution.js';
import { executeRipgrepSearch } from '../../src/tools/local_ripgrep/execution.js';
import { executeViewStructure } from '../../src/tools/local_view_structure/execution.js';

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
  it('returns a per-query error for contradictory localSearchCode flags', async () => {
    const result = await executeRipgrepSearch({
      queries: [
        {
          keywords: 'token',
          path: '/repo',
          caseSensitive: true,
          caseInsensitive: true,
        },
      ],
    });

    const rows = getRows(result);
    expect(result.isError).toBe(true);
    expect(rows[0]?.status).toBe('error');
    expect(getError(rows[0])).toContain(
      'caseSensitive and caseInsensitive are mutually exclusive'
    );
  });

  it('returns a per-query error for contradictory localViewStructure flags', async () => {
    const result = await executeViewStructure({
      queries: [
        {
          path: '/repo',
          filesOnly: true,
          directoriesOnly: true,
        },
      ],
    });

    const rows = getRows(result);
    expect(result.isError).toBe(true);
    expect(rows[0]?.status).toBe('error');
    expect(getError(rows[0])).toContain(
      'filesOnly and directoriesOnly are mutually exclusive'
    );
  });

  it('returns a per-query error for inverted localFindFiles depth', async () => {
    const result = await executeFindFiles({
      queries: [
        {
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
    const result = await searchMultipleGitHubPullRequests({
      queries: [
        {
          owner: 'octo',
          repo: 'repo',
          prNumber: 1,
          content: { patches: { mode: 'selected' } },
        },
      ],
    });

    const rows = getRows(result);
    expect(result.isError).toBe(true);
    expect(rows[0]?.status).toBe('error');
    expect(getError(rows[0])).toContain(
      'content.patches.mode="selected" requires non-empty files or ranges'
    );
  });
});
