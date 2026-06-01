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
    // #A1: the presentation-only TSV envelope lives in content[0].text; the
    // structuredContent carries the canonical structured records only.
    // (text is YAML/JSON, so the row tabs are escaped as literal "\t".)
    const text = (result.content[0] as { text: string }).text;
    expect(text).toMatch(/format.*tsv/);
    expect(text).toContain('o\\tr\\t');
    expect(text).toContain('TypeScript');
    expect(text).toContain('2026-05-24');

    const sc = result.structuredContent as Record<string, unknown>;
    expect(sc.format).toBeUndefined();
    expect(sc.columns).toBeUndefined();
    expect(sc.rows).toBeUndefined();
    expect(sc.base).toBeUndefined();
    expect(sc.shared).toBeUndefined();
    expect(Array.isArray(sc.results)).toBe(true);
  });

  it('does NOT emit TSV envelope when config.format=json (default branch silent)', async () => {
    const result = await executeBulkOperation(
      [{ id: 'q1' }],
      vi.fn().mockResolvedValue({
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
