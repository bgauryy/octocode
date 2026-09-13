import { describe, expect, it } from 'vitest';
import { DIRECT_TOOL_SPECIFICATIONS } from '@octocodeai/octocode-core/schema';
import { applyHintPolicy } from '../../src/utils/response/hintPolicy.js';

describe('error recovery guidance', () => {
  it.each(DIRECT_TOOL_SPECIFICATIONS.map(tool => tool.name))(
    'does not invent a query repair for an operational %s failure',
    toolName => {
      const rows = [
        {
          index: 0,
          status: 'error',
          data: {
            error: 'The provider process failed.',
            errorCode: 'execution_failed',
          },
        },
      ];
      applyHintPolicy(rows, { toolName, queries: [{ apply: true }] });
      expect(rows[0]?.data).not.toHaveProperty('hints');
    }
  );

  it('preserves explicit recovery and its executable query', () => {
    const query = { path: '/fixture', searchText: '[', regex: 'literal' };
    const rows = [
      {
        status: 'error',
        data: {
          error: 'Invalid regex.',
          hints: ['Use literal matching.'],
          next: { retry: { tool: 'localSearch', query } },
        },
      },
    ];
    applyHintPolicy(rows, { toolName: 'localSearch' });
    expect(rows[0]?.data.hints).toEqual(['Use literal matching.']);
    expect(rows[0]?.data.next.retry.query).toEqual(query);
  });

  it('retains discovery guidance for an empty result', () => {
    const rows = [{ status: 'empty', data: {} }];
    applyHintPolicy(rows, { toolName: 'localSearch' });
    expect(rows[0]?.data).toHaveProperty('hints', [
      'Broaden searchText, path, or filters.',
    ]);
  });
});
