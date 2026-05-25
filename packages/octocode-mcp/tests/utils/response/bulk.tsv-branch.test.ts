/**
 * Branch coverage for the generic-bulk TSV emission path in `bulk.ts`.
 *
 * When `config.format === 'tsv'` and a projection exists for the toolName,
 * `responseData` gains `format`, `columns`, `rows`. The previous test
 * surface only exercised TSV via custom finalizers; this test hits the
 * generic path directly.
 */

import { describe, it, expect, vi } from 'vitest';
import { executeBulkOperation } from '../../../src/utils/response/bulk.js';
import { TOOL_NAMES } from '../../../src/tools/toolMetadata/proxies.js';

describe('bulk.ts — TSV branch in the generic path', () => {
  it('emits format/columns/rows when config.format=tsv and a projection exists', async () => {
    const result = await executeBulkOperation(
      [{ id: 'q1' }],
      vi.fn().mockResolvedValue({
        status: 'hasResults' as const,
        repositories: [
          {
            owner: 'o',
            repo: 'r',
            stars: 1,
            language: 'TypeScript',
            pushedAt: '2026-05-24',
            forksCount: 0,
            openIssuesCount: 0,
            topics: ['x'],
            description: 'd',
          },
        ],
      }),
      {
        toolName: TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
        format: 'tsv',
        peerHints: true,
      }
    );
    const sc = result.structuredContent as Record<string, unknown>;
    expect(sc.format).toBe('tsv');
    expect(Array.isArray(sc.columns)).toBe(true);
    expect(typeof sc.rows).toBe('string');
    expect(String(sc.rows)).toContain('o\tr\t1');
  });

  it('does NOT emit TSV envelope when config.format=json (default branch silent)', async () => {
    const result = await executeBulkOperation(
      [{ id: 'q1' }],
      vi.fn().mockResolvedValue({
        status: 'hasResults' as const,
        repositories: [{ owner: 'o', repo: 'r' }],
      }),
      {
        toolName: TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
        format: 'json',
        peerHints: true,
      }
    );
    const sc = result.structuredContent as Record<string, unknown>;
    expect(sc.format).toBeUndefined();
    expect(sc.columns).toBeUndefined();
    expect(sc.rows).toBeUndefined();
  });

  it('skips TSV emission gracefully when no projection is registered for the toolName', async () => {
    const result = await executeBulkOperation(
      [{ id: 'q1' }],
      vi.fn().mockResolvedValue({
        status: 'hasResults' as const,
        data: { x: 1 },
      }),
      {
        toolName: 'unknown-tool-without-projection',
        format: 'tsv',
      }
    );
    const sc = result.structuredContent as Record<string, unknown>;
    expect(sc.format).toBeUndefined();
    expect(sc.rows).toBeUndefined();
  });
});
