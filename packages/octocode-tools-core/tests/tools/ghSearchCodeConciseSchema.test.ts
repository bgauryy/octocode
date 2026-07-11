import { describe, expect, it } from 'vitest';
import { GitHubCodeSearchOutputLocalSchema } from '../../src/tools/github_search_code/scheme.js';

describe('ghSearchCode output schema — concise files', () => {
  it('accepts concise:true string file rows (MCP structuredContent)', () => {
    const parsed = GitHubCodeSearchOutputLocalSchema.safeParse({
      results: [
        {
          id: 'c',
          data: {
            files: ['sindresorhus/is:AGENTS.md', 'sindresorhus/is:readme.md'],
            pagination: {
              currentPage: 1,
              totalPages: 1,
              hasMore: false,
            },
          },
        },
      ],
    });
    expect(parsed.success).toBe(true);
  });

  it('still accepts structured file objects', () => {
    const parsed = GitHubCodeSearchOutputLocalSchema.safeParse({
      results: [
        {
          id: 'c',
          data: {
            files: [
              {
                owner: 'sindresorhus',
                repo: 'is',
                path: 'readme.md',
                matches: [{ value: 'assert' }],
              },
            ],
          },
        },
      ],
    });
    expect(parsed.success).toBe(true);
  });
});
