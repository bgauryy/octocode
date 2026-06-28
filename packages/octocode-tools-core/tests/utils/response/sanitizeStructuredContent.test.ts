import { describe, expect, it } from 'vitest';

import { sanitizeStructuredContent } from '../../../src/responses.js';

const AWS = 'AKIAIOSFODNN7EXAMPLE';
const PAT = 'ghp_1234567890abcdefghijklmnopqrstuvwxyzAB';

describe('sanitizeStructuredContent', () => {
  it('redacts secrets nested in arrays/objects while preserving structure and benign data', () => {
    const input = {
      results: [
        {
          id: 'r1',
          data: {
            files: [
              { path: 'src/a.ts', matches: [{ line: 3, value: 'foo(bar)' }] },
              {
                path: 'src/secret.ts',
                matches: [
                  { line: 5, value: `const key = "${AWS}";` },
                  { line: 9, value: `const tok = "${PAT}";` },
                ],
              },
            ],
          },
        },
      ],
      pagination: { currentPage: 1, hasMore: false },
      enabled: true,
      count: 2,
    };

    const out = sanitizeStructuredContent(input) as typeof input;
    const flat = JSON.stringify(out);

    // Secrets are gone, replaced by JSON-safe redaction tokens.
    expect(flat).not.toContain(AWS);
    expect(flat).not.toContain('ghp_1234567890');
    expect(flat).toContain('[REDACTED-');

    // Structure + benign data preserved exactly.
    expect(out.results[0].data.files[0].matches[0].value).toBe('foo(bar)');
    expect(out.results[0].data.files[0].path).toBe('src/a.ts');
    expect(out.results[0].data.files[1].path).toBe('src/secret.ts');
    expect(out.results[0].data.files[1].matches).toHaveLength(2);
    expect(out.pagination).toEqual({ currentPage: 1, hasMore: false });
    expect(out.enabled).toBe(true);
    expect(out.count).toBe(2);
  });

  it('handles a bare secret string and non-string primitives', () => {
    expect(sanitizeStructuredContent(PAT)).toContain('[REDACTED-');
    expect(sanitizeStructuredContent('foo(bar)')).toBe('foo(bar)');
    expect(sanitizeStructuredContent(42)).toBe(42);
    expect(sanitizeStructuredContent(true)).toBe(true);
    expect(sanitizeStructuredContent(null)).toBe(null);
    expect(sanitizeStructuredContent(undefined)).toBe(undefined);
  });
});
