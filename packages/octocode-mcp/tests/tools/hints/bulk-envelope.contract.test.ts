/**
 * Bulk hint envelope contract.
 *
 * Drives `executeBulkOperation` (src/utils/response/bulk.ts) with synthetic
 * per-query results to verify:
 *
 *  1. Per-query hints are preserved on each `results[i]`.
 *  2. With `peerHints: true`, hints are deduped and lifted to root.
 *  3. With `peerHints: false`, hints stay nested.
 *  4. Empty + error queries co-exist in one bulk response.
 *  5. The hint envelope tolerates a mix of empty and non-empty hint arrays.
 */

import { describe, it, expect } from 'vitest';

import { executeBulkOperation } from '../../../src/utils/response/bulk.js';
import type {
  ProcessedBulkResult,
  ToolSuccessResult,
} from '../../../src/types/toolResults.js';
import { sanitizeStructuredContent } from '../../../src/responses.js';
import { STATIC_TOOL_NAMES } from '../../../src/tools/toolNames.js';

function payload(
  result: import('@modelcontextprotocol/sdk/types').CallToolResult
) {
  return sanitizeStructuredContent(
    result.structuredContent as Record<string, unknown>
  );
}

function ok(
  _id: string,
  data: Record<string, unknown>,
  hints?: string[]
): ProcessedBulkResult {
  const success: ToolSuccessResult = {
    ...data,
    ...(hints?.length ? { hints } : {}),
  };
  return success as ProcessedBulkResult;
}

function empty(_id: string, hints?: string[]): ProcessedBulkResult {
  return {
    status: 'empty',
    ...(hints?.length ? { hints } : {}),
  } as ProcessedBulkResult;
}

describe('executeBulkOperation — hint envelope', () => {
  it('preserves per-query hints when peerHints=false', async () => {
    const queries = [{ id: 'q1' }, { id: 'q2' }];
    const result = await executeBulkOperation(
      queries,
      async (q, i) => ok(q.id, { value: i }, [`hint-for-${q.id}`]),
      {
        toolName: STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
        peerHints: false,
      }
    );

    const data = payload(result);
    expect(data.results).toBeDefined();
    const results = data.results as Array<{
      id: string;
      data?: Record<string, unknown>;
    }>;
    expect(results).toHaveLength(2);
    // hints stay nested in each query's data
    const r1Hints = (results[0]?.data as { hints?: string[] })?.hints;
    expect(r1Hints).toEqual(['hint-for-q1']);
    // and not lifted
    expect(data.hints).toBeUndefined();
  });

  it('lifts + dedupes hints when peerHints=true', async () => {
    const queries = [{ id: 'a' }, { id: 'b' }, { id: 'c' }];
    const result = await executeBulkOperation(
      queries,
      async q => ok(q.id, { v: 1 }, ['shared-hint', `unique-${q.id}`]),
      {
        toolName: STATIC_TOOL_NAMES.LOCAL_RIPGREP,
        peerHints: true,
      }
    );

    const data = payload(result);
    expect(Array.isArray(data.hints)).toBe(true);
    const hints = data.hints as string[];
    expect(hints.filter(h => h === 'shared-hint')).toHaveLength(1);
    expect(hints).toContain('unique-a');
    expect(hints).toContain('unique-b');
    expect(hints).toContain('unique-c');
    // and the per-query nested hint is removed
    const results = data.results as Array<{ data?: Record<string, unknown> }>;
    expect((results[0]?.data as { hints?: unknown })?.hints).toBeUndefined();
  });

  it('handles mixed status — hasResults + empty + (peer-lifted) hints', async () => {
    const queries = [{ id: 'has' }, { id: 'none' }];
    const result = await executeBulkOperation(
      queries,
      async q => {
        if (q.id === 'has') return ok(q.id, { v: 1 }, ['ok-hint']);
        return empty(q.id, ['empty-hint']);
      },
      {
        toolName: STATIC_TOOL_NAMES.LOCAL_RIPGREP,
        peerHints: true,
      }
    );

    const data = payload(result);
    expect(data.hints).toEqual(
      expect.arrayContaining(['ok-hint', 'empty-hint'])
    );
    const results = data.results as Array<{ id: string; status: string }>;
    expect(results.find(r => r.id === 'has')?.status).toBeUndefined();
    expect(results.find(r => r.id === 'none')?.status).toBe('empty');
  });

  it('isolates per-query errors without aborting the bulk', async () => {
    const queries = [{ id: 'good' }, { id: 'bad' }];
    const result = await executeBulkOperation(
      queries,
      async q => {
        if (q.id === 'bad') throw new Error('synthetic failure');
        return ok(q.id, { v: 1 });
      },
      {
        toolName: STATIC_TOOL_NAMES.LOCAL_RIPGREP,
        peerHints: true,
      }
    );

    const data = payload(result);
    const results = data.results as Array<{ id: string; status: string }>;
    expect(results.find(r => r.id === 'good')?.status).toBeUndefined();
    expect(results.find(r => r.id === 'bad')?.status).toBe('error');
  });

  it('emits no top-level hints when every query has zero hints', async () => {
    const queries = [{ id: 'q' }];
    const result = await executeBulkOperation(
      queries,
      async q => ok(q.id, { v: 1 }),
      {
        toolName: STATIC_TOOL_NAMES.LOCAL_RIPGREP,
        peerHints: true,
      }
    );

    const data = payload(result);
    expect(data.hints).toBeUndefined();
  });

  it('respects the dedup contract — same hint twice across queries collapses', async () => {
    const queries = [{ id: 'a' }, { id: 'b' }, { id: 'c' }];
    const result = await executeBulkOperation(
      queries,
      async q =>
        ok(q.id, { v: 1 }, ['Showing page 1 of 5. Use page=2 for more.']),
      {
        toolName: STATIC_TOOL_NAMES.LSP_FIND_REFERENCES,
        peerHints: true,
      }
    );

    const data = payload(result);
    const hints = data.hints as string[];
    expect(hints).toHaveLength(1);
    expect(hints[0]).toContain('page=2');
  });
});

describe('executeBulkOperation — single query (degenerate bulk)', () => {
  it('one-query call still wraps as bulk with results[]', async () => {
    const result = await executeBulkOperation(
      [{ id: 'only' }],
      async q => ok(q.id, { found: true }, ['lone-hint']),
      {
        toolName: STATIC_TOOL_NAMES.LOCAL_RIPGREP,
        peerHints: true,
      }
    );

    const data = payload(result);
    expect(data.results).toHaveLength(1);
    expect(data.hints).toEqual(['lone-hint']);
  });
});
