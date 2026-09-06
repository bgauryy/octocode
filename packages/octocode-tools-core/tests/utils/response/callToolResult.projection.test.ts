import { beforeEach, describe, expect, it, vi } from 'vitest';

const scan = vi.hoisted(() => vi.fn());
vi.mock('@octocodeai/octocode-engine/security', () => ({
  ContentSanitizer: { sanitizeContent: scan },
}));
vi.mock('@octocodeai/octocode-engine/contentSanitizer', () => ({
  ContentSanitizer: { sanitizeContent: scan },
}));
vi.mock('../../../src/utils/contextUtils.js', () => ({ contextUtils: {} }));

import {
  buildToolErrorResult,
  sanitizeCallToolResult,
} from '../../../src/utils/response/callToolResult.js';

describe('structured-only direct output projection', () => {
  beforeEach(() => {
    scan.mockReset().mockImplementation((content: string) => ({
      content: content.replaceAll('sensitive-fixture', '[redacted]'),
    }));
  });

  it('preserves compact output and scans every structured string without scanning unused text', () => {
    const input = {
      content: [
        { type: 'text' as const, text: 'unused rendered text'.repeat(3_000) },
      ],
      structuredContent: {
        results: [
          {
            data: {
              nested: ['sensitive-fixture', { path: 'safe.ts' }],
              count: 1,
            },
          },
        ],
        next: { query: { file: 'next.ts' } },
      },
      isError: false,
    };
    const full = sanitizeCallToolResult(input);
    scan.mockClear();

    const compact = sanitizeCallToolResult(input, 'structured');

    expect(compact.structuredContent).toEqual(full.structuredContent);
    expect(compact.isError).toBe(false);
    expect(compact.content).toEqual([]);
    expect(scan.mock.calls.map(([value]) => value)).toEqual([
      'sensitive-fixture',
      'safe.ts',
      'next.ts',
    ]);
    expect(JSON.stringify(compact)).not.toContain('sensitive-fixture');
    expect(input.content[0].text).toContain('unused rendered text');
  });

  it('withholds structured output on scanner failure without falling back to raw text', () => {
    scan.mockImplementation(() => {
      throw new Error('scanner failed');
    });
    const result = sanitizeCallToolResult(
      {
        content: [{ type: 'text', text: 'sensitive-fixture' }],
        structuredContent: { nested: ['sensitive-fixture'] },
      },
      'structured'
    );

    expect(result.isError).toBe(true);
    expect(result.content).toEqual([]);
    expect(result.structuredContent).toMatchObject({
      code: 'SANITIZATION_FAILED',
    });
    expect(JSON.stringify(result)).not.toContain('sensitive-fixture');
  });

  it('retains sanitized text-only fallback output', () => {
    const input = {
      content: [{ type: 'text' as const, text: 'sensitive-fixture' }],
    };
    expect(sanitizeCallToolResult(input, 'structured')).toEqual(
      sanitizeCallToolResult(input)
    );
    expect(scan).toHaveBeenCalledWith('sensitive-fixture');
  });

  it('withholds text-only fallback on scanner failure', () => {
    scan.mockImplementation(() => {
      throw new Error('scanner failed');
    });
    const result = sanitizeCallToolResult(
      {
        content: [{ type: 'text', text: 'sensitive-fixture' }],
      },
      'structured'
    );
    expect(JSON.stringify(result)).not.toContain('sensitive-fixture');
    expect(JSON.stringify(result)).toContain('withheld');
  });

  it('retains sanitized error text needed for CLI exit classification', () => {
    const error = new Error('sensitive-fixture');
    const full = buildToolErrorResult('localSearch', error);
    const compact = sanitizeCallToolResult(full, 'structured');
    expect(compact).toEqual(full);
    expect(compact.isError).toBe(true);
    expect(full.content).toHaveLength(1);
    expect(JSON.stringify(compact)).not.toContain('sensitive-fixture');
  });
});
