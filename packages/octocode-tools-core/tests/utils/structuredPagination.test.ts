import { describe, expect, it } from 'vitest';

import { STATIC_TOOL_NAMES } from '../../src/tools/toolNames.js';
import type { BulkToolResponse } from '../../src/types/bulk.js';
import type { FlatQueryResult } from '../../src/types/toolResults.js';
import {
  applyBulkResponsePagination,
  applyQueryOutputPagination,
} from '../../src/utils/response/structuredPagination.js';

function localSearchResult(id: string, value: string): FlatQueryResult {
  return {
    id,
    data: {
      files: [
        {
          path: `src/${id}.ts`,
          matches: [
            {
              lineNumber: 1,
              value,
            },
          ],
        },
      ],
    },
  };
}

describe('structured response pagination', () => {
  it('applies explicit per-query output pagination with continuation hints', () => {
    const result = applyQueryOutputPagination(
      localSearchResult('q1', 'x'.repeat(240)),
      { charOffset: 0, charLength: 90 },
      STATIC_TOOL_NAMES.LOCAL_RIPGREP
    );

    expect(result.data.outputPagination).toMatchObject({
      currentPage: 1,
      hasMore: true,
      charOffset: 0,
    });
    expect(result.data.hints).toEqual(
      expect.arrayContaining([
        expect.stringContaining('Use charOffset='),
      ])
    );
    expect(result.data.files).toHaveLength(1);
  });

  it('does not paginate query results that already carry an error status', () => {
    const original: FlatQueryResult = {
      ...localSearchResult('q1', 'x'.repeat(240)),
      status: 'error',
    };

    expect(
      applyQueryOutputPagination(
        original,
        { charOffset: 0, charLength: 90 },
        STATIC_TOOL_NAMES.LOCAL_RIPGREP
      )
    ).toBe(original);
  });

  it('applies bulk result pagination without adding finalizer metadata', () => {
    const response: BulkToolResponse = {
      results: [
        localSearchResult('q1', 'a'.repeat(240)),
        localSearchResult('q2', 'b'.repeat(240)),
      ],
    };

    const paged = applyBulkResponsePagination(
      response,
      { offset: 0, length: 160 },
      STATIC_TOOL_NAMES.LOCAL_RIPGREP
    );

    expect(paged).not.toHaveProperty('responsePagination');
    expect(paged).not.toHaveProperty('hints');
    expect(paged.results.length).toBeGreaterThan(0);
    expect(paged.results.length).toBeLessThanOrEqual(response.results.length);
  });
});
