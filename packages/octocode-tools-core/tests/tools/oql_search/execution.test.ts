import { describe, expect, it } from 'vitest';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { executeOqlSearchTool } from '../../../src/tools/oql_search/execution.js';

const here = path.dirname(fileURLToPath(import.meta.url));
const OQL_SRC = path.resolve(here, '../../../src/oql');

describe('executeOqlSearchTool', () => {
  it('strips MCP transport fields before validating OQL batch input', async () => {
    const result = await executeOqlSearchTool({
      sessionId: 'test-session',
      signal: new AbortController().signal,
      responseCharOffset: 0,
      responseCharLength: 1000,
      queries: [
        {
          target: 'files',
          from: { kind: 'local', path: OQL_SRC },
          where: {
            kind: 'field',
            field: 'basename',
            op: 'glob',
            value: 'planner.ts',
          },
        },
      ],
    });

    expect(result.isError).not.toBe(true);
    expect(result.structuredContent).toMatchObject({
      results: [
        {
          id: 'q0',
        },
      ],
    });

    const first = (result.structuredContent as { results: Array<{ data: { results: Array<{ path?: string }> } }> }).results[0];
    expect(first?.data.results.some(row => row.path?.endsWith('planner.ts'))).toBe(true);
  });
});
